use super::run;
use crate::domain::signals::{self, Query};
use crate::runtime::{DottieApp, Lane};
use aomi_sdk::schemars::JsonSchema;
use aomi_sdk::{DynAomiTool, DynToolCallCtx};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SignalsArgs {
    /// Look back this many minutes, 10–240. null = 60.
    pub window_min: Option<i64>,
    /// Market-cap band floor in USD (FDV). null = none.
    pub min_fdv_usd: Option<f64>,
    /// Market-cap band ceiling in USD (FDV). null = none.
    pub max_fdv_usd: Option<f64>,
    /// Also list coins KOLs and smart money are selling. null = false.
    pub include_selling: Option<bool>,
    /// Coins, 1–8. null = 6.
    pub limit: Option<u8>,
}

pub struct Signals;

impl DynAomiTool for Signals {
    type App = DottieApp;
    type Args = SignalsArgs;
    const NAME: &'static str = "dottie_signals";
    const DESCRIPTION: &'static str = "GMGN's KOL and smart-money feed on Solana, grouped per coin: named buyers, dollars, whether they still hold, FDV at the latest trade, price-spike/ATH events. These wallets are mostly fast bots and launch flippers (median trade ~$100): use it for new or degen plays, not for \"what big traders/KOLs buy\" (that's dottie_people flow). Leads to research, not picks.";

    fn run(app: &DottieApp, args: SignalsArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let window_min = args.window_min.unwrap_or(60).clamp(10, 240);
            let query = Query {
                window_min,
                min_fdv_usd: args.min_fdv_usd,
                max_fdv_usd: args.max_fdv_usd,
                include_selling: args.include_selling.unwrap_or(false),
                limit: args.limit.unwrap_or(6).clamp(1, 8) as usize,
            };
            let feed = signals::feed(rt, turn, &query).await?;
            Ok(crate::view::people::signals(&feed, window_min))
        })
    }
}
