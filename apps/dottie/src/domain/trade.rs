//! Dottie's trade rules as code, for quick actions that run without the
//! model (the trade skill states the same rules for the agent): what pays
//! for a buy, exact sizes, slippage, and risks to review. A plan that
//! passes is a complete swap for the host to stage, simulate and commit.
use super::amount::{self, Amount};
use super::exit::{ExitResult, Exits, THIN_PCT, units, units_str};
use super::market::Stage;
use super::settings::{Pay, Settings};
use super::token::{Identity, Security, security};
use crate::card::format::usd;
use crate::providers::gmgn::Gmgn;
use crate::providers::jupiter::Jupiter;
use crate::providers::rpc::Rpc;
use crate::providers::{SOL, USDC};
use crate::runtime::{Fail, Runtime, Turn};
use crate::view::sig;

/// SOL a wallet keeps back for network fees and a new token account's rent.
pub const FEE_RESERVE_SOL: f64 = 0.01;

/// One swap, sized exactly now.
#[derive(Clone, Debug, PartialEq)]
pub struct Trade {
    /// What `stage_jupiter_swap` takes: `SOL` or an exact mint.
    pub input: String,
    pub output: String,
    /// Exact input in the input token's whole units.
    pub amount: String,
    pub slippage_bps: u32,
    /// What the input is worth.
    pub value_usd: Option<f64>,
    /// Expected output in whole units.
    pub out: Option<f64>,
}

pub enum Plan {
    Ready(Trade),
    /// A trade with risks for the user to accept before execution.
    Review(Trade, String),
    /// The trade cannot be prepared; the technical reason for the user.
    No(String),
}

/// What pays for a dollar-sized buy: the user's pick, else USDC when it
/// covers the buy, else SOL when it covers the buy plus the fee reserve.
/// Balances in dollars; the result is never `Auto`.
pub fn pay_with(
    usd_amount: f64,
    pick: Pay,
    usdc: f64,
    sol_usd: f64,
    sol_price: f64,
) -> Result<Pay, String> {
    let reserve = FEE_RESERVE_SOL * sol_price;
    let usdc_covers = usdc >= usd_amount;
    let sol_covers = sol_usd >= usd_amount + reserve;
    let pay = match pick {
        Pay::Usdc | Pay::Auto if usdc_covers => Pay::Usdc,
        Pay::Sol | Pay::Auto if sol_covers => Pay::Sol,
        _ => {
            return Err(format!(
                "Not enough: you have {} USDC and {} of SOL.",
                usd(usdc, false),
                usd(sol_usd, false)
            ));
        }
    };
    if sol_usd < reserve {
        return Err("You need a little SOL for network fees.".into());
    }
    Ok(pay)
}

/// The host's Jupiter cap. A higher tolerance is refused, never clipped.
pub const MAX_SLIPPAGE_BPS: u32 = 500;

/// Dottie's defaults, so the model never works them out: 0.5% for base
/// assets, 3% for memecoins with over $50k liquidity, 5% at or below that,
/// with liquidity unknown, or on a curve.
pub fn default_slippage_bps(base: bool, stage: Stage, liquidity_usd: Option<f64>) -> u32 {
    if base {
        50
    } else if stage != Stage::Curve && liquidity_usd.is_some_and(|l| l > 50_000.0) {
        300
    } else {
        500
    }
}

pub fn slippage_bps(id: &Identity) -> u32 {
    default_slippage_bps(id.base, id.stage, id.liquidity_usd)
}

/// The user's own tolerance when they gave one, else the default.
pub fn chosen_slippage(requested: Option<u32>, default: u32) -> Result<u32, Fail> {
    match requested {
        None => Ok(default),
        Some(bps) if (1..=MAX_SLIPPAGE_BPS).contains(&bps) => Ok(bps),
        Some(bps) => Err(Fail::Invalid(format!(
            "slippage_bps {bps} is outside 1–{MAX_SLIPPAGE_BPS}, the host's cap; never widen or clip it silently"
        ))),
    }
}

/// Advisory findings, never host execution eligibility or a permanent veto.
pub fn warnings(
    security: Option<&Security>,
    exit: ExitResult,
    loss_pct: Option<f64>,
) -> Vec<String> {
    let mut out = vec![];
    if let Some(s) = security {
        if s.honeypot == Some(true) {
            out.push("flagged as a honeypot; you may be unable to sell".into());
        }
        if let Some(tax) = s.sell_tax_pct.filter(|t| *t > 10.0) {
            out.push(format!("sell tax is {tax}%"));
        }
        // Set only by a mint read; a failed read stays unknown, never stale.
        if s.transfer_fee_pct.is_none() {
            out.push(
                "the mint account couldn't be read just now; its transfer fee, authorities and extensions are unknown"
                    .into(),
            );
        }
        if let Some(fee) = s.transfer_fee_pct.filter(|f| *f > 0.0) {
            out.push(format!(
                "a {fee}% Token-2022 transfer fee comes off the buy and the exit"
            ));
        }
        if s.freeze_renounced == Some(false) {
            out.push("the freeze authority is live; your tokens can be frozen".into());
        }
        if s.mint_renounced == Some(false) {
            out.push("the mint authority is live; supply can be printed".into());
        }
        if !s.risky_extensions.is_empty() {
            out.push(format!(
                "Token-2022 extensions can move or block your tokens: {}",
                s.risky_extensions.join(", ")
            ));
        }
    } else {
        out.push("contract security is unavailable".into());
    }
    match exit {
        ExitResult::Ok => {}
        ExitResult::Thin | ExitResult::Trap => out.push(format!(
            "the quoted round trip loses {:.0}% at this size",
            loss_pct.unwrap_or(THIN_PCT)
        )),
        ExitResult::NoSellRoute => out.push(
            "the router returned no sell quote at this size; you may be unable to exit".into(),
        ),
        ExitResult::NoBuyRoute => out.push("the exit probe found no buy route".into()),
        ExitResult::Unknown => {
            out.push("the exit could not be verified; you may be unable to sell".into())
        }
    }
    out
}

/// Plans a buy: picks what pays, fixes the exact amount, quotes it and,
/// for memecoins, runs the security and exit rules at this size.
pub async fn buy(
    rt: &Runtime,
    turn: &Turn,
    id: &Identity,
    size: &Amount,
    settings: &Settings,
    accepted_risks: Option<&str>,
) -> Result<Plan, Fail> {
    let wallet = turn.wallet.as_deref().ok_or(Fail::NoWallet)?;
    let (rpc, jupiter) = (Rpc::new(rt, turn), Jupiter::new(rt, turn));
    let (sol_price, usdc, lamports) = tokio::join!(
        jupiter.sol_usd(),
        rpc.balance(USDC, wallet),
        rpc.native_balance(wallet)
    );
    let (sol_price, sol) = (sol_price?, units(u128::from(lamports?), 9));
    let usdc = units(usdc?, 6);
    let number = |n: &str| n.parse::<f64>().unwrap_or(0.0);
    if id.token == SOL {
        return Ok(Plan::No("SOL is what you buy with; pick a coin.".into()));
    }
    let (input, decimals, amount) = match size {
        Amount::Sol(n) | Amount::Bare(n) => {
            if sol < number(n) + FEE_RESERVE_SOL {
                return Ok(Plan::No(format!(
                    "Not enough SOL: you have {} and need {n} plus a little for fees.",
                    sig(sol, 4)
                )));
            }
            ("SOL", 9, n.clone())
        }
        Amount::Usdc(n) if id.token != USDC => {
            if usdc < number(n) {
                return Ok(Plan::No(format!(
                    "Not enough USDC: you have {}.",
                    sig(usdc, 6)
                )));
            }
            (USDC, 6, n.clone())
        }
        Amount::Usd(d) => {
            // USDC itself is only ever bought with SOL.
            let pick = if id.token == USDC {
                Pay::Sol
            } else {
                settings.pay
            };
            match pay_with(*d, pick, usdc, sol * sol_price, sol_price) {
                Ok(Pay::Usdc) => (USDC, 6, format!("{d:.6}")),
                Ok(_) => ("SOL", 9, format!("{:.9}", d / sol_price)),
                Err(why) => return Ok(Plan::No(why)),
            }
        }
        _ => {
            return Ok(Plan::No(
                "Say how much to spend, like 0.5 SOL or $20.".into(),
            ));
        }
    };
    let atomic = amount::from_decimal(&amount, decimals).map_err(Fail::Invalid)?;
    if atomic == 0 {
        return Ok(Plan::No("That's too small to buy.".into()));
    }
    let amount = amount::format(atomic, decimals);
    let value_usd = match input {
        "SOL" => number(&amount) * sol_price,
        _ => number(&amount),
    };
    let bps = settings.slippage_bps.unwrap_or_else(|| slippage_bps(id));
    let input_mint = if input == "SOL" { SOL } else { USDC };
    let sol_size = format!("{:.9}", value_usd / sol_price);
    // A memecoin also gets the security screen and the round trip at this
    // size; a SOL-paid buy's buy leg is this same quote, read once.
    let raw = atomic.to_string();
    let (quote, warnings) = tokio::join!(jupiter.quote(input_mint, &id.token, &raw, bps), async {
        if id.base {
            return vec![];
        }
        let (screen, mint, trip) = tokio::join!(
            Gmgn::new(rt, turn).live().security(&id.token),
            rpc.mint_live(&id.token),
            Exits::new(rt, turn).round_trip(&id.token, &sol_size, bps)
        );
        let (result, loss) = match trip {
            Ok(trip) => (trip.result, trip.loss_pct),
            Err(_) => (ExitResult::Unknown, None),
        };
        let mut screen = screen.ok().map(security);
        if let (Some(s), Ok(m)) = (screen.as_mut(), &mint) {
            s.merge(m);
        }
        warnings(screen.as_ref(), result, loss)
    });
    let quote = match quote {
        Err(Fail::NoRoute(_)) => {
            return Ok(Plan::No(format!(
                "There's no route to buy **{}** right now.",
                id.symbol
            )));
        }
        other => other?,
    };
    let trade = Trade {
        input: input.into(),
        output: id.token.clone(),
        amount,
        slippage_bps: bps,
        value_usd: Some(value_usd),
        out: units_str(&quote.out_amount, id.decimals),
    };
    let warnings = warnings.join("; ");
    Ok(
        if warnings.is_empty() || accepted_risks == Some(warnings.as_str()) {
            Plan::Ready(trade)
        } else {
            Plan::Review(trade, warnings)
        },
    )
}

/// Plans a sell for SOL from a fresh balance: a percent becomes an exact
/// token amount now; dollars and SOL are sized at current prices.
/// Selling is always allowed; only a missing route stops it.
pub async fn sell(
    rt: &Runtime,
    turn: &Turn,
    id: &Identity,
    size: &Amount,
    settings: &Settings,
    accept_loss: bool,
) -> Result<Plan, Fail> {
    let wallet = turn.wallet.as_deref().ok_or(Fail::NoWallet)?;
    let symbol = &id.symbol;
    if id.token == SOL {
        return Ok(Plan::No("To sell SOL, buy a coin with it.".into()));
    }
    let held = match Rpc::new(rt, turn).balance(&id.token, wallet).await {
        Ok(0) => return Ok(Plan::No(format!("You don't hold any **{symbol}**."))),
        Ok(held) => held,
        Err(_) => {
            return Ok(Plan::No(format!(
                "Couldn't read your **{symbol}** balance right now."
            )));
        }
    };
    let worth = |usd_amount: f64| match id.price_usd.filter(|p| *p > 0.0) {
        Some(price) => Ok(format!(
            "{:.*}",
            id.decimals.min(6) as usize,
            usd_amount / price
        )),
        None => Err(format!(
            "**{symbol}** has no price right now; sell a percent instead."
        )),
    };
    let tokens = match size {
        Amount::Pct(p) => held * ((p * 100.0).round() as u128) / 10_000,
        Amount::Tokens(n, s) if s.eq_ignore_ascii_case(symbol) => {
            amount::from_decimal(n, id.decimals).map_err(Fail::Invalid)?
        }
        Amount::Usd(d) => match worth(*d) {
            Ok(n) => amount::from_decimal(&n, id.decimals).map_err(Fail::Invalid)?,
            Err(why) => return Ok(Plan::No(why)),
        },
        Amount::Sol(n) => {
            let sol_price = Jupiter::new(rt, turn).sol_usd().await?;
            match worth(n.parse::<f64>().unwrap_or(0.0) * sol_price) {
                Ok(n) => amount::from_decimal(&n, id.decimals).map_err(Fail::Invalid)?,
                Err(why) => return Ok(Plan::No(why)),
            }
        }
        _ => {
            return Ok(Plan::No(format!(
                "Say how much to sell: 50%, $10 or 1000 {symbol}."
            )));
        }
    };
    if tokens == 0 {
        return Ok(Plan::No("That's too small to sell.".into()));
    }
    if tokens > held {
        return Ok(Plan::No(format!(
            "You only have {} **{symbol}**.",
            amount::format(held, id.decimals)
        )));
    }
    let bps = settings.slippage_bps.unwrap_or_else(|| slippage_bps(id));
    let sale = Exits::new(rt, turn).sell(&id.token, tokens, bps).await?;
    match sale.sell {
        Err(Fail::NoRoute(_)) => {
            return Ok(Plan::No(format!(
                "The router can't sell **{symbol}** right now."
            )));
        }
        Err(fail) => return Err(fail),
        Ok(_) => {}
    }
    let trade = Trade {
        input: id.token.clone(),
        output: "SOL".into(),
        amount: amount::format(tokens, id.decimals),
        slippage_bps: bps,
        value_usd: sale.value_usd,
        out: sale.sol_out,
    };
    Ok(match sale.loss_pct {
        Some(loss) if loss > THIN_PCT && !accept_loss => Plan::Review(
            trade,
            format!("the quoted sale loses ~{loss:.0}% versus the current token price"),
        ),
        _ => Plan::Ready(trade),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usdc_pays_first_then_sol_keeping_the_fee_reserve() {
        // SOL at $100: the reserve is $1.
        assert_eq!(pay_with(5.0, Pay::Auto, 10.0, 20.0, 100.0), Ok(Pay::Usdc));
        assert_eq!(pay_with(5.0, Pay::Auto, 3.0, 20.0, 100.0), Ok(Pay::Sol));
        assert_eq!(pay_with(5.0, Pay::Sol, 10.0, 20.0, 100.0), Ok(Pay::Sol));
        assert_eq!(pay_with(5.0, Pay::Usdc, 3.0, 20.0, 100.0).ok(), None);
        assert_eq!(
            pay_with(5.0, Pay::Auto, 3.0, 2.0, 100.0),
            Err("Not enough: you have $3.00 USDC and $2.00 of SOL.".into())
        );
        assert!(pay_with(5.0, Pay::Auto, 0.0, 5.5, 100.0).is_err());
        assert_eq!(
            pay_with(5.0, Pay::Auto, 10.0, 0.5, 100.0),
            Err("You need a little SOL for network fees.".into())
        );
    }

    #[test]
    fn slippage_defaults_follow_liquidity_and_curve_and_respect_the_cap() {
        let meme = |l| default_slippage_bps(false, Stage::Pool, l);
        assert_eq!(meme(None), 500);
        assert_eq!(meme(Some(50_000.0)), 500);
        assert_eq!(meme(Some(50_001.0)), 300);
        assert_eq!(default_slippage_bps(false, Stage::Curve, Some(1e9)), 500);
        assert_eq!(default_slippage_bps(true, Stage::Pool, None), 50);
        assert_eq!(chosen_slippage(None, 300).unwrap(), 300);
        assert_eq!(chosen_slippage(Some(120), 300).unwrap(), 120);
        assert!(chosen_slippage(Some(0), 300).is_err());
        assert!(chosen_slippage(Some(501), 300).is_err());
    }

    #[test]
    fn buy_warnings_preserve_all_findings_without_claiming_a_trap() {
        let clean = Security {
            honeypot: Some(false),
            buy_tax_pct: Some(0.0),
            sell_tax_pct: Some(1.0),
            mint_renounced: Some(true),
            freeze_renounced: Some(true),
            transfer_fee_pct: Some(0.0),
            ..Security::default()
        };
        assert!(warnings(Some(&clean), ExitResult::Ok, Some(3.0)).is_empty());
        let unread = Security {
            transfer_fee_pct: None,
            ..clean.clone()
        };
        assert!(warnings(Some(&unread), ExitResult::Ok, None)[0].contains("unknown"));
        assert!(!warnings(None, ExitResult::Ok, Some(3.0)).is_empty());
        let taxed = Security {
            sell_tax_pct: Some(25.0),
            ..clean.clone()
        };
        assert_eq!(
            warnings(Some(&taxed), ExitResult::NoSellRoute, None),
            vec![
                "sell tax is 25%",
                "the router returned no sell quote at this size; you may be unable to exit"
            ]
        );
        let frozen = Security {
            freeze_renounced: Some(false),
            transfer_fee_pct: Some(3.0),
            ..clean.clone()
        };
        let found = warnings(Some(&frozen), ExitResult::Ok, None);
        assert!(found[0].contains("3% Token-2022 transfer fee"));
        assert!(found[1].contains("freeze authority"));
        assert!(warnings(Some(&clean), ExitResult::Thin, Some(14.0))[0].contains("loses 14%"));
    }
}
