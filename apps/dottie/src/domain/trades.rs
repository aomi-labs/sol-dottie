//! Who is buying and selling: dollar windows from GMGN (one definition of
//! flow), the latest KOL and smart-money fills in this coin, and how much
//! of the top traders' selling the biggest seller did.
use super::signals::{WalletKind, display_name};
use super::token::{Flow, TokenSnapshot, Window};
use crate::providers::gmgn::Gmgn;
use crate::runtime::{Fail, Runtime, Turn};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TradeRow {
    pub side: Side,
    pub usd: Option<f64>,
    pub wallet: String,
    pub at: Option<i64>,
    /// KOL or smart-money per GMGN's feeds, with a name when known.
    pub tag: Option<(WalletKind, Option<String>)>,
    pub closed_position: bool,
}

pub struct Tape {
    pub symbol: Option<String>,
    pub windows: Vec<(Window, Flow)>,
    /// Labelled fills in this coin, largest first.
    pub largest: Vec<TradeRow>,
    /// Minutes the KOL and smart-money feeds reach back (last 100 trades
    /// on all of Solana each).
    pub feeds_cover_min: Option<i64>,
    /// The top traders' biggest seller's share of their sold dollars.
    pub top_seller_share_pct: Option<f64>,
    /// Sold dollars of the top traders, summed.
    pub top_traders_sold_usd: Option<f64>,
}

pub struct Query {
    pub side: Option<Side>,
    pub min_usd: f64,
    pub limit: usize,
}

pub async fn tape(rt: &Runtime, turn: &Turn, token: &str, q: &Query) -> Result<Tape, Fail> {
    let gmgn = Gmgn::new(rt, turn);
    let (info, kol, smart, traders) = tokio::join!(
        gmgn.token_info(token),
        gmgn.kol_trades(),
        gmgn.smart_trades(),
        gmgn.top_traders(token)
    );
    let snap = match info? {
        info if info.address.trim() == token => TokenSnapshot::from_gmgn(info),
        _ => {
            return Err(Fail::NotFound(
                "no Solana market indexed for this exact mint".into(),
            ));
        }
    };
    let now = crate::view::now();
    let mut oldest: Option<i64> = None;
    let mut rows: Vec<TradeRow> = vec![];
    let mut seen = std::collections::HashSet::new();
    for (feed, kind) in [(kol, WalletKind::Kol), (smart, WalletKind::Smart)] {
        let feed = feed.unwrap_or_else(|fail| {
            turn.gap(format!("{kind:?} feed unavailable: {}", fail.message()).to_lowercase());
            vec![]
        });
        if let Some(t) = feed.iter().filter_map(|t| t.timestamp.ts()).min() {
            oldest = Some(oldest.map_or(t, |o| o.max(t)));
        }
        for t in feed.iter().filter(|t| t.base_address.trim() == token) {
            if !seen.insert((t.transaction_hash.clone(), t.maker.clone())) {
                continue;
            }
            let side = if t.side == "sell" {
                Side::Sell
            } else {
                Side::Buy
            };
            let usd = t.amount_usd.get();
            if q.side.is_some_and(|s| s != side) || usd.unwrap_or(0.0) < q.min_usd {
                continue;
            }
            rows.push(TradeRow {
                side,
                usd,
                wallet: t.maker.trim().to_string(),
                at: t.timestamp.ts(),
                tag: Some((kind, display_name(&t.maker_info))),
                closed_position: t.side == "sell" && t.is_open_or_close.get() == Some(1.0),
            });
        }
    }
    rows.sort_by(|a, b| b.usd.unwrap_or(0.0).total_cmp(&a.usd.unwrap_or(0.0)));
    rows.truncate(q.limit);
    let sold: Vec<f64> = match traders {
        Ok(t) => t
            .iter()
            .filter(|h| h.addr_type.get() != Some(2.0))
            .filter_map(|h| h.sell_volume_cur.get())
            .collect(),
        Err(fail) => {
            turn.gap(format!("top traders unavailable: {}", fail.message()));
            vec![]
        }
    };
    let total: f64 = sold.iter().sum();
    let top = sold.iter().copied().fold(0.0, f64::max);
    Ok(Tape {
        symbol: snap.symbol.clone(),
        windows: snap.flow.clone(),
        largest: rows,
        feeds_cover_min: oldest.map(|t| (now - t) / 60),
        top_seller_share_pct: (total > 0.0).then(|| top / total * 100.0),
        top_traders_sold_usd: (!sold.is_empty()).then_some(total),
    })
}
