//! Real traders from Fomo: who leads by PnL, what a trader holds, which
//! trader owns a wallet, what they buy big, and what holders write about a
//! coin. Credits are scarce, so each mode makes as few calls as it can and
//! caches long.
use crate::domain::market::is_base_asset;
use crate::providers::fomo::Fomo;
use crate::providers::{num, pubkey};
use crate::runtime::{Fail, Runtime, Turn};
use crate::view::people as view;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Leaders,
    Trader,
    Theses,
    Owner,
    Trending,
}

/// A Fomo handle: letters, digits, `_`, `.` and `-`, an optional `@`.
pub fn handle(raw: &str) -> Result<String, Fail> {
    let h = raw.trim().trim_start_matches('@');
    if (1..=32).contains(&h.len())
        && h.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    {
        Ok(h.to_string())
    } else {
        Err(Fail::Invalid(
            "handle must be a Fomo username like @Binkieee".into(),
        ))
    }
}

pub struct Query<'a> {
    pub mode: Mode,
    /// leaders: 24h, 7d or 30d.
    pub window: &'a str,
    /// trending: trending, most-held or flow.
    pub board: &'a str,
    /// theses: size (biggest holders) or likes.
    pub sort: &'a str,
    pub handle: Option<&'a str>,
    pub token: Option<&'a str>,
    pub wallet: Option<&'a str>,
    pub limit: u32,
}

pub async fn people(rt: &Runtime, turn: &Turn, q: &Query<'_>) -> Result<Value, Fail> {
    let fomo = Fomo::new(rt, turn);
    fn need<'a>(v: Option<&'a str>, what: &str) -> Result<&'a str, Fail> {
        v.ok_or_else(|| Fail::Invalid(format!("this mode needs {what}")))
    }
    Ok(match q.mode {
        Mode::Leaders => view::fomo_leaders(&fomo.leaders(q.window, q.limit).await?),
        Mode::Trending if q.board == "flow" => {
            let alerts = fomo.buy_alerts().await?;
            view::fomo_flow(&flow(&alerts, crate::view::now(), q.limit as usize))
        }
        Mode::Trending => view::fomo_tokens(&fomo.tokens(q.board, q.limit).await?),
        Mode::Theses => {
            let mint = pubkey(need(q.token, "token")?)?;
            if q.sort == "likes" {
                view::fomo_theses(mint, &fomo.theses(mint).await?)
            } else {
                view::fomo_sized_theses(mint, &fomo.holders(mint).await?)
            }
        }
        Mode::Owner => {
            let wallet = pubkey(need(q.wallet, "wallet")?)?;
            view::fomo_owner(wallet, &fomo.owner(wallet).await?)
        }
        Mode::Trader => {
            let handle = handle(need(q.handle, "handle")?)?;
            let user = fomo.trader(&handle).await?;
            // A trader still resolving has no verified wallet yet: skip the
            // positions read rather than spend on it.
            let positions = if user["wallets"]["status"] == "resolving" {
                None
            } else {
                match fomo.positions(&handle).await {
                    Ok(p) => Some(p),
                    Err(fail) => {
                        turn.gap(format!("positions unavailable: {}", fail.message()));
                        None
                    }
                }
            };
            // The positions page has no chain filter; when it holds no
            // Solana rows, one holdings read narrowed to Solana fills in.
            let holdings = match &positions {
                Some(p) if view::solana_positions(p) > 0 => None,
                None if user["wallets"]["status"] == "resolving" => None,
                _ => match fomo.solana_balances(&handle).await {
                    Ok(b) => Some(b),
                    Err(fail) => {
                        turn.gap(format!("solana holdings unavailable: {}", fail.message()));
                        None
                    }
                },
            };
            view::fomo_trader(&user, positions.as_ref(), holdings.as_ref())
        }
    })
}

/// One Fomo user's large buys of a coin.
pub struct FlowBuyer {
    pub handle: Option<String>,
    pub usd: f64,
    pub min_ago: i64,
}

/// A coin in Fomo's large-buy feed. Buyers are distinct users, biggest first.
pub struct FlowCoin {
    pub token: String,
    pub symbol: Option<String>,
    pub buyers: Vec<FlowBuyer>,
    /// 3+ distinct buyers within the last hour.
    pub strong: bool,
}

impl FlowCoin {
    pub fn buy_usd(&self) -> f64 {
        self.buyers.iter().map(|b| b.usd).sum()
    }

    pub fn last_min_ago(&self) -> i64 {
        self.buyers.iter().map(|b| b.min_ago).min().unwrap_or(0)
    }
}

pub struct Flow {
    pub coins: Vec<FlowCoin>,
    /// How far back the page reaches: it holds the last 100 alerts.
    pub covers_min: Option<i64>,
}

/// Fomo's large-buy alerts grouped per coin, ranked by distinct buyers then
/// dollars. Alert timestamps repeat, so rows dedupe on `id`.
pub fn flow(raw: &Value, now: i64, limit: usize) -> Flow {
    let mut seen = HashSet::new();
    let alerts: Vec<&Value> = raw["alerts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| a["id"].is_null() || seen.insert(a["id"].to_string()))
        .collect();
    let min_ago = |a: &Value| a["ts"].as_i64().map(|ts| (now - ts / 1000).max(0) / 60);
    let covers_min = alerts.iter().filter_map(|a| min_ago(a)).max();
    let mut coins: HashMap<&str, (Option<&str>, HashMap<&str, FlowBuyer>)> = HashMap::new();
    for a in alerts {
        let (Some(token), Some(ago)) = (a["tokenAddress"].as_str(), min_ago(a)) else {
            continue;
        };
        if a["type"] != "buy" || a["chain"] != "solana" || is_base_asset(token) {
            continue;
        }
        let handle = a["trader"].as_str();
        let Some(user) = a["userId"].as_str().or(handle) else {
            continue;
        };
        let (_, buyers) = coins
            .entry(token)
            .or_insert_with(|| (a["token"].as_str(), HashMap::new()));
        let buyer = buyers.entry(user).or_insert(FlowBuyer {
            handle: handle.map(str::to_string),
            usd: 0.0,
            min_ago: ago,
        });
        buyer.usd += num(&a["usdValue"]).unwrap_or(0.0);
        buyer.min_ago = buyer.min_ago.min(ago);
    }
    let mut coins: Vec<FlowCoin> = coins
        .into_iter()
        .map(|(token, (symbol, buyers))| {
            let mut buyers: Vec<FlowBuyer> = buyers.into_values().collect();
            buyers.sort_by(|a, b| b.usd.total_cmp(&a.usd));
            FlowCoin {
                token: token.to_string(),
                symbol: symbol.map(str::to_string),
                strong: buyers.iter().filter(|b| b.min_ago <= 60).count() >= 3,
                buyers,
            }
        })
        .collect();
    coins.sort_by(|a, b| {
        b.buyers
            .len()
            .cmp(&a.buyers.len())
            .then(b.buy_usd().total_cmp(&a.buy_usd()))
    });
    coins.truncate(limit);
    Flow { coins, covers_min }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let text = std::fs::read_to_string(format!(
            "{}/src/fixtures/fomo/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn handles_are_checked() {
        assert_eq!(handle("@Binkieee").unwrap(), "Binkieee");
        assert!(handle("../v2/me").is_err());
        assert!(handle("").is_err());
    }

    #[test]
    fn fomo_replies_keep_solana_rows_and_trim_theses() {
        let leaders = crate::view::compact(view::fomo_leaders(&fixture("leaders_24h")));
        let first = &leaders["traders"][0];
        assert_eq!(first["handle"], "Binkieee");
        assert!(
            first["top_solana_tokens"]
                .as_array()
                .unwrap()
                .iter()
                .all(|t| pubkey(t["token"].as_str().unwrap()).is_ok())
        );
        // Binkieee's first positions page is all BSC: the Solana holdings
        // read fills in, and without it the reply says it's a summary.
        let (user, positions) = (fixture("trader_binkieee"), fixture("positions_binkieee"));
        assert_eq!(view::solana_positions(&positions), 0);
        let holdings = fixture("balances_binkieee_solana");
        let trader =
            crate::view::compact(view::fomo_trader(&user, Some(&positions), Some(&holdings)));
        assert!(trader["solana_wallet"].is_string());
        assert_eq!(trader["coverage"], "solana_holdings");
        assert_eq!(trader["solana_rows"], 5);
        assert_eq!(trader["holdings_truncated"], true);
        let first = &trader["solana_holdings"][0];
        assert_eq!(first["symbol"], "FLOAT");
        assert!(pubkey(first["token"].as_str().unwrap()).is_ok());
        let summary = crate::view::compact(view::fomo_trader(&user, Some(&positions), None));
        assert_eq!(summary["coverage"], "top_token_summary");
        assert_eq!(summary["solana_rows"], 0);
        let theses = view::fomo_theses("m", &fixture("theses_float"));
        let list = theses["theses"].as_array().unwrap();
        assert!(!list.is_empty() && list.len() <= 8);
        assert!(
            list.iter()
                .all(|t| t["text"].as_str().unwrap().chars().count() <= 280)
        );
        let owner = view::fomo_owner("w", &fixture("owner_hit"));
        assert_eq!(
            (owner["found"].as_bool(), owner["handle"].as_str()),
            (Some(true), Some("bystevenr"))
        );
        let miss = view::fomo_owner("w", &serde_json::json!({"owner": null}));
        assert_eq!(miss["found"], false);
        let tokens = crate::view::compact(view::fomo_tokens(&fixture("tokens_trending")));
        assert!(!tokens["tokens"].as_array().unwrap().is_empty());
        let warnings = crate::view::compact(view::fomo_warnings(&fixture("warnings_bonk")));
        assert_eq!(warnings["allowlisted"], true);
    }

    #[test]
    fn flow_groups_large_buys_per_coin() {
        let mut raw = fixture("alerts_solana");
        let newest = raw["newestTs"].as_i64().unwrap() / 1000;
        // A base asset and another chain's buy never count.
        let extra = |token: &str, chain: &str| {
            serde_json::json!({"id": token, "ts": newest * 1000, "type": "buy", "chain": chain,
                "token": "X", "tokenAddress": token, "trader": "t", "userId": "u", "usdValue": 9e6})
        };
        let alerts = raw["alerts"].as_array_mut().unwrap();
        alerts.push(extra(
            "So11111111111111111111111111111111111111112",
            "solana",
        ));
        alerts.push(extra("EVM", "base"));
        let f = flow(&raw, newest + 60, 10);
        assert_eq!(f.covers_min, Some(61));
        let symbols: Vec<_> = f
            .coins
            .iter()
            .map(|c| c.symbol.as_deref().unwrap())
            .collect();
        assert_eq!(symbols[..4], ["PUSSY", "FLOAT", "STONK", "CHOO"]);
        assert_eq!(f.coins.len(), 7);
        // PUSSY: one alert is re-delivered with the same id.
        let pussy = &f.coins[0];
        assert_eq!(pussy.buyers.len(), 9);
        assert_eq!(pussy.buy_usd().round(), 272_937.0);
        assert_eq!(pussy.buyers[0].handle.as_deref(), Some("bystevenr"));
        assert_eq!((pussy.buyers[0].min_ago, pussy.last_min_ago()), (6, 4));
        assert!(pussy.strong && f.coins[3].strong && !f.coins[4].strong);
        let later = flow(&raw, newest + 3 * 3600, 2);
        assert!(later.coins.len() == 2 && later.coins.iter().all(|c| !c.strong));
        let out = crate::view::compact(view::fomo_flow(&f));
        assert_eq!(
            out["coins"][0]["buyers_named"][0],
            "@bystevenr $191k 6m ago"
        );
        assert_eq!(
            out["coins"][0]["fomo"],
            format!("https://fomo.family/tokens/solana/{}", pussy.token)
        );
        assert!(flow(&serde_json::json!({}), newest, 5).coins.is_empty());
    }

    #[test]
    fn fomo_holders_are_sized_and_named() {
        let raw = fixture("holders_stonk");
        let out = crate::view::compact(view::fomo_holders("m", &raw));
        assert_eq!(
            (&out["fomo_traders_holding"], &out["holders_over_1k"]),
            (&serde_json::json!(42178), &serde_json::json!(10))
        );
        let rows = out["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 8);
        assert!(
            rows.windows(2)
                .all(|w| w[0]["value_usd"].as_f64() >= w[1]["value_usd"].as_f64())
        );
        assert_eq!(
            rows[0]["profile"],
            "https://fomo.family/profile/pointfarmcap"
        );
        assert!(rows[0]["wallet"].is_string());
        assert!(rows.iter().all(|r| {
            r["thesis"]
                .as_str()
                .is_none_or(|t| t.chars().count() <= 200)
        }));
        assert!(out.get("stale").is_none() && out.get("available").is_none());
        let theses = crate::view::compact(view::fomo_sized_theses("m", &raw));
        let list = theses["theses"].as_array().unwrap();
        // monke365 (4th biggest) wrote nothing.
        assert_eq!(list.len(), 8);
        assert!(
            list.iter()
                .all(|t| t["handle"] != "monke365" && t["text"].is_string())
        );
        assert_eq!(list[3]["handle"], "TheOnlyOMP");
        // A fallback reply has no total: unknown, not zero.
        let fallback =
            serde_json::json!({"scope": "tracked-traders", "available": false, "holders": []});
        let out = crate::view::compact(view::fomo_holders("m", &fallback));
        assert!(out.get("fomo_traders_holding").is_none());
        assert_eq!(out["available"], false);
    }
}
