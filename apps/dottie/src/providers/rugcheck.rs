//! RugCheck's report summary: its score, named risks and how much of the
//! liquidity is locked. GET only; a POST to RugCheck files a report, which
//! Dottie never does. Authorities and Token-2022 fees come from the mint
//! account itself (`providers::rpc`).
use crate::providers::{get_json, num, pubkey};
use crate::runtime::{Fail, Runtime, Source, Turn};
use serde_json::Value;
use std::time::Duration;

const TTL: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Risk {
    pub name: String,
    /// `danger`, `warn` or `info`.
    pub level: String,
}

/// What Dottie reads from a summary. `None` is unknown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// RugCheck's normalised risk score; higher is riskier.
    pub score: Option<f64>,
    pub risks: Vec<Risk>,
    pub lp_locked_pct: Option<f64>,
}

pub async fn report(rt: &Runtime, turn: &Turn, mint: &str) -> Result<Report, Fail> {
    let mint = pubkey(mint)?;
    let url = format!("https://api.rugcheck.xyz/v1/tokens/{mint}/report/summary");
    read(&get_json(rt, turn, Source::Rugcheck, url, TTL).await?)
}

pub fn read(raw: &Value) -> Result<Report, Fail> {
    let Some(risks) = raw["risks"].as_array() else {
        return Err(Fail::bad(Source::Rugcheck, "summary without a risk list"));
    };
    Ok(Report {
        score: num(&raw["score_normalised"]),
        risks: risks
            .iter()
            .take(8)
            .filter_map(|r| {
                Some(Risk {
                    name: crate::view::safe(r["name"].as_str()?, 40),
                    level: r["level"].as_str().unwrap_or("").chars().take(8).collect(),
                })
            })
            .collect(),
        lp_locked_pct: num(&raw["lpLockedPct"]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_read_and_empty_ones_are_refused() {
        let raw = serde_json::json!({"risks": [{"name": "Mutable metadata", "level": "warn"}],
            "score_normalised": 7, "lpLockedPct": 17.3});
        let r = read(&raw).unwrap();
        assert_eq!((r.score, r.lp_locked_pct), (Some(7.0), Some(17.3)));
        assert_eq!(r.risks[0].name, "Mutable metadata");
        assert!(read(&serde_json::json!({})).is_err());
    }
}
