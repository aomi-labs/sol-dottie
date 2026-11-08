use super::{address, run};
use crate::domain::token::{Depth, snapshot};
use crate::runtime::{DottieApp, Lane};
use aomi_sdk::schemars::JsonSchema;
use aomi_sdk::{DynAomiTool, DynToolCallCtx};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CheckArgs {
    /// Exact Solana mint, copied from a tool result.
    pub token: String,
}

pub struct Check;

impl DynAomiTool for Check {
    type App = DottieApp;
    type Args = CheckArgs;
    const NAME: &'static str = "dottie_check";
    const DESCRIPTION: &'static str = "Flat numbers for one token, built for watchers (wake_on_condition polls one field against a value): price_usd, fdv_usd, liquidity_usd, holders, top10_pct, dev_pct, snipers_pct, kol_wallets, smart_wallets, buy_5m_usd, sell_5m_usd, change_1h_pct, curve_pct (launchpad coins: 0–100, 100 once graduated) and graduated (1 once it trades in a pool, else 0).";

    fn run(app: &DottieApp, args: CheckArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        // Watch polls run in their own lane so they never starve a chat.
        run(app, &ctx, Lane::Watch, async |rt, turn| {
            let token = address(&args.token)?;
            let snap = snapshot(rt, turn, &token, Depth::Basic).await?;
            Ok(crate::view::token::check(&snap))
        })
    }
}
