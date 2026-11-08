//! Dottie's catalog, manifest and Telegram config, and the partner tools
//! (FluxBeam, Solomon) on recorded responses. No network.
use aomi_sdk::{
    DynAomiApp,
    testing::{TestCtxBuilder, run_tool},
    validate_app_skills_with_tools,
};
use dottie::providers::fluxbeam::{self, FLUXB};
use dottie::providers::{SOL, USDC, solomon};
use dottie::runtime::DottieApp;
use dottie::tools;
use serde_json::{Value, json};

fn quote() -> Value {
    serde_json::from_str(include_str!("fixtures/fluxbeam-quote.json")).unwrap()
}
fn pos() -> Value {
    serde_json::from_str(include_str!("fixtures/solomon-position.json")).unwrap()
}
fn app() -> DottieApp {
    DottieApp::default()
}

#[test]
fn fluxbeam_quote_matches_live_fixture_without_claiming_execution() {
    let q = fluxbeam::normalize(&quote(), SOL, FLUXB, 10_000_000, 100).unwrap();
    assert_eq!(q["minimum_out_base_units"], "28272856");
    assert_eq!(q["executable"], false);
}

#[test]
fn fluxbeam_rejects_wrong_mint_amount_program_pool_slippage_and_zero_out() {
    for (field, value) in [
        ("inputMint", json!(FLUXB)),
        ("outputMint", json!(SOL)),
        ("amountIn", json!(1)),
        ("program", json!(SOL)),
        ("pool", json!(SOL)),
        ("minimumOut", json!(1)),
        ("outAmount", json!(0)),
    ] {
        let mut q = quote();
        q["quote"][field] = value;
        assert!(
            fluxbeam::normalize(&q, SOL, FLUXB, 10_000_000, 100).is_err(),
            "{field}"
        );
    }
    assert!(fluxbeam::pair(SOL, USDC).is_err());
    for a in ["-1", "0", "0.1", "1e6", "100000001", "18446744073709551616"] {
        assert!(fluxbeam::base_units(a).is_err(), "{a}");
    }
    assert_eq!(fluxbeam::base_units("10000000").unwrap(), 10_000_000);
}

#[test]
fn instruction_contract_checks_payer_mints_amounts_and_freshness() {
    let ix: Value =
        serde_json::from_str(include_str!("fixtures/fluxbeam-instruction.json")).unwrap();
    let wallet = "11111111111111111111111111111111";
    assert!(fluxbeam::validate_instruction(&ix, &quote(), wallet, 100, 110).is_ok());
    assert!(fluxbeam::validate_instruction(&ix, &quote(), SOL, 100, 110).is_err());
    assert!(fluxbeam::validate_instruction(&ix, &quote(), wallet, 100, 121).is_err());
    let mut wrong = ix.clone();
    wrong["instruction"]["accounts"][10]["PublicKey"] = json!(SOL);
    assert!(fluxbeam::validate_instruction(&wrong, &quote(), wallet, 100, 110).is_err());
}

#[test]
fn solomon_partial_holdings_stay_unknown_and_mismatches_fail() {
    let v = solomon::normalize(&pos(), "11111111111111111111111111111111").unwrap();
    assert!(v["account"].is_null());
    assert!(v["holdings"][0]["amount"].is_null());
    assert_eq!(v["observation"]["complete"], false);
    assert!(solomon::normalize(&pos(), SOL).is_err());
    assert!(solomon::normalize(&json!({"walletAddress": SOL}), SOL).is_err());
}

#[test]
fn disabled_preparations_emit_no_execution_routes() {
    let a = run_tool::<tools::FluxbeamPrepare>(
        &app(),
        json!({}),
        TestCtxBuilder::new("dottie_fluxbeam_prepare").build(),
    )
    .unwrap();
    let b = run_tool::<tools::SolomonPrepare>(
        &app(),
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
    let assets = |q: &str| {
        run_tool::<tools::Assets>(
            &app(),
            json!({ "query": q }),
            TestCtxBuilder::new("dottie_assets").build(),
        )
        .unwrap()
        .value
    };
    assert_eq!(
        assets("NVDAx")["assets"][0]["mint"],
        "Xsc9qvGR1efVDFGLrVsmkzv3qi45LTBjeUKSPmx9qEh"
    );
    assert_eq!(assets("usdc")["assets"][0]["kind"], "stable");
    assert_eq!(
        assets("5h9W6mpNmz8MeG3fgjLrajHSB2BrRSuGeWGD61tLmoon")["status"],
        "not_found"
    );
}

#[test]
fn manifest_skills_validate_against_tools() {
    let m = app().manifest();
    assert_eq!(m.sdk_version, "7.0.0");
    assert_eq!(m.ffi_version, Some(1));
    assert_eq!(m.namespaces.clone().unwrap(), vec!["aomi-core", "svm-core"]);
    validate_app_skills_with_tools(&m.name, &m.skills, &m.tools)
        .expect("skills validate against tools");
    for tool in &m.tools {
        if tool.parameters_schema["properties"] == json!({}) {
            assert_eq!(tool.parameters_schema["required"], json!([]));
        }
    }
}

#[test]
fn about_reports_build_and_session() {
    let ctx = TestCtxBuilder::new("dottie_about")
        .session_id("s-1")
        .build();
    let out = run_tool::<tools::About>(&app(), json!({}), ctx)
        .unwrap()
        .value;
    assert_eq!(out["app"], "dottie");
    assert_eq!(out["build"], dottie::BUILD);
    assert_eq!(out["sdk"], aomi_sdk::AOMI_SDK_VERSION);
    assert_eq!(out["session_id"], "s-1");
}

#[test]
fn telegram_config_provisions_solana_wallets_and_routes_shortcuts_to_direct_tools() {
    let config: toml::Value = toml::from_str(include_str!("../aomi.toml")).unwrap();
    let wallets = &config["wallets"];
    assert_eq!(wallets["agent"].as_bool(), Some(true));
    assert_eq!(wallets["create"].as_bool(), Some(true));
    assert_eq!(wallets["suggest"].as_str(), Some("instant"));
    assert_eq!(
        wallets["families"].as_array().unwrap(),
        &[toml::Value::String("svm".into())]
    );
    let manifest = app().manifest();
    let shortcuts = config["shortcuts"].as_array().unwrap();
    for shortcut in shortcuts {
        let tool = manifest
            .tools
            .iter()
            .find(|t| Some(t.name.as_str()) == shortcut["tool"].as_str())
            .unwrap_or_else(|| panic!("{shortcut}"));
        assert_eq!(tool.namespace.as_deref(), Some("direct"), "{}", tool.name);
        let arg = shortcut["arg"].as_str().unwrap();
        assert!(
            tool.parameters_schema["properties"].get(arg).is_some(),
            "{arg}"
        );
    }
    let commands: Vec<&str> = shortcuts
        .iter()
        .filter_map(|s| s.get("command").and_then(|c| c.as_str()))
        .collect();
    assert_eq!(commands, ["buy", "sell", "positions", "settings"]);

    let pattern = regex::Regex::new(shortcuts[0]["pattern"].as_str().unwrap()).unwrap();
    assert_eq!(&pattern.captures(&format!(" {SOL}\n")).unwrap()[1], SOL);
    for input in [
        "0x17c4365eb2bac646c1903f398e7f2e047b1e473a",
        "buy SOL",
        "So11111111111111111111111111111111111111110",
    ] {
        assert!(!pattern.is_match(input), "{input}");
    }
    // A base58-shaped value can still have the wrong decoded size. The tool
    // rejects it before making an external request.
    let invalid = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
    assert!(pattern.is_match(invalid));
    let out = run_tool::<tools::Preview>(
        &app(),
        json!({ "token": invalid }),
        TestCtxBuilder::new("dottie_preview").build(),
    )
    .unwrap();
    assert!(out.value.get("tx").is_none());
    assert!(out.value["text"].as_str().unwrap().contains("not a Solana"));
}

#[test]
fn workspace_and_app_share_the_sdk7_pin() {
    let root: toml::Value = toml::from_str(include_str!("../../../Cargo.toml")).unwrap();
    let app: toml::Value = toml::from_str(include_str!("../Cargo.toml")).unwrap();
    assert_eq!(
        root["workspace"]["dependencies"]["aomi-sdk"].as_str(),
        Some("=7.0.0")
    );
    assert_eq!(
        app["dependencies"]["aomi-sdk"]["workspace"].as_bool(),
        Some(true)
    );
}
