use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct Failure {
    pub code: &'static str,
    pub message: String,
}
impl Failure {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
#[derive(Clone)]
pub struct Read {
    pub value: Value,
    pub fetched_at: u64,
    pub cached: bool,
}
struct Entry {
    read: Read,
    at: Instant,
}
#[derive(Default)]
struct ProviderState {
    cache: HashMap<String, Entry>,
    last_request: Option<Instant>,
    paused_until: Option<Instant>,
}
pub struct Runtime {
    http: reqwest::Client,
    pub executor: tokio::runtime::Runtime,
    gates: [Mutex<ProviderState>; 3],
}
impl Runtime {
    fn new() -> Result<Self, String> {
        Ok(Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(4))
                .timeout(Duration::from_secs(12))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent("sol-dottie/0.1")
                .build()
                .map_err(|_| "HTTP initialization failed")?,
            executor: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|_| "Runtime initialization failed")?,
            gates: std::array::from_fn(|_| Mutex::new(ProviderState::default())),
        })
    }
    pub async fn get(&self, provider: usize, url: &str, ttl: Duration) -> Result<Read, Failure> {
        // Global provider pacing, bounded queue wait and shared cached reads.
        let mut state = tokio::time::timeout(Duration::from_secs(5), self.gates[provider].lock())
            .await
            .map_err(|_| Failure::new("BUSY", "Provider queue is busy; try again shortly"))?;
        if let Some(entry) = state.cache.get(url)
            && entry.at.elapsed() < ttl
        {
            return Ok(Read {
                cached: true,
                ..entry.read.clone()
            });
        }
        if state
            .paused_until
            .is_some_and(|until| until > Instant::now())
        {
            return Err(Failure::new(
                "RATE_LIMITED",
                "Provider cooldown is active; no retry was sent",
            ));
        }
        if let Some(last) = state.last_request {
            tokio::time::sleep(Duration::from_secs(1).saturating_sub(last.elapsed())).await;
        }
        state.last_request = Some(Instant::now());
        let mut response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|_| Failure::new("UNAVAILABLE", "Provider request failed or timed out"))?;
        if response.status().as_u16() == 429 {
            let delay = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(30)
                .clamp(1, 300);
            state.paused_until = Some(Instant::now() + Duration::from_secs(delay));
        }
        check_status(response.status().as_u16())?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| Failure::new("UNAVAILABLE", "Provider response interrupted"))?
        {
            if bytes.len() + chunk.len() > 2_000_000 {
                return Err(Failure::new(
                    "MALFORMED_RESPONSE",
                    "Provider response exceeds size limit",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let value = serde_json::from_slice(&bytes)
            .map_err(|_| Failure::new("MALFORMED_RESPONSE", "Provider returned invalid JSON"))?;
        let read = Read {
            value,
            fetched_at: now(),
            cached: false,
        };
        state
            .cache
            .retain(|_, e| e.at.elapsed() < Duration::from_secs(60));
        if state.cache.len() >= 128 {
            state.cache.clear();
        }
        if !ttl.is_zero() {
            state.cache.insert(
                url.into(),
                Entry {
                    read: read.clone(),
                    at: Instant::now(),
                },
            );
        }
        Ok(read)
    }
}
pub fn check_status(status: u16) -> Result<(), Failure> {
    match status {
        200..=299 => Ok(()),
        404 => Err(Failure::new(
            "NOT_FOUND",
            "Provider has no result for this request",
        )),
        429 => Err(Failure::new(
            "RATE_LIMITED",
            "Provider rate limit reached; retry later",
        )),
        401 | 403 => Err(Failure::new(
            "UNAVAILABLE",
            "Provider access is unavailable",
        )),
        _ => Err(Failure::new(
            "UNAVAILABLE",
            format!("Provider returned HTTP {status}"),
        )),
    }
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
#[derive(Clone, Default)]
pub struct DottieApp;
impl DottieApp {
    pub fn runtime(&self) -> Result<Arc<Runtime>, String> {
        static SHARED: OnceLock<Result<Arc<Runtime>, String>> = OnceLock::new();
        SHARED.get_or_init(|| Runtime::new().map(Arc::new)).clone()
    }
}
