//! One module per data source. Each owns its auth, endpoints and response
//! types (only the fields Dottie reads) and knows nothing about Dottie's
//! rules. Every request goes through [`crate::runtime::Runtime::fetch`].
pub mod fluxbeam;
pub mod fomo;
pub mod gmgn;
pub mod jupiter;
pub mod rpc;
pub mod rugcheck;
pub mod settings;
pub mod solomon;

use crate::runtime::{Fail, Source};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// Wrapped SOL: the mint swaps use for native SOL.
pub const SOL: &str = "So11111111111111111111111111111111111111112";
pub const USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
pub const USDT: &str = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";
pub const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const TOKEN_2022_PROGRAM: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
pub const ASSOCIATED_TOKEN_PROGRAM: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";

/// A provider number that may arrive as a JSON number, a string, or null.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Num(pub Option<f64>);

impl Num {
    pub fn get(self) -> Option<f64> {
        self.0
    }

    /// A 0–1 fraction as a percent 0–100.
    pub fn pct(self) -> Option<f64> {
        self.0.map(|v| v * 100.0)
    }

    /// As a unix timestamp; 0 means unset.
    pub fn ts(self) -> Option<i64> {
        self.0.filter(|t| *t > 0.0).map(|t| t as i64)
    }
}

impl<'de> Deserialize<'de> for Num {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(d)?;
        Ok(Num(num(&value)))
    }
}

pub fn num(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|v| v.is_finite())
}

/// A yes / no / unknown flag in any of the shapes GMGN uses for it
/// (`true`, `1`, `"yes"`, `"1"`; `-1` and null are unknown).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flag(pub Option<bool>);

impl Flag {
    pub fn get(self) -> Option<bool> {
        self.0
    }
}

impl<'de> Deserialize<'de> for Flag {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(d)?;
        Ok(Flag(match &value {
            Value::Bool(b) => Some(*b),
            Value::Number(n) => match n.as_i64() {
                Some(1) => Some(true),
                Some(0) => Some(false),
                _ => None,
            },
            Value::String(s) => match s.trim() {
                "yes" | "1" | "true" => Some(true),
                "no" | "0" | "false" => Some(false),
                _ => None,
            },
            _ => None,
        }))
    }
}

/// Decodes a provider response into its typed shape. Nulls are dropped
/// first, so a null anywhere reads as "missing" and takes the default
/// instead of failing the whole response.
pub fn parse<T: DeserializeOwned>(source: Source, value: Value) -> Result<T, Fail> {
    serde_json::from_value(drop_nulls(value))
        .map_err(|e| Fail::bad(source, format!("unexpected response shape: {e}")))
}

fn drop_nulls(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k, drop_nulls(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(drop_nulls).collect()),
        other => other,
    }
}

/// A keyless JSON GET through the shared request path, for the small
/// partner APIs (RugCheck, FluxBeam, Solomon), one request a second each.
pub async fn get_json(
    rt: &crate::runtime::Runtime,
    turn: &crate::runtime::Turn,
    source: Source,
    url: String,
    ttl: std::time::Duration,
) -> Result<Value, Fail> {
    use crate::runtime::{Cost, Request};
    let cost = Cost::Paced(source, std::time::Duration::from_secs(1));
    let request = Request::new(source, format!("{}:{url}", source.name()), cost, ttl);
    rt.fetch(turn, request, || async {
        let response = rt
            .http
            .get(&url)
            .timeout(std::time::Duration::from_secs(12))
            .send()
            .await
            .map_err(|_| Fail::unavailable(source, "request failed or timed out"))?;
        match response.status().as_u16() {
            200..=299 => {}
            404 => {
                return Err(Fail::NotFound(format!(
                    "{} has no result for this",
                    source.name()
                )));
            }
            429 => {
                return Err(Fail::RateLimited {
                    source,
                    retry_after_s: 30,
                });
            }
            status => return Err(Fail::unavailable(source, format!("HTTP {status}"))),
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| Fail::unavailable(source, "response interrupted"))?;
        if bytes.len() > 2_000_000 {
            return Err(Fail::bad(source, "response exceeds size limit"));
        }
        serde_json::from_slice(&bytes).map_err(|_| Fail::bad(source, "invalid JSON"))
    })
    .await
}

/// Checks a Solana address or mint: base58 that decodes to 32 bytes.
/// Case is kept; base58 is case-sensitive.
pub fn pubkey(value: &str) -> Result<&str, Fail> {
    let v = value.trim();
    if v.len() < 32 || v.len() > 44 || bs58::decode(v).into_vec().map_or(true, |b| b.len() != 32) {
        return Err(Fail::Invalid(format!(
            "`{}` is not a Solana address; copy the mint from a tool result",
            v.chars().take(48).collect::<String>()
        )));
    }
    Ok(v)
}

/// Checks a transaction signature: base58 that decodes to 64 bytes.
pub fn signature(value: &str) -> Result<&str, Fail> {
    let v = value.trim();
    if v.len() < 64 || v.len() > 90 || bs58::decode(v).into_vec().map_or(true, |b| b.len() != 64) {
        return Err(Fail::Invalid("not a Solana transaction signature".into()));
    }
    Ok(v)
}

/// The wallet's associated token account for a mint under one token
/// program: the program address of (wallet, program, mint) owned by the
/// associated-token program. None for an address that isn't base58.
pub fn associated_token_account(wallet: &str, mint: &str, program: &str) -> Option<String> {
    use curve25519_dalek::edwards::CompressedEdwardsY;
    use sha2::{Digest, Sha256};
    let key = |s: &str| -> Option<[u8; 32]> { bs58::decode(s).into_vec().ok()?.try_into().ok() };
    let seeds = [key(wallet)?, key(program)?, key(mint)?];
    let owner = key(ASSOCIATED_TOKEN_PROGRAM)?;
    (0..=u8::MAX).rev().find_map(|bump| {
        let mut hash = Sha256::new();
        for seed in &seeds {
            hash.update(seed);
        }
        hash.update([bump]);
        hash.update(owner);
        hash.update(b"ProgramDerivedAddress");
        let address: [u8; 32] = hash.finalize().into();
        // A program address is one no private key can sign for: off the curve.
        CompressedEdwardsY(address)
            .decompress()
            .is_none()
            .then(|| bs58::encode(address).into_string())
    })
}

/// A string that may be null; empty means unset.
pub fn text(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nulls_read_as_missing() {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Row {
            labels: Vec<String>,
            n: Num,
        }
        let row: Row = parse(Source::Gmgn, json!({ "labels": null, "n": null })).unwrap();
        assert!(row.labels.is_empty() && row.n.get().is_none());
        let rows: Vec<Row> = parse(Source::Gmgn, json!([{ "labels": ["BOT"] }])).unwrap();
        assert_eq!(rows[0].labels, ["BOT"]);
    }

    #[test]
    fn addresses_are_base58_32_bytes_with_case_kept() {
        let bonk = "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263";
        assert_eq!(pubkey(&format!(" {bonk} ")).unwrap(), bonk);
        assert!(pubkey("0x5fc5360d0400a0fd4f2af552add042d716f1d168").is_err());
        assert!(pubkey("https://example.com/../../x").is_err());
        assert!(pubkey("So1111111111111111111111111111111111111111/").is_err());
        let sig = "2vSjGUroogbM4Xu7BiiEmK9NcthCm39Gi2CMxFUeVWZRSUrVfSZhasAvEBYCNKk1wKiUoJGFs8ykUzMViDanj3w3";
        assert!(signature(sig).is_ok() && signature(bonk).is_err());
    }

    #[test]
    fn associated_token_accounts_match_the_chain() {
        // The fork wallet's BONK account, read back from getAccountInfo.
        let (wallet, bonk) = (
            "DfFpn9sPxGcuEgs7M6KJ9qKtNneMZisb4EDfNKgZuJQD",
            "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263",
        );
        assert_eq!(
            associated_token_account(wallet, bonk, TOKEN_PROGRAM).as_deref(),
            Some("53CdxgP5TQ3qHrQXsRPY7ThRN2Bzj29gNscpgciFfbpg")
        );
        let t22 = associated_token_account(wallet, bonk, TOKEN_2022_PROGRAM).unwrap();
        assert!(pubkey(&t22).is_ok() && t22 != "53CdxgP5TQ3qHrQXsRPY7ThRN2Bzj29gNscpgciFfbpg");
        assert_eq!(associated_token_account("0x12", bonk, TOKEN_PROGRAM), None);
    }

    #[test]
    fn numbers_and_flags_parse_every_shape() {
        let n: Num = serde_json::from_value(json!("18.77")).unwrap();
        assert_eq!(n.get(), Some(18.77));
        let n: Num = serde_json::from_value(json!(null)).unwrap();
        assert_eq!(n.get(), None);
        for (raw, want) in [
            (json!(true), Some(true)),
            (json!(0), Some(false)),
            (json!(-1), None),
            (json!("yes"), Some(true)),
            (json!("no"), Some(false)),
            (json!(null), None),
        ] {
            let f: Flag = serde_json::from_value(raw).unwrap();
            assert_eq!(f.get(), want);
        }
    }
}
