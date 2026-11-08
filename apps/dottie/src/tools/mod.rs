//! Thin adapters, one file per tool: arguments with their docs, a short
//! "when to call" description, a call into domain, then view. No HTTP and
//! no formatting here; field meanings live in the research skill.
use crate::runtime::{DottieApp, Fail, Lane, Runtime, Turn};
use aomi_sdk::DynToolCallCtx;
use serde_json::Value;

mod card;
mod chart;
mod check;
mod direct;
mod exit;
mod find;
mod holders;
mod partners;
mod people;
mod portfolio;
mod scan;
mod settings;
mod signals;
mod token;
mod trades;
mod wallet;

pub use card::CardTool;
pub use chart::Chart;
pub use check::Check;
pub use direct::{Buy, PositionsView, Preview, Sell, SettingsView};
pub use exit::Exit;
pub use find::Find;
pub use holders::Holders;
pub use partners::{
    About, Assets, FluxbeamPrepare, FluxbeamQuote, SolomonPosition, SolomonPrepare, SolomonProduct,
};
pub use people::People;
pub use portfolio::{History, Pnl, Positions};
pub use scan::Scan;
pub use settings::{SettingsGet, SettingsSet};
pub use signals::Signals;
pub use token::Token;
pub use trades::Trades;
pub use wallet::Wallet;

/// Runs a tool body on the app's runtime and wraps its result in the
/// reply envelope (or an error the model can act on).
pub(crate) fn run<F>(
    app: &DottieApp,
    ctx: &DynToolCallCtx,
    lane: Lane,
    body: F,
) -> Result<Value, String>
where
    F: AsyncFnOnce(&Runtime, &Turn) -> Result<Value, Fail>,
{
    let rt = app.runtime()?;
    let turn = Turn::new(ctx, lane);
    let out = rt.tokio.block_on(body(&rt, &turn));
    Ok(match out {
        Ok(data) => crate::view::reply(data, &turn),
        Err(fail) => crate::view::error(&fail),
    })
}

/// A checked Solana address or mint argument.
pub(crate) fn address(value: &str) -> Result<String, Fail> {
    crate::view::address(value).map_err(Fail::Invalid)
}
