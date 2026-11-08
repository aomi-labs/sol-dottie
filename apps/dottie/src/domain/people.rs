//! Real traders from Fomo: who leads by PnL, what a trader holds, which
//! trader owns a wallet, and what holders write about a coin. Credits are
//! scarce, so each mode makes as few calls as it can and caches long.
use crate::providers::fomo::Fomo;
use crate::providers::pubkey;
use crate::runtime::{Fail, Runtime, Turn};
use crate::view::people as view;
use serde_json::Value;

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
    /// trending: trending or most-held.
    pub board: &'a str,
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
        Mode::Trending => view::fomo_tokens(&fomo.tokens(q.board, q.limit).await?),
        Mode::Theses => {
            let mint = pubkey(need(q.token, "token")?)?;
            view::fomo_theses(mint, &fomo.theses(mint).await?)
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
}
