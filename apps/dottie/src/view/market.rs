//! Market-shaped replies: the trade tape, charts and exit quotes.
use super::{MAX_REPLY, int, min_ago, pct, price, ratio, series, sig, size, usd};
use crate::domain::amount;
use crate::domain::chart::Chart;
use crate::domain::exit::{RoundTrip, Sale};
use crate::domain::signals::WalletKind;
use crate::domain::trades::Tape;
use crate::providers::jupiter::Quote;
use crate::runtime::{Fail, Turn};
use serde_json::{Map, Value, json};

pub fn tape(token: &str, t: &Tape, limit_note: Option<&str>) -> Value {
    let mut out = Map::new();
    out.insert("symbol".into(), super::label(t.symbol.as_deref(), 20));
    out.insert("token".into(), json!(token));
    for (w, f) in &t.windows {
        let w = w.label();
        out.insert(format!("buy_{w}_usd"), usd(f.buy_usd));
        out.insert(format!("sell_{w}_usd"), usd(f.sell_usd));
        out.insert(
            format!("net_{w}_usd"),
            usd(f.buy_usd.zip(f.sell_usd).map(|(b, s)| b - s)),
        );
        out.insert(format!("buys_{w}"), int(f.buys));
        out.insert(format!("sells_{w}"), int(f.sells));
    }
    let largest: Vec<Value> = t
        .largest
        .iter()
        .map(|r| {
            let (kind, name) = match &r.tag {
                Some((WalletKind::Kol, name)) => (Some("kol"), name.clone()),
                Some((WalletKind::Smart, name)) => (Some("smart"), name.clone()),
                None => (None, None),
            };
            json!({
                "side": r.side,
                "usd": usd(r.usd),
                "wallet": r.wallet,
                "min_ago": min_ago(r.at),
                "tag": kind,
                "name": name,
                "closed_position": r.closed_position.then_some(true),
            })
        })
        .collect();
    out.insert("labelled_trades".into(), json!(largest));
    out.insert("feeds_cover_min".into(), json!(t.feeds_cover_min));
    out.insert("top_traders_sold_usd".into(), usd(t.top_traders_sold_usd));
    out.insert(
        "top_seller_share_of_top_trader_sells_pct".into(),
        pct(t.top_seller_share_pct),
    );
    if let Some(note) = limit_note {
        out.insert("note".into(), json!(note));
    }
    Value::Object(out)
}

pub fn chart(token: &str, c: &Chart, turn: &Turn) -> Value {
    let b = &c.bars;
    let f = &c.facts;
    let t0 = b.t[0];
    let covered_h = (b.t.last().unwrap_or(&t0) - t0 + c.step_s) as f64 / 3600.0;
    let digits3 = |s: &[Option<f64>]| series(s.iter().copied(), 3);
    let mut out = json!({
        "token": token,
        "range": c.range.label(),
        "candle": c.candle.label(),
        "bars": b.t.len(),
        "covered_h": sig(covered_h, 3),
        "facts": {
            "open_usd": price(f.open),
            "last_usd": price(f.last),
            "change_pct": pct(f.change_pct),
            "high_usd": price(f.high),
            "low_usd": price(f.low),
            "from_high_pct": pct(f.from_high_pct),
            "from_low_pct": pct(f.from_low_pct),
            "volume_usd": usd(f.volume_usd),
            "volume_last_vs_first_third_x": ratio(f.volume_trend_x),
            "low_last_third_vs_first_third_x": ratio(f.lows_trend_x),
        },
        "buy_usd": usd(b.buy_usd),
        "sell_usd": usd(b.sell_usd),
        "t0": t0,
        "t_min": series(b.t.iter().map(|x| Some(((x - t0) / 60) as f64)), 15),
        "h": digits3(&b.h),
        "l": digits3(&b.l),
        "c": digits3(&b.c),
        "v_usd": digits3(&b.v),
    });
    if let Some(x) = &c.context {
        out["context"] = json!({
            "range": x.range.label(),
            "t0": x.t0,
            "step_min": x.step_min,
            "c": digits3(&x.closes),
        });
    }
    // Highs and lows go first: the facts keep the extremes.
    if size(&out) > MAX_REPLY - 300
        && let Some(m) = out.as_object_mut()
    {
        m.remove("h");
        m.remove("l");
        turn.gap("per-bar highs and lows left out to keep the reply short");
    }
    out
}

fn leg(result: &Result<Quote, Fail>) -> Value {
    match result {
        Ok(q) => json!({
            "status": "quoted",
            "route": q.route,
            "price_impact_pct": pct(q.price_impact_pct),
        }),
        Err(Fail::NoRoute(why)) => json!({ "status": "no_route", "why": why }),
        Err(f) => json!({
            "status": "unavailable",
            "why": f.message(),
            "retry_after_s": f.retry_after_s(),
        }),
    }
}

pub fn round_trip(token: &str, r: &RoundTrip, sol_usd: Option<f64>) -> Value {
    let sol_in: Option<f64> = r.sol_in.parse().ok();
    let bought = r.buy.as_ref().ok().map(|q| q.out_amount.as_str());
    json!({
        "token": token,
        "mode": "round_trip",
        "sol_in": r.sol_in,
        "usd_in": usd(sol_usd.zip(sol_in).map(|(p, e)| p * e)),
        "tokens_bought": bought
            .and_then(|b| b.parse::<u128>().ok())
            .map(|b| amount::format(b, r.decimals)),
        "sol_back": r.sol_back.map(|b| sig(b, 4)),
        "loss_pct": pct(r.loss_pct),
        "result": r.result,
        "buy": leg(&r.buy),
        "sell": r.sell.as_ref().map(leg),
        "note": "Jupiter quotes at current prices, not fills; network fees and a Token-2022 transfer fee are not in loss_pct",
    })
}

pub fn sale(token: &str, s: &Sale, slippage_bps: u32) -> Value {
    let tokens = amount::format(s.tokens_in, s.decimals);
    let mut out = json!({
        "token": token,
        "mode": "sell",
        "tokens_in": tokens,
        "slippage_bps": slippage_bps,
        "value_usd": usd(s.value_usd),
        "sol_out": s.sol_out.map(|o| sig(o, 4)),
        "sol_min_out": s.sol_min_out.map(|o| sig(o, 4)),
        "loss_pct": pct(s.loss_pct),
        "result": s.result,
        "sell": leg(&s.sell),
        "note": "a Jupiter quote, not a fill; loss_pct is SOL out vs the tokens' value at the current price",
    });
    if s.sell.is_ok() {
        out["stage"] = super::direct::stage(token, "SOL", &tokens, slippage_bps);
    }
    out
}
