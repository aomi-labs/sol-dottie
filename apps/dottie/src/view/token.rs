//! Token-shaped replies: the card, the watcher's flat numbers, board rows
//! and search candidates.
use super::{int, label, pct, price, put_span, usd};
use crate::domain::amount;
use crate::domain::boards::Listing;
use crate::domain::exit::{Probe, RoundTrip};
use crate::domain::find::Candidate;
use crate::domain::market::{is_base_asset, links, lookalike};
use crate::domain::token::{Security, TokenSnapshot, Window, watch_progress};
use serde_json::{Map, Value, json};

fn put(out: &mut Map<String, Value>, key: &str, value: Value) {
    if !value.is_null() {
        out.insert(key.to_string(), value);
    }
}

fn identity(out: &mut Map<String, Value>, s: &TokenSnapshot) {
    put(out, "symbol", label(s.symbol.as_deref(), 20));
    put(out, "token", json!(s.token));
    put(out, "launchpad", json!(s.launchpad));
    put(out, "stage", json!(s.stage));
    put(out, "pair", json!(s.pair));
    put(out, "migrated_to", label(s.migrated_to.as_deref(), 20));
    put_span(out, "age", s.created_at);
}

/// The full card for one token. `fomo` is Fomo's warning box and holder
/// summary.
pub fn card(s: &TokenSnapshot, probe: Option<&Probe>, fomo: Option<Value>) -> Value {
    let mut out = Map::new();
    identity(&mut out, s);
    put(&mut out, "name", label(s.name.as_deref(), 40));
    if let Some(real) = lookalike(&s.token) {
        out.insert(
            "warning".into(),
            json!(format!(
                "mint copies the start and end of the real {real}; this is not it"
            )),
        );
    }
    put(&mut out, "price_usd", price(s.price_usd));
    put(&mut out, "fdv_usd", usd(s.fdv_usd));
    put(&mut out, "liquidity_usd", usd(s.liquidity_usd));
    if is_base_asset(&s.token) {
        out.insert(
            "base_asset".into(),
            json!("a catalog asset (SOL, stable, LST, major or stock token): no meme checks; price and route it with Jupiter."),
        );
        exit_check(&mut out, probe);
        out.insert("links".into(), links(&s.token));
        return Value::Object(out);
    }
    put(&mut out, "volume_24h_usd", usd(s.volume_24h_usd));
    for (w, v) in &s.change_pct {
        put(
            &mut out,
            &format!("change_{}_pct", w.label()),
            pct(Some(*v)),
        );
    }
    for (w, f) in &s.flow {
        if matches!(*w, Window::M5 | Window::H6) {
            continue;
        }
        let w = w.label();
        put(&mut out, &format!("buy_{w}_usd"), usd(f.buy_usd));
        put(&mut out, &format!("sell_{w}_usd"), usd(f.sell_usd));
        put(&mut out, &format!("buys_{w}"), int(f.buys));
        put(&mut out, &format!("sells_{w}"), int(f.sells));
    }
    holder_fields(&mut out, s);
    put(&mut out, "ath_usd", price(s.ath_usd));
    put(&mut out, "from_ath_pct", from_ath(s));
    put(&mut out, "curve_pct", pct(s.curve_pct));
    if let Some(wallet) = &s.creator {
        out.insert(
            "dev".into(),
            json!({
                "wallet": wallet,
                "status": s.dev_status,
                "holds_pct": pct(s.mix.dev_pct),
                "team_holds_pct": pct(s.mix.dev_team_pct),
            }),
        );
    }
    if let Some(sec) = &s.security {
        out.insert("security".into(), security(sec));
    }
    if let Some(r) = &s.rugcheck {
        let risks: Vec<Value> = r
            .risks
            .iter()
            .take(4)
            .map(|k| json!(format!("{} ({})", k.name, k.level)))
            .collect();
        out.insert(
            "rugcheck".into(),
            json!({ "score": r.score.map(|v| super::sig(v, 3)), "lp_locked_pct": pct(r.lp_locked_pct), "risks": risks }),
        );
    }
    if let Some(f) = fomo {
        out.insert("fomo".into(), f);
    }
    exit_check(&mut out, probe);
    let x = s.socials.x();
    let socials = json!({
        "x": x.as_ref().map(|(_, path)| label(Some(path), 60)),
        "x_link": x.as_ref().map(|(kind, _)| kind),
        "x_launches": int(s.socials.x_launches.filter(|n| *n > 1.0)),
        "x_renames": s.socials.x_renames.filter(|n| *n > 0),
        "community_takeover": s.socials.community_takeover.filter(|t| *t),
        "web": label(s.socials.website.as_deref(), 80),
        "tg": label(s.socials.telegram.as_deref(), 60),
    });
    out.insert("socials".into(), socials);
    out.insert("links".into(), links(&s.token));
    Value::Object(out)
}

/// Buy facts only. This is a quote screen, never host execution eligibility.
pub fn trade(
    s: &TokenSnapshot,
    trip: &RoundTrip,
    sol_usd: Option<f64>,
    slippage_bps: u32,
) -> Value {
    let mut out = Map::new();
    identity(&mut out, s);
    out.insert("mode".into(), json!("trade"));
    out.insert("liquidity_usd".into(), usd(s.liquidity_usd));
    out.insert("slippage_bps".into(), json!(slippage_bps));
    out.insert(
        "security".into(),
        s.security
            .as_ref()
            .map(security)
            .unwrap_or_else(|| json!({"honeypot":"unknown"})),
    );
    let warnings = crate::domain::trade::warnings(s.security.as_ref(), trip.result, trip.loss_pct);
    out.insert("checks_passed".into(), json!(warnings.is_empty()));
    out.insert("warnings".into(), json!(warnings));
    out.insert(
        "exit_check".into(),
        super::market::round_trip(&s.token, trip, sol_usd),
    );
    if trip.buy.is_ok()
        && let Ok(lamports) = amount::from_decimal(&trip.sol_in, 9)
    {
        let sol = amount::format(lamports, 9);
        out.insert(
            "stage".into(),
            super::direct::stage("SOL", &s.token, &sol, slippage_bps),
        );
    }
    if let Some(real) = lookalike(&s.token) {
        out.insert(
            "warning".into(),
            json!(format!("mint imitates {real}; verify the exact mint")),
        );
    }
    out.insert("links".into(), links(&s.token));
    Value::Object(out)
}

fn exit_check(out: &mut Map<String, Value>, probe: Option<&Probe>) {
    if let Some(p) = probe {
        out.insert(
            "exit_check".into(),
            json!({
                "sol_in": p.sol_in,
                "loss_pct": pct(p.loss_pct),
                "sell": if p.no_sell_route { json!("no_route") } else { Value::Null },
                "route": p.route,
                "result": p.result,
            }),
        );
    }
}

fn holder_fields(out: &mut Map<String, Value>, s: &TokenSnapshot) {
    put(out, "holders", int(s.holders));
    put(out, "top10_pct", pct(s.top10_pct));
    let m = &s.mix;
    put(out, "snipers_pct", pct(m.snipers_pct));
    put(out, "bundler_trading_pct", pct(m.bundler_trading_pct));
    put(out, "rat_trading_pct", pct(m.rat_trading_pct));
    put(out, "bot_degen_pct", pct(m.bot_degen_pct));
    put(out, "entrapment_trading_pct", pct(m.entrapment_trading_pct));
    put(out, "fresh_wallet_pct", pct(m.fresh_wallet_pct));
    let t = &s.tagged;
    put(out, "smart_wallets", int(t.smart));
    put(out, "kol_wallets", int(t.kol));
    put(out, "sniper_wallets", int(t.snipers));
}

fn from_ath(s: &TokenSnapshot) -> Value {
    match (s.ath_usd, s.price_usd) {
        (Some(a), Some(p)) if a > 0.0 => pct(Some(((p / a).min(1.0) - 1.0) * 100.0)),
        _ => Value::Null,
    }
}

fn security(s: &Security) -> Value {
    let honeypot = match s.honeypot {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    };
    json!({
        "honeypot": honeypot,
        "buy_tax_pct": pct(s.buy_tax_pct),
        "sell_tax_pct": pct(s.sell_tax_pct),
        "mint_renounced": s.mint_renounced,
        "freeze_renounced": s.freeze_renounced,
        "lp_locked": s.lp_locked,
        "transfer_fee_pct": pct(s.transfer_fee_pct),
        "risky_extensions": s.risky_extensions,
        "flags": s.flags,
    })
}

/// Flat numbers for watchers: every field always present when known,
/// `curve_pct` 100 once graduated.
pub fn check(s: &TokenSnapshot) -> Value {
    let flow = |w: Window| s.flow(w).copied().unwrap_or_default();
    let change = |w: Window| s.change_pct.iter().find(|(x, _)| *x == w).map(|(_, v)| *v);
    let (curve_pct, graduated) = watch_progress(s);
    json!({
        "symbol": label(s.symbol.as_deref(), 20),
        "token": s.token,
        "price_usd": price(s.price_usd),
        "fdv_usd": usd(s.fdv_usd),
        "liquidity_usd": usd(s.liquidity_usd),
        "holders": int(s.holders),
        "top10_pct": pct(s.top10_pct),
        "dev_pct": pct(s.mix.dev_pct),
        "snipers_pct": pct(s.mix.snipers_pct),
        "kol_wallets": int(s.tagged.kol),
        "smart_wallets": int(s.tagged.smart),
        "buy_5m_usd": usd(flow(Window::M5).buy_usd),
        "sell_5m_usd": usd(flow(Window::M5).sell_usd),
        "change_1h_pct": pct(change(Window::H1)),
        "curve_pct": pct(curve_pct),
        "graduated": graduated.map(u8::from),
    })
}

/// One board row, about 500 chars pretty-printed.
pub fn listing(l: &Listing) -> Value {
    let mut out = Map::new();
    put(&mut out, "symbol", label(l.symbol.as_deref(), 20));
    put(&mut out, "token", json!(l.token));
    put(&mut out, "launchpad", json!(l.launchpad));
    put(&mut out, "stage", json!(l.stage));
    put(&mut out, "pair", json!(l.pair));
    put_span(&mut out, "age", l.created_at);
    put(&mut out, "fdv_usd", usd(l.fdv_usd));
    put(&mut out, "liquidity_usd", usd(l.liquidity_usd));
    let w = l.window;
    put(&mut out, &format!("volume_{w}_usd"), usd(l.volume_usd));
    put(&mut out, &format!("change_{w}_pct"), pct(l.change_pct));
    put(&mut out, &format!("buys_{w}"), int(l.buys));
    put(&mut out, &format!("sells_{w}"), int(l.sells));
    put(&mut out, "net_buy_24h_usd", usd(l.net_buy_usd));
    put(&mut out, "holders", int(l.holders));
    put(&mut out, "top10_pct", pct(l.top10_pct));
    put(&mut out, "smart_wallets", int(l.smart));
    put(&mut out, "kol_wallets", int(l.kol));
    // Risk fields appear only when GMGN flags something.
    let flagged = |v: Option<f64>| v.filter(|v| *v > 0.0);
    put(&mut out, "sniper_wallets", int(flagged(l.snipers)));
    put(&mut out, "bundler_trading_pct", pct(flagged(l.bundler_pct)));
    put(&mut out, "dev_pct", pct(flagged(l.dev_pct)));
    put(&mut out, "rug_pct", pct(flagged(l.rug_pct)));
    put(&mut out, "curve_pct", pct(l.curve_pct));
    if let (Some(ath), Some(fdv)) = (l.ath_fdv_usd, l.fdv_usd)
        && ath > 0.0
    {
        put(
            &mut out,
            "from_ath_pct",
            pct(Some(((fdv / ath).min(1.0) - 1.0) * 100.0)),
        );
    }
    if let Some((made, graduated)) = l.dev_launches {
        put(&mut out, "dev_launches", int(Some(made)));
        put(&mut out, "dev_graduated", int(Some(graduated)));
    }
    Value::Object(out)
}

pub fn candidate(c: &Candidate) -> Value {
    let mut out = Map::new();
    put(&mut out, "symbol", label(c.symbol.as_deref(), 20));
    put(&mut out, "name", label(c.name.as_deref(), 32));
    put(&mut out, "token", json!(c.token));
    put(
        &mut out,
        "match",
        json!(if c.exact_symbol {
            "exact_symbol"
        } else {
            "partial"
        }),
    );
    put(&mut out, "launchpad", json!(c.launchpad));
    put(&mut out, "stage", json!(c.stage));
    put_span(&mut out, "age", c.created_at);
    put(&mut out, "fdv_usd", usd(c.fdv_usd));
    put(&mut out, "liquidity_usd", usd(c.liquidity_usd));
    put(&mut out, "volume_1h_usd", usd(c.volume_1h_usd));
    put(&mut out, "holders", int(c.holders));
    put(&mut out, "kol_wallets", int(c.kols));
    if c.honeypot == Some(true) {
        put(&mut out, "honeypot", json!("yes"));
    }
    if c.verified {
        put(&mut out, "verified", json!("dottie catalog"));
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::exit::ExitResult;
    use crate::domain::market::Stage;
    use crate::providers::gmgn::types::TokenInfo;
    use crate::view::{MAX_REPLY, reply, size};

    fn fresh() -> TokenSnapshot {
        let text = std::fs::read_to_string(format!(
            "{}/src/fixtures/gmgn/info_fresh.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        let info: TokenInfo = serde_json::from_value(v["data"].clone()).unwrap();
        TokenSnapshot::from_gmgn(info)
    }

    #[test]
    fn widest_card_fits_one_reply() {
        let mut s = fresh();
        s.name = Some("N".repeat(40));
        s.creator = Some("GqEQKo2Xp76X3idMSnE2qBuqTr3rURfLNKJW9yVTVhB7".into());
        s.security = Some(Security {
            honeypot: None,
            buy_tax_pct: Some(1.0),
            sell_tax_pct: Some(1.0),
            mint_renounced: Some(true),
            freeze_renounced: Some(false),
            lp_locked: Some(true),
            transfer_fee_pct: Some(3.0),
            risky_extensions: vec!["permanentDelegate".into(), "transferHook".into()],
            flags: vec!["x".repeat(20); 6],
        });
        s.rugcheck = Some(crate::providers::rugcheck::Report {
            score: Some(51.0),
            risks: vec![
                crate::providers::rugcheck::Risk {
                    name: "r".repeat(40),
                    level: "warn".into()
                };
                8
            ],
            ..Default::default()
        });
        s.socials.website = Some("w".repeat(80));
        let probe = Probe {
            sol_in: "0.1".into(),
            loss_pct: Some(4.2),
            no_sell_route: false,
            route: vec!["Pump.fun Amm".into(), "Whirlpool".into()],
            result: ExitResult::Ok,
        };
        let warnings = json!({"allowlisted": false, "warnings": [{"type": "FEW_HOLDERS", "severity": "MODERATE", "name": "Few holders"}]});
        let turn = crate::runtime::Turn::default();
        turn.gap("rugcheck unavailable (UNAVAILABLE); authorities and transfer fee are gmgn's or unknown");
        let holders: Value = serde_json::from_str(
            &std::fs::read_to_string(format!(
                "{}/src/fixtures/fomo/holders_stonk.json",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap(),
        )
        .unwrap();
        let fomo = crate::view::people::fomo_card(Some(&warnings), Some(&holders));
        let out = reply(card(&s, Some(&probe), fomo), &turn);
        assert!(size(&out) <= MAX_REPLY, "{}", size(&out));
        assert_eq!(out["status"], "partial");
        assert_eq!(out["security"]["honeypot"], "unknown");
        assert_eq!(out["security"]["freeze_renounced"], false);
        assert!(out["links"]["gmgn"].as_str().unwrap().ends_with(&s.token));
        assert!(out["buy_1h_usd"].is_number());
        assert_eq!(out["fomo"]["warnings"][0], "Few holders (MODERATE)");
        assert_eq!(out["fomo"]["traders_holding"], 42178);
        assert_eq!(out["fomo"]["over_1k_in_top100"], 10);
        assert_eq!(out["fomo"]["biggest"][0], "@pointfarmcap $2.21M (+$449k)");
    }

    #[test]
    fn check_reports_curve_100_after_graduation() {
        let mut s = fresh();
        s.stage = Stage::Graduated;
        let out = crate::view::compact(check(&s));
        assert_eq!(out["curve_pct"], 100.0);
        assert_eq!(out["graduated"], 1);
        assert!(out.get("buy_5m_usd").is_some());
        s.stage = Stage::Pool;
        let out = crate::view::compact(check(&s));
        assert!(out.get("graduated").is_none() && out.get("curve_pct").is_none());
    }
}
