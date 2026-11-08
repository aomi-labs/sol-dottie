//! Dottie's knowledge as typed structs and rules: what a coin is, where it
//! is in its life, who is buying, who holds it, what traders say, and
//! whether a size gets back out. Providers supply raw reads; view turns
//! these into replies.
pub mod amount;
pub mod boards;
pub mod chart;
pub mod exit;
pub mod find;
pub mod holders;
pub mod market;
pub mod people;
pub mod pnl;
pub mod portfolio;
pub mod settings;
pub mod signals;
pub mod token;
pub mod trade;
pub mod trades;
pub mod wallet;
