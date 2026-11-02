use aomi_sdk::{
    DynAomiApp,
    testing::{TestCtxBuilder, run_tool},
};
use dottie::{
    domain::{self, FLUXB, SOL},
    providers::{fluxbeam, rugcheck, solomon},
    runtime::{DottieApp, check_status},
    tools,
};
use serde_json::{Value, json};
fn rug() -> Value {
    serde_json::from_str(include_str!("fixtures/rugcheck-report.json")).unwrap()
}
fn quote() -> Value {
    serde_json::from_str(include_str!("fixtures/fluxbeam-quote.json")).unwrap()
}
fn pos() -> Value {
    serde_json::from_str(include_str!("fixtures/solomon-position.json")).unwrap()
}
#[test]
fn rugcheck_preserves_transfer_fees_and_detection_semantics() {
    let v = rugcheck::normalize(&rug(), FLUXB).unwrap();
    assert_eq!(
        v["token_extensions"]["transferFeeConfig"]["newerTransferFee"]["transferFeeBasisPoints"],
        300
    );
    assert!(v["report_generated_at"].is_null());
    assert!(v["mint_authority_known"].as_bool().unwrap());
}
#[test]
fn rugcheck_missing_authority_is_unknown_not_revoked() {
    let mut raw = rug();
    raw["token"]
        .as_object_mut()
        .unwrap()
        .remove("mintAuthority");
    assert_eq!(
        rugcheck::normalize(&raw, FLUXB).unwrap()["mint_authority_known"],
        false
    );
}
#[test]
fn rugcheck_rejects_empty_and_wrong_mint() {
    assert!(rugcheck::normalize(&json!({}), FLUXB).is_err());
    assert!(rugcheck::normalize(&rug(), SOL).is_err());
}
#[test]
fn fluxbeam_quote_matches_live_fixture_without_claiming_execution() {
    let q = fluxbeam::normalize(&quote(), SOL, FLUXB, 10_000_000, 100).unwrap();
    assert_eq!(q["minimum_out_base_units"], "28272856");
    assert_eq!(q["executable"], false);
}
#[test]
fn fluxbeam_rejects_wrong_mint_amount_program_pool_and_slippage() {
    for (field, value) in [
        ("inputMint", json!(FLUXB)),
        ("outputMint", json!(SOL)),
        ("amountIn", json!(1)),
        ("program", json!(SOL)),
        ("pool", json!(SOL)),
        ("minimumOut", json!(1)),
    ] {
        let mut q = quote();
        q["quote"][field] = value;
        assert!(
            fluxbeam::normalize(&q, SOL, FLUXB, 10_000_000, 100).is_err(),
            "{field}"
        );
    }
}
#[test]
fn fluxbeam_rejects_zero_liquidity() {
    let mut q = quote();
    q["quote"]["outAmount"] = json!(0);
    assert!(fluxbeam::normalize(&q, SOL, FLUXB, 10_000_000, 100).is_err());
}
#[test]
fn unsupported_pairs_do_not_fall_back() {
    assert!(domain::pair(SOL, "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").is_err());
}
#[test]
fn amounts_are_bounded_integers() {
    for a in ["-1", "0", "0.1", "1e6", "100000001", "18446744073709551616"] {
        assert!(domain::amount(a).is_err());
    }
    assert_eq!(domain::amount("10000000").unwrap(), 10_000_000);
}
#[test]
fn address_parser_rejects_urls_and_path_injection() {
    for p in ["https://example.com", "../report", "0x123", ""] {
        assert!(domain::pubkey(p).is_err());
    }
    assert!(domain::pubkey(SOL).is_ok());
}
#[test]
fn solomon_partial_holdings_stay_unknown() {
    let v = solomon::normalize(&pos(), "11111111111111111111111111111111").unwrap();
    assert!(v["account"].is_null());
    assert!(v["holdings"][0]["amount"].is_null());
    assert_eq!(v["observation"]["complete"], false);
}
#[test]
fn solomon_wallet_mismatch_is_rejected() {
    assert!(solomon::normalize(&pos(), SOL).is_err());
}
#[test]
fn solomon_missing_data_is_not_empty_portfolio() {
    assert!(solomon::normalize(&json!({"walletAddress":SOL}), SOL).is_err());
}
#[test]
fn provider_http_errors_have_stable_codes() {
    assert_eq!(check_status(429).unwrap_err().code, "RATE_LIMITED");
    assert_eq!(check_status(404).unwrap_err().code, "NOT_FOUND");
    assert_eq!(check_status(503).unwrap_err().code, "UNAVAILABLE");
}
#[test]
fn disabled_preparations_emit_no_execution_routes() {
    let a = run_tool::<tools::FluxbeamPrepare>(
        &DottieApp,
        json!({}),
        TestCtxBuilder::new("dottie_fluxbeam_prepare").build(),
    )
    .unwrap();
    let b = run_tool::<tools::SolomonPrepare>(
        &DottieApp,
        json!({}),
        TestCtxBuilder::new("dottie_solomon_prepare").build(),
    )
    .unwrap();
    for r in [a, b] {
        assert!(r.routes.is_empty());
        assert_eq!(r.value["status"], "unavailable");
        assert!(r.value["transaction"].is_null());
    }
}
#[test]
fn catalog_exact_mint_never_substitutes_ticker() {
    let r = run_tool::<tools::Assets>(
        &DottieApp,
        json!({"query":"NVDAx"}),
        TestCtxBuilder::new("dottie_assets").build(),
    )
    .unwrap();
    assert_eq!(
        r.value["assets"][0]["mint"],
        "Xsc9qvGR1efVDFGLrVsmkzv3qi45LTBjeUKSPmx9qEh"
    );
    let r = run_tool::<tools::Assets>(
        &DottieApp,
        json!({"query":"5h9W6mpNmz8MeG3fgjLrajHSB2BrRSuGeWGD61tLmoon"}),
        TestCtxBuilder::new("dottie_assets").build(),
    )
    .unwrap();
    assert_eq!(r.value["status"], "not_found");
}
#[test]
fn manifest_exposes_scoped_skills_and_svm_tools() {
    let m = DottieApp.manifest();
    assert_eq!(m.sdk_version, "6.0.0");
    assert_eq!(m.namespaces.unwrap(), vec!["aomi-core", "svm-core"]);
    assert_eq!(m.tools.len(), 7);
    assert_eq!(m.skills.len(), 3);
    assert!(
        m.tools
            .iter()
            .all(|t| t.parameters_schema["additionalProperties"] == false)
    );
}
#[test]
fn instruction_contract_checks_payer_mints_amounts_and_freshness() {
    let ix: Value =
        serde_json::from_str(include_str!("fixtures/fluxbeam-instruction.json")).unwrap();
    let wallet = "11111111111111111111111111111111";
    assert!(fluxbeam::validate_instruction(&ix, &quote(), wallet, 100, 110).is_ok());
    assert!(fluxbeam::validate_instruction(&ix, &quote(), SOL, 100, 110).is_err());
    assert_eq!(
        fluxbeam::validate_instruction(&ix, &quote(), wallet, 100, 121)
            .unwrap_err()
            .code,
        "STALE_QUOTE"
    );
    let mut wrong = ix.clone();
    wrong["instruction"]["accounts"][10]["PublicKey"] = json!(SOL);
    assert!(fluxbeam::validate_instruction(&wrong, &quote(), wallet, 100, 110).is_err());
    let mut q = quote();
    q["quote"]["amountIn"] = json!(2);
    assert!(fluxbeam::validate_instruction(&ix, &q, wallet, 100, 110).is_err());
}
