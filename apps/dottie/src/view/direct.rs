//! Quick-action replies for Telegram, which the host shows without the
//! model and keeps in the chat history: Markdown `text` that names the
//! coin and its full mint, up to six `controls`, and a `tx` the host
//! stages, simulates and commits when Dottie has a checked trade.
use super::{compact, safe, sig};
use crate::card::format;
use crate::domain::market::{Stage, links};
use crate::domain::settings::{Pay, Settings};
use crate::domain::token::Identity;
use crate::domain::trade::{Plan, Trade};
use crate::providers::USDC;
use crate::runtime::Fail;
use serde_json::{Value, json};

pub const PREVIEW: &str = "dottie_preview";
pub const BUY: &str = "dottie_buy";
pub const SELL: &str = "dottie_sell";
pub const POSITIONS: &str = "dottie_positions_view";
pub const SETTINGS: &str = "dottie_settings_view";

/// Tap: the user says `text` to the agent.
fn say(label: &str, text: String) -> Value {
    json!({ "label": label, "action": "say", "text": text })
}

/// Tap: the host calls a direct tool with `args`.
fn run(label: String, tool: &str, args: Value) -> Value {
    json!({ "label": label, "action": "run", "tool": tool, "args": compact(args) })
}

/// Tap: the host asks `prompt`; the user's reply arrives as `args.text`.
fn ask(tool: &str, args: Value, prompt: String) -> Value {
    let mut control = run("Custom".into(), tool, args);
    control["ask"] = json!(prompt);
    control
}

pub fn text(text: impl Into<String>) -> Value {
    json!({ "text": text.into() })
}

pub fn failed(fail: &Fail) -> Value {
    text(match fail {
        Fail::NoWallet => "Connect a Solana wallet first.".into(),
        Fail::NotFound(why) | Fail::Invalid(why) => why.clone(),
        other => format!(
            "Couldn't do that right now ({}). Try again in a moment.",
            other.message()
        ),
    })
}

/// Token or SOL units the way a person writes them.
fn units(v: f64) -> String {
    match v {
        v if v >= 100_000.0 => format::compact(v, 3),
        v if v >= 1000.0 => format::thousands(v.round() as i64),
        v => sig(v, 4).to_string(),
    }
}

/// Dollars as the card writes them, in plain text.
fn usd(v: f64) -> String {
    format::plain(&format::usd(v, false))
}

fn slippage(bps: u32) -> String {
    format!("{}%", bps as f64 / 100.0)
}

fn analyze(id: &Identity) -> Value {
    say("Analyze", format!("Analyze {} ({})", id.symbol, id.token))
}

fn buys(id: &Identity, settings: &Settings) -> Vec<Value> {
    settings
        .buy
        .iter()
        .map(|b| {
            run(
                format!("Buy {b}"),
                BUY,
                json!({ "token": id.token, "amount": b }),
            )
        })
        .collect()
}

fn sells(id: &Identity, settings: &Settings) -> Vec<Value> {
    settings
        .sell
        .iter()
        .map(|s| {
            run(
                format!("Sell {s}"),
                SELL,
                json!({ "token": id.token, "amount": s }),
            )
        })
        .collect()
}

fn custom_buy(id: &Identity) -> Value {
    ask(
        BUY,
        json!({ "token": id.token }),
        "How much SOL? e.g. 0.25, or $20".into(),
    )
}

/// The card for a pasted mint. `held` is the connected wallet's
/// balance in whole units: `None` without a wallet, `Some(None)` when it
/// couldn't be read (never shown as zero).
pub fn preview(id: &Identity, held: Option<Option<f64>>, settings: &Settings) -> Value {
    let mut lines = vec![match &id.name {
        Some(name) if !name.eq_ignore_ascii_case(&id.symbol) => {
            format!("**{}** · {name}", id.symbol)
        }
        _ => format!("**{}**", id.symbol),
    }];
    lines.push(format!("Solana · `{}`", id.token));
    let facts: Vec<String> = [
        id.price_usd
            .map(|p| format!("Price {}", format::plain(&format::price(p)))),
        id.liquidity_usd.map(|l| format!("Liquidity {}", usd(l))),
        id.fdv_usd.map(|f| format!("FDV {}", usd(f))),
        (id.stage == Stage::Curve).then(|| "on its launch curve".into()),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !facts.is_empty() {
        lines.push(facts.join(" · "));
    }
    lines.push(match held {
        None => "Connect a Solana wallet to trade it.".into(),
        Some(Some(v)) if v > 0.0 => match id.price_usd {
            Some(p) => format!("You hold {} {} (~{}).", units(v), id.symbol, usd(v * p)),
            None => format!("You hold {} {}.", units(v), id.symbol),
        },
        Some(Some(_)) => "You don't hold any.".into(),
        Some(None) => "Your balance couldn't be read right now.".into(),
    });
    let mut controls = match held {
        Some(Some(v)) if v > 0.0 => {
            let mut c = sells(id, settings);
            c.extend(buys(id, settings).into_iter().take(1));
            c
        }
        _ => buys(id, settings),
    };
    controls.extend([custom_buy(id), analyze(id)]);
    let l = links(&id.token);
    lines.push(format!(
        "[GMGN]({}) · [DexScreener]({}) · [Solscan]({})",
        l["gmgn"].as_str().unwrap_or(""),
        l["dexscreener"].as_str().unwrap_or(""),
        l["solscan"].as_str().unwrap_or("")
    ));
    json!({ "text": lines.join("\n"), "controls": controls })
}

/// A token was given but no size: the size buttons.
pub fn how_much(id: &Identity, settings: &Settings, sell: bool) -> Value {
    let mut controls = if sell {
        let mut c = sells(id, settings);
        c.push(ask(
            SELL,
            json!({ "token": id.token }),
            format!("How much to sell? e.g. 50%, $10 or 1000 {}", id.symbol),
        ));
        c
    } else {
        let mut c = buys(id, settings);
        c.push(custom_buy(id));
        c
    };
    controls.push(analyze(id));
    json!({
        "text": format!(
            "How much **{}** (`{}`) to {}?",
            id.symbol,
            id.token,
            if sell { "sell" } else { "buy" }
        ),
        "controls": controls,
    })
}

/// The host's Jupiter fast path, with its exact argument names: it
/// quotes, builds and stages the swap; the host then simulates and commits
/// exactly what it staged.
fn tx(t: &Trade) -> Value {
    json!({
        "tool": "stage_jupiter_swap",
        "args": {
            "input_token": t.input,
            "output_token": t.output,
            "amount": t.amount,
            "slippage_bps": t.slippage_bps,
        }
    })
}

fn paid(t: &Trade) -> &'static str {
    if t.input == USDC { "USDC" } else { "SOL" }
}

pub fn buy(id: &Identity, plan: Plan, wallet: Option<&str>) -> Value {
    let t = match plan {
        Plan::Ready(t) => t,
        Plan::No(why) => return json!({ "text": why, "controls": [analyze(id)] }),
        Plan::Review(t, why) => {
            let amount = format!("{} {}", t.amount, paid(&t));
            return json!({
                "text": format!("Buying **{}** (`{}`) with {amount}: {why}. Buy anyway if you accept these risks.", id.symbol, id.token),
                "controls": [run("Buy anyway".into(), BUY, json!({
                    "token": id.token,
                    "amount": amount,
                    "slippage_bps": t.slippage_bps,
                    "accepted_risks": why,
                    "reviewed_wallet": wallet,
                })), analyze(id)],
            });
        }
    };
    let spend = match t.value_usd {
        _ if t.input == USDC => format!("{} USDC", t.amount),
        Some(v) => format!("{} SOL (~{})", t.amount, usd(v)),
        None => format!("{} SOL", t.amount),
    };
    let out = t
        .out
        .map(|o| format!(" · ≈{} {}", units(o), id.symbol))
        .unwrap_or_default();
    json!({
        "text": format!(
            "Preparing to buy **{}** (`{}`) with {spend}{out} · slippage {}",
            id.symbol,
            id.token,
            slippage(t.slippage_bps)
        ),
        "tx": tx(&t),
    })
}

/// `again` is the tap that repeats this sell with the loss accepted.
pub fn sell(id: &Identity, plan: Plan, settings: &Settings, again: Value) -> Value {
    let (t, warning) = match plan {
        Plan::Ready(t) => (t, None),
        Plan::Review(t, why) => (t, Some(why)),
        Plan::No(why) => return text(why),
    };
    let tokens = units(t.amount.parse().unwrap_or(0.0));
    let worth = t
        .value_usd
        .map(|v| format!(" (~{})", usd(v)))
        .unwrap_or_default();
    let back = t
        .out
        .map(|o| format!("≈{} SOL", units(o)))
        .unwrap_or_else(|| "SOL".into());
    let Some(warning) = warning else {
        return json!({
            "text": format!(
                "Preparing to sell {tokens} **{}** (`{}`){worth} for {back} · slippage {}",
                id.symbol,
                id.token,
                slippage(t.slippage_bps)
            ),
            "tx": tx(&t),
        });
    };
    let mut controls = sells(id, settings);
    controls.push(run("Sell anyway".into(), SELL, again));
    json!({
        "text": format!(
            "Selling {tokens} **{}** (`{}`){worth} for {back}: {warning}. Sell a smaller part, or sell anyway.",
            id.symbol, id.token
        ),
        "controls": controls,
    })
}

/// One page of `domain::portfolio::positions`; the SOL balance and its
/// price come on the first page only.
pub fn positions(wallet: &str, page: &Value) -> Value {
    let mut lines = vec![format!("**Positions** of `{wallet}`")];
    let mut total = 0.0;
    if let Some(sol) = page["native_sol"].as_f64() {
        let value = page["sol_usd"].as_f64().map(|p| sol * p);
        total += value.unwrap_or(0.0);
        lines.push(match value {
            Some(v) => format!("SOL {} · {}", units(sol), usd(v)),
            None => format!("SOL {}", units(sol)),
        });
    }
    let mut controls = vec![];
    for item in page["items"].as_array().into_iter().flatten() {
        let symbol = safe(item["symbol"].as_str().unwrap_or("?"), 12);
        let amount = item["amount"].as_f64();
        let value = item["estimated_value_usd"].as_f64();
        total += value.unwrap_or(0.0);
        let amount = amount.map(units).unwrap_or_else(|| "?".into());
        let shown = value.map(usd);
        lines.push(match &shown {
            Some(v) => format!("**{symbol}** {amount} · {v}"),
            None => format!("**{symbol}** {amount} · value unknown"),
        });
        controls.push(run(
            format!("{symbol} {}", shown.unwrap_or_default())
                .trim()
                .into(),
            PREVIEW,
            json!({ "token": item["token"] }),
        ));
    }
    if lines.len() == 1 {
        lines.push("Nothing here.".into());
    } else {
        let more = page["next_cursor"].is_string();
        lines.push(format!(
            "{} ≈ {}",
            if more { "This page" } else { "Total" },
            usd(total)
        ));
    }
    if let Some(cursor) = page["next_cursor"].as_str() {
        controls.push(run("Next".into(), POSITIONS, json!({ "cursor": cursor })));
    }
    json!({ "text": lines.join("\n"), "controls": controls })
}

pub fn settings(s: &Settings, note: Option<&str>) -> Value {
    let pay = match s.pay {
        Pay::Auto => "SOL; USDC for dollar buys it covers",
        Pay::Sol => "SOL",
        Pay::Usdc => "USDC",
    };
    let slip = s.slippage_bps.map(slippage).unwrap_or_else(|| {
        "Dottie's defaults (0.5% for SOL, stables, majors and stocks; 3–5% for memecoins)".into()
    });
    let mut lines = vec![
        "**Your Dottie settings**".to_string(),
        format!("Quick buys: {}", s.buy.join(", ")),
        format!("Sell buttons: {}", s.sell.join(", ")),
        format!("Pay with: {pay}"),
        format!("Slippage: {slip}"),
        "To change, just tell me, e.g. \"set my buys to 0.25, 1 and 2 SOL\".".into(),
    ];
    lines.extend(note.map(str::to_string));
    text(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_keeps_exact_amount_and_slippage_basis_points() {
        for bps in [1, 50, 300, 500] {
            let trade = Trade {
                input: "SOL".into(),
                output: "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263".into(),
                amount: "0.100000001".into(),
                slippage_bps: bps,
                value_usd: Some(9.0),
                out: Some(100.0),
            };
            let request = tx(&trade);
            assert_eq!(request["tool"], "stage_jupiter_swap");
            assert_eq!(request["args"]["input_token"], "SOL");
            assert_eq!(request["args"]["amount"], "0.100000001");
            assert_eq!(request["args"]["slippage_bps"], bps);
            assert_eq!(slippage(bps), format!("{}%", bps as f64 / 100.0));
        }
    }
}
