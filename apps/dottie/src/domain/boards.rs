//! Discovery boards. GMGN ranks by attention from labelled wallets and
//! filters rugs, honeypots and wash trading server-side, which is what a
//! trader means by "what's good".
use super::market::{Stage, is_base_asset, known, launchpad, stage};
use crate::providers::gmgn::{Gmgn, Trench, types::BoardRow};
use crate::providers::text;
use crate::runtime::{Fail, Runtime, Turn};
use serde_json::{Map, Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Board {
    /// Moving now, by GMGN's trending rank over `interval`.
    Trending,
    /// Launched in the last few hours.
    New,
    /// On a launchpad curve, close to graduating.
    NearGraduation,
    /// Recently graduated from a curve.
    Graduated,
    /// Deep, liquid coins with the biggest holder base.
    Established,
}

/// How old a coin on the `new` board may be unless the user says.
const NEW_MAX_AGE_H: f64 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    /// The board's own order.
    Rank,
    SmartMoney,
    Kol,
    Change,
    Volume,
    Newest,
    Holders,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interval {
    M5,
    H1,
    H6,
    H24,
}

impl Interval {
    pub fn label(self) -> &'static str {
        match self {
            Interval::M5 => "5m",
            Interval::H1 => "1h",
            Interval::H6 => "6h",
            Interval::H24 => "24h",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Query {
    pub board: Board,
    pub interval: Interval,
    pub sort: Sort,
    pub min_fdv_usd: Option<f64>,
    pub max_fdv_usd: Option<f64>,
    pub min_liquidity_usd: Option<f64>,
    pub max_age_h: Option<f64>,
    pub launchpad: Option<String>,
    pub min_smart: Option<u32>,
    pub min_kol: Option<u32>,
    pub max_top10_pct: Option<f64>,
    pub limit: usize,
}

/// One board row.
#[derive(Clone, Debug, PartialEq)]
pub struct Listing {
    pub token: String,
    pub symbol: Option<String>,
    pub launchpad: Option<String>,
    pub stage: Stage,
    pub pair: Option<String>,
    pub created_at: Option<i64>,
    pub fdv_usd: Option<f64>,
    pub liquidity_usd: Option<f64>,
    /// Volume, change and trade counts over `window`.
    pub window: &'static str,
    pub volume_usd: Option<f64>,
    pub change_pct: Option<f64>,
    pub buys: Option<f64>,
    pub sells: Option<f64>,
    pub net_buy_usd: Option<f64>,
    pub holders: Option<f64>,
    pub top10_pct: Option<f64>,
    pub smart: Option<f64>,
    pub kol: Option<f64>,
    pub snipers: Option<f64>,
    pub bundler_pct: Option<f64>,
    pub dev_pct: Option<f64>,
    pub curve_pct: Option<f64>,
    pub rug_pct: Option<f64>,
    pub ath_fdv_usd: Option<f64>,
    /// Dev's launches and how many graduated (trenches rows only).
    pub dev_launches: Option<(f64, f64)>,
    pub wash_trading: bool,
    pub honeypot: Option<bool>,
    pub creator: Option<String>,
}

pub struct Scanned {
    pub scope: String,
    pub scanned: usize,
    pub rows: Vec<Listing>,
}

pub async fn scan(rt: &Runtime, turn: &Turn, q: &Query) -> Result<Scanned, Fail> {
    let gmgn = Gmgn::new(rt, turn);
    // Base assets, wash-traded coins and client-side filters thin the list,
    // so ask for more than we show.
    let fetch = if q.launchpad.is_some() {
        50
    } else {
        (q.limit * 4).clamp(12, 40) as u32
    };
    let (raw, window, scope) = match q.board {
        // GMGN's trenches "new" section lags; its volume ranking with an age
        // limit finds launches that are actually trading.
        Board::Trending | Board::Established | Board::New => {
            let interval = match q.board {
                Board::Established => Interval::H24,
                Board::New => Interval::H1,
                _ => q.interval,
            };
            let rows = gmgn
                .rank(interval.label(), rank_params(q, interval, fetch))
                .await;
            let scope = match q.board {
                Board::Established => "established coins ($250k+ liquidity) by holders".to_string(),
                Board::New => format!(
                    "launches under {}h old by 1h volume",
                    q.max_age_h.unwrap_or(NEW_MAX_AGE_H)
                ),
                _ => format!("gmgn {} trending by volume", interval.label()),
            };
            (rows, interval.label(), scope)
        }
        Board::NearGraduation | Board::Graduated => {
            let (section, scope) = match q.board {
                Board::NearGraduation => {
                    (Trench::NearCompletion, "launchpad curves near graduation")
                }
                _ => (Trench::Completed, "recent graduates"),
            };
            let rows = gmgn.trenches(section, fetch, trench_filters(q)).await;
            (rows, "24h", scope.to_string())
        }
    };
    let raw = raw?;
    let scanned = raw.len();
    let mut rows: Vec<Listing> = raw
        .into_iter()
        .map(|r| listing(r, window))
        .filter(|l| keep(l, q))
        .collect();
    sort(&mut rows, q.sort, q.board);
    rows.truncate(q.limit);
    Ok(Scanned {
        scope,
        scanned,
        rows,
    })
}

fn rank_params(q: &Query, interval: Interval, fetch: u32) -> Vec<(&'static str, String)> {
    let mut p: Vec<(&'static str, String)> = vec![("limit", fetch.to_string())];
    let order = match q.sort {
        Sort::Rank if q.board == Board::Established => Some("holder_count"),
        // GMGN's own order favours coins that piled up smart and KOL holders
        // weeks ago; dollar volume in the window is where the money is now.
        Sort::Rank => Some("volume"),
        Sort::SmartMoney => Some("smart_degen_count"),
        Sort::Kol => Some("renowned_count"),
        // GMGN sorts by 5m or 1h change only; longer windows sort here.
        Sort::Change => match interval {
            Interval::M5 => Some("change5m"),
            Interval::H1 => Some("change1h"),
            _ => None,
        },
        Sort::Volume => Some("volume"),
        Sort::Newest => Some("creation_timestamp"),
        Sort::Holders => Some("holder_count"),
    };
    if let Some(order) = order {
        p.push(("order_by", order.into()));
        p.push(("direction", "desc".into()));
    }
    let (min_liquidity, min_volume) = match q.board {
        Board::Established => (250_000.0, 50_000.0),
        Board::New => (500.0, 100.0),
        _ => (3_000.0, 500.0),
    };
    p.push((
        "min_liquidity",
        q.min_liquidity_usd.unwrap_or(min_liquidity).to_string(),
    ));
    p.push(("min_volume", min_volume.to_string()));
    push_common(&mut p, q);
    let age_h = match q.board {
        Board::New => Some(q.max_age_h.unwrap_or(NEW_MAX_AGE_H)),
        _ => q.max_age_h,
    };
    if let Some(h) = age_h.filter(|h| *h > 0.0) {
        p.push(("max_created", format!("{}m", (h * 60.0).round() as i64)));
    }
    p
}

fn push_common(p: &mut Vec<(&'static str, String)>, q: &Query) {
    if let Some(v) = q.min_fdv_usd {
        p.push(("min_marketcap", v.to_string()));
    }
    if let Some(v) = q.max_fdv_usd {
        p.push(("max_marketcap", v.to_string()));
    }
    if let Some(v) = q.min_smart {
        p.push(("min_smart_degen_count", v.to_string()));
    }
    if let Some(v) = q.min_kol {
        p.push(("min_renowned_count", v.to_string()));
    }
}

/// Trenches filters are a JSON body; GMGN rejects counts sent as floats.
fn trench_filters(q: &Query) -> Value {
    let mut out = Map::new();
    if let Some(v) = q.min_fdv_usd {
        out.insert("min_marketcap".into(), json!(v));
    }
    if let Some(v) = q.max_fdv_usd {
        out.insert("max_marketcap".into(), json!(v));
    }
    if let Some(v) = q.min_smart {
        out.insert("min_smart_degen_count".into(), json!(v));
    }
    if let Some(v) = q.min_kol {
        out.insert("min_renowned_count".into(), json!(v));
    }
    // Most launches never trade; graduates whose pool was drained aren't
    // tradeable either.
    let (volume, holders, liquidity): (f64, u32, Option<f64>) = match q.board {
        Board::NearGraduation => (300.0, 10, None),
        _ => (5_000.0, 50, Some(5_000.0)),
    };
    out.insert("min_volume_24h".into(), json!(volume));
    out.insert("min_holder_count".into(), json!(holders));
    if let Some(v) = q.min_liquidity_usd.or(liquidity) {
        out.insert("min_liquidity".into(), json!(v));
    }
    if let Some(h) = q.max_age_h.filter(|h| *h > 0.0) {
        out.insert(
            "max_created".into(),
            json!(format!("{}m", (h * 60.0).round() as i64)),
        );
    }
    Value::Object(out)
}

fn listing(r: BoardRow, window: &'static str) -> Listing {
    let pad = launchpad(r.launchpad_platform.as_deref());
    let stage = stage(pad.is_some(), r.launchpad_status.get(), r.progress.get());
    let pair = text(&r.launch_quote_address).and_then(known);
    let supply_mcap = r.market_cap.get().or(r.usd_market_cap.get());
    Listing {
        token: r.address.to_string(),
        symbol: r.symbol,
        curve_pct: (stage == Stage::Curve)
            .then(|| r.progress.get().map(|p| p * 100.0))
            .flatten(),
        launchpad: pad,
        stage,
        pair: pair.map(|p| p.symbol.clone()),
        created_at: r.creation_timestamp.ts(),
        fdv_usd: supply_mcap,
        liquidity_usd: r.liquidity.get(),
        window,
        volume_usd: r.volume.get().or(r.volume_24h.get()),
        change_pct: r.price_change_percent.get(),
        buys: r.buys.get().or(r.buys_24h.get()),
        sells: r.sells.get().or(r.sells_24h.get()),
        net_buy_usd: r.net_buy_24h.get(),
        holders: r.holder_count.get(),
        top10_pct: r.top_10_holder_rate.pct(),
        smart: r.smart_degen_count.get(),
        kol: r.renowned_count.get(),
        snipers: r.sniper_count.get(),
        bundler_pct: r.bundler_rate.pct().or(r.bundler_trader_amount_rate.pct()),
        dev_pct: r.creator_balance_rate.pct(),
        rug_pct: r.rug_ratio.pct(),
        ath_fdv_usd: r.history_highest_market_cap.get(),
        dev_launches: r
            .creator_created_count
            .get()
            .zip(r.creator_created_open_count.get()),
        wash_trading: r.is_wash_trading.get() == Some(true),
        honeypot: r.is_honeypot.get(),
        creator: text(&r.creator).map(str::to_string),
    }
}

/// Rows a trader should never see on a board, plus the user's filters
/// GMGN can't apply server-side.
fn keep(l: &Listing, q: &Query) -> bool {
    !is_base_asset(&l.token)
        && !l.wash_trading
        && l.honeypot != Some(true)
        && !l.rug_pct.is_some_and(|r| r >= 50.0)
        && q.launchpad
            .as_deref()
            .is_none_or(|p| l.launchpad == launchpad(Some(p)))
        && q.max_top10_pct
            .is_none_or(|max| l.top10_pct.is_some_and(|t| t <= max))
        // GMGN applies these server-side; checked again in case it didn't.
        && q.min_fdv_usd.is_none_or(|min| l.fdv_usd.is_some_and(|f| f >= min))
        && q.max_fdv_usd.is_none_or(|max| l.fdv_usd.is_some_and(|f| f <= max))
        && q.min_smart.is_none_or(|min| l.smart.is_some_and(|n| n >= min as f64))
        && q.min_kol.is_none_or(|min| l.kol.is_some_and(|n| n >= min as f64))
}

fn sort(rows: &mut [Listing], sort: Sort, board: Board) {
    let key = |l: &Listing| -> f64 {
        match (sort, board) {
            (Sort::Rank, Board::NearGraduation) => l.curve_pct.unwrap_or(0.0),
            (Sort::Rank, Board::Graduated) => l.net_buy_usd.unwrap_or(0.0),
            (Sort::Rank, _) => 0.0,
            (Sort::SmartMoney, _) => l.smart.unwrap_or(0.0),
            (Sort::Kol, _) => l.kol.unwrap_or(0.0),
            (Sort::Change, _) => l.change_pct.unwrap_or(f64::MIN),
            (Sort::Volume, _) => l.volume_usd.unwrap_or(0.0),
            (Sort::Newest, _) => l.created_at.unwrap_or(0) as f64,
            (Sort::Holders, _) => l.holders.unwrap_or(0.0),
        }
    };
    if sort == Sort::Rank && matches!(board, Board::Trending | Board::Established | Board::New) {
        return;
    }
    rows.sort_by(|a, b| key(b).total_cmp(&key(a)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(name: &str, path: &[&str]) -> Vec<BoardRow> {
        let text = std::fs::read_to_string(format!(
            "{}/src/fixtures/gmgn/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let mut v: Value = serde_json::from_str(&text).unwrap();
        for key in path {
            v = v[*key].take();
        }
        serde_json::from_value(v).unwrap()
    }

    fn query(board: Board) -> Query {
        Query {
            board,
            interval: Interval::H1,
            sort: Sort::Rank,
            min_fdv_usd: None,
            max_fdv_usd: None,
            min_liquidity_usd: None,
            max_age_h: None,
            launchpad: None,
            min_smart: None,
            min_kol: None,
            max_top10_pct: None,
            limit: 5,
        }
    }

    #[test]
    fn trending_rows_read_tags_and_drop_base_assets() {
        let listings: Vec<Listing> = rows("rank_1h", &["data", "data", "rank"])
            .into_iter()
            .map(|r| listing(r, "1h"))
            .collect();
        assert!(listings.len() >= 10);
        let pumped = listings
            .iter()
            .find(|l| l.launchpad.as_deref() == Some("pump.fun"))
            .unwrap();
        assert!(pumped.smart.is_some() && pumped.fdv_usd.is_some());
        assert!(crate::providers::pubkey(&pumped.token).is_ok());
        let mut q = query(Board::Trending);
        assert!(
            listings
                .iter()
                .filter(|l| keep(l, &q))
                .all(|l| !is_base_asset(&l.token))
        );
        q.launchpad = Some("Pump.fun".into());
        assert!(
            listings
                .iter()
                .filter(|l| keep(l, &q))
                .all(|l| l.launchpad.as_deref() == Some("pump.fun"))
        );
    }

    #[test]
    fn curve_rows_carry_progress() {
        let listings: Vec<Listing> = rows("trenches", &["data", "near_completion"])
            .into_iter()
            .map(|r| listing(r, "24h"))
            .collect();
        assert!(!listings.is_empty());
        assert!(listings.iter().all(|l| l.stage == Stage::Curve));
        assert!(
            listings
                .iter()
                .all(|l| l.curve_pct.is_some_and(|p| p < 100.0))
        );
    }

    #[test]
    fn params_carry_bands_and_sorts() {
        let mut q = query(Board::Trending);
        q.sort = Sort::SmartMoney;
        q.min_fdv_usd = Some(50_000.0);
        q.max_fdv_usd = Some(1_000_000.0);
        q.max_age_h = Some(6.0);
        let p = rank_params(&q, Interval::H1, 20);
        assert!(p.contains(&("order_by", "smart_degen_count".into())));
        assert!(p.contains(&("min_marketcap", "50000".into())));
        assert!(p.contains(&("max_created", "360m".into())));
        // The new board defaults to six hours old.
        let p = rank_params(&query(Board::New), Interval::H1, 20);
        assert!(p.contains(&("max_created", "360m".into())));
        let mut graduated = query(Board::Graduated);
        graduated.max_age_h = Some(2.0);
        let t = trench_filters(&graduated);
        assert_eq!(t["max_created"], "120m");
        assert!(t["min_holder_count"].is_u64());
    }
}
