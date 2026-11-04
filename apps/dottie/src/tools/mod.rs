use crate::{domain, providers, runtime::DottieApp, view};
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
    /// Optional exact symbol or mint; null lists the curated catalog.
    pub query: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MintArgs {
    pub mint: String,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WalletArgs {
    /// Public Solana address; omit/null to use host operating wallet.
    pub wallet: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuoteArgs {
    pub input_mint: String,
    pub output_mint: String,
    /// Integer base units, not a floating point amount. Demo cap: 100000000.
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
    const DESCRIPTION: &'static str = "List curated Solana asset identities and supported actions, or resolve one exact mint/symbol. Never substitute a lookalike. Stock token actions require current host routes.";
    fn run(_: &DottieApp, args: AssetArgs, _: DynToolCallCtx) -> Result<Value, String> {
        let catalog: Vec<Value> = serde_json::from_str(include_str!("../domain/assets.json"))
            .map_err(|_| "Invalid bundled catalog")?;
        let assets: Vec<_> = catalog
            .into_iter()
            .filter(|a| {
                args.query.as_ref().is_none_or(|q| {
                    a["mint"].as_str() == Some(q.trim())
                        || a["symbol"]
                            .as_str()
                            .is_some_and(|s| s.eq_ignore_ascii_case(q.trim()))
                })
            })
            .collect();
        Ok(
            json!({"status":if assets.is_empty(){"not_found"}else{"ok"},"cluster":"mainnet-beta","verified_on":"2026-11-02","assets":assets}),
        )
    }
}
pub struct TokenCheck;
impl DynAomiTool for TokenCheck {
    type App = DottieApp;
    type Args = MintArgs;
    const NAME: &'static str = "dottie_token_check";
    const DESCRIPTION: &'static str = "Read RugCheck risk signals for an exact Solana mint. Report missing data as unknown; scores never guarantee safety. Read-only.";
    fn run(app: &DottieApp, args: MintArgs, _: DynToolCallCtx) -> Result<Value, String> {
        let rt = app.runtime()?;
        view::result(
            rt.executor
                .block_on(providers::rugcheck::report(&rt, &args.mint)),
        )
    }
}
pub struct FluxbeamQuote;
impl DynAomiTool for FluxbeamQuote {
    type App = DottieApp;
    type Args = QuoteArgs;
    const NAME: &'static str = "dottie_fluxbeam_quote";
    const DESCRIPTION: &'static str = "Get a direct FluxBeam quote for the verified WSOL/FLUXB pair. Integer base units; max 100000000. Indicative only: full payload and net transfer-fee effects are not yet verified. Never silently use Jupiter.";
    fn run(app: &DottieApp, args: QuoteArgs, _: DynToolCallCtx) -> Result<Value, String> {
        let rt = app.runtime()?;
        view::result(rt.executor.block_on(providers::fluxbeam::quote(
            &rt,
            &args.input_mint,
            &args.output_mint,
            &args.amount,
            args.slippage_bps,
        )))
    }
}
pub struct FluxbeamPrepare;
impl DynAomiTool for FluxbeamPrepare {
    type App = DottieApp;
    type Args = EmptyArgs;
    const NAME: &'static str = "dottie_fluxbeam_prepare";
    const DESCRIPTION: &'static str = "Report direct FluxBeam execution readiness. Currently disabled pending full payload validation; returns no transaction and never signs or sends.";
    fn run(_: &DottieApp, _: EmptyArgs, _: DynToolCallCtx) -> Result<Value, String> {
        Ok(providers::fluxbeam::prepare())
    }
}
pub struct SolomonProduct;
impl DynAomiTool for SolomonProduct {
    type App = DottieApp;
    type Args = EmptyArgs;
    const NAME: &'static str = "dottie_solomon_product";
    const DESCRIPTION: &'static str = "Explain the verified current Solomon USDv rewards model, official app and available operations. No invented APR; not legacy vault staking.";
    fn run(_: &DottieApp, _: EmptyArgs, _: DynToolCallCtx) -> Result<Value, String> {
        Ok(providers::solomon::product())
    }
}
pub struct SolomonPosition;
impl DynAomiTool for SolomonPosition {
    type App = DottieApp;
    type Args = WalletArgs;
    const NAME: &'static str = "dottie_solomon_position";
    const DESCRIPTION: &'static str = "Read Solomon wallet enrollment, holdings and program positions. Null and incomplete holdings are unknown, not zero. Omit wallet to use the host operating Solana wallet.";
    fn run(app: &DottieApp, args: WalletArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        let Some(wallet) = args
            .wallet
            .or_else(|| ctx.attribute_string(&["domain", "svm", "address"]))
        else {
            return Ok(view::unavailable(
                "WALLET_REQUIRED",
                "Connect a Solana wallet or provide a public address to inspect",
            ));
        };
        if let Err(e) = domain::pubkey(&wallet) {
            return view::result(Err(e));
        }
        let rt = app.runtime()?;
        view::result(
            rt.executor
                .block_on(providers::solomon::position(&rt, &wallet)),
        )
    }
}
pub struct SolomonPrepare;
impl DynAomiTool for SolomonPrepare {
    type App = DottieApp;
    type Args = EmptyArgs;
    const NAME: &'static str = "dottie_solomon_prepare";
    const DESCRIPTION: &'static str = "Report Solomon enrollment readiness. Currently disabled pending ownership-proof validation; never request a signature or substitute a legacy vault deposit.";
    fn run(_: &DottieApp, _: EmptyArgs, _: DynToolCallCtx) -> Result<Value, String> {
        Ok(providers::solomon::prepare())
    }
}
