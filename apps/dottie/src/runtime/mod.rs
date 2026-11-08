//! Shared by every user on a backend host, built once per process: HTTP
//! clients, the async runtime, one cache, one gate per provider, and the
//! one request path every provider goes through.
mod cache;
mod error;
mod gate;
mod turn;

pub use cache::{Cache, Hit};
pub use error::Fail;
pub use gate::{Cost, Gates, Lane};
pub use turn::{
    DEFAULT_RPC, FOMO_KEY, GMGN_KEY, JUPITER_KEY, RPC_URL, SETTINGS_TOKEN, SETTINGS_URL, Turn,
};

use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// Where a number came from; every reply lists the sources it used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Gmgn,
    Fomo,
    Jupiter,
    /// Solana JSON-RPC.
    Rpc,
    Rugcheck,
    Fluxbeam,
    Solomon,
    /// Dottie's own settings store.
    Settings,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Gmgn => "gmgn",
            Source::Fomo => "fomo",
            Source::Jupiter => "jupiter",
            Source::Rpc => "rpc",
            Source::Rugcheck => "rugcheck",
            Source::Fluxbeam => "fluxbeam",
            Source::Solomon => "solomon",
            Source::Settings => "settings",
        }
    }

    pub fn parse(name: &str) -> Option<Source> {
        [
            Source::Gmgn,
            Source::Fomo,
            Source::Jupiter,
            Source::Rpc,
            Source::Rugcheck,
            Source::Fluxbeam,
            Source::Solomon,
            Source::Settings,
        ]
        .into_iter()
        .find(|s| s.name() == name)
    }
}

/// One cacheable read: its cache key, what it costs and how long it stays
/// fresh. A zero TTL is never served fresh but still backs a stale answer.
pub struct Request {
    pub source: Source,
    pub key: String,
    pub cost: Cost,
    pub ttl: Duration,
    /// Whether an older answer may stand in when the provider fails.
    pub stale_ok: bool,
}

impl Request {
    fn operation(&self) -> &str {
        match self.source {
            Source::Jupiter => "quote",
            Source::Rpc => "rpc",
            _ => self
                .key
                .split(':')
                .nth(1)
                .unwrap_or("read")
                .split('?')
                .next()
                .unwrap_or("read"),
        }
    }

    pub fn new(source: Source, key: impl Into<String>, cost: Cost, ttl: Duration) -> Self {
        Self {
            source,
            key: key.into(),
            cost,
            ttl,
            stale_ok: true,
        }
    }

    /// Never answer from an expired entry: quotes must be live.
    pub fn live_only(mut self) -> Self {
        self.stale_ok = false;
        self
    }
}

/// How long a failure is shared with identical requests that were waiting
/// on it, so five users on one slow board don't retry it back to back.
const FAILURE_TTL: Duration = Duration::from_secs(5);

pub struct Runtime {
    pub http: reqwest::Client,
    /// GMGN's API serves IPv4 only.
    pub http_v4: reqwest::Client,
    pub tokio: tokio::runtime::Runtime,
    pub cache: Cache,
    pub gates: Gates,
    inflight: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    failures: Mutex<HashMap<String, (std::time::Instant, Fail)>>,
    /// Recorded responses served instead of the network (offline tests).
    replay: Option<BTreeMap<String, Value>>,
    /// Responses captured for later replay.
    recording: Option<Mutex<BTreeMap<String, Value>>>,
}

impl Runtime {
    fn new() -> Result<Self, String> {
        let client = |v4: bool| {
            let mut builder = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(4))
                .timeout(Duration::from_secs(30))
                .user_agent("dottie/0.2");
            if v4 {
                builder = builder.local_address(std::net::IpAddr::from([0, 0, 0, 0]));
            }
            builder
                .build()
                .map_err(|_| "HTTP client initialization failed".to_string())
        };
        let tokio = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|_| "async runtime initialization failed".to_string())?;
        Ok(Self {
            http: client(false)?,
            http_v4: client(true)?,
            tokio,
            cache: Cache::default(),
            gates: Gates::default(),
            inflight: Mutex::default(),
            failures: Mutex::default(),
            replay: None,
            recording: std::env::var_os("DOTTIE_RECORD").map(|_| Mutex::default()),
        })
    }

    /// A runtime that answers only from recorded responses.
    /// A `$recorded_at` entry freezes the clock at recording time.
    pub fn replaying(recorded: BTreeMap<String, Value>) -> Result<Self, String> {
        if let Some(at) = recorded.get("$recorded_at").and_then(Value::as_i64) {
            crate::view::freeze_clock(at);
        }
        Ok(Self {
            replay: Some(recorded),
            ..Self::new()?
        })
    }

    /// Everything fetched so far, when started with `DOTTIE_RECORD` set.
    pub fn recorded(&self) -> BTreeMap<String, Value> {
        self.recording
            .as_ref()
            .and_then(|r| r.lock().ok().map(|m| m.clone()))
            .unwrap_or_default()
    }

    /// The one request path: fresh cache → wait for an identical request in
    /// flight → the provider's gate → `send` → cache. When the gate or the
    /// provider says no, an older cached answer is served with its age.
    pub async fn fetch<F, Fut>(&self, turn: &Turn, req: Request, send: F) -> Result<Value, Fail>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Value, Fail>>,
    {
        if let Some(replay) = &self.replay {
            turn.used(req.source);
            let value = replay.get(&req.key).cloned().ok_or_else(|| {
                Fail::unavailable(req.source, format!("no recorded response for {}", req.key))
            })?;
            return match Fail::replayed(&value) {
                Some(fail) => Err(fail),
                None => Ok(value),
            };
        }
        if let Some(Hit::Fresh(value)) = self.cache.get(&req.key) {
            turn.trace(
                req.source,
                req.operation(),
                "cache",
                Duration::ZERO,
                "fresh",
            );
            turn.used(req.source);
            return Ok(value);
        }
        let slot = self.slot(&req.key);
        let started = std::time::Instant::now();
        let _held = slot.lock().await;
        turn.trace(
            req.source,
            req.operation(),
            "inflight_wait",
            started.elapsed(),
            "acquired",
        );
        let result = self.fetch_held(turn, &req, send).await;
        drop(_held);
        self.release(&req.key, slot);
        if result.is_ok() {
            turn.used(req.source);
        }
        result
    }

    async fn fetch_held<F, Fut>(&self, turn: &Turn, req: &Request, send: F) -> Result<Value, Fail>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Value, Fail>>,
    {
        // Someone else may have fetched it, or just failed, while we waited.
        if let Some(Hit::Fresh(value)) = self.cache.get(&req.key) {
            return Ok(value);
        }
        if let Some(fail) = self.recent_failure(&req.key) {
            turn.trace(
                req.source,
                req.operation(),
                "failure_cache",
                Duration::ZERO,
                fail.code(),
            );
            return self.stale_or(turn, req, fail);
        }
        let wait = match self.gates.admit(req.cost, turn.lane) {
            Ok(wait) => wait,
            Err(fail) => {
                turn.trace(
                    req.source,
                    req.operation(),
                    "gate",
                    Duration::ZERO,
                    fail.code(),
                );
                return self.stale_or(turn, req, fail);
            }
        };
        turn.trace(req.source, req.operation(), "gate_wait", wait, "planned");
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
            // A 429 may have paused the provider while this one queued.
            if let Err(fail) = self.gates.still_open(req.cost) {
                return self.stale_or(turn, req, fail);
            }
        }
        let started = std::time::Instant::now();
        let sent = send().await;
        turn.trace(
            req.source,
            req.operation(),
            "provider",
            started.elapsed(),
            sent.as_ref().err().map_or("ok", Fail::code),
        );
        if let Some(recording) = &self.recording
            && let Ok(mut map) = recording.lock()
        {
            let entry = match &sent {
                Ok(value) => value.clone(),
                Err(fail) => fail.recorded(req.source),
            };
            map.insert(req.key.clone(), entry);
        }
        match sent {
            Ok(value) => {
                self.cache.put(req.key.clone(), value.clone(), req.ttl);
                Ok(value)
            }
            Err(fail) => {
                if let Fail::RateLimited {
                    source,
                    retry_after_s,
                } = fail
                {
                    self.gates.pause(source, retry_after_s);
                }
                if let Ok(mut failures) = self.failures.lock() {
                    failures.retain(|_, (at, _)| at.elapsed() < FAILURE_TTL);
                    failures.insert(req.key.clone(), (std::time::Instant::now(), fail.clone()));
                }
                self.stale_or(turn, req, fail)
            }
        }
    }

    fn recent_failure(&self, key: &str) -> Option<Fail> {
        let failures = self.failures.lock().ok()?;
        let (at, fail) = failures.get(key)?;
        (at.elapsed() < FAILURE_TTL).then(|| fail.clone())
    }

    fn stale_or(&self, turn: &Turn, req: &Request, fail: Fail) -> Result<Value, Fail> {
        if !fail.stale_ok() || !req.stale_ok {
            return Err(fail);
        }
        match self.cache.get(&req.key) {
            Some(Hit::Stale { value, age }) => {
                turn.trace(
                    req.source,
                    req.operation(),
                    "stale_fallback",
                    age,
                    fail.code(),
                );
                turn.gap(format!(
                    "{}; showing {} data from {} ago",
                    fail.message(),
                    req.source.name(),
                    crate::view::duration(age)
                ));
                Ok(value)
            }
            Some(Hit::Fresh(value)) => Ok(value),
            None => Err(fail),
        }
    }

    fn slot(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut map = self.inflight.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(key.to_string()).or_default().clone()
    }

    fn release(&self, key: &str, slot: Arc<tokio::sync::Mutex<()>>) {
        let mut map = self.inflight.lock().unwrap_or_else(|e| e.into_inner());
        // Ours plus the map's: nobody else is waiting on this key.
        if Arc::strong_count(&slot) <= 2 {
            map.remove(key);
        }
    }
}

/// The app handle the host creates. Every instance in a process shares one
/// runtime, so GMGN's per-IP limit and Fomo's
/// per-key limit are paced in one place.
#[derive(Clone, Default)]
pub struct DottieApp {
    /// Set only for replay tests.
    replay: Option<Arc<Runtime>>,
}

impl DottieApp {
    pub fn runtime(&self) -> Result<Arc<Runtime>, String> {
        static SHARED: OnceLock<Result<Arc<Runtime>, String>> = OnceLock::new();
        match &self.replay {
            Some(rt) => Ok(rt.clone()),
            None => SHARED.get_or_init(|| Runtime::new().map(Arc::new)).clone(),
        }
    }

    /// An app whose tools answer only from recorded responses.
    pub fn replaying(recorded: BTreeMap<String, Value>) -> Self {
        Self {
            replay: Runtime::replaying(recorded).ok().map(Arc::new),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn req(key: &str) -> Request {
        Request::new(Source::Rpc, key, Cost::Free, Duration::from_secs(30))
    }

    #[test]
    fn identical_requests_share_one_call() {
        let rt = Runtime::new().unwrap();
        let turn = Turn::default();
        let calls = AtomicU32::new(0);
        let send = || async {
            calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(json!(7))
        };
        let (a, b) = rt.tokio.block_on(async {
            tokio::join!(
                rt.fetch(&turn, req("k"), send),
                rt.fetch(&turn, req("k"), send)
            )
        });
        assert_eq!((a.unwrap(), b.unwrap()), (json!(7), json!(7)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(rt.inflight.lock().unwrap().is_empty());
        assert_eq!(turn.sources(), [Source::Rpc]);
    }

    #[test]
    fn outages_fall_back_to_stale_with_age() {
        let rt = Runtime::new().unwrap();
        let turn = Turn::default();
        rt.cache.put("k".into(), json!(1), Duration::ZERO);
        let out = rt.tokio.block_on(rt.fetch(&turn, req("k"), || async {
            Err(Fail::unavailable(Source::Rpc, "down"))
        }));
        assert_eq!(out.unwrap(), json!(1));
        assert!(turn.gaps()[0].starts_with("rpc: down; showing rpc data from"));
        // Within a few seconds the same key reuses that failure, no resend.
        let out = rt.tokio.block_on(rt.fetch(&turn, req("k"), || async {
            panic!("a failure seconds ago is shared, not retried")
        }));
        assert_eq!(out.unwrap(), json!(1));
        // A missing token isn't an outage: no stale substitute.
        rt.cache.put("k2".into(), json!(2), Duration::ZERO);
        let out = rt.tokio.block_on(rt.fetch(&turn, req("k2"), || async {
            Err(Fail::NotFound("no such token".into()))
        }));
        assert_eq!(out.unwrap_err(), Fail::NotFound("no such token".into()));
        // Live-only reads never fall back.
        rt.cache.put("q".into(), json!(3), Duration::ZERO);
        let out = rt
            .tokio
            .block_on(rt.fetch(&turn, req("q").live_only(), || async {
                Err(Fail::unavailable(Source::Jupiter, "down"))
            }));
        assert!(out.is_err());
    }
}
