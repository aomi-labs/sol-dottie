//! Receipt-based PnL on recorded Solana transactions: one buy and one sell
//! with hand-checked wallet legs (see fixtures/pnl-sol-provenance.md), and
//! a fork round trip found from the chain alone, with no GMGN history.
use aomi_sdk::testing::{TestCtxBuilder, run_tool};
use dottie::domain::pnl::settlement::legs;
use dottie::providers::{TOKEN_2022_PROGRAM, TOKEN_PROGRAM, associated_token_account};
use dottie::runtime::{DottieApp, GMGN_KEY};
use dottie::tools::Pnl;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/pnl-sol.json")).unwrap()
}

const BUY: &str =
    "ADuzc6tSPnNqUHw7EQ44UABit6J5YN8yxeC7YeSthw2REAjqyC1UaZPTiG6PfF7GkvBnA2xDH4tui91xSu79uXC";
fn tx_key(sig: &str) -> String {
    format!(
        "rpc:getTransaction:{}",
        json!([sig, {"commitment": "confirmed", "encoding": "jsonParsed", "maxSupportedTransactionVersion": 0}])
    )
}

fn signatures_key(account: &str) -> String {
    format!(
        "rpc:getSignaturesForAddress:{}",
        json!([account, {"commitment": "confirmed", "limit": 50}])
    )
}

const SELL: &str =
    "5wwb5qnfXZieuPmwWtyjU5zYVXmB6zzgRaVoLcb4fAmyt7Gmy7CcpgxFRkdRNH1SBcz3wzAha885SskCiHm8xgiy";

#[test]
fn wallet_legs_split_fee_and_rent_from_the_swap() {
    let f = fixture();
    let (wallet, token) = (f["wallet"].as_str().unwrap(), f["token"].as_str().unwrap());
    let buy = legs(&f["transactions"][BUY], wallet, token).unwrap();
    assert_eq!(buy.token, 17_660_310_761_849);
    assert_eq!(buy.sol, -3_031_000_000);
    assert_eq!((buy.fee, buy.rent), (1_005_000, 1_513_840));
    let sell = legs(&f["transactions"][SELL], wallet, token).unwrap();
    assert_eq!(sell.token, -17_660_310_761_849);
    assert_eq!(sell.sol, 3_024_542_906);
    assert_eq!((sell.fee, sell.rent), (1_005_000, 0));
    assert!(sell.slot > buy.slot);
    // Another wallet in the same transaction has no legs here.
    assert!(
        legs(
            &f["transactions"][BUY],
            "Gm5cm4KpqJYghgwCYZXryMfhFxjMKbTzhs3pVj61f4qv",
            token
        )
        .is_err()
    );
}

#[test]
fn the_card_is_the_round_from_its_receipts() {
    let f = fixture();
    let (wallet, token) = (f["wallet"].as_str().unwrap(), f["token"].as_str().unwrap());
    let mut recorded = BTreeMap::from([
        (
            format!(
                "gmgn:/v1/user/wallet_activity?chain=sol&wallet_address={wallet}&token_address={token}&limit=50"
            ),
            json!({"activities": f["activity"], "next": ""}),
        ),
        (tx_key(BUY), f["transactions"][BUY].clone()),
        (tx_key(SELL), f["transactions"][SELL].clone()),
        (
            format!(
                "rpc:getAccountInfo:{}",
                json!([token, {"commitment": "confirmed", "encoding": "jsonParsed"}])
            ),
            json!({"value": {"owner": TOKEN_PROGRAM, "data": {"parsed": {"type": "mint", "info": {"decimals": 6}}}}}),
        ),
    ]);
    // The RPC has no history for this pair, so GMGN's index names the round.
    for program in [TOKEN_PROGRAM, TOKEN_2022_PROGRAM] {
        let account = associated_token_account(wallet, token, program).unwrap();
        recorded.insert(signatures_key(&account), json!([]));
    }
    let app = DottieApp::replaying(recorded);
    let ctx = TestCtxBuilder::new("dottie_pnl")
        .secret(GMGN_KEY, "recorded")
        .attribute("domain", json!({"svm": {"address": wallet}}))
        .build();
    let out = run_tool::<Pnl>(&app, json!({"token": token}), ctx)
        .unwrap()
        .value;
    let pnl = &out["pnl"];
    assert_eq!(pnl["state"], "closed", "{out}");
    assert_eq!(pnl["basis_known"], true);
    let ret = pnl["return_pct"].as_f64().unwrap();
    assert!(
        (ret - (3_024_542_906.0 / 3_031_000_000.0 - 1.0) * 100.0).abs() < 1e-6,
        "{ret}"
    );
    assert!((pnl["fees_sol"].as_f64().unwrap() - 0.00201).abs() < 1e-12);
    assert_eq!(pnl["fee_transactions"], 2);
    assert_eq!(pnl["accounting"], "BEFORE FEES · EST. SOL/USD");
    assert!(out["gaps"][0].as_str().unwrap().contains("rent"), "{out}");
}

const FORK_BUY: &str =
    "3rFF4JNHunPiPnjmQaKF9rqzoAkkbGd1HacHDUdrSu1NigzmLjmkvBAV6gXGVsG6UT4TD4BSFWmMJ5fxH5UwbZE1";
const FORK_SELL: &str =
    "2LsgJsZQ1jhvf1DRFtTyKc6g4oLpgqmWR54J6meNcyJZPnUBVTxcjCyVT1S9a356voQPVPTZ3oV13vwtMLfev6dH";

/// The Surfpool round trip from the Telegram test: buy 0.05 SOL of BONK,
/// sell it all. Only the chain is recorded (plus a SOL price and BONK's
/// GMGN listing from the replay set), so GMGN never indexed it.
fn fork_app() -> (DottieApp, String, String) {
    let f: Value = serde_json::from_str(include_str!("fixtures/pnl-fork.json")).unwrap();
    let replay: BTreeMap<String, Value> =
        serde_json::from_str(include_str!("fixtures/replay.json")).unwrap();
    let (wallet, token) = (f["wallet"].as_str().unwrap(), f["token"].as_str().unwrap());
    let mut recorded: BTreeMap<String, Value> = replay
        .into_iter()
        .filter(|(k, _)| k.starts_with("jupiter:") || k.starts_with("gmgn:/v1/token/info"))
        .collect();
    for (account, sigs) in f["signatures"].as_object().unwrap() {
        recorded.insert(signatures_key(account), sigs.clone());
    }
    for (sig, tx) in f["transactions"].as_object().unwrap() {
        recorded.insert(tx_key(sig), tx.clone());
    }
    recorded.insert(
        format!(
            "rpc:getAccountInfo:{}",
            json!([token, {"commitment": "confirmed", "encoding": "jsonParsed"}])
        ),
        f["mint"].clone(),
    );
    (DottieApp::replaying(recorded), wallet.into(), token.into())
}

fn fork_pnl(signature: Option<&str>) -> Value {
    let (app, wallet, token) = fork_app();
    let ctx = TestCtxBuilder::new("dottie_pnl")
        .secret(GMGN_KEY, "recorded")
        .attribute("domain", json!({"svm": {"address": wallet}}))
        .build();
    run_tool::<Pnl>(&app, json!({"token": token, "signature": signature}), ctx)
        .unwrap()
        .value
}

#[test]
fn a_fork_round_trip_has_its_card_from_the_chain_alone() {
    let f: Value = serde_json::from_str(include_str!("fixtures/pnl-fork.json")).unwrap();
    let (wallet, token) = (f["wallet"].as_str().unwrap(), f["token"].as_str().unwrap());
    let buy = legs(&f["transactions"][FORK_BUY], wallet, token).unwrap();
    let sell = legs(&f["transactions"][FORK_SELL], wallet, token).unwrap();
    assert_eq!(
        (buy.sol, buy.fee, buy.rent),
        (-50_000_000, 15_001, 1_488_440)
    );
    assert_eq!((sell.sol, sell.fee, sell.rent), (49_846_039, 5_003, 0));
    assert_eq!(buy.token, -sell.token);
    assert_eq!((buy.held_before, sell.held_after), (0, 0));
    // The latest round, and the callback's card for the sell that just confirmed.
    for signature in [None, Some(FORK_SELL)] {
        let out = fork_pnl(signature);
        let pnl = &out["pnl"];
        assert_eq!(pnl["state"], "closed", "{out}");
        assert_eq!(pnl["basis_known"], true);
        assert_eq!(pnl["symbol"], "Bonk");
        let ret = pnl["return_pct"].as_f64().unwrap();
        assert!(
            (ret - (49_846_039.0 / 50_000_000.0 - 1.0) * 100.0).abs() < 1e-9,
            "{ret}"
        );
        // Fees and rent stay apart from the before-fee return.
        assert!((pnl["fees_sol"].as_f64().unwrap() - 0.000020004).abs() < 1e-12);
        assert_eq!(pnl["fee_transactions"], 2);
        assert_eq!(pnl["accounting"], "BEFORE FEES · EST. SOL/USD");
        let gaps = out["gaps"].to_string();
        assert!(
            gaps.contains("0.001488 SOL of token-account rent"),
            "{gaps}"
        );
        assert!(!gaps.contains("GMGN"), "{gaps}");
    }
}

#[test]
fn a_signature_the_chain_does_not_show_yet_is_no_data_after_one_retry() {
    let started = std::time::Instant::now();
    let out = fork_pnl(Some(SELL));
    assert_eq!(out["error"]["code"], "NO_DATA", "{out}");
    assert!(started.elapsed() >= std::time::Duration::from_secs(2));
}
