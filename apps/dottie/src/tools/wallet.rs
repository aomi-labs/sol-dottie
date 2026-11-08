use super::{address, run};
use crate::domain::wallet::{self, Period};
use crate::runtime::{DottieApp, Fail, Lane};
use aomi_sdk::schemars::JsonSchema;
use aomi_sdk::{DynAomiTool, DynToolCallCtx};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema, Clone, Copy)]
pub enum PeriodArg {
    #[serde(rename = "7d")]
    Week,
    #[serde(rename = "30d")]
    Month,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WalletArgs {
    /// Solana wallet to look at. null = the user's connected wallet.
    pub wallet: Option<String>,
    /// Record period. null = 7d.
    pub period: Option<PeriodArg>,
    /// Also list its latest trades. null = false.
    pub recent_trades: Option<bool>,
}

pub struct Wallet;

impl DynAomiTool for Wallet {
    type App = DottieApp;
    type Args = WalletArgs;
    const NAME: &'static str = "dottie_wallet";
    const DESCRIPTION: &'static str = "A Solana wallet's GMGN record: realized profit, win rate, how its closed tokens ended (under −50% up to over 5×), average hold, KOL or smart-money tags and X handle, coins its recent trades left open, and optionally its latest trades. Defaults to the user's connected wallet.";

    fn run(app: &DottieApp, args: WalletArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let target = match args.wallet.as_deref() {
                Some(w) => address(w)?,
                None => turn.wallet.clone().ok_or(Fail::NoWallet)?,
            };
            let period = match args.period.unwrap_or(PeriodArg::Week) {
                PeriodArg::Week => Period::Week,
                PeriodArg::Month => Period::Month,
            };
            let view = wallet::wallet(
                rt,
                turn,
                &target,
                period,
                args.recent_trades.unwrap_or(false),
            )
            .await?;
            let is_user = turn.wallet.as_deref() == Some(target.as_str());
            Ok(crate::view::people::wallet(&view, is_user))
        })
    }
}
