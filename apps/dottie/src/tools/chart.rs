use super::{address, run};
use crate::domain::chart::{self, Candle, Query, Range};
use crate::runtime::{DottieApp, Lane};
use aomi_sdk::schemars::JsonSchema;
use aomi_sdk::{DynAomiTool, DynToolCallCtx};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema, Clone, Copy)]
pub enum RangeArg {
    #[serde(rename = "1h")]
    H1,
    #[serde(rename = "6h")]
    H6,
    #[serde(rename = "24h")]
    H24,
    #[serde(rename = "7d")]
    D7,
    #[serde(rename = "30d")]
    D30,
    /// The whole life since launch, curve included.
    #[serde(rename = "life")]
    Life,
}

impl From<RangeArg> for Range {
    fn from(r: RangeArg) -> Self {
        match r {
            RangeArg::H1 => Range::H1,
            RangeArg::H6 => Range::H6,
            RangeArg::H24 => Range::H24,
            RangeArg::D7 => Range::D7,
            RangeArg::D30 => Range::D30,
            RangeArg::Life => Range::Life,
        }
    }
}

#[derive(Deserialize, JsonSchema, Clone, Copy)]
pub enum CandleArg {
    #[serde(rename = "1m")]
    M1,
    #[serde(rename = "5m")]
    M5,
    #[serde(rename = "15m")]
    M15,
    #[serde(rename = "1h")]
    H1,
    #[serde(rename = "4h")]
    H4,
    #[serde(rename = "1d")]
    D1,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChartArgs {
    /// Exact Solana mint, copied from a tool result.
    pub token: String,
    /// Main range; the candle is picked to give 24–60 bars. null = 24h.
    pub range: Option<RangeArg>,
    /// A second, wider range returned as closes only, e.g. life next to 6h. null = none.
    pub context_range: Option<RangeArg>,
    /// Candle size override for the main range (zooming in). null = automatic.
    pub interval: Option<CandleArg>,
    /// Bars for the main range when interval is set, 10–60. null = 48.
    pub bars: Option<u16>,
    /// Unix seconds the main range ends at, to page back in time. null = now.
    pub to: Option<i64>,
}

pub struct Chart;

impl DynAomiTool for Chart {
    type App = DottieApp;
    type Args = ChartArgs;
    const NAME: &'static str = "dottie_chart";
    const DESCRIPTION: &'static str = "USD price candles for a Solana token, curve phase included, plus facts computed from the same bars (change, high/low, distance from each, volume trend, higher lows). Pick a range (1h, 6h, 24h, 7d, 30d, life); add context_range for a wider zoom as closes. Read before any claim about chart structure.";

    fn run(app: &DottieApp, args: ChartArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let token = address(&args.token)?;
            let query = Query {
                range: args.range.map(Range::from).unwrap_or(Range::H24),
                context: args.context_range.map(Range::from),
                candle: args.interval.map(|c| match c {
                    CandleArg::M1 => Candle::M1,
                    CandleArg::M5 => Candle::M5,
                    CandleArg::M15 => Candle::M15,
                    CandleArg::H1 => Candle::H1,
                    CandleArg::H4 => Candle::H4,
                    CandleArg::D1 => Candle::D1,
                }),
                bars: args.bars.map(i64::from),
                to: args.to,
            };
            let chart = chart::chart(rt, turn, &token, &query).await?;
            Ok(crate::view::market::chart(&token, &chart, turn))
        })
    }
}
