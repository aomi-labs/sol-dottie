//! GMGN OpenAPI: discovery boards, KOL and smart-money feeds, wallet tags,
//! security, labelled holders and wallet records. Read-only routes with
//! the API key (`X-APIKEY` plus a timestamp and a fresh client id).
//! Each route carries its rate-limit weight; the runtime's gate paces them.
pub mod types;

use crate::runtime::{Cost, Fail, Request, Runtime, Source, Turn};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use types::*;

const HOST: &str = "https://openapi.gmgn.ai";
pub const CHAIN: &str = "sol";

/// One GMGN route: its weight in the plan's leaky bucket and how long its
/// answer stays fresh.
struct Route {
    path: &'static str,
    weight: u32,
    ttl: Duration,
}

const fn route(path: &'static str, weight: u32, ttl_s: u64) -> Route {
    Route {
        path,
        weight,
        ttl: Duration::from_secs(ttl_s),
    }
}

const TOKEN_INFO: Route = route("/v1/token/info", 1, 30);
const SECURITY: Route = route("/v1/token/security", 1, 600);
const RANK: Route = route("/v1/market/rank", 3, 30);
// Documented as 2; a 3 + 2 burst was refused, so it is counted as 3.
const TRENCHES: Route = route("/v1/trenches", 3, 30);
const SIGNALS: Route = route("/v1/market/token_signal", 3, 60);
const KOL: Route = route("/v1/user/kol", 1, 30);
const SMART_MONEY: Route = route("/v1/user/smartmoney", 1, 30);
const TOP_HOLDERS: Route = route("/v1/market/token_top_holders", 5, 120);
const TOP_TRADERS: Route = route("/v1/market/token_top_traders", 5, 120);
const WALLET_STATS: Route = route("/v1/user/wallet_stats", 3, 300);
const WALLET_ACTIVITY: Route = route("/v1/user/wallet_activity", 3, 60);
/// One coin's trades by one wallet; always live, a card follows a fresh trade.
const TOKEN_ACTIVITY: Route = route("/v1/user/wallet_activity", 3, 0);
const CREATED: Route = route("/v1/user/created_tokens", 2, 600);
const SEARCH: Route = route("/v1/market/search", 1, 180);
const KLINE: Route = route("/v1/market/token_kline", 2, 30);

/// Launchpad sections of the trenches board.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Trench {
    New,
    NearCompletion,
    Completed,
}

impl Trench {
    fn key(self) -> &'static str {
        match self {
            Trench::New => "new_creation",
            Trench::NearCompletion => "near_completion",
            Trench::Completed => "completed",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Gmgn<'a> {
    rt: &'a Runtime,
    turn: &'a Turn,
    live: bool,
}

impl<'a> Gmgn<'a> {
    pub fn new(rt: &'a Runtime, turn: &'a Turn) -> Self {
        Self {
            rt,
            turn,
            live: false,
        }
    }

    /// Execution facts never reuse the longer research cache or stale fallback.
    pub fn live(mut self) -> Self {
        self.live = true;
        self
    }

    pub async fn token_info(self, token: &str) -> Result<TokenInfo, Fail> {
        self.get(&TOKEN_INFO, vec![("address", token.into())]).await
    }

    pub async fn security(self, token: &str) -> Result<Security, Fail> {
        self.get(&SECURITY, vec![("address", token.into())]).await
    }

    /// Trending by `interval` (1m, 5m, 1h, 6h, 24h) with server-side
    /// filters and sort, as GMGN's `min_*`/`max_*` query parameters.
    pub async fn rank(
        &self,
        interval: &str,
        params: Vec<(&'static str, String)>,
    ) -> Result<Vec<BoardRow>, Fail> {
        #[derive(serde::Deserialize, Default)]
        #[serde(default)]
        struct Outer {
            data: Inner,
        }
        #[derive(serde::Deserialize, Default)]
        #[serde(default)]
        struct Inner {
            rank: Vec<BoardRow>,
        }
        let mut query = vec![("interval", interval.to_string())];
        query.extend(params);
        let outer: Outer = self.get(&RANK, query).await?;
        Ok(outer.data.rank)
    }

    /// One trenches section (newly created, near graduation, graduated),
    /// with server-side range filters such as `min_volume_24h`.
    pub async fn trenches(
        &self,
        section: Trench,
        limit: u32,
        filters: Value,
    ) -> Result<Vec<BoardRow>, Fail> {
        let mut spec = json!({
            "filters": ["offchain", "onchain"],
            "launchpad_platform_v2": true,
            "limit": limit,
        });
        if let (Some(spec), Value::Object(extra)) = (spec.as_object_mut(), filters) {
            spec.extend(extra);
        }
        let body = json!({ "version": "v2", section.key(): spec });
        let mut sections: std::collections::HashMap<String, Vec<BoardRow>> =
            self.post(&TRENCHES, vec![], body).await?;
        Ok(sections.remove(section.key()).unwrap_or_default())
    }

    /// Signal events, newest first. `groups` is GMGN's list of
    /// `{signal_type: [..], mc_min, mc_max}` objects.
    pub async fn signals(self, groups: Value) -> Result<Vec<Signal>, Fail> {
        self.post(
            &SIGNALS,
            vec![],
            json!({ "chain": CHAIN, "groups": groups }),
        )
        .await
    }

    /// The last 100 KOL trades on Solana.
    pub async fn kol_trades(self) -> Result<Vec<Trade>, Fail> {
        self.list(&KOL, vec![("limit", "100".into())], "list").await
    }

    /// The last 100 smart-money trades on Solana.
    pub async fn smart_trades(self) -> Result<Vec<Trade>, Fail> {
        self.list(&SMART_MONEY, vec![("limit", "100".into())], "list")
            .await
    }

    /// Top holders with tags, names and PnL; `tag` (e.g. `smart_degen`,
    /// `renowned`) keeps only wallets carrying it.
    pub async fn top_holders(self, token: &str, tag: Option<&str>) -> Result<Vec<Holder>, Fail> {
        let mut query = vec![("address", token.to_string()), ("limit", "20".into())];
        if let Some(tag) = tag {
            query.push(("tag", tag.to_string()));
        }
        self.list(&TOP_HOLDERS, query, "list").await
    }

    /// The most profitable traders of a token.
    pub async fn top_traders(self, token: &str) -> Result<Vec<Holder>, Fail> {
        let query = vec![("address", token.to_string()), ("limit", "20".into())];
        self.list(&TOP_TRADERS, query, "list").await
    }

    /// A wallet's record over `period` (7d or 30d).
    pub async fn wallet_stats(self, wallet: &str, period: &str) -> Result<WalletStats, Fail> {
        let query = vec![("wallet_address", wallet.into()), ("period", period.into())];
        self.get(&WALLET_STATS, query).await
    }

    pub async fn wallet_activity(self, wallet: &str) -> Result<Vec<Activity>, Fail> {
        let query = vec![("wallet_address", wallet.into()), ("limit", "20".into())];
        self.list(&WALLET_ACTIVITY, query, "activities").await
    }

    /// One page (newest first) of a wallet's trades in one coin, and the
    /// cursor of the next page.
    pub async fn token_activity(
        self,
        wallet: &str,
        token: &str,
        cursor: Option<&str>,
    ) -> Result<(Vec<Activity>, Option<String>), Fail> {
        let mut query = vec![
            ("wallet_address", wallet.to_string()),
            ("token_address", token.to_string()),
            ("limit", "50".into()),
        ];
        if let Some(c) = cursor {
            query.push(("cursor", c.to_string()));
        }
        let mut data: Value = self.get(&TOKEN_ACTIVITY, query).await?;
        let next = data
            .get("next")
            .and_then(Value::as_str)
            .filter(|c| !c.is_empty())
            .map(str::to_string);
        let items = data
            .get_mut("activities")
            .map(Value::take)
            .unwrap_or(Value::Null);
        Ok((
            if items.is_null() {
                vec![]
            } else {
                parse(items)?
            },
            next,
        ))
    }

    /// Paginated wallet history for structured account tools; no stale fallback.
    pub async fn activity_page(
        self,
        wallet: &str,
        token: Option<&str>,
        cursor: Option<&str>,
    ) -> Result<Value, Fail> {
        let mut query = vec![("wallet_address", wallet.to_string())];
        if let Some(token) = token {
            query.push(("token_address", token.to_string()));
        }
        query.push(("limit", "50".into()));
        if let Some(cursor) = cursor {
            query.push(("cursor", cursor.to_string()));
        }
        self.get(&TOKEN_ACTIVITY, query).await
    }

    pub async fn created_tokens(self, wallet: &str) -> Result<Created, Fail> {
        let query = vec![
            ("wallet_address", wallet.into()),
            ("order_by", "token_ath_mc".into()),
            ("direction", "desc".into()),
        ];
        self.get(&CREATED, query).await
    }

    /// Ticker, name or mint search on Solana.
    pub async fn search(self, q: &str) -> Result<Vec<SearchCoin>, Fail> {
        let coins: Vec<SearchCoin> = self.list(&SEARCH, vec![("q", q.into())], "coins").await?;
        Ok(coins.into_iter().filter(|c| c.chain == CHAIN).collect())
    }

    /// Candles; `from`/`to` are unix seconds, `resolution` e.g. 15m.
    pub async fn kline(
        &self,
        token: &str,
        resolution: &str,
        from: i64,
        to: i64,
    ) -> Result<Vec<Candle>, Fail> {
        let query = vec![
            ("address", token.to_string()),
            ("resolution", resolution.to_string()),
            ("from", (from * 1000).to_string()),
            ("to", (to * 1000).to_string()),
        ];
        self.list(&KLINE, query, "list").await
    }

    async fn list<T: DeserializeOwned>(
        &self,
        route: &Route,
        query: Vec<(&'static str, String)>,
        field: &str,
    ) -> Result<Vec<T>, Fail> {
        let mut data: Value = self.get(route, query).await?;
        let items = data.get_mut(field).map(Value::take).unwrap_or(Value::Null);
        if items.is_null() {
            return Ok(vec![]);
        }
        parse(items)
    }

    async fn get<T: DeserializeOwned>(
        &self,
        route: &Route,
        query: Vec<(&'static str, String)>,
    ) -> Result<T, Fail> {
        parse(self.send(route, query, None).await?)
    }

    async fn post<T: DeserializeOwned>(
        &self,
        route: &Route,
        query: Vec<(&'static str, String)>,
        body: Value,
    ) -> Result<T, Fail> {
        parse(self.send(route, query, Some(body)).await?)
    }

    async fn send(
        &self,
        route: &Route,
        mut query: Vec<(&'static str, String)>,
        body: Option<Value>,
    ) -> Result<Value, Fail> {
        let key = self
            .turn
            .key(Source::Gmgn)
            .ok_or(Fail::Unconfigured(Source::Gmgn))?
            .to_string();
        if body.is_none() || route.path == TRENCHES.path {
            query.insert(0, ("chain", CHAIN.into()));
        }
        let params: Vec<String> = query.iter().map(|(k, v)| format!("{k}={v}")).collect();
        let cache_key = format!(
            "{}:{}?{}{}",
            if self.live { "gmgn-live" } else { "gmgn" },
            route.path,
            params.join("&"),
            body.as_ref().map(|b| format!(" {b}")).unwrap_or_default()
        );
        let mut request = Request::new(
            Source::Gmgn,
            cache_key,
            Cost::Gmgn(route.weight),
            if self.live {
                Duration::from_secs(20)
            } else {
                route.ttl
            },
        );
        if self.live || route.ttl.is_zero() {
            request = request.live_only();
        }
        let (cost, lane) = (request.cost, self.turn.lane);
        // One send: its HTTP status (0 when none came back) and result.
        let attempt = || async {
            let mut query = query.clone();
            query.push(("timestamp", crate::view::now().to_string()));
            query.push(("client_id", client_id()));
            let url = format!("{HOST}{}", route.path);
            let http = &self.rt.http_v4;
            let builder = match &body {
                Some(b) => http.post(url).json(b),
                None => http.get(url),
            };
            let Ok(response) = builder
                .query(&query)
                .header("X-APIKEY", &key)
                .timeout(Duration::from_secs(15))
                .send()
                .await
            else {
                return (0, Err(Fail::unavailable(Source::Gmgn, "request failed")));
            };
            let status = response.status().as_u16();
            let reset_header = response
                .headers()
                .get("x-ratelimit-reset")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<i64>().ok());
            let Ok(text) = response.text().await else {
                return (
                    status,
                    Err(Fail::unavailable(Source::Gmgn, "response unreadable")),
                );
            };
            trace(route, status, &text);
            (status, unwrap_response(status, &text, reset_header))
        };
        self.rt
            .fetch(self.turn, request, || async move {
                // A plain 429 (not a ban) pauses the gate briefly and goes
                // once more, queued behind the pause.
                match attempt().await {
                    // A ban carries a long reset; only a short one is a plain 429.
                    (429, Err(Fail::RateLimited { retry_after_s, .. })) if retry_after_s <= 3 => {
                        tokio::time::sleep(self.rt.gates.after_429(cost, lane)?).await;
                        attempt().await.1
                    }
                    (_, result) => result,
                }
            })
            .await
    }
}

/// GMGN wraps every answer as `{code, data, message, error, reset_at}`.
/// With `DOTTIE_TRACE=<file>`, one line per GMGN request (time, route,
/// weight, status, and the body of any error) for checking the pacing.
fn trace(route: &Route, status: u16, text: &str) {
    let Some(path) = std::env::var_os("DOTTIE_TRACE") else {
        return;
    };
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let detail: String = if status == 200 {
        String::new()
    } else {
        text.chars().take(200).collect()
    };
    let line = format!("{ms} {} w{} {status} {detail}\n", route.path, route.weight);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = std::io::Write::write_all(&mut file, line.as_bytes());
    }
}

/// A 429 (or a 403, which is how a ban can look) pauses GMGN whatever the
/// body is; everything else is `{code, data, message, error, reset_at}`.
fn unwrap_response(status: u16, text: &str, reset_header: Option<i64>) -> Result<Value, Fail> {
    let body: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let error = body.get("error").and_then(Value::as_str).unwrap_or("");
    let limited = status == 429
        || status == 403
        || error.starts_with("RATE_LIMIT")
        || error == "ERROR_RATE_LIMIT_BLOCKED";
    if limited {
        let banned = error.contains("BAN") || error.contains("BLOCKED");
        let reset = body
            .get("reset_at")
            .and_then(Value::as_i64)
            .or(reset_header)
            .unwrap_or(crate::view::now() + 60);
        return Err(Fail::RateLimited {
            source: Source::Gmgn,
            // A ban keeps its own reset but never reads as a short (retryable) 429.
            retry_after_s: (reset - crate::view::now() + 1).max(if banned { 4 } else { 2 }) as u64,
        });
    }
    if body.get("code").and_then(Value::as_i64) != Some(0) {
        let message = body
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("error")
            .chars()
            .take(120)
            .collect::<String>();
        if status == 401 {
            return Err(Fail::Unconfigured(Source::Gmgn));
        }
        if message.to_ascii_lowercase().contains("not found") || status == 404 {
            return Err(Fail::NotFound(
                "gmgn has no Solana data for this mint".into(),
            ));
        }
        return Err(Fail::bad(Source::Gmgn, format!("HTTP {status}: {message}")));
    }
    if body.is_null() {
        return Err(Fail::bad(Source::Gmgn, format!("HTTP {status}, not JSON")));
    }
    Ok(body.get("data").cloned().unwrap_or(Value::Null))
}

fn parse<T: DeserializeOwned>(value: Value) -> Result<T, Fail> {
    crate::providers::parse(Source::Gmgn, value)
}

/// A UUID-shaped id, unique per request (GMGN rejects repeats within 7 s).
fn client_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id() as u64;
    let a = nanos ^ (pid << 32);
    let b = n.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ nanos.rotate_left(17);
    format!(
        "{:08x}-{:04x}-4{:03x}-a{:03x}-{:012x}",
        (a >> 32) as u32,
        (a >> 16) as u16,
        a as u16 & 0xfff,
        (b >> 48) as u16 & 0xfff,
        b & 0xffff_ffff_ffff
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let text = std::fs::read_to_string(format!(
            "{}/src/fixtures/gmgn/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        unwrap_response(200, &text, None).unwrap()
    }

    #[test]
    fn recorded_responses_parse() {
        let info: TokenInfo = parse(fixture("info_bonk")).unwrap();
        assert_eq!(info.symbol.as_deref(), Some("Bonk"));
        assert_eq!(info.decimals, Some(5));
        assert_eq!(
            info.dev.creator_token_status.as_deref(),
            Some("creator_close")
        );
        let security: Security = parse(fixture("security_bonk")).unwrap();
        assert_eq!(security.renounced_mint.get(), Some(true));
        assert_eq!(security.renounced_freeze_account.get(), Some(true));
        let fresh: Security = parse(fixture("security_fresh")).unwrap();
        assert!(fresh.renounced_mint.get().is_some());
        let kol: Value = fixture("kol");
        let trades: Vec<Trade> = parse(kol["list"].clone()).unwrap();
        assert!(trades.iter().all(|t| t.base_address.len() >= 32));
        assert!(
            trades
                .iter()
                .any(|t| t.maker_info.tags.iter().any(|g| g == "kol"))
        );
        let signals: Vec<Signal> = parse(fixture("signal")).unwrap();
        assert!(
            signals
                .iter()
                .any(|s| s.signal_type == 7 && s.trigger_at.ts().is_some())
        );
    }

    #[test]
    fn rate_limits_become_pauses() {
        let reset = crate::view::now() + 30;
        let text = format!(r#"{{"code":429,"error":"RATE_LIMIT_BANNED","reset_at":{reset}}}"#);
        let fail = unwrap_response(429, &text, None).unwrap_err();
        assert!(
            matches!(fail, Fail::RateLimited { retry_after_s, .. } if (30..=32).contains(&retry_after_s))
        );
        // An HTML ban page still pauses, and so does a 403.
        let fail = unwrap_response(429, "<html>slow down</html>", None).unwrap_err();
        assert!(matches!(fail, Fail::RateLimited { .. }));
        assert!(matches!(
            unwrap_response(403, "", None),
            Err(Fail::RateLimited { .. })
        ));
    }

    #[test]
    fn a_ban_is_never_a_short_429() {
        let soon = crate::view::now() + 1;
        let ban = format!(r#"{{"code":1,"error":"RATE_LIMIT_BANNED","reset_at":{soon}}}"#);
        assert!(matches!(
            unwrap_response(429, &ban, None),
            Err(Fail::RateLimited { retry_after_s, .. }) if retry_after_s > 3
        ));
    }

    #[test]
    fn client_ids_do_not_repeat() {
        let ids: std::collections::HashSet<String> = (0..1000).map(|_| client_id()).collect();
        assert_eq!(ids.len(), 1000);
        assert_eq!(client_id().len(), 36);
    }
}
