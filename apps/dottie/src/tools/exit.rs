use super::{address, run};
use crate::domain::exit::{Exits, Size};
use crate::domain::token::identity;
use crate::domain::trade;
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
    /// Only a tolerance the user stated, in bps (1–500, the host cap). null = Dottie's default for this coin. A sell's `stage` carries it to stage_jupiter_swap.
    pub slippage_bps: Option<u32>,
}

pub struct Exit;

impl DynAomiTool for Exit {
    type App = DottieApp;
    type Args = ExitArgs;
    const NAME: &'static str = "dottie_exit";
    const DESCRIPTION: &'static str = "Can a size get out? Live Jupiter quotes, the router the host trades through. round_trip buys with SOL and sells straight back; sell quotes a token amount back to SOL at the coin's default slippage (or the user's slippage_bps), and its `stage` is stage_jupiter_swap's exact args: pass it as is. Both report loss_pct and a result (ok, thin above 10% loss, trap over 50% lost, no_sell_route), each leg's route or why it failed. Quotes, not fills.";

    fn run(app: &DottieApp, args: ExitArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let token = address(&args.token)?;
            let default = trade::slippage_bps(&identity(rt, turn, &token).await?);
            let slippage = trade::chosen_slippage(args.slippage_bps, default)?;
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
                    Ok(crate::view::market::sale(&token, &sale, slippage))
                }
            }
        })
    }
}
