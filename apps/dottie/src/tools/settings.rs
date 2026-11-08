use super::run;
use crate::domain::settings::{self, Pay};
use crate::runtime::{DottieApp, Lane};
use aomi_sdk::schemars::JsonSchema;
use aomi_sdk::{DynAomiTool, DynToolCallCtx};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("properties" = {}, "required" = []))]
pub struct SettingsGetArgs {}

pub struct SettingsGet;
impl DynAomiTool for SettingsGet {
    type App = DottieApp;
    type Args = SettingsGetArgs;
    const NAME: &'static str = "dottie_settings_get";
    const DESCRIPTION: &'static str = "The user's quick-action settings: Telegram buy sizes, sell percents, what pays for buys, default slippage.";
    fn run(app: &DottieApp, _args: SettingsGetArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let saved = settings::saved(rt, turn).await?;
            let can_save = turn.account.is_some() && turn.settings_store().is_some();
            Ok(json!({ "settings": saved, "can_save": can_save }))
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SettingsSetArgs {
    /// 1–3 quick-buy sizes, 0.001–100 SOL or $1–$10,000, e.g. ["0.1 SOL","0.5 SOL","$50"]. null = unchanged.
    pub buy: Option<Vec<String>>,
    /// 1–3 sell-button percents, e.g. ["25%","100%"]. null = unchanged.
    pub sell: Option<Vec<String>>,
    /// auto (SOL; USDC for dollar buys it covers), sol or usdc. null = unchanged.
    pub pay: Option<Pay>,
    /// Default slippage for quick trades, 1–500 bps (the host cap); 0 = Dottie's per-coin defaults. null = unchanged.
    pub slippage_bps: Option<u32>,
}

pub struct SettingsSet;
impl DynAomiTool for SettingsSet {
    type App = DottieApp;
    type Args = SettingsSetArgs;
    const NAME: &'static str = "dottie_settings_set";
    const DESCRIPTION: &'static str =
        "Save quick-action settings. Pass only what changes; replies with everything saved.";
    fn run(app: &DottieApp, args: SettingsSetArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let mut s = settings::saved(rt, turn).await?;
            s.buy = args.buy.unwrap_or(s.buy);
            s.sell = args.sell.unwrap_or(s.sell);
            s.pay = args.pay.unwrap_or(s.pay);
            if let Some(bps) = args.slippage_bps {
                s.slippage_bps = (bps > 0).then_some(bps);
            }
            Ok(json!({ "saved": settings::save(rt, turn, s).await? }))
        })
    }
}
