//! Per-tool-call context: credentials, the thread and wallet it belongs to,
//! and the gaps and sources the reply must disclose. Shared by reference
//! across the concurrent reads of one tool call.
use super::{Lane, Source};
use aomi_sdk::DynToolCallCtx;
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::Mutex;

pub const GMGN_KEY: &str = "GMGN_API_KEY";
pub const FOMO_KEY: &str = "FOMO_API_KEY";
pub const JUPITER_KEY: &str = "JUPITER_API_KEY";
pub const RPC_URL: &str = "SOLANA_RPC_URL";
pub const SETTINGS_URL: &str = "DOTTIE_SETTINGS_URL";
pub const SETTINGS_TOKEN: &str = "DOTTIE_SETTINGS_TOKEN";

/// Public mainnet-beta RPC; rate limited, fine for a few reads per answer.
pub const DEFAULT_RPC: &str = "https://api.mainnet-beta.solana.com";

#[derive(Default)]
pub struct Turn {
    /// The chat thread; budgets that should be per answer key on it.
    pub thread: String,
    call_id: String,
    /// The user's connected Solana wallet (base58, case kept).
    pub wallet: Option<String>,
    /// The host account the chat belongs to; `None` for guests.
    pub account: Option<String>,
    pub lane: Lane,
    gmgn_key: Option<String>,
    fomo_key: Option<String>,
    jupiter_key: Option<String>,
    rpc_url: Option<String>,
    settings_url: Option<String>,
    settings_token: Option<String>,
    notes: Mutex<Notes>,
}

#[derive(Default)]
struct Notes {
    gaps: Vec<String>,
    sources: BTreeSet<Source>,
}

impl Turn {
    pub fn new(ctx: &DynToolCallCtx, lane: Lane) -> Self {
        let secret = |name: &str| {
            aomi_sdk::resolve_secret_value(ctx, None, name, "")
                .ok()
                .map(|v| v.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
                .filter(|v| !v.is_empty())
        };
        let wallet = ctx
            .attribute_path(&["domain", "svm", "address"])
            .and_then(Value::as_str)
            .and_then(|a| crate::view::address(a).ok());
        // It becomes a URL path segment in the settings store.
        let account = ctx
            .attribute_path(&["hosted", "account_id"])
            .and_then(Value::as_str)
            .filter(|a| {
                (1..=128).contains(&a.len())
                    && a.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            })
            .map(str::to_string);
        Self {
            thread: ctx.session_id.clone(),
            call_id: ctx.call_id.clone(),
            wallet,
            account,
            lane,
            gmgn_key: secret(GMGN_KEY),
            fomo_key: secret(FOMO_KEY),
            jupiter_key: secret(JUPITER_KEY),
            rpc_url: secret(RPC_URL)
                .filter(|u| u.starts_with("https://") || u.starts_with("http://")),
            settings_url: secret(SETTINGS_URL),
            settings_token: secret(SETTINGS_TOKEN),
            notes: Mutex::default(),
        }
    }

    pub fn key(&self, source: Source) -> Option<&str> {
        match source {
            Source::Gmgn => self.gmgn_key.as_deref(),
            Source::Fomo => self.fomo_key.as_deref(),
            Source::Jupiter => self.jupiter_key.as_deref(),
            Source::Settings => self.settings_token.as_deref(),
            _ => None,
        }
    }

    /// The Solana RPC endpoint: the operator's, else the public one.
    pub fn rpc_url(&self) -> &str {
        self.rpc_url.as_deref().unwrap_or(DEFAULT_RPC)
    }

    /// The settings store's base URL and token, when both are set.
    pub fn settings_store(&self) -> Option<(&str, &str)> {
        self.settings_url
            .as_deref()
            .zip(self.settings_token.as_deref())
    }

    /// Opt-in timing rows alongside the GMGN trace (`DOTTIE_TRACE=<file>`). No arguments or credentials.
    pub fn trace(
        &self,
        source: Source,
        operation: &str,
        phase: &str,
        elapsed: std::time::Duration,
        outcome: &str,
    ) {
        use std::io::Write;
        let Some(path) = std::env::var_os("DOTTIE_TRACE") else {
            return;
        };
        let row = serde_json::json!({"at":crate::view::now(),"call_id":self.call_id,
            "source":source,"operation":operation,"phase":phase,"ms":elapsed.as_secs_f64()*1000.0,"outcome":outcome});
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = file.write_all(format!("{row}\n").as_bytes());
        }
    }

    /// Something the reply is missing or approximating, said once.
    pub fn gap(&self, gap: impl Into<String>) {
        let gap = gap.into();
        if let Ok(mut notes) = self.notes.lock()
            && !notes.gaps.contains(&gap)
        {
            notes.gaps.push(gap);
        }
    }

    pub fn used(&self, source: Source) {
        if let Ok(mut notes) = self.notes.lock() {
            notes.sources.insert(source);
        }
    }

    pub fn gaps(&self) -> Vec<String> {
        self.notes
            .lock()
            .map(|n| n.gaps.clone())
            .unwrap_or_default()
    }

    pub fn sources(&self) -> Vec<Source> {
        self.notes
            .lock()
            .map(|n| n.sources.iter().copied().collect())
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub fn with_keys(gmgn: Option<&str>, fomo: Option<&str>) -> Self {
        Self {
            thread: "test".into(),
            gmgn_key: gmgn.map(str::to_string),
            fomo_key: fomo.map(str::to_string),
            ..Self::default()
        }
    }
}
