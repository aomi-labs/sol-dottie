//! A wallet's record and recent bag: win rate, PnL and hold time from
//! GMGN, and the coins its latest indexed trades left open.
use super::holders::tags;
use crate::providers::gmgn::{Gmgn, types as gm};
use crate::providers::text;
use crate::runtime::{Fail, Runtime, Turn};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Week,
    Month,
}

impl Period {
    fn gmgn(self) -> &'static str {
        match self {
            Period::Week => "7d",
            Period::Month => "30d",
        }
    }

    pub fn label(self) -> &'static str {
        self.gmgn()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Record {
    pub realized_profit_usd: Option<f64>,
    pub realized_profit_pct: Option<f64>,
    pub win_rate_pct: Option<f64>,
    pub tokens: Option<f64>,
    pub buys: Option<f64>,
    pub sells: Option<f64>,
    pub avg_hold_min: Option<f64>,
    /// Closed tokens by outcome: under −50%, −50–0%, 0–2×, 2–5×, over 5×.
    pub outcomes: [Option<f64>; 5],
    pub last_trade_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Position {
    pub token: String,
    pub symbol: Option<String>,
    /// Indexed buy dollars less sell dollars in the recent trades.
    pub net_usd: f64,
    pub since: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Move {
    pub at: Option<i64>,
    pub side: String,
    pub token: String,
    pub symbol: Option<String>,
    pub usd: Option<f64>,
    pub closed_position: bool,
}

pub struct WalletView {
    pub wallet: String,
    pub name: Option<String>,
    pub twitter: Option<String>,
    pub tags: Vec<&'static str>,
    pub period: Period,
    pub record: Option<Record>,
    pub bag: Vec<Position>,
    pub recent: Vec<Move>,
}

pub async fn wallet(
    rt: &Runtime,
    turn: &Turn,
    wallet: &str,
    period: Period,
    with_trades: bool,
) -> Result<WalletView, Fail> {
    let gmgn = Gmgn::new(rt, turn);
    let (stats, activity) = tokio::join!(
        gmgn.wallet_stats(wallet, period.gmgn()),
        gmgn.wallet_activity(wallet)
    );
    if let (Err(fail), Err(_)) = (&stats, &activity) {
        return Err(fail.clone());
    }
    let stats = match stats {
        Ok(s) => Some(s),
        Err(fail) => {
            turn.gap(format!("wallet record unavailable: {}", fail.message()));
            None
        }
    };
    let activity = activity.unwrap_or_else(|fail| {
        turn.gap(format!("recent trades unavailable: {}", fail.message()));
        vec![]
    });
    let bag = open_positions(&activity);
    let recent = if with_trades {
        activity.into_iter().take(10).map(move_of).collect()
    } else {
        vec![]
    };
    let common = stats.as_ref().map(|s| &s.common);
    Ok(WalletView {
        wallet: wallet.to_string(),
        name: common.and_then(|c| text(&c.twitter_name).or(text(&c.name)).map(str::to_string)),
        twitter: common.and_then(|c| text(&c.twitter_username).map(str::to_string)),
        tags: common.map(|c| tags(&[&c.tags])).unwrap_or_default(),
        period,
        record: stats.as_ref().map(record),
        bag,
        recent,
    })
}

fn record(s: &gm::WalletStats) -> Record {
    let p = &s.pnl_stat;
    Record {
        realized_profit_usd: s.realized_profit.get(),
        realized_profit_pct: s.realized_profit_pnl.pct(),
        win_rate_pct: p.winrate.pct(),
        tokens: p.token_num.get(),
        buys: s.buy.get(),
        sells: s.sell.get(),
        avg_hold_min: p.avg_holding_period.get().map(|s| s / 60.0),
        outcomes: [
            p.pnl_lt_nd5_num.get(),
            p.pnl_nd5_0x_num.get(),
            p.pnl_0x_2x_num.get(),
            p.pnl_2x_5x_num.get(),
            p.pnl_gt_5x_num.get(),
        ],
        last_trade_at: s.last_timestamp.ts(),
    }
}

/// Coins whose latest trade in the recent page didn't close the
/// position, by net indexed dollars in.
fn open_positions(activity: &[gm::Activity]) -> Vec<Position> {
    let mut out: Vec<(Position, bool)> = vec![];
    // Newest first: the first row per coin says whether it's still open.
    for a in activity
        .iter()
        .filter(|a| matches!(a.event_type.as_str(), "buy" | "sell"))
    {
        let usd = a.cost_usd.get().unwrap_or(0.0);
        let signed = if a.event_type == "buy" { usd } else { -usd };
        match out.iter_mut().find(|(p, _)| p.token == a.token.address) {
            Some((p, _)) => {
                p.net_usd += signed;
                p.since = a.timestamp.ts().or(p.since);
            }
            None => {
                let closed = a.event_type == "sell" && a.is_open_or_close.get() == Some(1.0);
                out.push((
                    Position {
                        token: a.token.address.trim().to_string(),
                        symbol: a.token.symbol.clone(),
                        net_usd: signed,
                        since: a.timestamp.ts(),
                    },
                    closed,
                ));
            }
        }
    }
    let mut open: Vec<Position> = out
        .into_iter()
        .filter(|(p, closed)| !closed && p.net_usd >= 1.0)
        .map(|(p, _)| p)
        .collect();
    open.sort_by(|a, b| b.net_usd.total_cmp(&a.net_usd));
    open.truncate(8);
    open
}

fn move_of(a: gm::Activity) -> Move {
    let closed_position = a.event_type == "sell" && a.is_open_or_close.get() == Some(1.0);
    Move {
        at: a.timestamp.ts(),
        side: a.event_type,
        token: a.token.address.to_string(),
        symbol: a.token.symbol,
        usd: a.cost_usd.get(),
        closed_position,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gmgn_record_reads_win_rate_and_outcomes() {
        let text = std::fs::read_to_string(format!(
            "{}/src/fixtures/gmgn/wallet_stats.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let stats: gm::WalletStats = serde_json::from_value(v["data"].clone()).unwrap();
        let r = record(&stats);
        assert!(r.win_rate_pct.is_some_and(|w| (0.0..=100.0).contains(&w)));
        assert!(r.outcomes.iter().any(Option::is_some));
        assert!(r.realized_profit_usd.is_some());
    }

    #[test]
    fn recent_activity_leaves_open_positions() {
        let text = std::fs::read_to_string(format!(
            "{}/src/fixtures/gmgn/wallet_activity.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let rows: Vec<gm::Activity> =
            serde_json::from_value(v["data"]["activities"].clone()).unwrap();
        let open = open_positions(&rows);
        assert!(open.iter().all(|p| p.net_usd >= 1.0 && p.token.len() >= 32));
    }
}
