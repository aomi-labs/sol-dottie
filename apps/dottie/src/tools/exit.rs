use super::{address, run};
use crate::domain::exit::{Exits, Size};
use crate::runtime::{DottieApp, Fail, Lane};
use aomi_sdk::schemars::JsonSchema;
use aomi_sdk::{DynAomiTool, DynToolCallCtx};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Buy with sol_amount (or usd_amount) and sell straight back.
    RoundTrip,
    /// Sell token_amount (or usd_amount worth) for SOL.
    Sell,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExitArgs {
    /// Exact Solana mint, copied from a tool result.
    pub token: String,
    /// round_trip or sell. null = sell when token_amount is set, else round_trip.
    pub mode: Option<Mode>,
    /// SOL to buy with for a round trip, e.g. "0.5". null = use usd_amount.
    pub sol_amount: Option<String>,
    /// Tokens to sell in whole units, e.g. "250000". null = use usd_amount.
    pub token_amount: Option<String>,
    /// Size in USD instead, converted at the current price. null = not used.
    pub usd_amount: Option<f64>,
    /// Slippage tolerance in percent for the quotes, at most 10. null = 5.
    pub slippage_pct: Option<f64>,
}

pub struct Exit;

impl DynAomiTool for Exit {
    type App = DottieApp;
    type Args = ExitArgs;
    const NAME: &'static str = "dottie_exit";
    const DESCRIPTION: &'static str = "Can a size get out? Live Jupiter quotes, the router the host trades through. round_trip buys with SOL and sells straight back; sell quotes a token amount back to SOL. Both report loss_pct and a result (ok, thin above 10% loss, trap over 50% lost, no_sell_route), each leg's route or why it failed. Quotes, not fills.";

    fn run(app: &DottieApp, args: ExitArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let token = address(&args.token)?;
            let slippage =
                (args.slippage_pct.unwrap_or(5.0).clamp(0.1, 10.0) * 100.0).round() as u32;
            let exits = Exits::new(rt, turn);
            let size = match (&args.sol_amount, &args.token_amount, args.usd_amount) {
                (Some(sol), _, _) => Size::Sol(sol.clone()),
                (None, Some(tokens), _) => Size::Tokens(tokens.clone()),
                (None, None, Some(usd)) => Size::Usd(usd),
                _ => {
                    return Err(Fail::Invalid(
                        "give sol_amount, token_amount or usd_amount".into(),
                    ));
                }
            };
            let mode = args.mode.unwrap_or(if args.token_amount.is_some() {
                Mode::Sell
            } else {
                Mode::RoundTrip
            });
            match mode {
                Mode::RoundTrip => {
                    let (trip, sol_usd) = exits.round_trip_sized(&token, size, slippage).await?;
                    Ok(crate::view::market::round_trip(&token, &trip, sol_usd))
                }
                Mode::Sell => {
                    let sale = exits.sell_sized(&token, size, slippage).await?;
                    Ok(crate::view::market::sale(&token, &sale))
                }
            }
        })
    }
}
