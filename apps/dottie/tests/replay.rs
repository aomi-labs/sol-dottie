//! Whole tools on recorded provider responses, no network: every reply has
//! the envelope, fits the size limit, carries no nulls and keeps its key
//! facts. Re-record with `scripts/dottie-record-replay.py`.
use aomi_sdk::testing::{TestCtxBuilder, run_tool};
use dottie::runtime::{DottieApp, FOMO_KEY, GMGN_KEY};
use dottie::tools::*;
use dottie::view::{MAX_REPLY, size};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::OnceLock;

const BONK: &str = "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263";
const FRESH: &str = "HsXo7d9bFQZTbhAquXJBmykCJoPnzZVnx92GNrZTpump";
const KOL: &str = "9St6ETbe3CFitw6UNSd8kg7kZ6STXy71wEGiERqQj89U";
const FLOAT: &str = "Bv1a7cEj5aRnJQhMw4kRQqAFesMSd5LwcDnUzq52BMmr";

fn recorded() -> BTreeMap<String, Value> {
    serde_json::from_str(include_str!("fixtures/replay.json")).unwrap()
}

fn app() -> &'static DottieApp {
    static APP: OnceLock<DottieApp> = OnceLock::new();
    APP.get_or_init(|| DottieApp::replaying(recorded()))
}

fn call<T: aomi_sdk::DynAomiTool<App = DottieApp>>(args: Value) -> Value {
    let mut ctx = TestCtxBuilder::new(T::NAME).session_id("replay");
    for name in [GMGN_KEY, FOMO_KEY] {
        ctx = ctx.secret(name, "recorded");
    }
    let out = run_tool::<T>(app(), args, ctx.build()).unwrap().value;
    assert!(
        size(&out) <= MAX_REPLY,
        "{} is {} chars",
        T::NAME,
        size(&out)
    );
    assert_ne!(out["status"], "error", "{}: {out}", T::NAME);
    assert!(out["as_of"].is_i64());
    assert!(
        !out.to_string().contains("null"),
        "{} has a null: {out}",
        T::NAME
    );
    out
}

fn is_mint(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| dottie::providers::pubkey(s).is_ok())
}

#[test]
fn signals_name_the_buyers_with_solana_wallets() {
    let out = call::<Signals>(json!({}));
    let coins = out["coins"].as_array().unwrap();
    assert!(!coins.is_empty());
    assert!(coins.iter().all(|c| is_mint(&c["token"])));
    let lines: Vec<String> = coins
        .iter()
        .flat_map(|c| c["buyers"].as_array().cloned().unwrap_or_default())
        .filter_map(|b| b.as_str().map(str::to_string))
        .collect();
    assert!(
        lines
            .iter()
            .all(|l| (l.contains("(kol, ") || l.contains("(smart, ")) && l.contains(" ago"))
    );
}

#[test]
fn established_board_has_no_base_assets() {
    let out = call::<Scan>(json!({ "board": "established" }));
    let rows = out["rows"].as_array().unwrap();
    assert!(!rows.is_empty());
    for row in rows {
        assert!(is_mint(&row["token"]));
        assert!(!dottie::domain::market::is_base_asset(
            row["token"].as_str().unwrap()
        ));
        assert!(row["liquidity_usd"].as_f64().unwrap() >= 200_000.0, "{row}");
    }
}

#[test]
fn launchpad_filter_keeps_only_pump_curves() {
    let out = call::<Scan>(json!({ "board": "near_graduation", "launchpad": "pump.fun" }));
    for row in out["rows"].as_array().unwrap() {
        assert_eq!(row["launchpad"], "pump.fun");
        assert_eq!(row["stage"], "curve");
        assert!(row["curve_pct"].as_f64().unwrap() < 100.0);
    }
}

#[test]
fn find_hides_lookalikes_and_ranks_exact_symbols() {
    let out = call::<Find>(json!({ "query": "bonk" }));
    let candidates = out["candidates"].as_array().unwrap();
    assert!(candidates.iter().all(|c| is_mint(&c["token"])));
    assert_eq!(candidates[0]["match"], "exact_symbol");
}

#[test]
fn established_card_has_links_authorities_and_an_exit() {
    let out = call::<Token>(json!({ "token": BONK }));
    assert_eq!(out["token"], BONK);
    assert_eq!(out["stage"], "pool");
    assert_eq!(out["security"]["mint_renounced"], true);
    assert_eq!(out["security"]["freeze_renounced"], true);
    assert_eq!(out["security"]["transfer_fee_pct"], 0.0);
    assert_eq!(
        out["links"]["gmgn"],
        format!("https://gmgn.ai/sol/token/{BONK}")
    );
    assert_eq!(
        out["links"]["solscan"],
        format!("https://solscan.io/token/{BONK}")
    );
    assert_eq!(out["exit_check"]["result"], "ok");
    assert!(out["fomo"]["allowlisted"].is_boolean());
}

#[test]
fn fresh_card_reads_the_launch_and_dev() {
    let out = call::<Token>(json!({ "token": FRESH }));
    assert_eq!(out["launchpad"], "pump.fun");
    assert!(out["dev"]["wallet"].is_string());
    assert!(out["holders"].is_i64() && out["fdv_usd"].is_number());
    assert!(out["rugcheck"].is_object());
    assert!(out["sources"].as_array().unwrap().contains(&json!("gmgn")));
}

#[test]
fn trade_mode_combines_security_and_the_exact_exit_without_research() {
    let out = call::<Token>(json!({"token":FRESH,"mode":"trade","sol_amount":"0.1"}));
    assert_eq!(out["mode"], "trade");
    // The coin's default is computed here and carried into the stage args.
    let bps = out["slippage_bps"].as_u64().unwrap();
    assert!(bps == 300 || bps == 500, "{bps}");
    assert_eq!(
        out["stage"],
        json!({"input_token":"SOL","output_token":FRESH,"amount":"0.1","slippage_bps":bps})
    );
    assert_eq!(out["exit_check"]["sol_in"], "0.1");
    assert!(out["checks_passed"].is_boolean());
    for field in ["dev", "socials", "ath_usd", "buy_1h_usd", "rugcheck"] {
        assert!(out.get(field).is_none(), "unexpected research: {field}");
    }
}

#[test]
fn dev_record_lists_launches() {
    let out = call::<Holders>(json!({ "token": FRESH, "view": "dev" }));
    assert!(is_mint(&out["dev"]));
    assert_eq!(out["launches"][0]["this_token"], true);
}

#[test]
fn watcher_numbers_are_flat() {
    let out = call::<Check>(json!({ "token": FRESH }));
    assert!(out["graduated"].is_u64());
    assert!(out.as_object().unwrap().values().all(|v| !v.is_object()));
}

#[test]
fn wallet_record_is_gmgn() {
    let out = call::<Wallet>(json!({ "wallet": KOL }));
    assert!(out["record"]["win_rate_pct"].as_f64().is_some());
}

#[test]
fn positions_read_both_token_programs_with_sol() {
    let out = call::<Positions>(json!({ "wallet": KOL }));
    assert!(out["native_sol"].as_f64().is_some());
    let items = out["items"].as_array().unwrap();
    assert!(!items.is_empty());
    assert!(items.iter().all(|i| is_mint(&i["token"])));
}

#[test]
fn history_rows_are_indexed_solana_signatures() {
    let out = call::<History>(json!({ "wallet": KOL }));
    let items = out["items"].as_array().unwrap();
    assert!(!items.is_empty(), "{out}");
    for row in items {
        assert!(dottie::providers::signature(row["tx_hash"].as_str().unwrap()).is_ok());
        assert!(row["indexed_value_usd"].is_number());
    }
}

#[test]
fn fomo_people_modes_answer() {
    let leaders = call::<People>(json!({"mode":"leaders","window":"24h"}));
    assert!(!leaders["traders"].as_array().unwrap().is_empty());
    let trending = call::<People>(json!({"mode":"trending"}));
    assert!(
        trending["tokens"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| is_mint(&t["token"]))
    );
    let theses = call::<People>(json!({"mode":"theses","token":FLOAT}));
    assert!(!theses["theses"].as_array().unwrap().is_empty());
}

#[test]
fn bad_input_is_an_error_the_model_can_fix() {
    let ctx = TestCtxBuilder::new("dottie_token").build();
    for token in ["BONK", "0x5fc5360d0400a0fd4f2af552add042d716f1d168"] {
        let out = run_tool::<Token>(app(), json!({ "token": token }), ctx.clone())
            .unwrap()
            .value;
        assert_eq!(out["error"]["code"], "INVALID_ARGUMENT");
    }
}

#[test]
fn trade_validates_size_and_slippage_before_any_reads() {
    let app = DottieApp::replaying(BTreeMap::new());
    for extra in [
        json!({}),
        json!({"sol_amount":"0"}),
        json!({"sol_amount":"-1"}),
        json!({"sol_amount":"0.1","usd_amount":1}),
        json!({"usd_amount":0}),
        json!({"sol_amount":"0.1","slippage_bps":0}),
        json!({"sol_amount":"0.1","slippage_bps":501}),
    ] {
        let mut args = json!({"token":FRESH,"mode":"trade"});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let out = run_tool::<Token>(&app, args, TestCtxBuilder::new("dottie_token").build())
            .unwrap()
            .value;
        assert_eq!(out["error"]["code"], "INVALID_ARGUMENT", "{out}");
    }
    // Percent slippage is gone: the stage carries basis points only.
    let pct = json!({"token":FRESH,"mode":"trade","sol_amount":"0.1","slippage_pct":5});
    assert!(run_tool::<Token>(&app, pct, TestCtxBuilder::new("dottie_token").build()).is_err());
}
