use super::{address, run};
use crate::domain::exit::{Exits, Size};
use crate::domain::market::is_base_asset;
use crate::domain::token::{Depth, snapshot};
use crate::domain::trade::{chosen_slippage, default_slippage_bps};
use crate::providers::fomo::Fomo;
use crate::runtime::{DottieApp, Fail, Lane};
use aomi_sdk::schemars::JsonSchema;
use aomi_sdk::{DynAomiTool, DynToolCallCtx};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Research card with an indicative 0.1 SOL exit probe.
    Full,
    /// Buy checks with one exit quote at the requested size; no research enrichment.
    Trade,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TokenArgs {
    /// Exact Solana mint, copied from a tool result or the user's text. Resolve tickers with dottie_find first.
    pub token: String,
    /// trade for a requested buy; full for research. null or omitted = full.
    pub mode: Option<Mode>,
    /// Exact SOL input for trade mode, e.g. "0.5". Give this or usd_amount, never both.
    pub sol_amount: Option<String>,
    /// USD budget for trade mode, converted at the current SOL price.
    pub usd_amount: Option<f64>,
    /// Only a tolerance the user stated, in bps (1–500, the host cap). null = Dottie's default for this coin, computed here. Pass the result's `stage` to stage_jupiter_swap as is.
    pub slippage_bps: Option<u32>,
}

impl TokenArgs {
    fn size(&self) -> Result<Option<Size>, Fail> {
        let invalid = |why: &str| Fail::Invalid(why.into());
        if self.mode != Some(Mode::Trade) {
            return if self.sol_amount.is_some()
                || self.usd_amount.is_some()
                || self.slippage_bps.is_some()
            {
                Err(invalid("size and slippage require mode trade"))
            } else {
                Ok(None)
            };
        }
        if let Some(bps) = self.slippage_bps {
            chosen_slippage(Some(bps), bps)?;
        }
        match (&self.sol_amount, self.usd_amount) {
            (Some(sol), None) => {
                let lamports =
                    crate::domain::amount::from_decimal(sol, 9).map_err(Fail::Invalid)?;
                if lamports == 0 {
                    return Err(invalid("sol_amount must be positive"));
                }
                Ok(Some(Size::Sol(sol.clone())))
            }
            (None, Some(usd)) if usd.is_finite() && usd > 0.0 => Ok(Some(Size::Usd(usd))),
            _ => Err(invalid(
                "trade requires exactly one positive sol_amount or usd_amount",
            )),
        }
    }
}

pub struct Token;

impl DynAomiTool for Token {
    type App = DottieApp;
    type Args = TokenArgs;
    const NAME: &'static str = "dottie_token";
    const DESCRIPTION: &'static str = "One Solana token by exact mint. mode trade: security, Token-2022 fee, liquidity and one Jupiter buy/sell quote at sol_amount or usd_amount and the coin's default slippage (or the user's slippage_bps); do not also call dottie_exit. Its `stage` is stage_jupiter_swap's exact args: pass it as is. checks_passed is advisory: disclose warnings and let the user explicitly proceed. Unknown never becomes a pass. mode full (default): the research card (launchpad and curve, authorities, holder mix, dev, socials, RugCheck, Fomo warnings and holders, links) with an indicative 0.1 SOL exit. Quotes are not fills.";

    fn run(app: &DottieApp, args: TokenArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let token = address(&args.token)?;
            if let Some(size) = args.size()? {
                let snap = snapshot(rt, turn, &token, Depth::Trade).await?;
                let default =
                    default_slippage_bps(is_base_asset(&token), snap.stage, snap.liquidity_usd);
                let slippage = chosen_slippage(args.slippage_bps, default)?;
                let (trip, sol_usd) = Exits::new(rt, turn)
                    .round_trip_sized(&token, size, slippage)
                    .await?;
                return Ok(crate::view::token::trade(&snap, &trip, sol_usd, slippage));
            }
            // The exit probe runs alongside the reads; it is the only test
            // that catches traps the security screen calls clean.
            let fomo = Fomo::new(rt, turn);
            let base = is_base_asset(&token);
            let (snap, probe, warnings, holders) = tokio::join!(
                snapshot(rt, turn, &token, Depth::Card),
                Exits::new(rt, turn).probe(&token),
                fomo_read(turn, "warnings", base, fomo.warnings(&token)),
                fomo_read(turn, "holders", base, fomo.holders(&token)),
            );
            let fomo = crate::view::people::fomo_card(warnings.as_ref(), holders.as_ref());
            Ok(crate::view::token::card(&snap?, Some(&probe), fomo))
        })
    }
}

/// A Fomo read for the card: skipped for base assets and without a key,
/// any other failure noted as a gap.
async fn fomo_read(
    turn: &crate::runtime::Turn,
    what: &str,
    base: bool,
    read: impl Future<Output = Result<Value, Fail>>,
) -> Option<Value> {
    if base {
        return None;
    }
    match read.await {
        Ok(v) => Some(v),
        Err(Fail::Unconfigured(_)) => None,
        Err(fail) => {
            turn.gap(format!("fomo {what} unavailable: {}", fail.message()));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slippage_needs_trade_mode() {
        let args = |v| serde_json::from_value::<TokenArgs>(v).unwrap();
        let research = args(serde_json::json!({"token":"t","slippage_bps":100}));
        assert!(research.size().is_err());
        let trade = args(serde_json::json!({"token":"t","mode":"trade","sol_amount":"0.1"}));
        assert!(matches!(trade.size(), Ok(Some(Size::Sol(_)))));
    }
}
