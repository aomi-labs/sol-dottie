//! Fomo API: real traders, their wallets and positions, and the theses
//! they write about coins. Every call spends credits from a monthly plan,
//! so answers are cached for as long as they stay useful and the gate
//! keeps us under the per-minute cap.
use crate::runtime::{Cost, Fail, Request, Runtime, Source, Turn};
use serde_json::{Value, json};
use std::time::Duration;

const HOST: &str = "https://api.fomoapi.io";
/// Fomo's network id for Solana.
pub const SOLANA: u64 = 1_399_811_149;

const BOARD_TTL: Duration = Duration::from_secs(300);
const HOLDERS_TTL: Duration = Duration::from_secs(300);
const ALERTS_TTL: Duration = Duration::from_secs(60);
const THESES_TTL: Duration = Duration::from_secs(900);
const WARNINGS_TTL: Duration = Duration::from_secs(900);
const TRADER_TTL: Duration = Duration::from_secs(3600);
const OWNER_TTL: Duration = Duration::from_secs(86_400);

/// The trader leaderboard windows Fomo keeps.
pub const WINDOWS: [&str; 3] = ["24h", "7d", "30d"];

#[derive(Clone, Copy)]
pub struct Fomo<'a> {
    rt: &'a Runtime,
    turn: &'a Turn,
}

impl<'a> Fomo<'a> {
    pub fn new(rt: &'a Runtime, turn: &'a Turn) -> Self {
        Self { rt, turn }
    }

    /// Traders ranked by PnL over `window` (24h, 7d or 30d).
    pub async fn leaders(self, window: &str, limit: u32) -> Result<Value, Fail> {
        if !WINDOWS.contains(&window) {
            return Err(Fail::Invalid("window must be 24h, 7d or 30d".into()));
        }
        let path = format!("/v2/leaderboard/{window}?limit={}", limit.clamp(1, 20));
        self.get(path, BOARD_TTL).await
    }

    /// The coins Fomo users trade now (`trending`) or hold most (`most-held`).
    pub async fn tokens(self, board: &str, limit: u32) -> Result<Value, Fail> {
        if !matches!(board, "trending" | "most-held") {
            return Err(Fail::Invalid("board must be trending or most-held".into()));
        }
        let path = format!(
            "/v2/leaderboard/tokens/{board}?limit={}",
            limit.clamp(1, 20)
        );
        self.get(path, BOARD_TTL).await
    }

    /// One page of the most-liked theses on a Solana coin.
    pub async fn theses(self, mint: &str) -> Result<Value, Fail> {
        let path = format!("/v2/thesis/token/{mint}?sort=likes&network=sol&pages=1&limit=12");
        self.get(path, THESES_TTL).await
    }

    /// The Fomo users holding a coin, biggest first, and how many hold any.
    /// The card and the holders view share this one read.
    pub async fn holders(self, mint: &str) -> Result<Value, Fail> {
        let path = format!("/token/{mint}/holders?network={SOLANA}&limit=100");
        self.get(path, HOLDERS_TTL).await
    }

    /// The latest large Solana buys by Fomo users (about $1.4k and up);
    /// 100 rows reach back several hours.
    pub async fn buy_alerts(self) -> Result<Value, Fail> {
        let path = "/v2/alerts?chain=solana&type=buy&limit=100".to_string();
        self.get(path, ALERTS_TTL).await
    }

    /// The warning box Fomo shows before a trade.
    pub async fn warnings(self, mint: &str) -> Result<Value, Fail> {
        let path = format!("/v2/token/{mint}/warnings?networkId={SOLANA}");
        self.get(path, WARNINGS_TTL).await
    }

    /// A trader by handle: verified wallets, PnL and stats (2,500 credits).
    pub async fn trader(self, handle: &str) -> Result<Value, Fail> {
        self.get(format!("/v2/users/{handle}"), TRADER_TTL).await
    }

    /// A trader's positions, one row per coin, every chain (the route has
    /// no chain filter).
    pub async fn positions(self, handle: &str) -> Result<Value, Fail> {
        let path = format!("/v2/users/{handle}/positions?limit=20");
        self.get(path, TRADER_TTL).await
    }

    /// A trader's Solana holdings snapshot. Fomo caps it near 100 rows
    /// with no paging; `truncated` says when it did.
    pub async fn solana_balances(self, handle: &str) -> Result<Value, Fail> {
        let path = format!("/v2/users/{handle}/balances?chain=solana");
        self.get(path, TRADER_TTL).await
    }

    /// Which Fomo user a wallet belongs to. A miss is an answer and is
    /// cached like a hit (it still costs credits).
    pub async fn owner(self, address: &str) -> Result<Value, Fail> {
        self.get(format!("/v2/wallets/owner/{address}"), OWNER_TTL)
            .await
    }

    async fn get(self, path: String, ttl: Duration) -> Result<Value, Fail> {
        let key = self
            .turn
            .key(Source::Fomo)
            .ok_or(Fail::Unconfigured(Source::Fomo))?
            .to_string();
        let owner = path.starts_with("/v2/wallets/owner/");
        let request = Request::new(Source::Fomo, format!("fomo:{path}"), Cost::Fomo, ttl);
        self.rt
            .fetch(self.turn, request, || async {
                let response = self
                    .rt
                    .http
                    .get(format!("{HOST}{path}"))
                    .bearer_auth(&key)
                    .timeout(Duration::from_secs(15))
                    .send()
                    .await
                    .map_err(|_| Fail::unavailable(Source::Fomo, "request failed"))?;
                let status = response.status().as_u16();
                let retry = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok());
                let body: Value = response.json().await.unwrap_or(Value::Null);
                match unwrap_response(status, body, retry) {
                    Err(Fail::NotFound(_)) if owner => Ok(json!({ "owner": null })),
                    other => other,
                }
            })
            .await
    }
}

/// 402 means the month's credits are spent; 429 the per-minute cap. Both
/// are normal answers the model can work around.
fn unwrap_response(status: u16, body: Value, retry_after: Option<u64>) -> Result<Value, Fail> {
    match status {
        200..=299 => Ok(body),
        402 => Err(Fail::Budget("fomo credits are used up for this month")),
        429 => Err(Fail::RateLimited {
            source: Source::Fomo,
            retry_after_s: retry_after.unwrap_or(30).clamp(2, 300),
        }),
        401 | 403 => Err(Fail::Unconfigured(Source::Fomo)),
        404 => Err(Fail::NotFound("fomo has no record of that".into())),
        _ => Err(Fail::unavailable(Source::Fomo, format!("HTTP {status}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credit_and_rate_limits_are_normal_failures() {
        assert_eq!(
            unwrap_response(402, Value::Null, None).unwrap_err().code(),
            "BUDGET"
        );
        assert!(matches!(
            unwrap_response(429, Value::Null, Some(12)),
            Err(Fail::RateLimited {
                retry_after_s: 12,
                ..
            })
        ));
        assert!(matches!(
            unwrap_response(404, Value::Null, None),
            Err(Fail::NotFound(_))
        ));
        assert_eq!(unwrap_response(200, json!({"a": 1}), None).unwrap()["a"], 1);
    }
}
