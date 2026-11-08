use super::run;
use crate::domain::boards::{self, Board, Interval, Query, Sort};
use crate::runtime::{DottieApp, Lane};
use aomi_sdk::schemars::JsonSchema;
use aomi_sdk::{DynAomiTool, DynToolCallCtx};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum BoardArg {
    /// Moving now: the most dollar volume over `interval`.
    Trending,
    /// Launches that are trading, under max_age_h old (default 6), by 1h volume.
    New,
    /// On a launchpad curve, closest to graduating first.
    NearGraduation,
    /// Graduated from a curve recently.
    Graduated,
    /// Deep, liquid coins ($250k+ liquidity) with the most holders.
    Established,
}

#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum SortArg {
    /// The board's own order: volume for trending, holders for
    /// established, closest to graduating, newest, net buying for graduates.
    Rank,
    /// Most smart-money holders.
    SmartMoney,
    /// Most KOL holders.
    Kol,
    /// Biggest price change over the interval.
    Change,
    Volume,
    Newest,
    Holders,
}

#[derive(Deserialize, JsonSchema, Clone, Copy)]
pub enum IntervalArg {
    #[serde(rename = "5m")]
    M5,
    #[serde(rename = "1h")]
    H1,
    #[serde(rename = "6h")]
    H6,
    #[serde(rename = "24h")]
    H24,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScanArgs {
    /// Which board. null = trending.
    pub board: Option<BoardArg>,
    /// Trending window. null = 1h.
    pub interval: Option<IntervalArg>,
    /// Order of the rows. null = the board's own.
    pub sort: Option<SortArg>,
    /// Market-cap band floor in USD (FDV). null = none.
    pub min_fdv_usd: Option<f64>,
    /// Market-cap band ceiling in USD (FDV). null = none.
    pub max_fdv_usd: Option<f64>,
    /// Minimum liquidity in USD. null = the board's default.
    pub min_liquidity_usd: Option<f64>,
    /// Max token age in hours, e.g. 1 for "launched in the last hour". null = 6 for `new`, no limit otherwise.
    pub max_age_h: Option<f64>,
    /// Only one launchpad, e.g. "pump.fun", "letsbonk", "launchlab", "meteora_dbc". null = all.
    pub launchpad: Option<String>,
    /// At least this many smart-money holders. null = any.
    pub min_smart_wallets: Option<u32>,
    /// At least this many KOL holders. null = any.
    pub min_kol_wallets: Option<u32>,
    /// Max share held by the top 10 wallets, percent 0–100. null = any.
    pub max_top10_pct: Option<f64>,
    /// Rows, 1–6. null = 5.
    pub limit: Option<u8>,
}

pub struct Scan;

impl DynAomiTool for Scan {
    type App = DottieApp;
    type Args = ScanArgs;
    const NAME: &'static str = "dottie_scan";
    const DESCRIPTION: &'static str = "List Solana tokens from one board (trending 5m/1h/6h/24h, new, near_graduation, graduated, established), optionally in a market-cap band, from one launchpad, or sorted by smart-money or KOL holders. Rugs, honeypots, wash-traded coins and base assets are filtered out. Rows carry stage, FDV, liquidity, flow, holders, smart and KOL counts and curve progress. A ranked starting list, not picks.";

    fn run(app: &DottieApp, args: ScanArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let query = Query {
                board: match args.board.unwrap_or(BoardArg::Trending) {
                    BoardArg::Trending => Board::Trending,
                    BoardArg::New => Board::New,
                    BoardArg::NearGraduation => Board::NearGraduation,
                    BoardArg::Graduated => Board::Graduated,
                    BoardArg::Established => Board::Established,
                },
                interval: match args.interval.unwrap_or(IntervalArg::H1) {
                    IntervalArg::M5 => Interval::M5,
                    IntervalArg::H1 => Interval::H1,
                    IntervalArg::H6 => Interval::H6,
                    IntervalArg::H24 => Interval::H24,
                },
                sort: match args.sort.unwrap_or(SortArg::Rank) {
                    SortArg::Rank => Sort::Rank,
                    SortArg::SmartMoney => Sort::SmartMoney,
                    SortArg::Kol => Sort::Kol,
                    SortArg::Change => Sort::Change,
                    SortArg::Volume => Sort::Volume,
                    SortArg::Newest => Sort::Newest,
                    SortArg::Holders => Sort::Holders,
                },
                min_fdv_usd: args.min_fdv_usd,
                max_fdv_usd: args.max_fdv_usd,
                min_liquidity_usd: args.min_liquidity_usd,
                max_age_h: args.max_age_h,
                launchpad: crate::domain::market::launchpad(args.launchpad.as_deref()),
                min_smart: args.min_smart_wallets,
                min_kol: args.min_kol_wallets,
                max_top10_pct: args.max_top10_pct,
                limit: args.limit.unwrap_or(5).clamp(1, 6) as usize,
            };
            let scanned = boards::scan(rt, turn, &query).await?;
            let rows: Vec<Value> = scanned
                .rows
                .iter()
                .map(crate::view::token::listing)
                .collect();
            Ok(json!({
                "board": scanned.scope,
                "scanned": scanned.scanned,
                "returned": rows.len(),
                "rows": rows,
            }))
        })
    }
}
