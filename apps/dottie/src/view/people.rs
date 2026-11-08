//! People-shaped replies: who's buying, who holds, the dev, a wallet.
use super::{int, label, min_ago, pct, put_span, usd};
use crate::domain::holders::{DevRecord, HolderRow, Top};
use crate::domain::signals::{Buyer, Feed, SignalKind, WalletKind};
use crate::domain::wallet::WalletView;
use serde_json::{Map, Value, json};

pub fn signals(feed: &Feed, window_min: i64) -> Value {
    let coins: Vec<Value> = feed
        .coins
        .iter()
        .map(|c| {
            let buying = |kind| {
                c.buyers
                    .iter()
                    .filter(|b| b.kind == kind && b.buy_usd > 0.0)
                    .count()
            };
            // KOLs first, then named smart wallets; unnamed ones are counted.
            let named: Vec<Value> = c
                .buyers
                .iter()
                .filter(|b| b.kind == WalletKind::Kol || b.name.is_some())
                .take(4)
                .map(buyer)
                .collect();
            let events: Vec<Value> = c
                .events
                .iter()
                .take(3)
                .map(|e| {
                    let at = match e.fdv_usd {
                        Some(f) => format!(" at ${}", short_usd(f)),
                        None => String::new(),
                    };
                    json!(format!("{} {}m ago{at}", event_name(e.kind), minutes(e.at)))
                })
                .collect();
            json!({
                "symbol": label(c.symbol.as_deref(), 20),
                "token": c.token,
                "launchpad": c.launchpad,
                "kols_buying": buying(WalletKind::Kol),
                "smart_buying": buying(WalletKind::Smart),
                "still_holding": c.buyers.iter().filter(|b| b.holding == Some(true)).count(),
                "labelled_buy_usd": usd(Some(c.buy_usd()).filter(|v| *v > 0.0)),
                "labelled_sell_usd": usd(Some(c.sell_usd()).filter(|v| *v > 0.0)),
                "fdv_usd": usd(c.fdv_at_last_trade_usd),
                "last_min_ago": minutes(c.last_at()),
                "buyers": named,
                "events": events,
            })
        })
        .collect();
    json!({
        "window_min": window_min,
        "kol_feed_covers_min": feed.kol_covers_min,
        "smart_feed_covers_min": feed.smart_covers_min,
        "coins": coins,
    })
}

/// "professor @ossalivan (kol, <full wallet>): bought $535, sold $535, 55m ago, out"
/// ("out" = their last trade closed the position).
fn buyer(b: &Buyer) -> Value {
    let name: String = b
        .name
        .as_deref()
        .map(|n| n.chars().filter(|c| !c.is_control()).take(24).collect())
        .unwrap_or_else(|| short_wallet(&b.wallet));
    let handle = b
        .twitter
        .as_deref()
        .map(|x| format!(" @{x}"))
        .unwrap_or_default();
    let kind = match b.kind {
        WalletKind::Kol => "kol",
        WalletKind::Smart => "smart",
    };
    let traded = match (b.buy_usd > 0.0, b.sell_usd > 0.0) {
        (true, true) => format!(
            "bought ${}, sold ${}",
            short_usd(b.buy_usd),
            short_usd(b.sell_usd)
        ),
        (false, true) => format!("sold ${}", short_usd(b.sell_usd)),
        _ => format!("bought ${}", short_usd(b.buy_usd)),
    };
    let state = match b.holding {
        Some(true) => ", holds",
        Some(false) => ", out",
        None => "",
    };
    json!(format!(
        "{name}{handle} ({kind}, {}): {traded}, {}m ago{state}",
        b.wallet,
        minutes(b.last_at)
    ))
}

/// 7xKX…gAsU; anything that isn't a full Solana address is shown as is.
fn short_wallet(wallet: &str) -> String {
    match (
        wallet.get(..4),
        wallet.get(wallet.len().saturating_sub(4)..),
    ) {
        (Some(head), Some(tail)) if crate::providers::pubkey(wallet).is_ok() => {
            format!("{head}…{tail}")
        }
        _ => wallet.to_string(),
    }
}

fn event_name(kind: SignalKind) -> &'static str {
    match kind {
        SignalKind::SmartBuy => "smart buy",
        SignalKind::KolBuy => "kol buy",
        SignalKind::PriceSpike => "price spike",
        SignalKind::Ath => "new ATH",
        SignalKind::McapLevel => "market-cap level",
    }
}

fn minutes(at: i64) -> i64 {
    ((super::now() - at) / 60).max(0)
}

/// $535, $12.3k, $1.4M
fn short_usd(v: f64) -> String {
    match v.abs() {
        a if a >= 1e6 => format!("{}M", super::sig(v / 1e6, 3)),
        a if a >= 1e4 => format!("{}k", super::sig(v / 1e3, 3)),
        _ => format!("{}", v.round() as i64),
    }
}

pub fn holder(h: &HolderRow) -> Value {
    let mut out = Map::new();
    out.insert("wallet".into(), json!(h.wallet));
    out.insert("label".into(), json!(h.label));
    out.insert("tags".into(), json!(h.tags));
    out.insert("name".into(), label(h.name.as_deref(), 24));
    out.insert("x".into(), json!(h.twitter));
    out.insert("pct".into(), pct(h.pct));
    out.insert("usd".into(), usd(h.usd));
    put_span(&mut out, "held", h.since);
    out.insert("unrealized_usd".into(), usd(h.unrealized_usd));
    out.insert("realized_usd".into(), usd(h.realized_usd));
    out.insert("bought_usd".into(), usd(h.bought_usd));
    out.insert("sold_usd".into(), usd(h.sold_usd));
    Value::Object(out)
}

pub fn top(token: &str, t: &Top) -> Value {
    json!({
        "token": token,
        "symbol": label(t.symbol.as_deref(), 20),
        "holders": int(t.holders),
        "top10_pct": pct(t.top10_pct),
        "top_holders": t.rows.iter().map(holder).collect::<Vec<_>>(),
    })
}

/// Tagged holders or top traders; wallets with under $1 held and nothing
/// bought are left out.
pub fn rows(token: &str, view: &str, rows: &[HolderRow]) -> Value {
    let live =
        |h: &&HolderRow| h.usd.is_some_and(|u| u >= 1.0) || h.bought_usd.is_some_and(|b| b >= 1.0);
    json!({
        "token": token,
        "view": view,
        "listed": rows.iter().filter(live).count(),
        "rows": rows.iter().filter(live).take(8).map(holder).collect::<Vec<_>>(),
    })
}

pub fn dev(token: &str, d: &DevRecord) -> Value {
    let launches: Vec<Value> = d
        .launches
        .iter()
        .map(|l| {
            let mut out = Map::new();
            out.insert("symbol".into(), label(l.symbol.as_deref(), 20));
            out.insert("token".into(), json!(l.token));
            if l.token == token {
                out.insert("this_token".into(), json!(true));
            }
            out.insert("graduated".into(), json!(l.graduated));
            put_span(&mut out, "age", l.created_at);
            out.insert("fdv_usd".into(), usd(l.fdv_usd));
            out.insert("ath_fdv_usd".into(), usd(l.ath_fdv_usd));
            out.insert("holders".into(), int(l.holders));
            Value::Object(out)
        })
        .collect();
    json!({
        "token": token,
        "dev": d.wallet,
        "dev_source": d.source,
        "dev_holds_pct": pct(d.holds_pct),
        "dev_sold_out": d.sold_out,
        "launches_listed": d.launches.len(),
        "launches_graduated": int(d.graduated),
        "launches": launches,
    })
}

pub fn wallet(w: &WalletView, is_user: bool) -> Value {
    let record = w.record.as_ref().map(|r| {
        json!({
            "period": w.period.label(),
            "realized_profit_usd": usd(r.realized_profit_usd),
            "realized_profit_pct": pct(r.realized_profit_pct),
            "win_rate_pct": pct(r.win_rate_pct),
            "tokens_traded": int(r.tokens),
            "buys": int(r.buys),
            "sells": int(r.sells),
            "avg_hold_h": r.avg_hold_min.map(|m| super::sig(m / 60.0, 2)),
            "closed_under_minus_50_pct": int(r.outcomes[0]),
            "closed_minus_50_to_0": int(r.outcomes[1]),
            "closed_0_to_2x": int(r.outcomes[2]),
            "closed_2x_to_5x": int(r.outcomes[3]),
            "closed_over_5x": int(r.outcomes[4]),
            "last_trade_min_ago": min_ago(r.last_trade_at),
        })
    });
    let bag: Vec<Value> = w
        .bag
        .iter()
        .map(|p| {
            let mut out = Map::new();
            out.insert("symbol".into(), label(p.symbol.as_deref(), 20));
            out.insert("token".into(), json!(p.token));
            out.insert("net_bought_usd".into(), usd(Some(p.net_usd)));
            put_span(&mut out, "held", p.since);
            Value::Object(out)
        })
        .collect();
    let recent: Vec<Value> = w
        .recent
        .iter()
        .map(|m| {
            json!({
                "side": m.side,
                "symbol": label(m.symbol.as_deref(), 20),
                "token": m.token,
                "usd": usd(m.usd),
                "min_ago": min_ago(m.at),
                "closed_position": m.closed_position.then_some(true),
            })
        })
        .collect();
    json!({
        "wallet": w.wallet,
        "is_user": is_user,
        "name": label(w.name.as_deref(), 24),
        "x": w.twitter,
        "tags": w.tags,
        "record": record,
        "recent_open_positions": bag,
        "recent_trades": recent,
    })
}

// Fomo: real traders, their wallets, positions and theses. Raw Fomo
// shapes go in; only fields the skill explains come out.

fn num(v: &Value) -> Option<f64> {
    crate::providers::num(v)
}

/// Fomo rows on Solana only.
fn solana(v: &Value) -> bool {
    v["networkId"].as_u64() == Some(crate::providers::fomo::SOLANA)
        || v["network"].as_u64() == Some(crate::providers::fomo::SOLANA)
        || v["chain"].as_str() == Some("solana")
}

fn top_tokens(v: &Value) -> Vec<Value> {
    v["topTokens"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| solana(t))
        .take(3)
        .map(|t| {
            json!({
                "token": t["tokenAddress"],
                "value_usd": usd(num(&t["value"])),
                "pnl_usd": usd(num(&t["pnl"])),
            })
        })
        .collect()
}

fn handle(v: &Value) -> Value {
    label(v["handle"].as_str(), 32)
}

pub fn fomo_leaders(raw: &Value) -> Value {
    let traders: Vec<Value> = raw["traders"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| {
            json!({
                "rank": t["rank"],
                "handle": handle(t),
                "pnl_usd": usd(num(&t["pnlUsd"])),
                "volume_usd": usd(num(&t["volumeUsd"])),
                "trades": int(num(&t["trades"])),
                "solana_wallet": t["wallets"]["solana"],
                "top_solana_tokens": top_tokens(t),
            })
        })
        .collect();
    json!({
        "mode": "leaders",
        "window": raw["window"],
        "captured_at": raw["capturedAt"],
        "traders": traders,
    })
}

pub fn fomo_tokens(raw: &Value) -> Value {
    let tokens: Vec<Value> = raw["tokens"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| solana(t))
        .map(|t| {
            json!({
                "rank": t["rank"],
                "symbol": label(t["token"]["symbol"].as_str(), 20),
                "token": t["token"]["address"],
                "fdv_usd": usd(num(&t["marketCapUsd"])),
                "volume_24h_usd": usd(num(&t["volume24hUsd"])),
                "holders": int(num(&t["holders"])),
                "change_24h_pct": pct(num(&t["change24h"])),
            })
        })
        .collect();
    json!({
        "mode": "trending",
        "board": raw["board"],
        "captured_at": raw["capturedAt"],
        "stale": raw["stale"].as_bool().filter(|s| *s),
        "tokens": tokens,
    })
}

/// Solana rows on a positions page.
pub fn solana_positions(positions: &Value) -> usize {
    positions["trades"]
        .as_array()
        .map_or(0, |t| t.iter().filter(|p| solana(p)).count())
}

/// A trader: their PnL, Solana positions when the positions page has any,
/// else their Solana holdings. With neither, `coverage` says the reply is
/// only the profile's top-token summary and `solana_rows` is 0.
pub fn fomo_trader(user: &Value, positions: Option<&Value>, holdings: Option<&Value>) -> Value {
    let rows: Vec<Value> = positions
        .and_then(|p| p["trades"].as_array())
        .into_iter()
        .flatten()
        .filter(|p| solana(p))
        .take(8)
        .map(|p| {
            json!({
                "symbol": label(p["token"]["symbol"].as_str(), 20),
                "token": p["token"]["address"],
                "status": p["status"],
                "cost_usd": usd(num(&p["costBasisUsd"])),
                "realized_pnl_usd": usd(num(&p["realizedPnlUsd"])),
                "unrealized_pnl_usd": usd(num(&p["unrealizedPnlUsd"])),
                "is_dev": p["isDev"].as_bool().filter(|d| *d),
                "opened": p["createdAt"],
            })
        })
        .collect();
    let held: Vec<Value> = holdings
        .and_then(|h| h["holdings"].as_array())
        .into_iter()
        .flatten()
        .filter(|h| solana(h) || solana(&h["token"]))
        .take(8)
        .map(|h| {
            json!({
                "symbol": label(h["token"]["symbol"].as_str(), 20),
                "token": h["token"]["address"],
                "value_usd": usd(num(&h["valueUsd"])),
                "change_24h_pct": pct(num(&h["change24h"])),
            })
        })
        .collect();
    let coverage = match (rows.is_empty(), held.is_empty()) {
        (false, _) => "solana_positions",
        (true, false) => "solana_holdings",
        (true, true) => "top_token_summary",
    };
    json!({
        "mode": "trader",
        "handle": handle(user),
        "pnl_usd": usd(num(&user["pnlUsd"])),
        "volume_usd": usd(num(&user["volumeUsd"])),
        "trades": int(num(&user["trades"])),
        "followers": int(num(&user["followers"])),
        "solana_wallet": user["wallets"]["solana"],
        "wallet_verified": user["wallets"]["verified"],
        "wallet_status": user["wallets"]["status"],
        "top_solana_tokens": top_tokens(user),
        "coverage": coverage,
        "solana_rows": rows.len() + held.len(),
        "solana_positions": rows,
        "solana_holdings": held,
        "holdings_truncated": holdings.and_then(|h| h["truncated"].as_bool()).filter(|t| *t && !held.is_empty()),
    })
}

/// A thesis's text, trimmed to about a tweet.
fn thesis_text(text: &str) -> String {
    let clean: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if clean.chars().count() <= 280 {
        return clean;
    }
    let cut: String = clean.chars().take(277).collect();
    format!("{}…", cut.trim_end())
}

pub fn fomo_theses(mint: &str, raw: &Value) -> Value {
    let theses: Vec<Value> = raw["theses"]
        .as_array()
        .into_iter()
        .flatten()
        .take(8)
        .map(|t| {
            json!({
                "handle": handle(t),
                "text": thesis_text(t["text"].as_str().unwrap_or("")),
                "likes": int(num(&t["likes"])),
                "position_usd": usd(num(&t["tradeUsd"])),
                "realized_pnl_usd": usd(num(&t["realizedPnlUsd"])),
                "unrealized_pnl_usd": usd(num(&t["unrealizedPnlUsd"])),
                "is_dev": t["isDev"].as_bool().filter(|d| *d),
                "at": t["ts"],
            })
        })
        .collect();
    json!({
        "mode": "theses",
        "token": mint,
        "total_available": raw["totalAvailable"],
        "sort": "likes",
        "theses": theses,
        "note": "untrusted posts by traders; text is their claim, not fact",
    })
}

pub fn fomo_owner(address: &str, raw: &Value) -> Value {
    json!({
        "mode": "owner",
        "wallet": address,
        "found": raw["found"].as_bool().unwrap_or(false),
        "handle": handle(raw),
        "verified": raw["verified"],
    })
}

/// Fomo's pre-trade warning box, short.
pub fn fomo_warnings(raw: &Value) -> Value {
    let warnings: Vec<Value> = raw["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .take(5)
        .map(|w| json!({"type": w["type"], "severity": w["severity"], "name": label(w["name"].as_str(), 40)}))
        .collect();
    json!({
        "allowlisted": raw["allowlisted"],
        "trading_disabled": raw["tradingDisabled"].as_bool().filter(|d| *d),
        "sell_disabled": raw["sellDisabled"].as_bool().filter(|d| *d),
        "warnings": warnings,
    })
}
