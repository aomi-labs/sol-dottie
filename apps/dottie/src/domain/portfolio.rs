//! Structured wallet reads. Filtered pages preserve unreturned rows in a
//! chat-scoped cursor, so response size limits cannot silently skip trades.
use super::market::{Kind, known};
use crate::providers::gmgn::{Gmgn, types::Activity};
use crate::providers::jupiter::Jupiter;
use crate::providers::rpc::Rpc;
use crate::providers::{pubkey, signature};
use crate::runtime::{Fail, Runtime, Source, Turn};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Default, Deserialize, Serialize)]
struct Page {
    scope: String,
    at: i64,
    rows: Vec<Value>,
    next: Option<Value>,
}

fn start(rt: &Runtime, turn: &Turn, scope: Value, cursor: Option<&str>) -> Result<Page, Fail> {
    let scope = scope.to_string();
    let Some(cursor) = cursor else {
        return Ok(Page {
            scope,
            at: crate::view::now(),
            ..Page::default()
        });
    };
    if cursor.len() > 100
        || !cursor
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(Fail::Invalid("invalid page cursor".into()));
    }
    let page: Page = rt
        .cache
        .fresh(&cursor_key(turn, cursor))
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or_else(|| {
            Fail::NotFound(
                "page cursor expired or belongs to another chat; start again without cursor".into(),
            )
        })?;
    if page.scope != scope {
        return Err(Fail::Invalid(
            "cursor does not match this wallet and filters".into(),
        ));
    }
    Ok(page)
}

fn cursor_key(turn: &Turn, id: &str) -> String {
    format!("portfolio:{}:{}:{id}", turn.thread.len(), turn.thread)
}

fn finish(rt: &Runtime, turn: &Turn, mut page: Page, limit: usize) -> Value {
    let mut items = vec![];
    // Budget before making the continuation: envelope trimming must never
    // remove an item that the next cursor has already advanced past.
    while !page.rows.is_empty() && items.len() < limit {
        let row = page.rows.remove(0);
        items.push(row);
        if crate::view::size(&json!({"items":items})) > 1900 && items.len() > 1 {
            page.rows.insert(0, items.pop().unwrap());
            break;
        }
    }
    let more = !page.rows.is_empty() || page.next.is_some();
    let at = page.at;
    let cursor = if more {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let id = format!("p{time:x}-{:x}", NEXT.fetch_add(1, Ordering::Relaxed));
        rt.cache.put(
            cursor_key(turn, &id),
            serde_json::to_value(page).unwrap(),
            Duration::from_secs(600),
        );
        Some(id)
    } else {
        None
    };
    json!({"items":items,"has_more":more,"next_cursor":cursor,"snapshot_at":at})
}

pub struct HistoryQuery<'a> {
    pub wallet: &'a str,
    pub token: Option<&'a str>,
    pub side: Option<&'a str>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub min_usd: f64,
    pub limit: usize,
    pub cursor: Option<&'a str>,
}

pub async fn history(rt: &Runtime, turn: &Turn, q: HistoryQuery<'_>) -> Result<Value, Fail> {
    let scope = json!([
        "history", q.wallet, q.token, q.side, q.from, q.to, q.min_usd
    ]);
    let mut page = start(rt, turn, scope, q.cursor)?;
    if page.rows.is_empty() {
        let next = page.next.take();
        let data = Gmgn::new(rt, turn)
            .activity_page(q.wallet, q.token, next.as_ref().and_then(Value::as_str))
            .await?;
        page.at = crate::view::now();
        let rows = data["activities"]
            .as_array()
            .filter(|r| r.len() <= 50)
            .ok_or_else(|| Fail::bad(Source::Gmgn, "wallet history page missing or too large"))?;
        for value in rows {
            let row: Activity = crate::providers::parse(Source::Gmgn, value.clone())?;
            if pubkey(&row.token.address).is_err()
                || signature(&row.tx_hash).is_err()
                || !matches!(
                    row.event_type.as_str(),
                    "buy" | "sell" | "transferIn" | "transferOut" | "add" | "remove"
                )
            {
                return Err(Fail::bad(
                    Source::Gmgn,
                    "wallet history identifiers are invalid",
                ));
            }
            let at = row.timestamp.ts();
            if q.side.is_some_and(|s| s != row.event_type)
                || q.token.is_some_and(|t| t != row.token.address.trim())
                || q.from.is_some_and(|t| at.is_none_or(|a| a < t))
                || q.to.is_some_and(|t| at.is_none_or(|a| a > t))
                || (q.min_usd > 0.0 && row.cost_usd.get().is_none_or(|v| v < q.min_usd))
            {
                continue;
            }
            let event = if row.is_open_or_close.get() == Some(1.0) {
                match row.event_type.as_str() {
                    "buy" => Some("opened"),
                    "sell" => Some("closed"),
                    _ => None,
                }
            } else {
                None
            };
            page.rows.push(json!({"tx_hash":row.tx_hash,"token":row.token.address,"symbol":row.token.symbol.map(|s|crate::card::format::clamp(&s,20)),"side":row.event_type,"timestamp":at,"token_amount":row.token_amount.get(),"indexed_value_usd":row.cost_usd.get(),"position_event":event}));
        }
        page.next = provider_next(&data, "next")?;
        if page.next == next && page.next.is_some() {
            return Err(Fail::bad(
                Source::Gmgn,
                "wallet history cursor did not advance",
            ));
        }
    }
    let mut out = finish(rt, turn, page, q.limit);
    out["wallet"] = json!(q.wallet);
    turn.used(Source::Gmgn);
    out["value_policy"] = json!(
        "indexed_value_usd is GMGN's pool-leg estimate, not wallet spend or PnL. Use dottie_pnl for verified round accounting. Pages cover indexed history only."
    );
    Ok(out)
}

/// Tokens priced per first page; the rest show without a value.
const PRICED: usize = 6;
const STABLES_USD: f64 = 1.0;

pub async fn positions(
    rt: &Runtime,
    turn: &Turn,
    wallet: &str,
    token: Option<&str>,
    min_usd: f64,
    limit: usize,
    cursor: Option<&str>,
) -> Result<Value, Fail> {
    let mut page = start(
        rt,
        turn,
        json!(["positions", wallet, token, min_usd]),
        cursor,
    )?;
    let rpc = Rpc::new(rt, turn);
    let first = cursor.is_none();
    if first {
        let mut held = rpc.holdings(wallet).await?;
        held.retain(|h| token.is_none_or(|t| t == h.mint));
        // Catalog assets first, then the rest; only the first few get a price.
        held.sort_by_key(|h| known(&h.mint).is_none());
        let gmgn = Gmgn::new(rt, turn);
        let mut unpriced = 0;
        for (i, h) in held.iter().enumerate() {
            // The GMGN gate paces these; cached prices answer at once.
            let info = if i < PRICED && known(&h.mint).is_none_or(|a| a.kind != Kind::Stable) {
                gmgn.token_info(&h.mint).await.ok()
            } else {
                None
            };
            let catalog = known(&h.mint);
            let stable = catalog.is_some_and(|a| a.kind == Kind::Stable);
            let info = info.filter(|i| i.address.trim() == h.mint);
            let price = if stable {
                Some(STABLES_USD)
            } else {
                info.as_ref().and_then(|i| i.price.price.get())
            };
            if price.is_none() {
                unpriced += 1;
            }
            let usd = price.map(|p| p * h.amount()).filter(|v| v.is_finite());
            if min_usd > 0.0 && usd.is_none_or(|v| v < min_usd) {
                continue;
            }
            let symbol = catalog
                .map(|a| a.symbol.clone())
                .or_else(|| info.as_ref().and_then(|i| i.symbol.clone()))
                .map(|s| crate::card::format::clamp(&s, 20));
            page.rows.push(json!({"token":h.mint,"symbol":symbol,"balance_raw":h.raw.to_string(),"decimals":h.decimals,"amount":h.amount(),"estimated_value_usd":usd,"token_2022":h.token_2022.then_some(true)}));
        }
        if unpriced > 0 {
            turn.gap(format!(
                "{unpriced} holdings have no price; their value is unknown, not zero"
            ));
        }
    }
    let (native, sol_usd) = if first {
        let (lamports, price) =
            tokio::join!(rpc.native_balance(wallet), Jupiter::new(rt, turn).sol_usd());
        let native = match lamports {
            Ok(v) => Some(v as f64 / 1e9),
            Err(fail) => {
                turn.gap(format!("SOL balance unavailable: {}", fail.message()));
                None
            }
        };
        (native, price.ok())
    } else {
        (None, None)
    };
    let mut out = finish(rt, turn, page, limit);
    out["wallet"] = json!(wallet);
    turn.used(Source::Rpc);
    out["native_sol"] = json!(native);
    out["sol_usd"] = json!(sol_usd.map(|p| crate::view::sig(p, 4)));
    out["value_policy"] = json!(
        "Token balances from Solana RPC, both token programs; dust and unpriced coins included when min_value_usd is 0. Values are GMGN prices (stables at $1), not cost basis. Use dottie_pnl for a coin's round."
    );
    Ok(out)
}

fn provider_next(data: &Value, key: &str) -> Result<Option<Value>, Fail> {
    match &data[key] {
        Value::Null => Ok(None),
        Value::String(s) if s.is_empty() => Ok(None),
        Value::String(s) if s.len() <= 2048 => Ok(Some(json!(s))),
        _ => Err(Fail::NoData("provider page cursor is invalid".into())),
    }
}
