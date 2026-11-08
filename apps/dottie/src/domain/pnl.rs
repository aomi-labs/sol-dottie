//! GMGN identifies trade rounds; confirmed Solana transactions determine
//! money in and out, one fill at a time. Route fees are already in those
//! legs. Network fees and token-account rent are reported separately in
//! SOL, never mixed into a before-fee return.
//!
//! Policy: profit and return cover the whole round, (proceeds + value now −
//! buy cost) ÷ buy cost. Realized and unrealized split that profit by
//! moving weighted-average cost: a sale releases its share of the basis.
pub mod settlement;
use crate::card::Card;
use crate::providers::gmgn::{Gmgn, types::Activity};
use crate::providers::rpc::Rpc;
use crate::providers::signature;
use crate::runtime::{Fail, Runtime, Turn};
use std::collections::BTreeSet;
use std::time::Duration;

/// Pages of 50 trades read before giving up on finding the round's start.
const MAX_PAGES: usize = 4;
/// How long to wait for GMGN to index a trade that just landed.
const INDEX_WAIT: [u64; 3] = [0, 3, 5];

/// The latest round (or the one holding `tx`) as card figures.
pub async fn card(
    rt: &Runtime,
    turn: &Turn,
    wallet: &str,
    token: &str,
    tx: Option<&str>,
) -> Result<Card, Fail> {
    let gmgn = Gmgn::new(rt, turn);
    let tx = tx.map(signature).transpose()?.map(str::to_string);
    let mut rows = vec![];
    for wait in INDEX_WAIT {
        tokio::time::sleep(Duration::from_secs(wait)).await;
        rows = history(gmgn, wallet, token, tx.as_deref()).await?;
        if tx
            .as_ref()
            .is_none_or(|t| rows.iter().any(|r| r.tx_hash == *t))
        {
            break;
        }
    }
    if let Some(t) = &tx
        && !rows.iter().any(|r| r.tx_hash == *t)
    {
        return Err(Fail::NoData(format!(
            "GMGN hasn't indexed {} yet; try again in a minute",
            &t[..t.len().min(12)]
        )));
    }
    let round = round(rows, tx.as_deref())?;
    let now = crate::view::now();
    let rpc = Rpc::new(rt, turn);
    let settled = settlement::read(rpc, turn, wallet, token, &round.rows).await?;
    let mut card = round.base(now);
    let basis_known = round.basis_known() && settled.sold <= settled.bought;
    card.cost = settled.cost.filter(|_| basis_known);
    card.out = settled.out;
    card.proceeds = Some(settled.out);
    card.realized = settled
        .basis_sold
        .filter(|_| basis_known && settled.sold > 0)
        .map(|basis| settled.out - basis);
    card.fee_sol = Some(settled.fee_sol);
    card.fee_transactions = settled.fee_transactions;
    if settled.rent_sol.abs() > 0.0 {
        turn.gap(format!(
            "{:.6} SOL of token-account rent moved in this round (recoverable when the account closes); it is not in the return",
            settled.rent_sol
        ));
    }
    card.accounting_note = if settled.estimated_fx {
        "BEFORE FEES · EST. SOL/USD"
    } else {
        "BEFORE FEES · USDC AT $1"
    }
    .into();
    let decimals = match super::market::known(token) {
        Some(asset) => Ok(asset.decimals),
        None => rpc.mint_id(token).await.map(|m| m.decimals),
    };
    let bought = settlement::token_amount(settled.bought, decimals.as_ref().ok().copied());
    let sold = settlement::token_amount(settled.sold, decimals.as_ref().ok().copied());
    card.entry = card.cost.zip(bought).map(|(cost, amount)| cost / amount);
    card.exit = sold.map(|amount| settled.out / amount);
    if !round.open {
        if round.basis_known() && settled.bought > settled.sold {
            return Err(Fail::NoData(
                "on-chain fills leave tokens unsold; trade history is incomplete".into(),
            ));
        }
        if card.cost.is_none() {
            card.accounting_note = if settled.estimated_fx {
                "COST UNKNOWN · EST. USD · BEFORE FEES"
            } else {
                "COST UNKNOWN · USDC AT $1 · BEFORE FEES"
            }
            .into();
        }
        return Ok(card);
    }
    // Preserve which dependency failed. A short, bounded price retry is
    // useful for temporary outages; rate-limit and configuration failures
    // retain their meaning and are not retried.
    let (mut info, balance) = tokio::join!(gmgn.token_info(token), rpc.balance(token, wallet));
    if matches!(
        info,
        Err(Fail::Unavailable { .. } | Fail::BadResponse { .. })
    ) {
        tokio::time::sleep(Duration::from_secs(6)).await;
        info = gmgn.token_info(token).await;
    }
    let info = info.map_err(|e| valuation_error("price", e))?;
    let price = info
        .price
        .price
        .get()
        .filter(|p| *p >= 0.0)
        .ok_or_else(|| {
            Fail::NoData("current position price is missing from GMGN token info".into())
        })?;
    let decimals = decimals.map_err(|e| valuation_error("decimals", e))?;
    let balance = balance.map_err(|e| valuation_error("balance", e))?;
    let held = balance as f64 / 10f64.powi(i32::from(decimals));
    card.out += price * held;
    card.unrealized = settled
        .basis_left
        .filter(|_| card.cost.is_some())
        .map(|basis| price * held - basis);
    card.open = true;
    card.exit = Some(price);
    card.held_s = round
        .rows
        .first()
        .and_then(|r| r.timestamp.ts())
        .map(|start| now - start);
    card.ts = now;
    card.accounting_note = "BEFORE FEES · EST. CURRENT VALUE".into();
    if settled.bought != settled.sold + balance {
        card.cost = None;
        card.entry = None;
        card.realized = None;
        card.unrealized = None;
        turn.gap(
            "current token balance does not reconcile with this round; cost basis is unavailable",
        );
    }
    if card.cost.is_none() {
        card.accounting_note = "COST UNKNOWN · EST. VALUE · BEFORE FEES".into();
    }
    Ok(card)
}

fn valuation_error(field: &str, fail: Fail) -> Fail {
    Fail::NoData(format!(
        "current position {field} is unavailable ({}: {}); try again later",
        fail.code(),
        fail.message()
    ))
}

/// Every record GMGN has for the pair, oldest first, up to [`MAX_PAGES`].
async fn history(
    gmgn: Gmgn<'_>,
    wallet: &str,
    token: &str,
    tx: Option<&str>,
) -> Result<Vec<Activity>, Fail> {
    let mut rows = vec![];
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let (mut page, next) = gmgn
            .token_activity(wallet, token, cursor.as_deref())
            .await?;
        // GMGN is newest first. Preserve its order for trades sharing a
        // timestamp, including fills that straddle two pages.
        page.reverse();
        page.extend(rows);
        rows = page;
        rows.sort_by_key(|r| r.timestamp.ts().unwrap_or(0));
        let opened = history_complete(&rows, tx);
        match next {
            // Read through the requested round, which can precede the latest one.
            Some(c) if !opened && cursor.as_ref() != Some(&c) => cursor = Some(c),
            _ => break,
        }
    }
    rows.sort_by_key(|r| r.timestamp.ts().unwrap_or(0));
    Ok(rows)
}

fn history_complete(rows: &[Activity], tx: Option<&str>) -> bool {
    round(rows.to_vec(), tx).is_ok_and(|r| r.rows.first().is_some_and(opening_buy))
}

fn opening_buy(row: &Activity) -> bool {
    row.event_type == "buy" && row.is_open_or_close.get() == Some(1.0)
}

pub struct Round {
    rows: Vec<Activity>,
    open: bool,
}

/// Splits records (oldest first) into rounds and picks the one with `tx`,
/// else the latest.
pub fn round(rows: Vec<Activity>, tx: Option<&str>) -> Result<Round, Fail> {
    let mut rounds: Vec<Vec<Activity>> = vec![vec![]];
    let mut positions: std::collections::BTreeMap<(String, String), usize> =
        std::collections::BTreeMap::new();
    let mut unique: Vec<Activity> = vec![];
    for row in rows.into_iter().filter(|r| {
        matches!(
            r.event_type.as_str(),
            "buy" | "sell" | "transferIn" | "transferOut"
        )
    }) {
        let key = (row.tx_hash.clone(), row.event_type.clone());
        if let Some(&i) = positions.get(&key) {
            if row.is_open_or_close.get() == Some(1.0) {
                unique[i].is_open_or_close = row.is_open_or_close;
            }
        } else {
            positions.insert(key, unique.len());
            unique.push(row);
        }
    }
    for row in unique {
        if opening_buy(&row) && !rounds.last().expect("one round").is_empty() {
            rounds.push(vec![]);
        }
        let closes = row.event_type == "sell" && row.is_open_or_close.get() == Some(1.0);
        rounds.last_mut().expect("one round").push(row);
        if closes {
            rounds.push(vec![]);
        }
    }
    let open_tail = rounds.last().is_some_and(|r| {
        r.iter()
            .any(|a| matches!(a.event_type.as_str(), "buy" | "transferIn"))
    });
    if !open_tail {
        rounds.pop();
    }
    let index = match tx {
        Some(t) => rounds.iter().position(|r| r.iter().any(|a| a.tx_hash == t)),
        None => rounds.len().checked_sub(1),
    }
    .ok_or_else(|| Fail::NotFound("no trades of this coin from this wallet on record".into()))?;
    let open = open_tail && index + 1 == rounds.len();
    Ok(Round {
        rows: rounds.swap_remove(index),
        open,
    })
}

impl Round {
    fn side(&self, side: &str) -> Vec<&Activity> {
        self.rows.iter().filter(|r| r.event_type == side).collect()
    }

    fn txs(rows: &[&Activity]) -> u32 {
        rows.iter()
            .map(|r| r.tx_hash.as_str())
            .collect::<BTreeSet<_>>()
            .len() as u32
    }

    fn basis_known(&self) -> bool {
        self.rows.first().is_some_and(opening_buy)
            && self.side("transferIn").is_empty()
            && self.side("transferOut").is_empty()
    }

    fn base(&self, now: i64) -> Card {
        let (buys, sells) = (self.side("buy"), self.side("sell"));
        let first = self.rows.first().and_then(|r| r.timestamp.ts());
        let last = self.rows.last();
        Card {
            symbol: self
                .rows
                .iter()
                .find_map(|r| r.token.symbol.clone())
                .map(|s| crate::card::format::clamp(&s, 20))
                .unwrap_or_else(|| "COIN".into()),
            open: false,
            cost: None,
            out: 0.0,
            proceeds: None,
            realized: None,
            unrealized: None,
            entry: None,
            exit: None,
            held_s: first.map(|f| last.and_then(|l| l.timestamp.ts()).unwrap_or(now).max(f) - f),
            buys: Self::txs(&buys),
            sells: Self::txs(&sells),
            fee_sol: None,
            fee_transactions: 0,
            accounting_note: String::new(),
            ts: last.and_then(|l| l.timestamp.ts()).unwrap_or(now),
            tx: last.map(|l| l.tx_hash.clone()).unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(kind: &str, ts: i64, tx: &str, flag: i64) -> Activity {
        serde_json::from_value(
            json!({"event_type":kind,"timestamp":ts,"tx_hash":tx,"is_open_or_close":flag}),
        )
        .unwrap()
    }

    #[test]
    fn pagination_requires_the_requested_rounds_opening_buy() {
        let latest = row("buy", 300, "new", 1);
        assert!(!history_complete(
            std::slice::from_ref(&latest),
            Some("old-sell")
        ));
        assert!(history_complete(std::slice::from_ref(&latest), None));
        let mut rows = vec![row("sell", 200, "old-sell", 1), latest];
        assert!(!history_complete(&rows, Some("old-sell")));
        rows.insert(0, row("buy", 100, "old-buy", 1));
        assert!(history_complete(&rows, Some("old-sell")));
        let selected = round(rows, Some("old-sell")).unwrap();
        assert!(!selected.open);
        assert_eq!(selected.base(0).held_s, Some(100));
    }

    #[test]
    fn transfers_and_orphan_sells_do_not_establish_basis() {
        for rows in [
            vec![row("transferIn", 1, "a", 0), row("sell", 2, "b", 1)],
            vec![row("buy", 1, "a", 0), row("sell", 2, "b", 1)],
            vec![row("sell", 2, "b", 1)],
            vec![row("buy", 1, "a", 1), row("transferOut", 2, "b", 0)],
        ] {
            assert!(!round(rows, None).unwrap().basis_known());
        }
    }

    #[test]
    fn a_new_opening_buy_after_transfer_starts_a_new_round() {
        let selected = round(
            vec![
                row("buy", 1, "a", 1),
                row("transferOut", 2, "b", 0),
                row("buy", 3, "c", 1),
                row("sell", 4, "d", 1),
            ],
            None,
        )
        .unwrap();
        assert!(selected.basis_known());
        assert_eq!(selected.base(0).buys, 1);
        assert_eq!(selected.base(0).held_s, Some(1));
    }

    #[test]
    fn dependency_errors_keep_the_field_and_underlying_cause() {
        let e = valuation_error(
            "balance",
            Fail::unavailable(crate::runtime::Source::Rpc, "network busy"),
        );
        assert!(e.message().contains("balance"));
        assert!(e.message().contains("UNAVAILABLE: rpc: network busy"));
    }
}
