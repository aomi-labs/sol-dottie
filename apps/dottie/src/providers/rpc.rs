//! Solana JSON-RPC reads: SOL and token balances (both token programs),
//! mint accounts and confirmed transactions. The operator's
//! `SOLANA_RPC_URL`, else the public mainnet-beta endpoint, paced.
use crate::providers::{TOKEN_2022_PROGRAM, TOKEN_PROGRAM};
use crate::runtime::{Cost, Fail, Request, Runtime, Source, Turn};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;

/// The public endpoint allows about 100 requests per 10 s per IP.
const PUBLIC_GAP: Duration = Duration::from_millis(120);
const BALANCE_TTL: Duration = Duration::from_secs(10);
/// Mint decimals and program never change.
const MINT_TTL: Duration = Duration::from_secs(86_400);
/// A confirmed transaction never changes.
const TX_TTL: Duration = Duration::from_secs(3600);
const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";

/// One token the wallet holds, summed over its accounts for that mint.
#[derive(Clone, Debug, PartialEq)]
pub struct Holding {
    pub mint: String,
    pub raw: u128,
    pub decimals: u8,
    pub token_2022: bool,
}

impl Holding {
    pub fn amount(&self) -> f64 {
        self.raw as f64 / 10f64.powi(i32::from(self.decimals))
    }
}

/// A mint account: what a token is on chain.
#[derive(Clone, Debug, PartialEq)]
pub struct Mint {
    pub decimals: u8,
    pub token_2022: bool,
    pub mint_authority: Option<String>,
    pub freeze_authority: Option<String>,
    /// Token-2022 transfer fee in basis points; 0 for classic SPL.
    pub transfer_fee_bps: f64,
    /// Token-2022 extensions that let someone move, block or tax holders'
    /// tokens (`permanentDelegate`, `transferHook`…).
    pub risky_extensions: Vec<String>,
}

/// Extensions that put a holder's tokens in someone else's hands.
const RISKY: &[&str] = &[
    "permanentDelegate",
    "transferHook",
    "nonTransferable",
    "pausableConfig",
    "defaultAccountState",
    "confidentialTransferMint",
];

#[derive(Clone, Copy)]
pub struct Rpc<'a> {
    rt: &'a Runtime,
    turn: &'a Turn,
}

impl<'a> Rpc<'a> {
    pub fn new(rt: &'a Runtime, turn: &'a Turn) -> Self {
        Self { rt, turn }
    }

    /// Lamports held by the wallet itself.
    pub async fn native_balance(self, wallet: &str) -> Result<u64, Fail> {
        let value = self
            .call(
                "getBalance",
                json!([wallet, {"commitment": "confirmed"}]),
                BALANCE_TTL,
            )
            .await?;
        value["value"]
            .as_u64()
            .ok_or_else(|| Fail::bad(Source::Rpc, "balance without a value"))
    }

    /// Every token the wallet holds, both programs, largest raw first.
    /// Zero balances are left out.
    pub async fn holdings(self, wallet: &str) -> Result<Vec<Holding>, Fail> {
        let (classic, t22) = tokio::join!(
            self.accounts(wallet, json!({"programId": TOKEN_PROGRAM})),
            self.accounts(wallet, json!({"programId": TOKEN_2022_PROGRAM}))
        );
        let mut by_mint: BTreeMap<String, Holding> = BTreeMap::new();
        for (accounts, token_2022) in [(classic?, false), (t22?, true)] {
            for account in accounts {
                let Some((mint, raw, decimals)) = parsed_account(&account) else {
                    continue;
                };
                if raw == 0 {
                    continue;
                }
                by_mint
                    .entry(mint.clone())
                    .and_modify(|h| h.raw = h.raw.saturating_add(raw))
                    .or_insert(Holding {
                        mint,
                        raw,
                        decimals,
                        token_2022,
                    });
            }
        }
        Ok(by_mint.into_values().collect())
    }

    /// The wallet's raw balance of one mint (0 when it has no account).
    pub async fn balance(self, mint: &str, wallet: &str) -> Result<u128, Fail> {
        let accounts = self.accounts(wallet, json!({"mint": mint})).await?;
        Ok(accounts
            .iter()
            .filter_map(parsed_account)
            .filter(|(m, ..)| m == mint)
            .map(|(_, raw, _)| raw)
            .fold(0u128, u128::saturating_add))
    }

    /// The mint account. A wallet or a program is `NotFound`, said for the user.
    pub async fn mint(self, mint: &str) -> Result<Mint, Fail> {
        let value = self
            .call(
                "getAccountInfo",
                json!([mint, {"encoding": "jsonParsed", "commitment": "confirmed"}]),
                MINT_TTL,
            )
            .await?;
        parse_mint(&value["value"])
    }

    /// A confirmed transaction, parsed, v0 included.
    pub async fn transaction(self, signature: &str) -> Result<Value, Fail> {
        let value = self
            .call(
                "getTransaction",
                json!([signature, {"encoding": "jsonParsed", "commitment": "confirmed", "maxSupportedTransactionVersion": 0}]),
                TX_TTL,
            )
            .await?;
        if value.is_null() {
            return Err(Fail::NoData(
                "the RPC has no confirmed transaction with that signature yet".into(),
            ));
        }
        Ok(value)
    }

    async fn accounts(self, wallet: &str, filter: Value) -> Result<Vec<Value>, Fail> {
        let value = self
            .call(
                "getTokenAccountsByOwner",
                json!([wallet, filter, {"encoding": "jsonParsed", "commitment": "confirmed"}]),
                BALANCE_TTL,
            )
            .await?;
        value["value"]
            .as_array()
            .cloned()
            .ok_or_else(|| Fail::bad(Source::Rpc, "token accounts without a list"))
    }

    async fn call(self, method: &str, params: Value, ttl: Duration) -> Result<Value, Fail> {
        let url = self.turn.rpc_url().to_string();
        let cost = if url == crate::runtime::DEFAULT_RPC {
            Cost::Paced(Source::Rpc, PUBLIC_GAP)
        } else {
            Cost::Free
        };
        let request = Request::new(Source::Rpc, format!("rpc:{method}:{params}"), cost, ttl);
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        self.rt
            .fetch(self.turn, request, || async {
                let response = self
                    .rt
                    .http
                    .post(&url)
                    .json(&body)
                    .timeout(Duration::from_secs(15))
                    .send()
                    .await
                    .map_err(|_| Fail::unavailable(Source::Rpc, "request failed"))?;
                let status = response.status().as_u16();
                if status == 429 {
                    return Err(Fail::RateLimited {
                        source: Source::Rpc,
                        retry_after_s: 5,
                    });
                }
                let reply: Value = response
                    .json()
                    .await
                    .map_err(|_| Fail::bad(Source::Rpc, format!("HTTP {status}, not JSON")))?;
                unwrap_reply(reply)
            })
            .await
    }
}

/// JSON-RPC puts failures in `error`; an invalid address is the caller's
/// mistake, the rest are outages.
fn unwrap_reply(mut reply: Value) -> Result<Value, Fail> {
    if let Some(error) = reply.get("error").filter(|e| !e.is_null()) {
        let message: String = error["message"]
            .as_str()
            .unwrap_or("error")
            .chars()
            .take(120)
            .collect();
        return Err(if message.to_ascii_lowercase().contains("invalid") {
            Fail::Invalid(format!("the RPC rejected the request: {message}"))
        } else {
            Fail::unavailable(Source::Rpc, message)
        });
    }
    Ok(reply
        .get_mut("result")
        .map(Value::take)
        .unwrap_or(Value::Null))
}

/// (mint, raw amount, decimals) of a jsonParsed token account.
fn parsed_account(account: &Value) -> Option<(String, u128, u8)> {
    let info = account.pointer("/account/data/parsed/info")?;
    let amount = info
        .pointer("/tokenAmount/amount")?
        .as_str()?
        .parse()
        .ok()?;
    let decimals = info.pointer("/tokenAmount/decimals")?.as_u64()?;
    Some((
        info["mint"].as_str()?.to_string(),
        amount,
        decimals.try_into().ok()?,
    ))
}

pub fn parse_mint(account: &Value) -> Result<Mint, Fail> {
    if account.is_null() {
        return Err(Fail::NotFound("No token exists at that address.".into()));
    }
    let owner = account["owner"].as_str().unwrap_or("");
    let parsed = &account["data"]["parsed"];
    if owner == SYSTEM_PROGRAM {
        return Err(Fail::NotFound("That's a wallet, not a token.".into()));
    }
    if !(owner == TOKEN_PROGRAM || owner == TOKEN_2022_PROGRAM) || parsed["type"] != "mint" {
        return Err(Fail::NotFound(
            "That address isn't a token I can trade.".into(),
        ));
    }
    let info = &parsed["info"];
    let decimals = info["decimals"]
        .as_u64()
        .and_then(|d| u8::try_from(d).ok())
        .ok_or_else(|| Fail::bad(Source::Rpc, "mint without decimals"))?;
    let text = |field: &str| info[field].as_str().map(str::to_string);
    let extensions = info["extensions"].as_array().cloned().unwrap_or_default();
    let transfer_fee_bps = extensions
        .iter()
        .find(|e| e["extension"] == "transferFeeConfig")
        .and_then(|e| e.pointer("/state/newerTransferFee/transferFeeBasisPoints"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let risky_extensions = extensions
        .iter()
        .filter_map(|e| e["extension"].as_str())
        .filter(|e| RISKY.contains(e))
        .map(str::to_string)
        .collect();
    Ok(Mint {
        decimals,
        token_2022: owner == TOKEN_2022_PROGRAM,
        mint_authority: text("mintAuthority"),
        freeze_authority: text("freezeAuthority"),
        transfer_fee_bps,
        risky_extensions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mints_wallets_and_errors_are_told_apart() {
        let mint = json!({"owner": TOKEN_2022_PROGRAM, "data": {"parsed": {"type": "mint",
            "info": {"decimals": 8, "mintAuthority": null, "freezeAuthority": "F1", "extensions": [
                {"extension": "transferFeeConfig", "state": {"newerTransferFee": {"transferFeeBasisPoints": 300}}},
                {"extension": "permanentDelegate", "state": {"delegate": "D"}}]}}}});
        let m = parse_mint(&mint).unwrap();
        assert_eq!((m.decimals, m.token_2022), (8, true));
        assert_eq!(m.transfer_fee_bps, 300.0);
        assert_eq!(m.risky_extensions, ["permanentDelegate"]);
        assert_eq!(
            (m.mint_authority, m.freeze_authority.as_deref()),
            (None, Some("F1"))
        );
        let wallet = json!({"owner": SYSTEM_PROGRAM, "data": ["", "base64"]});
        assert_eq!(
            parse_mint(&wallet).unwrap_err().message(),
            "That's a wallet, not a token."
        );
        assert!(matches!(parse_mint(&Value::Null), Err(Fail::NotFound(_))));
        let bad =
            unwrap_reply(json!({"error": {"code": -32602, "message": "Invalid param: WrongSize"}}));
        assert!(matches!(bad, Err(Fail::Invalid(_))));
        let account = json!({"account": {"data": {"parsed": {"info": {"mint": "M",
            "tokenAmount": {"amount": "1500", "decimals": 3}}}}}});
        assert_eq!(parsed_account(&account), Some(("M".into(), 1500, 3)));
    }
}
