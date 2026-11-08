use super::{address, run};
use crate::domain::trades::{self, Query, Side};
use crate::runtime::{DottieApp, Lane};
use aomi_sdk::schemars::JsonSchema;
use aomi_sdk::{DynAomiTool, DynToolCallCtx};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum SideArg {
    Buy,
    Sell,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TradesArgs {
    /// Exact Solana mint, copied from a tool result.
    pub token: String,
    /// Only buys or only sells. null = both.
    pub side: Option<SideArg>,
    /// Only trades of at least this many USD. null = all.
    pub min_usd: Option<f64>,
    /// Largest trades to list, 1–10. null = 6.
    pub limit: Option<u8>,
}

pub struct Trades;

impl DynAomiTool for Trades {
    type App = DottieApp;
    type Args = TradesArgs;
    const NAME: &'static str = "dottie_trades";
    const DESCRIPTION: &'static str = "Who is buying and selling a Solana token: buy vs sell dollars and trade counts for 5m, 1h, 6h and 24h, the latest KOL and smart-money fills in it with names and whether they closed, and the biggest top trader's share of the top traders' selling.";

    fn run(app: &DottieApp, args: TradesArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let token = address(&args.token)?;
            let query = Query {
                side: args.side.map(|s| match s {
                    SideArg::Buy => Side::Buy,
                    SideArg::Sell => Side::Sell,
                }),
                min_usd: args.min_usd.unwrap_or(0.0).max(0.0),
                limit: args.limit.unwrap_or(6).clamp(1, 10) as usize,
            };
            let tape = trades::tape(rt, turn, &token, &query).await?;
            Ok(crate::view::market::tape(&token, &tape, None))
        })
    }
}
