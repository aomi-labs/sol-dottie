//! Receipt-based PnL on recorded Solana transactions: one buy and one sell
//! with hand-checked wallet legs (see fixtures/pnl-sol-provenance.md).
use aomi_sdk::testing::{TestCtxBuilder, run_tool};
use dottie::domain::pnl::settlement::legs;
use dottie::providers::TOKEN_PROGRAM;
use dottie::runtime::{DottieApp, GMGN_KEY};
use dottie::tools::Pnl;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/pnl-sol.json")).unwrap()
}

const BUY: &str =
    "ADuzc6tSPnNqUHw7EQ44UABit6J5YN8yxeC7YeSthw2REAjqyC1UaZPTiG6PfF7GkvBnA2xDH4tui91xSu79uXC";
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
    let tx_key = |sig: &str| {
        format!(
            "rpc:getTransaction:{}",
            json!([sig, {"commitment": "confirmed", "encoding": "jsonParsed", "maxSupportedTransactionVersion": 0}])
        )
    };
    let recorded = BTreeMap::from([
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
