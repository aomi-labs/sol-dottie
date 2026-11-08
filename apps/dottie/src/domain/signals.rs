//! Who's buying: GMGN's KOL and smart-money trade feeds and signal events,
//! grouped per coin with the buyers named. The main source of fresh,
//! varied picks.
use super::market::{is_base_asset, launchpad};
use crate::providers::gmgn::{Gmgn, types as gm};
use crate::providers::text;
use crate::runtime::{Fail, Runtime, Turn};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WalletKind {
    Kol,
    Smart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalKind {
    SmartBuy,
    KolBuy,
    PriceSpike,
    Ath,
    McapLevel,
}

impl SignalKind {
    fn of(code: u32) -> Option<Self> {
        Some(match code {
            12 => SignalKind::SmartBuy,
            20 => SignalKind::KolBuy,
            6 => SignalKind::PriceSpike,
            7 => SignalKind::Ath,
            8 => SignalKind::McapLevel,
            _ => return None,
        })
    }
}

/// One labelled wallet's activity in one coin.
#[derive(Clone, Debug, PartialEq)]
pub struct Buyer {
    pub wallet: String,
    pub name: Option<String>,
    pub twitter: Option<String>,
    pub kind: WalletKind,
    pub buy_usd: f64,
    pub sell_usd: f64,
    pub last_at: i64,
    /// False once their latest trade in the feed was a sell that closed
    /// the position.
    pub holding: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub kind: SignalKind,
    pub at: i64,
    pub fdv_usd: Option<f64>,
    pub times: Option<f64>,
}

/// Labelled-wallet attention on one coin.
#[derive(Clone, Debug, PartialEq)]
pub struct Buzz {
    pub token: String,
    pub symbol: Option<String>,
    pub launchpad: Option<String>,
    pub buyers: Vec<Buyer>,
    pub events: Vec<Event>,
    /// FDV at the latest labelled trade (price × supply then).
    pub fdv_at_last_trade_usd: Option<f64>,
    last_trade_at: i64,
    pub holders: Option<f64>,
    pub liquidity_usd: Option<f64>,
    pub creator: Option<String>,
}

impl Buzz {
    pub fn buy_usd(&self) -> f64 {
        self.buyers.iter().map(|b| b.buy_usd).sum()
    }

    pub fn sell_usd(&self) -> f64 {
        self.buyers.iter().map(|b| b.sell_usd).sum()
    }

    pub fn last_at(&self) -> i64 {
        let trades = self.buyers.iter().map(|b| b.last_at);
        let events = self.events.iter().map(|e| e.at);
        trades.chain(events).max().unwrap_or(0)
    }

    /// Buyers of a kind, counting one who already sold out as a third:
    /// a round trip is attention, not conviction.
    fn buying(&self, kind: WalletKind) -> f64 {
        self.buyers
            .iter()
            .filter(|b| b.kind == kind && b.buy_usd > 0.0)
            .map(|b| {
                if b.holding == Some(false) {
                    1.0 / 3.0
                } else {
                    1.0
                }
            })
            .sum()
    }

    /// KOLs count most, then smart wallets, then events and net dollars.
    fn score(&self) -> f64 {
        self.buying(WalletKind::Kol) * 3.0
            + self.buying(WalletKind::Smart)
            + self.events.len() as f64 * 0.5
            + (self.buy_usd() - self.sell_usd()).max(0.0).ln_1p() * 0.1
    }
}

pub struct Query {
    pub window_min: i64,
    pub min_fdv_usd: Option<f64>,
    pub max_fdv_usd: Option<f64>,
    /// Keep coins where labelled wallets are net sellers too.
    pub include_selling: bool,
    pub limit: usize,
}

pub struct Feed {
    pub coins: Vec<Buzz>,
    /// How far back each feed reaches: they hold the last 100 trades.
    pub kol_covers_min: Option<i64>,
    pub smart_covers_min: Option<i64>,
}

pub async fn feed(rt: &Runtime, turn: &Turn, q: &Query) -> Result<Feed, Fail> {
    let gmgn = Gmgn::new(rt, turn);
    let mut band = serde_json::Map::new();
    if let Some(v) = q.min_fdv_usd {
        band.insert("mc_min".into(), v.into());
    }
    if let Some(v) = q.max_fdv_usd {
        band.insert("mc_max".into(), v.into());
    }
    let groups = serde_json::json!([
        merge(serde_json::json!({ "signal_type": [12, 20] }), &band),
        merge(serde_json::json!({ "signal_type": [6, 7, 8] }), &band),
    ]);
    let (kol, smart, signals) =
        tokio::join!(gmgn.kol_trades(), gmgn.smart_trades(), gmgn.signals(groups));
    if let (Err(fail), Err(_), Err(_)) = (&kol, &smart, &signals) {
        return Err(fail.clone());
    }
    for (name, failed) in [
        ("KOL feed", kol.is_err()),
        ("smart-money feed", smart.is_err()),
        ("signal feed", signals.is_err()),
    ] {
        if failed {
            turn.gap(format!("{name} unavailable"));
        }
    }
    let kol = kol.unwrap_or_default();
    let smart = smart.unwrap_or_default();
    let now = crate::view::now();
    let since = now - q.window_min * 60;
    let covers = |trades: &[gm::Trade]| {
        trades
            .iter()
            .filter_map(|t| t.timestamp.ts())
            .min()
            .map(|t| (now - t) / 60)
    };
    let (kol_covers_min, smart_covers_min) = (covers(&kol), covers(&smart));
    let mut coins = group(
        kol.iter()
            .map(|t| (t, WalletKind::Kol))
            .chain(smart.iter().map(|t| (t, WalletKind::Smart))),
        since,
    );
    for s in signals.unwrap_or_default() {
        let (Some(kind), Some(at)) = (SignalKind::of(s.signal_type), s.trigger_at.ts()) else {
            continue;
        };
        if at < since {
            continue;
        }
        let token = s.token_address.to_string();
        let coin = coins.entry(token.clone()).or_insert_with(|| Buzz {
            token,
            symbol: s.data.symbol.clone(),
            launchpad: launchpad(s.data.launchpad_platform.as_deref()),
            buyers: vec![],
            events: vec![],
            fdv_at_last_trade_usd: None,
            last_trade_at: 0,
            holders: None,
            liquidity_usd: None,
            creator: None,
        });
        coin.creator = coin
            .creator
            .take()
            .or(text(&s.data.creator).map(str::to_string));
        coin.holders = coin.holders.or(s.data.holder_count.get());
        coin.liquidity_usd = coin.liquidity_usd.or(s.data.liquidity.get());
        coin.fdv_at_last_trade_usd = coin
            .fdv_at_last_trade_usd
            .or(s.data.market_cap.get())
            .or(s.trigger_mc.get());
        coin.events.push(Event {
            kind,
            at,
            fdv_usd: s.trigger_mc.get(),
            times: s.signal_times.get(),
        });
    }
    let mut coins: Vec<Buzz> = coins
        .into_values()
        .filter(|c| !is_base_asset(&c.token))
        .filter(|c| q.include_selling || c.buy_usd() > c.sell_usd() || !c.events.is_empty())
        .filter(|c| in_band(c.fdv_at_last_trade_usd, q))
        .collect();
    coins.sort_by(|a, b| {
        b.score()
            .total_cmp(&a.score())
            .then(b.last_at().cmp(&a.last_at()))
    });
    coins.truncate(q.limit);
    for coin in &mut coins {
        coin.events.sort_by_key(|e| std::cmp::Reverse(e.at));
        coin.events.dedup_by_key(|e| e.kind);
        coin.buyers.sort_by(|a, b| {
            (b.kind == WalletKind::Kol)
                .cmp(&(a.kind == WalletKind::Kol))
                .then(b.buy_usd.total_cmp(&a.buy_usd))
        });
    }
    Ok(Feed {
        coins,
        kol_covers_min,
        smart_covers_min,
    })
}

/// Labelled wallets seen in the KOL and smart-money feeds, for marking
/// trades and holders elsewhere. Both feeds are cached for 30 s.
pub async fn known_wallets(
    rt: &Runtime,
    turn: &Turn,
) -> HashMap<String, (WalletKind, Option<String>)> {
    let gmgn = Gmgn::new(rt, turn);
    let (kol, smart) = tokio::join!(gmgn.kol_trades(), gmgn.smart_trades());
    let mut out = HashMap::new();
    for (trades, kind) in [(smart, WalletKind::Smart), (kol, WalletKind::Kol)] {
        for t in trades.unwrap_or_default() {
            out.insert(t.maker.to_string(), (kind, display_name(&t.maker_info)));
        }
    }
    out
}

fn group<'t>(
    trades: impl Iterator<Item = (&'t gm::Trade, WalletKind)>,
    since: i64,
) -> HashMap<String, Buzz> {
    let mut coins: HashMap<String, Buzz> = HashMap::new();
    // A wallet tagged both KOL and smart shows up in both feeds.
    let mut seen = std::collections::HashSet::new();
    for (t, kind) in trades {
        let Some(at) = t.timestamp.ts().filter(|at| *at >= since) else {
            continue;
        };
        if !t.transaction_hash.is_empty()
            && !seen.insert((t.transaction_hash.clone(), t.maker.to_string()))
        {
            continue;
        }
        let token = t.base_address.to_string();
        let usd = t.amount_usd.get().unwrap_or(0.0);
        let coin = coins.entry(token.clone()).or_insert_with(|| Buzz {
            token,
            symbol: t.base_token.symbol.clone(),
            launchpad: launchpad(t.base_token.launchpad.as_deref()),
            buyers: vec![],
            events: vec![],
            fdv_at_last_trade_usd: None,
            last_trade_at: 0,
            holders: None,
            liquidity_usd: None,
            creator: text(&t.base_token.creator).map(str::to_string),
        });
        let fdv = t
            .price_usd
            .get()
            .zip(t.base_token.total_supply.get())
            .map(|(p, s)| p * s);
        let maker = t.maker.to_string();
        let buyer = match coin.buyers.iter_mut().find(|b| b.wallet == maker) {
            Some(b) => b,
            None => {
                coin.buyers.push(Buyer {
                    wallet: maker,
                    name: display_name(&t.maker_info),
                    twitter: text(&t.maker_info.twitter_username).map(str::to_string),
                    kind,
                    buy_usd: 0.0,
                    sell_usd: 0.0,
                    last_at: 0,
                    holding: None,
                });
                coin.buyers.last_mut().expect("just pushed")
            }
        };
        if t.side == "buy" {
            buyer.buy_usd += usd;
        } else {
            buyer.sell_usd += usd;
        }
        if at >= buyer.last_at {
            buyer.last_at = at;
            let closed = t.side == "sell" && t.is_open_or_close.get() == Some(1.0);
            buyer.holding = Some(!closed);
        }
        if at >= coin.last_trade_at
            && let Some(f) = fdv
        {
            coin.last_trade_at = at;
            coin.fdv_at_last_trade_usd = Some(f);
        }
    }
    coins
}

pub fn display_name(m: &gm::Maker) -> Option<String> {
    text(&m.twitter_name)
        .or(text(&m.name))
        .or(text(&m.twitter_username))
        .map(|s| s.chars().filter(|c| !c.is_control()).take(24).collect())
}

fn merge(
    mut group: serde_json::Value,
    band: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    if let Some(g) = group.as_object_mut() {
        g.extend(band.clone());
    }
    group
}

fn in_band(fdv: Option<f64>, q: &Query) -> bool {
    match fdv {
        Some(f) => q.min_fdv_usd.is_none_or(|m| f >= m) && q.max_fdv_usd.is_none_or(|m| f <= m),
        None => q.min_fdv_usd.is_none() && q.max_fdv_usd.is_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trades(name: &str) -> Vec<gm::Trade> {
        let text = std::fs::read_to_string(format!(
            "{}/src/fixtures/gmgn/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        serde_json::from_value(v["data"]["list"].clone()).unwrap()
    }

    #[test]
    fn trades_group_per_coin_with_named_buyers() {
        let kol = trades("kol");
        let coins = group(kol.iter().map(|t| (t, WalletKind::Kol)), 0);
        assert!(!coins.is_empty());
        let busiest = coins.values().max_by_key(|c| c.buyers.len()).unwrap();
        assert!(busiest.buyers.iter().any(|b| b.name.is_some()));
        assert!(busiest.buyers.iter().all(|b| b.wallet.len() >= 32));
        // One entry per wallet per coin, mints with case kept.
        let mut wallets: Vec<&str> = busiest.buyers.iter().map(|b| b.wallet.as_str()).collect();
        wallets.sort();
        wallets.dedup();
        assert_eq!(wallets.len(), busiest.buyers.len());
        assert!(coins.keys().all(|k| crate::providers::pubkey(k).is_ok()));
    }

    #[test]
    fn named_buyers_outrank_volume() {
        let buyer = |kind, usd| Buyer {
            wallet: format!("{usd}"),
            name: None,
            twitter: None,
            kind,
            buy_usd: usd,
            sell_usd: 0.0,
            last_at: 1,
            holding: Some(true),
        };
        let coin = |buyers| Buzz {
            token: "mint".into(),
            symbol: None,
            launchpad: None,
            buyers,
            events: vec![],
            fdv_at_last_trade_usd: None,
            last_trade_at: 0,
            holders: None,
            liquidity_usd: None,
            creator: None,
        };
        let two_kols = coin(vec![
            buyer(WalletKind::Kol, 100.0),
            buyer(WalletKind::Kol, 50.0),
        ]);
        let one_whale = coin(vec![buyer(WalletKind::Smart, 50_000.0)]);
        assert!(two_kols.score() > one_whale.score());
    }
}
