//! The only place model-facing JSON is built. One field vocabulary for
//! every tool, flat and with the window and unit in the name:
//! `change_1h_pct`, `buy_1h_usd`, `buys_1h` (trade count), `kol_wallets`
//! (holders tagged KOL), `top10_pct` (top 10 wallets; pools, curves and
//! lockers excluded). `null` (left out) means unknown, never zero.
pub mod direct;
mod envelope;
mod format;
pub mod market;
pub mod people;
pub mod token;

pub use envelope::{MAX_REPLY, compact, error, fit, reply, size};
pub use format::*;
