use super::{address, run};
use crate::domain::holders;
use crate::providers::fomo::Fomo;
use crate::runtime::{DottieApp, Lane};
use aomi_sdk::schemars::JsonSchema;
use aomi_sdk::{DynAomiTool, DynToolCallCtx};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum View {
    /// The biggest holders, contracts labelled, KOL and smart wallets marked.
    Top,
    /// Only smart-money holders, with names and unrealized PnL.
    Smart,
    /// Only KOL holders, with names, X handles and unrealized PnL.
    Kol,
    /// The most profitable traders of the coin, realized and unrealized.
    Traders,
    /// The dev: what they still hold and their launches, graduated or not.
    Dev,
    /// Real Fomo traders holding it: how many, and the biggest by name with position, PnL and their thesis.
    Fomo,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HoldersArgs {
    /// Exact Solana mint, copied from a tool result.
    pub token: String,
    /// What to show. null = top.
    pub view: Option<View>,
}

pub struct Holders;

impl DynAomiTool for Holders {
    type App = DottieApp;
    type Args = HoldersArgs;
    const NAME: &'static str = "dottie_holders";
    const DESCRIPTION: &'static str = "Who holds a Solana token. view=top: holder count, top-10 share and the biggest holders with % of supply, USD value, how long held and labels (pools and curves labelled). view=smart or kol: those holders by name with PnL. view=traders: who made money on it. view=dev: the dev wallet, what it still holds and its other launches. view=fomo: how many real Fomo traders hold it and the biggest by name with PnL and thesis. Other wallets are addresses, not people.";

    fn run(app: &DottieApp, args: HoldersArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let token = address(&args.token)?;
            let view = crate::view::people::rows;
            Ok(match args.view.unwrap_or(View::Top) {
                View::Top => {
                    crate::view::people::top(&token, &holders::top(rt, turn, &token).await?)
                }
                View::Smart => view(
                    &token,
                    "smart",
                    &holders::labelled(rt, turn, &token, Some("smart_degen")).await?,
                ),
                View::Kol => view(
                    &token,
                    "kol",
                    &holders::labelled(rt, turn, &token, Some("renowned")).await?,
                ),
                View::Traders => view(
                    &token,
                    "traders",
                    &holders::traders(rt, turn, &token).await?,
                ),
                View::Dev => {
                    crate::view::people::dev(&token, &holders::dev(rt, turn, &token).await?)
                }
                View::Fomo => crate::view::people::fomo_holders(
                    &token,
                    &Fomo::new(rt, turn).holders(&token).await?,
                ),
            })
        })
    }
}
