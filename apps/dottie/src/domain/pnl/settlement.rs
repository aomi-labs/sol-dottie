//! Reconcile each distinct fill transaction once from its confirmed
//! balances: the wallet's token delta is post − pre token balances for
//! (owner = wallet, mint); its SOL leg is post − pre lamports of the wallet
//! and its wrapped-SOL accounts, with the network fee and token-account
//! rent taken out and reported apart. GMGN supplies the history, never
//! cash. Route fees are already in the wallet legs.
use crate::providers::gmgn::types::Activity;
use crate::providers::rpc::Rpc;
use crate::providers::{SOL, USDC, signature};
use crate::runtime::{Fail, Turn};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const MAX_TRANSACTIONS: usize = 24;

pub struct Settlement {
    /// Total wallet spend on buys.
    pub cost: Option<f64>,
    /// Net sale proceeds.
    pub out: f64,
    /// Moving-average basis released by sales, and left on unsold coins.
    pub basis_sold: Option<f64>,
    pub basis_left: Option<f64>,
    pub bought: u128,
    pub sold: u128,
    /// Network fees the wallet paid, in SOL.
    pub fee_sol: f64,
    /// Token-account rent locked (positive) or refunded (negative), in SOL;
    /// recoverable when an account closes.
    pub rent_sol: f64,
    pub fee_transactions: u32,
    pub estimated_fx: bool,
}

/// One transaction's wallet legs, in atomic units.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Legs {
    pub slot: u64,
    /// The coin, signed: positive bought, negative sold.
    pub token: i128,
    /// SOL paid (negative) or received, fee and rent excluded.
    pub sol: i128,
    pub usdc: i128,
    /// Lamports of network fee, when the wallet paid it.
    pub fee: u64,
    /// Lamports moved into (positive) or out of the wallet's token accounts.
    pub rent: i128,
}

/// The wallet's legs in one jsonParsed `getTransaction` result.
pub fn legs(tx: &Value, wallet: &str, token: &str) -> Result<Legs, Fail> {
    let meta = &tx["meta"];
    if !meta["err"].is_null() {
        return Err(Fail::NoData(
            "a transaction in this round failed on chain".into(),
        ));
    }
    let keys: Vec<&str> = tx
        .pointer("/transaction/message/accountKeys")
        .and_then(Value::as_array)
        .map(|keys| {
            keys.iter()
                .map(|k| k["pubkey"].as_str().or(k.as_str()).unwrap_or(""))
                .collect()
        })
        .unwrap_or_default();
    let lamports =
        |field: &str, i: usize| -> i128 { meta[field][i].as_u64().map_or(0, i128::from) };
    let delta = |i: usize| lamports("postBalances", i) - lamports("preBalances", i);
    let Some(me) = keys.iter().position(|k| *k == wallet) else {
        return Err(Fail::NoData(
            "the wallet isn't in an indexed transaction".into(),
        ));
    };
    let payer = me == 0;
    let fee = if payer {
        meta["fee"].as_u64().unwrap_or(0)
    } else {
        0
    };
    // Token accounts the wallet owns in this transaction, by account index.
    let mut owned: BTreeMap<u64, (String, i128, i128)> = BTreeMap::new();
    for (field, post) in [("preTokenBalances", false), ("postTokenBalances", true)] {
        for b in meta[field].as_array().into_iter().flatten() {
            if b["owner"].as_str() != Some(wallet) {
                continue;
            }
            let (Some(index), Some(mint)) = (b["accountIndex"].as_u64(), b["mint"].as_str()) else {
                continue;
            };
            let amount: i128 = b
                .pointer("/uiTokenAmount/amount")
                .and_then(Value::as_str)
                .and_then(|a| a.parse().ok())
                .unwrap_or(0);
            let entry = owned.entry(index).or_insert((mint.to_string(), 0, 0));
            if post {
                entry.2 = amount;
            } else {
                entry.1 = amount;
            }
        }
    }
    let mut out = Legs {
        slot: tx["slot"].as_u64().unwrap_or(0),
        fee,
        sol: delta(me) + i128::from(fee),
        ..Legs::default()
    };
    for (index, (mint, pre, post)) in &owned {
        let lamport_delta = delta(*index as usize);
        if mint == SOL {
            // A wrapped-SOL account's lamports are SOL (its rent included).
            out.sol += lamport_delta;
            continue;
        }
        // Rent moved into (or back out of) a token account isn't a swap
        // cost: count it apart and add it back to the SOL leg.
        out.rent += lamport_delta;
        out.sol += lamport_delta;
        let moved = post - pre;
        if mint == token {
            out.token += moved;
        } else if mint == USDC {
            out.usdc += moved;
        }
    }
    Ok(out)
}

/// One buy or sell in chain order.
struct Op {
    at: (u64, usize),
    buy: bool,
    quantity: u128,
    /// Signed wallet cash flow in USD; None when it cannot be priced.
    cash: Option<f64>,
}

pub async fn read(
    rpc: Rpc<'_>,
    turn: &Turn,
    wallet: &str,
    token: &str,
    rows: &[Activity],
) -> Result<Settlement, Fail> {
    // One entry per transaction, oldest first.
    let mut trades: Vec<(String, Vec<&Activity>)> = vec![];
    for row in rows
        .iter()
        .filter(|r| matches!(r.event_type.as_str(), "buy" | "sell"))
    {
        let sig = signature(&row.tx_hash)
            .map_err(|_| Fail::NoData("trade history has an invalid signature".into()))?
            .to_string();
        match trades.iter_mut().find(|(s, _)| *s == sig) {
            Some((_, rows)) => rows.push(row),
            None => trades.push((sig, vec![row])),
        }
    }
    if trades.len() > MAX_TRANSACTIONS {
        return Err(Fail::NoData(
            "this round has too many transactions to reconcile in one card".into(),
        ));
    }
    let fx = sol_usd(rows);
    let mut s = Settlement {
        cost: None,
        out: 0.0,
        basis_sold: None,
        basis_left: None,
        bought: 0,
        sold: 0,
        fee_sol: 0.0,
        rent_sol: 0.0,
        fee_transactions: 0,
        estimated_fx: false,
    };
    let mut ops = vec![];
    let mut fees_seen = BTreeSet::new();
    for (n, (sig, rows)) in trades.iter().enumerate() {
        let tx = rpc.transaction(sig).await?;
        let l = legs(&tx, wallet, token)?;
        if l.token == 0 {
            return Err(Fail::NoData(
                "indexed trade has no wallet change of this token".into(),
            ));
        }
        let buy = l.token > 0;
        if !rows.iter().any(|r| (r.event_type == "buy") == buy) {
            return Err(Fail::NoData(
                "indexed trade direction disagrees with the wallet's balances".into(),
            ));
        }
        if l.fee > 0 && fees_seen.insert(sig.clone()) {
            s.fee_sol += l.fee as f64 / 1e9;
            s.fee_transactions += 1;
        }
        s.rent_sol += l.rent as f64 / 1e9;
        let cash = cash(&l, fx, &mut s.estimated_fx);
        if buy && cash.is_none_or(|c| c >= 0.0) {
            turn.gap("one buy's wallet spend cannot be priced; cost basis is unavailable");
        }
        ops.push(Op {
            at: (l.slot, n),
            buy,
            quantity: l.token.unsigned_abs(),
            cash,
        });
    }
    ops.sort_by_key(|op| op.at);
    account(&ops, &mut s)?;
    Ok(s)
}

/// The legs' signed USD: USDC at $1, SOL at the round's implied rate
/// (marked estimated). Fee and rent are already out of the SOL leg.
fn cash(l: &Legs, fx: Option<f64>, estimated: &mut bool) -> Option<f64> {
    let sol = l.sol;
    let mut total = l.usdc as f64 / 1e6;
    if sol != 0 {
        *estimated = true;
        total += sol as f64 / 1e9 * fx?;
    }
    // Zero cash is a valid answer only when a leg moved.
    (sol != 0 || l.usdc != 0).then_some(total)
}

/// SOL's dollar price implied by the round's own indexed SOL-quoted trades.
fn sol_usd(rows: &[Activity]) -> Option<f64> {
    let rates: Vec<f64> = rows
        .iter()
        .filter(|r| r.quote_address.as_deref() == Some(SOL))
        .filter_map(|r| {
            let (usd, sol) = (r.cost_usd.get()?, r.quote_amount.get()?);
            (sol > 0.0 && usd > 0.0).then(|| usd / sol)
        })
        .collect();
    (!rates.is_empty()).then(|| rates.iter().sum::<f64>() / rates.len() as f64)
}

/// Moving weighted-average cost. A sale releases its share of the
/// remaining basis; selling more than the history bought leaves basis
/// unknown rather than inventing a zero cost.
fn account(ops: &[Op], s: &mut Settlement) -> Result<(), Fail> {
    let (mut cost, mut basis, mut basis_sold) = (Some(0.0), Some(0.0), Some(0.0));
    let mut proceeds = 0.0;
    let mut held: u128 = 0;
    for op in ops {
        if op.buy {
            let spend = op.cash.filter(|c| *c < 0.0).map(|c| -c);
            cost = cost.zip(spend).map(|(a, b)| a + b);
            basis = basis.zip(spend).map(|(a, b)| a + b);
            held += op.quantity;
            s.bought += op.quantity;
            continue;
        }
        let Some(cash) = op.cash.filter(|c| *c >= 0.0) else {
            return Err(Fail::NoData(
                "final wallet sale proceeds are unavailable; no pool-leg estimate was substituted"
                    .into(),
            ));
        };
        proceeds += cash;
        s.sold += op.quantity;
        if op.quantity > held {
            (basis, basis_sold, held) = (None, None, 0);
            continue;
        }
        let share = op.quantity as f64 / held as f64;
        let removed = basis.map(|b| b * share);
        basis_sold = basis_sold.zip(removed).map(|(a, b)| a + b);
        basis = basis.zip(removed).map(|(a, b)| a - b);
        held -= op.quantity;
    }
    s.out = proceeds;
    s.cost = cost;
    s.basis_sold = basis_sold;
    s.basis_left = basis;
    Ok(())
}

pub fn token_amount(raw: u128, decimals: Option<u8>) -> Option<f64> {
    decimals.map(|d| raw as f64 / 10f64.powi(i32::from(d)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn average_cost_releases_basis_on_sales() {
        let op = |buy, quantity, cash| Op {
            at: (0, 0),
            buy,
            quantity,
            cash: Some(cash),
        };
        let mut s = Settlement {
            cost: None,
            out: 0.0,
            basis_sold: None,
            basis_left: None,
            bought: 0,
            sold: 0,
            fee_sol: 0.0,
            rent_sol: 0.0,
            fee_transactions: 0,
            estimated_fx: false,
        };
        account(&[op(true, 100, -10.0), op(false, 50, 8.0)], &mut s).unwrap();
        assert_eq!((s.cost, s.out), (Some(10.0), 8.0));
        assert_eq!((s.basis_sold, s.basis_left), (Some(5.0), Some(5.0)));
        let mut over = Settlement { ..s };
        account(&[op(false, 50, 8.0)], &mut over).unwrap();
        assert_eq!(over.basis_sold, None);
    }
}
