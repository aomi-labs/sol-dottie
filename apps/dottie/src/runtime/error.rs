//! One failure type for every source, mapped once to the error codes the
//! model sees.
use super::Source;
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq)]
pub enum Fail {
    /// The app has no key for this source.
    Unconfigured(Source),
    /// A spend or rate budget on our side said no.
    Budget(&'static str),
    /// The provider is rate limiting us; `retry_after_s` from its reset time.
    RateLimited {
        source: Source,
        retry_after_s: u64,
    },
    Unavailable {
        source: Source,
        why: String,
    },
    BadResponse {
        source: Source,
        why: String,
    },
    /// The subject (token, wallet, market) isn't indexed.
    NotFound(String),
    /// A swap route doesn't exist.
    NoRoute(String),
    /// Indexed, but nothing in the asked range.
    NoData(String),
    Invalid(String),
    NoWallet,
}

impl Fail {
    pub fn unavailable(source: Source, why: impl Into<String>) -> Self {
        Fail::Unavailable {
            source,
            why: why.into(),
        }
    }

    pub fn bad(source: Source, why: impl Into<String>) -> Self {
        Fail::BadResponse {
            source,
            why: why.into(),
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Fail::Unconfigured(_) => "UNCONFIGURED",
            Fail::Budget(_) => "BUDGET",
            Fail::RateLimited { .. } => "RATE_LIMITED",
            Fail::Unavailable { .. } => "UNAVAILABLE",
            Fail::BadResponse { .. } => "BAD_RESPONSE",
            Fail::NotFound(_) => "NOT_FOUND",
            Fail::NoRoute(_) => "NO_ROUTE",
            Fail::NoData(_) => "NO_DATA",
            Fail::Invalid(_) => "INVALID_ARGUMENT",
            Fail::NoWallet => "NO_WALLET",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Fail::Unconfigured(s) => format!("{} data is not configured for this app", s.name()),
            Fail::Budget(why) => (*why).to_string(),
            Fail::RateLimited { source, .. } => format!("{} is rate limiting", source.name()),
            Fail::Unavailable { source, why } | Fail::BadResponse { source, why } => {
                format!("{}: {why}", source.name())
            }
            Fail::NotFound(why) | Fail::NoRoute(why) | Fail::NoData(why) | Fail::Invalid(why) => {
                why.clone()
            }
            Fail::NoWallet => {
                "no Solana wallet given and none connected; ask for an address or to connect".into()
            }
        }
    }

    pub fn retry_after_s(&self) -> Option<u64> {
        match self {
            Fail::RateLimited { retry_after_s, .. } => Some(*retry_after_s),
            Fail::Unavailable { .. } => Some(10),
            _ => None,
        }
    }

    /// A failure as recorded for replay tests.
    pub fn recorded(&self, source: Source) -> Value {
        json!({ "$fail": { "code": self.code(), "message": self.message(), "source": source.name() } })
    }

    /// The failure a recorded response stands for, if it is one.
    pub fn replayed(value: &Value) -> Option<Fail> {
        let f = value.get("$fail")?;
        let message = f["message"].as_str().unwrap_or("").to_string();
        let source = f["source"]
            .as_str()
            .and_then(Source::parse)
            .unwrap_or(Source::Rpc);
        Some(match f["code"].as_str()? {
            "NOT_FOUND" => Fail::NotFound(message),
            "NO_ROUTE" => Fail::NoRoute(message),
            "NO_DATA" => Fail::NoData(message),
            "INVALID_ARGUMENT" => Fail::Invalid(message),
            "UNCONFIGURED" => Fail::Unconfigured(source),
            "RATE_LIMITED" => Fail::RateLimited {
                source,
                retry_after_s: 30,
            },
            "BAD_RESPONSE" => Fail::bad(source, message),
            _ => Fail::unavailable(source, message),
        })
    }

    /// Whether an older cached answer is a fair substitute. A wrong input or
    /// a missing token is an answer, not an outage.
    pub fn stale_ok(&self) -> bool {
        matches!(
            self,
            Fail::Budget(_)
                | Fail::RateLimited { .. }
                | Fail::Unavailable { .. }
                | Fail::BadResponse { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_staleness() {
        let limited = Fail::RateLimited {
            source: Source::Gmgn,
            retry_after_s: 4,
        };
        assert_eq!(limited.code(), "RATE_LIMITED");
        assert!(limited.stale_ok());
        assert!(!Fail::NotFound("x".into()).stale_ok());
        assert_eq!(
            Fail::Unconfigured(Source::Fomo).message(),
            "fomo data is not configured for this app"
        );
        let no_route = Fail::NoRoute("no sell route".into());
        assert_eq!(
            Fail::replayed(&no_route.recorded(Source::Jupiter)),
            Some(no_route)
        );
    }
}
