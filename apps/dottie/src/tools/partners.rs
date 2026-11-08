//! Dottie's own catalog and build, and the partner tools: FluxBeam quotes
//! and Solomon USDv. The partner tools belong to their skills, so the
//! model sees them only when the user asks about that partner.
use super::{address, run};
use crate::domain::market::catalog;
use crate::providers::{fluxbeam, solomon};
use crate::runtime::{DottieApp, Fail, Lane};
use aomi_sdk::{
    AOMI_FFI_VERSION, AOMI_SDK_VERSION, DynAomiTool, DynToolCallCtx, schemars::JsonSchema,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("properties" = {}, "required" = []))]
pub struct EmptyArgs {}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssetArgs {
    /// Exact symbol or mint; null lists the catalog.
    pub query: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WalletArgs {
    /// Public Solana address; null = the connected wallet.
    pub wallet: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuoteArgs {
    pub input_mint: String,
    pub output_mint: String,
    /// Integer base units, not a decimal amount. Demo cap: 100000000.
    pub amount: String,
    /// 1 through 100 basis points.
    pub slippage_bps: u16,
}

pub struct About;
impl DynAomiTool for About {
    type App = DottieApp;
    type Args = EmptyArgs;
    const NAME: &'static str = "dottie_about";
    const DESCRIPTION: &'static str = "Report which build of Dottie is running: app name, build, SDK and FFI versions, and the session id.";
    fn run(_: &DottieApp, _: EmptyArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        Ok(
            json!({"app":"dottie","build":crate::BUILD,"sdk":AOMI_SDK_VERSION,"ffi":AOMI_FFI_VERSION,"session_id":ctx.session_id}),
        )
    }
}

pub struct Assets;
impl DynAomiTool for Assets {
    type App = DottieApp;
    type Args = AssetArgs;
    const NAME: &'static str = "dottie_assets";
    const DESCRIPTION: &'static str = "Dottie's verified Solana catalog: SOL, stables, LSTs, majors, xStocks and partner tokens with exact mints and decimals, or one exact symbol or mint. Never substitute a lookalike; stock tokens are not brokerage shares.";
    fn run(_: &DottieApp, args: AssetArgs, _: DynToolCallCtx) -> Result<Value, String> {
        let q = args.query.as_deref().map(str::trim);
        let assets: Vec<&crate::domain::market::Asset> = catalog()
            .iter()
            .filter(|a| q.is_none_or(|q| a.mint == q || a.symbol.eq_ignore_ascii_case(q)))
            .collect();
        Ok(
            json!({"status":if assets.is_empty(){"not_found"}else{"ok"},"cluster":"mainnet-beta","assets":assets}),
        )
    }
}

pub struct FluxbeamQuote;
impl DynAomiTool for FluxbeamQuote {
    type App = DottieApp;
    type Args = QuoteArgs;
    const NAME: &'static str = "dottie_fluxbeam_quote";
    const DESCRIPTION: &'static str = "Direct FluxBeam quote for the verified WSOL/FLUXB pair. Integer base units; max 100000000. Indicative: net Token-2022 fee effects are not verified. Never silently use another venue.";
    fn run(app: &DottieApp, args: QuoteArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            fluxbeam::quote(
                rt,
                turn,
                &args.input_mint,
                &args.output_mint,
                &args.amount,
                args.slippage_bps,
            )
            .await
            .map(|quote| json!({ "quote": quote }))
        })
    }
}

pub struct FluxbeamPrepare;
impl DynAomiTool for FluxbeamPrepare {
    type App = DottieApp;
    type Args = EmptyArgs;
    const NAME: &'static str = "dottie_fluxbeam_prepare";
    const DESCRIPTION: &'static str = "FluxBeam execution readiness. Disabled pending full payload validation; returns no transaction and never signs or sends.";
    fn run(_: &DottieApp, _: EmptyArgs, _: DynToolCallCtx) -> Result<Value, String> {
        Ok(fluxbeam::prepare())
    }
}

pub struct SolomonProduct;
impl DynAomiTool for SolomonProduct {
    type App = DottieApp;
    type Args = EmptyArgs;
    const NAME: &'static str = "dottie_solomon_product";
    const DESCRIPTION: &'static str = "The verified current Solomon USDv rewards model, official app and available operations. No invented APR; not legacy vault staking.";
    fn run(_: &DottieApp, _: EmptyArgs, _: DynToolCallCtx) -> Result<Value, String> {
        Ok(solomon::product())
    }
}

pub struct SolomonPosition;
impl DynAomiTool for SolomonPosition {
    type App = DottieApp;
    type Args = WalletArgs;
    const NAME: &'static str = "dottie_solomon_position";
    const DESCRIPTION: &'static str = "A wallet's Solomon enrollment, holdings and program positions. Null and incomplete holdings are unknown, not zero. null wallet = the connected Solana wallet.";
    fn run(app: &DottieApp, args: WalletArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let wallet = match args.wallet.as_deref() {
                Some(w) => address(w)?,
                None => turn.wallet.clone().ok_or(Fail::NoWallet)?,
            };
            solomon::position(rt, turn, &wallet)
                .await
                .map(|position| json!({ "position": position }))
        })
    }
}

pub struct SolomonPrepare;
impl DynAomiTool for SolomonPrepare {
    type App = DottieApp;
    type Args = EmptyArgs;
    const NAME: &'static str = "dottie_solomon_prepare";
    const DESCRIPTION: &'static str = "Solomon enrollment readiness. Disabled pending ownership-proof validation; never request a signature or substitute a legacy vault deposit.";
    fn run(_: &DottieApp, _: EmptyArgs, _: DynToolCallCtx) -> Result<Value, String> {
        Ok(solomon::prepare())
    }
}
