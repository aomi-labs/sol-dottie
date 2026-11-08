//! Rounds found on chain, so a trade that just confirmed has its PnL at
//! once, with no indexer: the signatures of the wallet's token account for
//! the mint (both token programs; only the one that exists has any), each
//! read back through `getTransaction`. Each becomes an [`Activity`] row the
//! round and settlement code already understand.
use super::settlement::{Legs, legs};
use crate::providers::gmgn::types::Activity;
use crate::providers::rpc::{Rpc, Signature};
use crate::providers::{TOKEN_2022_PROGRAM, TOKEN_PROGRAM, associated_token_account};
use crate::runtime::Fail;
use serde_json::json;
use std::time::Duration;

/// How long to wait before reading again for a signature that just landed.
const LAND_WAIT: Duration = Duration::from_secs(2);

/// The wallet's rows for the mint, oldest first, read back from the
/// newest until the round holding `tx` (else the latest) has its opening
/// buy, or the signatures run out. `Ok(None)`: the chain has no history.
pub async fn history(
    rpc: Rpc<'_>,
    wallet: &str,
    token: &str,
    symbol: Option<&str>,
    tx: Option<&str>,
) -> Result<Option<Vec<Activity>>, Fail> {
    // A requested signature is new: skip the short cache, and give it one
    // more read before saying the chain doesn't show it yet.
    let mut sigs = signatures(rpc, wallet, token, tx.is_some()).await?;
    if let Some(t) = tx
        && !sigs.is_empty()
        && !sigs.iter().any(|s| s.signature == t)
    {
        tokio::time::sleep(LAND_WAIT).await;
        sigs = signatures(rpc, wallet, token, true).await?;
    }
    if sigs.is_empty() {
        return Ok(None);
    }
    if let Some(t) = tx
        && !sigs.iter().any(|s| s.signature == t)
    {
        return Err(Fail::NoData(format!(
            "the chain doesn't show {} for this coin yet; try again in a moment",
            &t[..t.len().min(12)]
        )));
    }
    let mut rows = vec![];
    let mut seen = tx.is_none();
    for sig in &sigs {
        let l = legs(&rpc.transaction(&sig.signature).await?, wallet, token)?;
        seen |= tx == Some(sig.signature.as_str());
        let Some(row) = row(sig, &l, token, symbol) else {
            continue;
        };
        rows.push(row);
        // An opening buy at or before the round we want ends the walk.
        if seen && l.held_before == 0 && l.token > 0 {
            break;
        }
    }
    rows.reverse();
    Ok(Some(rows))
}

/// Signatures of both possible token accounts, newest first, at most
/// [`crate::providers::rpc::MAX_SIGNATURES`].
async fn signatures(
    rpc: Rpc<'_>,
    wallet: &str,
    token: &str,
    live: bool,
) -> Result<Vec<Signature>, Fail> {
    let account = |program| {
        associated_token_account(wallet, token, program)
            .ok_or_else(|| Fail::Invalid("not a Solana wallet and mint".into()))
    };
    let (classic, t22) = (account(TOKEN_PROGRAM)?, account(TOKEN_2022_PROGRAM)?);
    let (a, b) = tokio::join!(rpc.signatures(&classic, live), rpc.signatures(&t22, live));
    let mut sigs = a?;
    sigs.extend(b?);
    sigs.sort_by(|x, y| y.slot.cmp(&x.slot));
    sigs.dedup_by(|x, y| x.signature == y.signature);
    sigs.truncate(crate::providers::rpc::MAX_SIGNATURES);
    Ok(sigs)
}

/// One transaction as an activity row: a buy or sell when SOL or USDC
/// paid for it, a transfer otherwise. The buy from nothing and the sell to
/// nothing open and close the round. No change of the coin: no row.
fn row(sig: &Signature, l: &Legs, token: &str, symbol: Option<&str>) -> Option<Activity> {
    let paid = l.sol < 0 || l.usdc < 0;
    let received = l.sol > 0 || l.usdc > 0;
    let (kind, edge) = match l.token {
        0 => return None,
        t if t > 0 && paid => ("buy", l.held_before == 0),
        t if t > 0 => ("transferIn", false),
        _ if received => ("sell", l.held_after == 0),
        _ => ("transferOut", false),
    };
    serde_json::from_value(json!({
        "event_type": kind,
        "timestamp": sig.block_time,
        "tx_hash": sig.signature,
        "is_open_or_close": u8::from(edge),
        "token": {"address": token, "symbol": symbol},
    }))
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_name_buys_sells_and_transfers_from_the_legs() {
        let sig = Signature {
            signature: "s".into(),
            slot: 1,
            block_time: Some(10),
        };
        let kind =
            |l: Legs| row(&sig, &l, "M", None).map(|r| (r.event_type, r.is_open_or_close.get()));
        let opening = Legs {
            token: 5,
            sol: -100,
            held_after: 5,
            ..Legs::default()
        };
        assert_eq!(kind(opening.clone()), Some(("buy".into(), Some(1.0))));
        let add = Legs {
            held_before: 5,
            ..opening.clone()
        };
        assert_eq!(kind(add), Some(("buy".into(), Some(0.0))));
        let gift = Legs {
            sol: 2_039_280,
            ..opening
        };
        assert_eq!(kind(gift).unwrap().0, "transferIn");
        let close = Legs {
            token: -5,
            sol: 90,
            held_before: 5,
            ..Legs::default()
        };
        assert_eq!(kind(close), Some(("sell".into(), Some(1.0))));
        let out = Legs {
            token: -5,
            held_before: 9,
            held_after: 4,
            ..Legs::default()
        };
        assert_eq!(kind(out).unwrap().0, "transferOut");
        assert!(kind(Legs::default()).is_none());
    }
}
