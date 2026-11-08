use super::{address, run};
use crate::{
    domain::{pnl, portfolio},
    runtime::{DottieApp, Fail, Lane},
};
use aomi_sdk::{DynAomiTool, DynToolCallCtx, schemars::JsonSchema};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PnlArgs {
    /// Exact Solana mint. No transaction lookup needed for the latest round.
    pub token: String,
    /// null = connected wallet.
    pub wallet: Option<String>,
    /// Optional transaction signature in an older round; null = latest round.
    pub signature: Option<String>,
}

pub struct Pnl;
impl DynAomiTool for Pnl {
    type App = DottieApp;
    type Args = PnlArgs;
    const NAME: &'static str = "dottie_pnl";
    const DESCRIPTION: &'static str = "Token round PnL without an image. Reconciles each fill from its confirmed Solana transaction (token and SOL/USDC deltas); network fees and account rent are reported apart. signature selects an older round.";
    fn run(app: &DottieApp, args: PnlArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let token = address(&args.token)?;
            let wallet = target(args.wallet.as_deref(), turn)?;
            let figures = pnl::card(rt, turn, &wallet, &token, args.signature.as_deref()).await?;
            Ok(json!({"wallet":wallet,"token":token,"pnl":super::card::summary(&figures)}))
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PositionsArgs {
    /// null = connected wallet.
    pub wallet: Option<String>,
    /// Optional exact mint filter. null = all SPL and Token-2022 tokens.
    pub token: Option<String>,
    /// Minimum estimated USD value. null = 0, including dust and unpriced assets.
    pub min_value_usd: Option<f64>,
    /// Rows per reply, 1–5. null = 5.
    pub limit: Option<u8>,
    /// next_cursor from this tool; retain the wallet and filters. Expires in 10 minutes.
    pub cursor: Option<String>,
}

pub struct Positions;
impl DynAomiTool for Positions {
    type App = DottieApp;
    type Args = PositionsArgs;
    const NAME: &'static str = "dottie_positions";
    const DESCRIPTION: &'static str = "Solana wallet holdings: SOL plus SPL and Token-2022 tokens, dust included. Mint/value filters and pagination. Values are prices, not cost; use dottie_pnl for cost and return.";
    fn run(app: &DottieApp, args: PositionsArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let wallet = target(args.wallet.as_deref(), turn)?;
            let token = args.token.as_deref().map(address).transpose()?;
            portfolio::positions(
                rt,
                turn,
                &wallet,
                token.as_deref(),
                minimum(args.min_value_usd)?,
                limit(args.limit)?,
                args.cursor.as_deref(),
            )
            .await
        })
    }
}

#[derive(Deserialize, JsonSchema, Clone, Copy)]
pub enum Side {
    #[serde(rename = "buy")]
    Buy,
    #[serde(rename = "sell")]
    Sell,
    #[serde(rename = "transferIn")]
    TransferIn,
    #[serde(rename = "transferOut")]
    TransferOut,
}
impl Side {
    fn name(self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
            Self::TransferIn => "transferIn",
            Self::TransferOut => "transferOut",
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryArgs {
    /// null = connected wallet.
    pub wallet: Option<String>,
    /// Optional exact mint.
    pub token: Option<String>,
    /// Optional buy, sell, transferIn or transferOut filter.
    pub side: Option<Side>,
    /// Inclusive Unix seconds, null = no lower bound.
    pub from_ts: Option<i64>,
    /// Inclusive Unix seconds, null = no upper bound.
    pub to_ts: Option<i64>,
    /// Minimum GMGN indexed trade value; this is not verified wallet spend. null = 0.
    pub min_usd: Option<f64>,
    /// Rows per reply, 1–5. null = 5.
    pub limit: Option<u8>,
    /// next_cursor from this tool; keep wallet and filters unchanged. Expires in 10 minutes.
    pub cursor: Option<String>,
}

pub struct History;
impl DynAomiTool for History {
    type App = DottieApp;
    type Args = HistoryArgs;
    const NAME: &'static str = "dottie_history";
    const DESCRIPTION: &'static str = "Solana wallet swaps and transfers (GMGN indexed) with mint, side, date and size filters and pagination. Indexed values are not PnL.";
    fn run(app: &DottieApp, args: HistoryArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            if args.from_ts.is_some_and(|t| t < 0)
                || args.to_ts.is_some_and(|t| t < 0)
                || args.from_ts.zip(args.to_ts).is_some_and(|(a, b)| a > b)
            {
                return Err(Fail::Invalid(
                    "date range must be nonnegative Unix seconds with from_ts <= to_ts".into(),
                ));
            }
            let wallet = target(args.wallet.as_deref(), turn)?;
            let token = args.token.as_deref().map(address).transpose()?;
            portfolio::history(
                rt,
                turn,
                portfolio::HistoryQuery {
                    wallet: &wallet,
                    token: token.as_deref(),
                    side: args.side.map(Side::name),
                    from: args.from_ts,
                    to: args.to_ts,
                    min_usd: minimum(args.min_usd)?,
                    limit: limit(args.limit)?,
                    cursor: args.cursor.as_deref(),
                },
            )
            .await
        })
    }
}

fn target(wallet: Option<&str>, turn: &crate::runtime::Turn) -> Result<String, Fail> {
    match wallet {
        Some(w) => address(w),
        None => turn.wallet.clone().ok_or(Fail::NoWallet),
    }
}
fn limit(n: Option<u8>) -> Result<usize, Fail> {
    let n = n.unwrap_or(5);
    if !(1..=5).contains(&n) {
        return Err(Fail::Invalid("limit must be 1–5".into()));
    }
    Ok(n as usize)
}
fn minimum(n: Option<f64>) -> Result<f64, Fail> {
    let n = n.unwrap_or(0.0);
    if !n.is_finite() || n < 0.0 {
        return Err(Fail::Invalid(
            "minimum USD value must be finite and nonnegative".into(),
        ));
    }
    Ok(n)
}
