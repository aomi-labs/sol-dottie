//! Direct tools: the host runs them from Telegram shortcuts and buttons,
//! without the model, and shows their reply as is (`view::direct`). Their
//! descriptions are notes for maintainers; the model never sees them.
use super::address;
use crate::domain::amount::{Amount, order, parse};
use crate::domain::token::identity;
use crate::domain::{portfolio, settings, trade};
use crate::providers::rpc::Rpc;
use crate::runtime::{DottieApp, Fail, Lane, Runtime, Turn};
use crate::view::direct as view;
use aomi_sdk::schemars::{JsonSchema, schema_for};
use aomi_sdk::{DynAomiApp, DynAomiTool, DynToolCallCtx, DynToolMetadata};
use serde::Deserialize;
use serde_json::{Value, json};

/// A direct tool's descriptor: its own, in the `direct` namespace the
/// host keeps away from the model.
fn direct<T: DynAomiTool>(app: &T::App) -> DynToolMetadata {
    DynToolMetadata {
        name: T::NAME.into(),
        app: app.name().into(),
        description: T::DESCRIPTION.into(),
        parameters_schema: serde_json::to_value(schema_for!(T::Args)).unwrap_or(Value::Null),
        supports_async: T::IS_ASYNC,
        namespace: Some("direct".into()),
    }
}

/// Runs a direct tool body; a failure becomes a short reply too.
fn reply<F>(app: &DottieApp, ctx: &DynToolCallCtx, body: F) -> Result<Value, String>
where
    F: AsyncFnOnce(&Runtime, &Turn) -> Result<Value, Fail>,
{
    let rt = app.runtime()?;
    let turn = Turn::new(ctx, Lane::Chat);
    let out = rt.tokio.block_on(body(&rt, &turn));
    Ok(out.unwrap_or_else(|fail| view::failed(&fail)))
}

/// The token and size from a button's `amount`, typed `text` ("Dez… 0.5"),
/// or both.
fn order_of(
    token: Option<&str>,
    amount: Option<&str>,
    text: Option<&str>,
) -> Result<(Option<String>, Option<Amount>), Fail> {
    let (typed, size) = order(text.unwrap_or("")).map_err(Fail::Invalid)?;
    let token = typed.or(token.map(address).transpose()?);
    let size = match amount {
        Some(a) => Some(parse(a).map_err(Fail::Invalid)?),
        None => size,
    };
    Ok((token, size))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviewArgs {
    /// The pasted Solana mint.
    pub token: String,
}

pub struct Preview;
impl DynAomiTool for Preview {
    type App = DottieApp;
    type Args = PreviewArgs;
    const NAME: &'static str = view::PREVIEW;
    const DESCRIPTION: &'static str =
        "Direct: a pasted mint's identity card, holding and buy/sell buttons.";
    fn descriptor(app: &DottieApp) -> DynToolMetadata {
        direct::<Self>(app)
    }
    fn run(app: &DottieApp, args: PreviewArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        reply(app, &ctx, async |rt, turn| {
            let token = address(&args.token)?;
            let (id, settings, held) = tokio::join!(
                identity(rt, turn, &token),
                settings::load(rt, turn),
                async {
                    let wallet = turn.wallet.as_deref()?;
                    Some(Rpc::new(rt, turn).balance(&token, wallet).await.ok())
                }
            );
            let id = id?;
            let held = held.map(|h| h.map(|raw| crate::domain::exit::units(raw, id.decimals)));
            Ok(view::preview(&id, held, &settings))
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BuyArgs {
    /// Solana mint; null = from text.
    pub token: Option<String>,
    /// "0.5 SOL", "$20" or "5 USDC"; a bare number is SOL. null = from text.
    pub amount: Option<String>,
    /// What the user typed: "/buy" rest ("Dez… 0.5") or a reply ("0.5").
    pub text: Option<String>,
    /// Warnings explicitly accepted by Buy anyway; null = not yet reviewed.
    pub accepted_risks: Option<String>,
    /// Wallet that reviewed those warnings; a changed wallet needs a new review.
    pub reviewed_wallet: Option<String>,
    /// Bounds preserved by Buy anyway. null = the user's saved/default slippage.
    pub slippage_bps: Option<u32>,
}

pub struct Buy;
impl DynAomiTool for Buy {
    type App = DottieApp;
    type Args = BuyArgs;
    const NAME: &'static str = view::BUY;
    const DESCRIPTION: &'static str =
        "Direct: a checked buy with SOL or USDC, returned as a stage_jupiter_swap tx.";
    fn descriptor(app: &DottieApp) -> DynToolMetadata {
        direct::<Self>(app)
    }
    fn run(app: &DottieApp, args: BuyArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        reply(app, &ctx, async |rt, turn| {
            let (token, size) = order_of(
                args.token.as_deref(),
                args.amount.as_deref(),
                args.text.as_deref(),
            )?;
            let Some(token) = token else {
                return Ok(view::text(
                    "Which coin? Send `/buy <mint> <amount>`, e.g. `/buy Dez… 0.5`, or paste its mint.",
                ));
            };
            let (id, settings) =
                tokio::join!(identity(rt, turn, &token), settings::saved(rt, turn));
            let id = id?;
            let mut settings = settings?;
            if let Some(bps) = args.slippage_bps {
                if !(1..=1000).contains(&bps) {
                    return Err(Fail::Invalid("slippage_bps must be 1–1000".into()));
                }
                settings.slippage_bps = Some(bps);
            }
            let Some(size) = size else {
                return Ok(view::how_much(&id, &settings, false));
            };
            let plan = trade::buy(
                rt,
                turn,
                &id,
                &size,
                &settings,
                args.accepted_risks
                    .as_deref()
                    .filter(|_| args.reviewed_wallet == turn.wallet),
            )
            .await?;
            Ok(view::buy(&id, plan, turn.wallet.as_deref()))
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SellArgs {
    /// Solana mint; null = from text.
    pub token: Option<String>,
    /// "25%", "1000 BONK", "$5" or "0.2 SOL" to receive; null = from text.
    pub amount: Option<String>,
    /// What the user typed: "/sell" rest ("Dez… 50%") or a reply ("50%").
    pub text: Option<String>,
    /// Sell even though price impact loses over 10%. null = false.
    pub accept_loss: Option<bool>,
}

pub struct Sell;
impl DynAomiTool for Sell {
    type App = DottieApp;
    type Args = SellArgs;
    const NAME: &'static str = view::SELL;
    const DESCRIPTION: &'static str =
        "Direct: a sell for SOL from a fresh balance, returned as a stage_jupiter_swap tx.";
    fn descriptor(app: &DottieApp) -> DynToolMetadata {
        direct::<Self>(app)
    }
    fn run(app: &DottieApp, args: SellArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        reply(app, &ctx, async |rt, turn| {
            let (token, size) = order_of(
                args.token.as_deref(),
                args.amount.as_deref(),
                args.text.as_deref(),
            )?;
            let Some(token) = token else {
                return Ok(view::text(
                    "Which coin? Send `/sell <mint> <amount>`, e.g. `/sell Dez… 50%`.",
                ));
            };
            let (id, settings) =
                tokio::join!(identity(rt, turn, &token), settings::saved(rt, turn));
            let id = id?;
            let settings = settings?;
            let Some(size) = size else {
                return Ok(view::how_much(&id, &settings, true));
            };
            let accept = args.accept_loss == Some(true);
            let plan = trade::sell(rt, turn, &id, &size, &settings, accept).await?;
            let again = json!({
                "token": token,
                "amount": args.amount,
                "text": args.text,
                "accept_loss": true,
            });
            Ok(view::sell(&id, plan, &settings, again))
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PositionsViewArgs {
    /// "/positions" rest; ignored.
    pub text: Option<String>,
    /// Next page from this tool's Next button; null = first page.
    pub cursor: Option<String>,
}

pub struct PositionsView;
impl DynAomiTool for PositionsView {
    type App = DottieApp;
    type Args = PositionsViewArgs;
    const NAME: &'static str = view::POSITIONS;
    const DESCRIPTION: &'static str =
        "Direct: the connected wallet's holdings, five per page, one button each.";
    fn descriptor(app: &DottieApp) -> DynToolMetadata {
        direct::<Self>(app)
    }
    fn run(app: &DottieApp, args: PositionsViewArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        reply(app, &ctx, async |rt, turn| {
            let wallet = turn.wallet.clone().ok_or(Fail::NoWallet)?;
            let cursor = args.cursor.as_deref().filter(|c| !c.is_empty());
            let page = portfolio::positions(rt, turn, &wallet, None, 0.0, 5, cursor).await?;
            Ok(view::positions(&wallet, &page))
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SettingsViewArgs {
    /// "/settings" rest; ignored.
    pub text: Option<String>,
}

pub struct SettingsView;
impl DynAomiTool for SettingsView {
    type App = DottieApp;
    type Args = SettingsViewArgs;
    const NAME: &'static str = view::SETTINGS;
    const DESCRIPTION: &'static str = "Direct: the user's quick-action settings.";
    fn descriptor(app: &DottieApp) -> DynToolMetadata {
        direct::<Self>(app)
    }
    fn run(app: &DottieApp, _args: SettingsViewArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        reply(app, &ctx, async |rt, turn| {
            let note = match (&turn.account, turn.settings_store()) {
                (None, _) => Some("Sign in to save settings."),
                (_, None) => Some("Settings can't be saved here yet."),
                _ => None,
            };
            Ok(view::settings(&settings::load(rt, turn).await, note))
        })
    }
}
