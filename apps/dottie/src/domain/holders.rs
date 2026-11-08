//! Who holds a coin: GMGN's holder lists with tags, names and PnL (pools
//! labelled, not counted as traders), KOL and smart wallets marked from
//! the trade feeds, and the dev's launch record.
use super::signals::{WalletKind, known_wallets};
use super::token::{Depth, snapshot};
use crate::providers::gmgn::{Gmgn, types as gm};
use crate::providers::text;
use crate::runtime::{Fail, Runtime, Turn};

/// Accounts that hold supply without being a trader.
const KNOWN: &[(&str, &str)] = &[
    (
        "5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1",
        "raydium pool authority",
    ),
    ("1nc1nerator11111111111111111111111111111111", "burn"),
];

/// GMGN wallet tags worth showing, in our words.
fn tag(raw: &str) -> Option<&'static str> {
    Some(match raw {
        "renowned" | "kol" => "kol",
        "smart_degen" => "smart",
        "sniper" => "sniper",
        "bundler" => "bundler",
        "whale" => "whale",
        "fresh_wallet" => "fresh",
        "rat_trader" => "rat",
        "sandwich_bot" | "bot" => "bot",
        "dev" | "creator" => "dev",
        "insider" => "insider",
        "fomo" => "fomo_user",
        _ => return None,
    })
}

pub fn tags(lists: &[&[String]]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = lists
        .iter()
        .flat_map(|l| l.iter())
        .filter_map(|t| tag(t))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

pub fn contract_label(address: &str) -> Option<&'static str> {
    KNOWN.iter().find(|(a, _)| *a == address).map(|(_, l)| *l)
}

#[derive(Clone, Debug, PartialEq)]
pub struct HolderRow {
    pub wallet: String,
    /// A known contract (pool, locker, curve) or "dev".
    pub label: Option<&'static str>,
    pub tags: Vec<&'static str>,
    pub name: Option<String>,
    pub twitter: Option<String>,
    pub pct: Option<f64>,
    pub usd: Option<f64>,
    pub since: Option<i64>,
    pub unrealized_usd: Option<f64>,
    pub realized_usd: Option<f64>,
    pub bought_usd: Option<f64>,
    pub sold_usd: Option<f64>,
}

pub struct Top {
    pub symbol: Option<String>,
    pub holders: Option<f64>,
    pub top10_pct: Option<f64>,
    pub rows: Vec<HolderRow>,
}

/// The biggest holders, with pools labelled and KOL or smart wallets
/// marked from the trade feeds.
pub async fn top(rt: &Runtime, turn: &Turn, token: &str) -> Result<Top, Fail> {
    let (list, snap, known) = tokio::join!(
        Gmgn::new(rt, turn).top_holders(token, None),
        snapshot(rt, turn, token, Depth::Basic),
        known_wallets(rt, turn)
    );
    let snap = snap.ok();
    let creator = snap.as_ref().and_then(|s| s.creator.clone());
    let rows = list?
        .into_iter()
        .take(10)
        .map(|h| {
            let pool = h.addr_type.get() == Some(2.0);
            let mut row = gmgn_row(h);
            row.label = row
                .label
                .or(pool.then_some("pool or curve"))
                .or((creator.as_deref() == Some(row.wallet.as_str())).then_some("dev"));
            if let Some((kind, name)) = known.get(&row.wallet) {
                let tag = if *kind == WalletKind::Kol {
                    "kol"
                } else {
                    "smart"
                };
                if !row.tags.contains(&tag) {
                    row.tags.push(tag);
                }
                row.name = row.name.take().or(name.clone());
            }
            row
        })
        .collect();
    Ok(Top {
        symbol: snap.as_ref().and_then(|s| s.symbol.clone()),
        holders: snap.as_ref().and_then(|s| s.holders),
        top10_pct: snap.as_ref().and_then(|s| s.top10_pct),
        rows,
    })
}

/// GMGN's holder list with tags, names and PnL; `only` keeps one kind.
pub async fn labelled(
    rt: &Runtime,
    turn: &Turn,
    token: &str,
    only: Option<&str>,
) -> Result<Vec<HolderRow>, Fail> {
    let rows = Gmgn::new(rt, turn).top_holders(token, only).await?;
    Ok(rows
        .into_iter()
        .filter(|h| h.addr_type.get() != Some(2.0))
        .map(gmgn_row)
        .collect())
}

/// The most profitable traders of the coin (GMGN).
pub async fn traders(rt: &Runtime, turn: &Turn, token: &str) -> Result<Vec<HolderRow>, Fail> {
    let rows = Gmgn::new(rt, turn).top_traders(token).await?;
    Ok(rows
        .into_iter()
        .filter(|h| h.addr_type.get() != Some(2.0))
        .map(gmgn_row)
        .collect())
}

fn gmgn_row(h: gm::Holder) -> HolderRow {
    let address = h.address.to_string();
    HolderRow {
        label: contract_label(&address),
        tags: tags(&[&h.tags, &h.maker_token_tags]),
        name: text(&h.twitter_name)
            .or(text(&h.name))
            .map(|s| s.chars().take(24).collect()),
        twitter: text(&h.twitter_username).map(str::to_string),
        pct: h.amount_percentage.pct(),
        usd: h.usd_value.get(),
        since: h.start_holding_at.ts(),
        unrealized_usd: h.unrealized_profit.get(),
        realized_usd: h.realized_profit.get(),
        bought_usd: h.buy_volume_cur.get(),
        sold_usd: h.sell_volume_cur.get(),
        wallet: address,
    }
}

pub struct Launch {
    pub token: String,
    pub symbol: Option<String>,
    pub created_at: Option<i64>,
    pub graduated: Option<bool>,
    pub fdv_usd: Option<f64>,
    pub ath_fdv_usd: Option<f64>,
    pub holders: Option<f64>,
}

pub struct DevRecord {
    pub wallet: String,
    pub source: &'static str,
    pub holds_pct: Option<f64>,
    pub sold_out: Option<bool>,
    pub graduated: Option<f64>,
    pub launches: Vec<Launch>,
}

/// The dev wallet, what it still holds, and its launches.
pub async fn dev(rt: &Runtime, turn: &Turn, token: &str) -> Result<DevRecord, Fail> {
    let snap = snapshot(rt, turn, token, Depth::Basic).await?;
    let Some(wallet) = snap.creator.clone() else {
        return Err(Fail::NotFound("dev wallet unknown for this token".into()));
    };
    let created = Gmgn::new(rt, turn).created_tokens(&wallet).await;
    let (graduated, launches) = match created {
        Ok(c) => (
            c.open_count.get(),
            c.tokens
                .into_iter()
                .take(10)
                .map(|t| Launch {
                    token: t.token_address.to_string(),
                    symbol: t.symbol,
                    created_at: t.create_timestamp.ts(),
                    graduated: t.is_open.get(),
                    fdv_usd: t.market_cap.get(),
                    ath_fdv_usd: t.token_ath_mc.get(),
                    holders: t.holders.get(),
                })
                .collect(),
        ),
        Err(fail) => {
            turn.gap(format!(
                "dev launch history unavailable: {}",
                fail.message()
            ));
            (None, vec![])
        }
    };
    Ok(DevRecord {
        wallet,
        source: "gmgn",
        holds_pct: snap.mix.dev_pct,
        sold_out: snap.dev_status.as_deref().map(|s| s == "creator_close"),
        graduated,
        launches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gmgn_holders_carry_tags_and_label_pools() {
        let text = std::fs::read_to_string(format!(
            "{}/src/fixtures/gmgn/holders_fresh.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let rows: Vec<gm::Holder> = serde_json::from_value(v["data"]["list"].clone()).unwrap();
        assert_eq!(rows[0].addr_type.get(), Some(2.0));
        let rows: Vec<HolderRow> = rows.into_iter().map(gmgn_row).collect();
        assert!(rows.iter().any(|r| r.tags.contains(&"fomo_user")));
        assert!(rows.iter().all(|r| r.pct.is_some()));
        assert_eq!(
            contract_label("1nc1nerator11111111111111111111111111111111"),
            Some("burn")
        );
        assert_eq!(
            tags(&[&["smart_degen".into(), "gmgn".into()], &["bundler".into()]]),
            ["bundler", "smart"]
        );
    }
}
