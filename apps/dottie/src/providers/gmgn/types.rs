//! GMGN response shapes on Solana, only the fields Dottie reads. Rates are 0–1
//! fractions; prices and dollars are USD.
use crate::providers::{Flag, Num};
use serde::Deserialize;

/// `token/info`: one token with flow windows, holder mix and dev status.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct TokenInfo {
    pub address: String,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub decimals: Option<u8>,
    pub holder_count: Num,
    pub liquidity: Num,
    pub total_supply: Num,
    pub creation_timestamp: Num,
    pub open_timestamp: Num,
    pub launchpad_platform: Option<String>,
    pub launchpad_status: Num,
    pub launchpad_progress: Num,
    pub migrated_timestamp: Num,
    pub ath_price: Num,
    pub price: PriceStats,
    pub pool: PoolInfo,
    pub dev: DevInfo,
    pub link: Links,
    pub stat: HolderStat,
    pub wallet_tags_stat: WalletTags,
}

/// Price now and at the start of each window, with buy and sell counts and
/// dollars per window.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PriceStats {
    pub price: Num,
    pub price_5m: Num,
    pub price_1h: Num,
    pub price_6h: Num,
    pub price_24h: Num,
    pub buys_5m: Num,
    pub buys_1h: Num,
    pub buys_6h: Num,
    pub buys_24h: Num,
    pub sells_5m: Num,
    pub sells_1h: Num,
    pub sells_6h: Num,
    pub sells_24h: Num,
    pub volume_24h: Num,
    pub buy_volume_5m: Num,
    pub buy_volume_1h: Num,
    pub buy_volume_6h: Num,
    pub buy_volume_24h: Num,
    pub sell_volume_5m: Num,
    pub sell_volume_1h: Num,
    pub sell_volume_6h: Num,
    pub sell_volume_24h: Num,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PoolInfo {
    pub quote_address: Option<String>,
    /// The DEX of the biggest pool (`pump_amm`, `raydium`, `orca`…).
    pub exchange: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct DevInfo {
    pub creator_address: Option<String>,
    /// `creator_hold` or `creator_close` (sold out).
    pub creator_token_status: Option<String>,
    /// Tokens that point at the same X account (copycats of a known brand).
    pub twitter_create_token_count: Num,
    /// Past names of the linked X account.
    pub twitter_name_change_history: Vec<serde_json::Value>,
    /// 1 after a community takeover.
    pub cto_flag: Num,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Links {
    pub twitter_username: Option<String>,
    pub website: Option<String>,
    pub telegram: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct HolderStat {
    /// Top 10 wallets, pools and lockers excluded.
    pub top_10_holder_rate: Num,
    pub creator_hold_rate: Num,
    pub dev_team_hold_rate: Num,
    pub top70_sniper_hold_rate: Num,
    pub top_bundler_trader_percentage: Num,
    pub top_rat_trader_percentage: Num,
    pub bot_degen_rate: Num,
    pub fresh_wallet_rate: Num,
    /// Share of trading by wallets GMGN flags as bait ("entrapment").
    pub top_entrapment_trader_percentage: Num,
}

/// How many holders carry each wallet tag.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct WalletTags {
    pub smart_wallets: Num,
    pub renowned_wallets: Num,
    pub sniper_wallets: Num,
    pub bundler_wallets: Num,
    pub whale_wallets: Num,
    pub fresh_wallets: Num,
    pub rat_trader_wallets: Num,
}

/// `token/security`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Security {
    pub honeypot: Flag,
    pub buy_tax: Num,
    pub sell_tax: Num,
    pub renounced: Flag,
    /// No one can mint more (SPL mint authority revoked).
    pub renounced_mint: Flag,
    /// No one can freeze holders' accounts.
    pub renounced_freeze_account: Flag,
    pub open_source: Flag,
    pub blacklist: Flag,
    pub lock_summary: LockSummary,
    pub flags: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct LockSummary {
    pub is_locked: Flag,
}

/// A token row as `market/rank`, `trenches` and `token_signal` send it.
/// Each endpoint fills a different subset.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct BoardRow {
    pub address: String,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub price: Num,
    /// Change over the asked interval, in percent (`market/rank`).
    pub price_change_percent: Num,
    /// Volume over the asked interval (`market/rank`).
    pub volume: Num,
    pub volume_1h: Num,
    pub volume_24h: Num,
    pub buys: Num,
    pub sells: Num,
    pub buys_24h: Num,
    pub sells_24h: Num,
    pub net_buy_24h: Num,
    pub liquidity: Num,
    pub market_cap: Num,
    pub usd_market_cap: Num,
    pub history_highest_market_cap: Num,
    pub holder_count: Num,
    pub top_10_holder_rate: Num,
    #[serde(alias = "created_timestamp")]
    pub creation_timestamp: Num,
    pub launchpad_platform: Option<String>,
    pub launchpad_status: Num,
    /// Bonding-curve progress 0–1.
    pub progress: Num,
    #[serde(alias = "quote_address")]
    pub launch_quote_address: Option<String>,
    pub is_wash_trading: Flag,
    pub is_honeypot: Flag,
    pub renounced_mint: Flag,
    pub renounced_freeze_account: Flag,
    pub entrapment_ratio: Num,
    pub rat_trader_amount_rate: Num,
    pub bot_degen_rate: Num,
    pub dev_team_hold_rate: Num,
    pub creator_token_status: Option<String>,
    pub twitter_create_token_count: Num,
    pub twitter_rename_count: Num,
    pub cto_flag: Flag,
    /// Where a graduated coin trades (`pump_amm`, `raydium`…).
    pub migrated_pool_exchange: Option<String>,
    pub rug_ratio: Num,
    pub bundler_rate: Num,
    pub bundler_trader_amount_rate: Num,
    pub smart_degen_count: Num,
    pub renowned_count: Num,
    pub sniper_count: Num,
    pub top70_sniper_hold_rate: Num,
    pub creator: Option<String>,
    pub creator_balance_rate: Num,
    pub creator_created_count: Num,
    pub creator_created_open_count: Num,
}

/// `market/token_signal` event.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Signal {
    pub token_address: String,
    pub signal_type: u32,
    pub trigger_at: Num,
    pub trigger_mc: Num,
    pub signal_times: Num,
    pub data: BoardRow,
}

/// A KOL or smart-money trade from `user/kol` and `user/smartmoney`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Trade {
    pub transaction_hash: String,
    pub maker: String,
    pub base_address: String,
    pub amount_usd: Num,
    pub price_usd: Num,
    pub timestamp: Num,
    pub side: String,
    /// 1 when the trade opened or closed the whole position.
    pub is_open_or_close: Num,
    pub base_token: TradeToken,
    pub maker_info: Maker,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct TradeToken {
    pub symbol: Option<String>,
    pub creator: Option<String>,
    pub total_supply: Num,
    pub launchpad: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Maker {
    pub name: Option<String>,
    pub tags: Vec<String>,
    pub twitter_username: Option<String>,
    pub twitter_name: Option<String>,
}

/// A holder or trader row from `market/token_top_holders` and
/// `market/token_top_traders`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Holder {
    pub address: String,
    /// 2 = a pool.
    pub addr_type: Num,
    pub amount_percentage: Num,
    pub usd_value: Num,
    pub realized_profit: Num,
    pub unrealized_profit: Num,
    pub buy_volume_cur: Num,
    pub sell_volume_cur: Num,
    pub start_holding_at: Num,
    pub tags: Vec<String>,
    pub maker_token_tags: Vec<String>,
    pub name: Option<String>,
    pub twitter_username: Option<String>,
    pub twitter_name: Option<String>,
}

/// `user/wallet_stats` for one period.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct WalletStats {
    pub realized_profit: Num,
    pub realized_profit_pnl: Num,
    pub buy: Num,
    pub sell: Num,
    pub last_timestamp: Num,
    pub pnl_stat: PnlStat,
    pub common: WalletCommon,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PnlStat {
    pub token_num: Num,
    pub winrate: Num,
    /// Tokens closed below −50%.
    pub pnl_lt_nd5_num: Num,
    /// −50% to 0.
    pub pnl_nd5_0x_num: Num,
    /// 0 to 2×.
    pub pnl_0x_2x_num: Num,
    pub pnl_2x_5x_num: Num,
    pub pnl_gt_5x_num: Num,
    /// Seconds.
    pub avg_holding_period: Num,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct WalletCommon {
    pub name: Option<String>,
    pub tags: Vec<String>,
    pub twitter_username: Option<String>,
    pub twitter_name: Option<String>,
    pub created_at: Num,
}

/// `user/wallet_activity` item.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Activity {
    pub timestamp: Num,
    /// buy, sell, transferIn, transferOut, add, remove.
    pub event_type: String,
    pub token: ActivityToken,
    pub tx_hash: String,
    pub token_amount: Num,
    /// Indexed pool-leg value, not the wallet's final routed settlement.
    pub cost_usd: Num,
    /// On a sell: the cost basis of what was sold, buy fees included.
    pub buy_cost_usd: Num,
    pub gas_usd: Num,
    pub dex_usd: Num,
    pub price_usd: Num,
    pub quote_address: Option<String>,
    pub quote_amount: Num,
    /// 1 on the buy that opens a position and the sell that closes it.
    pub is_open_or_close: Num,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ActivityToken {
    pub address: String,
    pub symbol: Option<String>,
}

/// `user/created_tokens`: a creator's launch record.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Created {
    /// Launches that graduated.
    pub open_count: Num,
    pub tokens: Vec<CreatedToken>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CreatedToken {
    pub token_address: String,
    pub symbol: Option<String>,
    pub create_timestamp: Num,
    pub is_open: Flag,
    pub market_cap: Num,
    pub token_ath_mc: Num,
    pub holders: Num,
    pub volume_1h: Num,
    pub launchpad_platform: Option<String>,
}

/// `market/search` coin.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SearchCoin {
    pub chain: String,
    pub address: String,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub created_at: Num,
    pub volume_1h: Num,
    pub mcp: Num,
    pub liquidity: Num,
    pub holder_count: Num,
    pub launchpad_platform: Option<String>,
    pub launchpad_status: Num,
    pub progress: Num,
    pub is_honeypot: Flag,
    pub kol_count: Num,
    pub quote_address: Option<String>,
}

/// `market/token_kline` candle; `time` is in milliseconds.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Candle {
    pub time: Num,
    pub open: Num,
    pub high: Num,
    pub low: Num,
    pub close: Num,
    pub volume: Num,
}
