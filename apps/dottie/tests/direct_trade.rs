//! Real direct-tool flow with offline providers: warn, accept, prepare. A
//! checked buy comes back as the host's `stage_jupiter_swap` call, with
//! Jupiter's exact argument names, for the host to stage, simulate and
//! commit.
use aomi_sdk::testing::{TestCtxBuilder, run_tool};
use dottie::providers::{SOL, TOKEN_PROGRAM, USDC};
use dottie::runtime::{DottieApp, GMGN_KEY};
use dottie::tools::{Buy, Sell};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const TOKEN: &str = "uKHabaEbP6T5eSn4jDo6S5Ha8n3GcBgiZDeJzuXZfFz";
const WALLET: &str = "Gm5cm4KpqJYghgwCYZXryMfhFxjMKbTzhs3pVj61f4qv";

fn rpc(method: &str, params: Value) -> String {
    format!("rpc:{method}:{params}")
}

fn accounts(mint: &str, raw: &str, decimals: u8) -> Value {
    json!({"value": [{"account": {"data": {"parsed": {"info": {"mint": mint,
        "tokenAmount": {"amount": raw, "decimals": decimals}}}}}}]})
}

fn quote(input: &str, output: &str, amount: &str, out: &str) -> (String, Value) {
    (
        format!("jupiter:{input}>{output}:{amount}:500"),
        json!({"inAmount": amount, "outAmount": out, "otherAmountThreshold": out,
            "routePlan": [{"swapInfo": {"label": "Pump.fun Amm"}}]}),
    )
}

fn responses() -> BTreeMap<String, Value> {
    let confirmed = json!({"commitment": "confirmed", "encoding": "jsonParsed"});
    let mut out = BTreeMap::from([
        (
            rpc("getAccountInfo", json!([TOKEN, confirmed])),
            json!({"value": {"owner": TOKEN_PROGRAM, "data": {"parsed": {"type": "mint",
                "info": {"decimals": 6, "mintAuthority": null, "freezeAuthority": null}}}}}),
        ),
        (
            format!("gmgn:/v1/token/info?chain=sol&address={TOKEN}"),
            json!({"address": TOKEN, "symbol": "DOT", "liquidity": 40000, "price": {"price": "0.0001"}}),
        ),
        (
            format!("gmgn-live:/v1/token/security?chain=sol&address={TOKEN}"),
            json!({"honeypot": 0, "buy_tax": "0", "sell_tax": "0", "renounced_mint": true, "renounced_freeze_account": true}),
        ),
        (
            rpc("getBalance", json!([WALLET, {"commitment": "confirmed"}])),
            json!({"value": 2_000_000_000u64}),
        ),
        (
            rpc(
                "getTokenAccountsByOwner",
                json!([WALLET, {"mint": USDC}, confirmed]),
            ),
            accounts(USDC, "0", 6),
        ),
        (
            rpc(
                "getTokenAccountsByOwner",
                json!([WALLET, {"mint": TOKEN}, confirmed]),
            ),
            accounts(TOKEN, "5000000000", 6),
        ),
        (
            format!("jupiter:{SOL}>{USDC}:1000000000:50"),
            json!({"inAmount": "1000000000", "outAmount": "90000000", "otherAmountThreshold": "89000000"}),
        ),
    ]);
    out.extend([
        quote(SOL, TOKEN, "100000000", "90000000000"),
        quote(TOKEN, SOL, "2500000000", "95000000"),
    ]);
    // No sell quote back at the bought size: unverified, not a honeypot.
    out.insert(
        format!("jupiter:{TOKEN}>{SOL}:90000000000:500"),
        json!({"$fail": {"code": "NO_ROUTE", "message": "Jupiter found no route", "source": "jupiter"}}),
    );
    out
}

fn ctx(tool: &str) -> aomi_sdk::DynToolCallCtx {
    TestCtxBuilder::new(tool)
        .secret(GMGN_KEY, "recorded")
        .attribute("domain", json!({"svm": {"address": WALLET}}))
        .build()
}

fn buy(recorded: BTreeMap<String, Value>, args: Value) -> Value {
    let app = DottieApp::replaying(recorded);
    run_tool::<Buy>(&app, args, ctx("dottie_buy"))
        .unwrap()
        .value
}

#[test]
fn a_reviewed_buy_returns_the_exact_stage_jupiter_swap_call() {
    let first = buy(responses(), json!({"text": format!("{TOKEN} 0.1")}));
    assert!(first.get("tx").is_none(), "{first}");
    assert!(
        first["text"].as_str().unwrap().contains("unable to exit"),
        "{first}"
    );
    let control = &first["controls"][0];
    assert_eq!(control["label"], "Buy anyway");
    assert_eq!(control["args"]["amount"], "0.1 SOL");
    assert_eq!(control["args"]["slippage_bps"], 500);
    assert_eq!(control["args"]["reviewed_wallet"], WALLET);
    let accepted = buy(responses(), control["args"].clone());
    assert_eq!(
        accepted["tx"],
        json!({
            "tool": "stage_jupiter_swap",
            "args": {
                "input_token": "SOL",
                "output_token": TOKEN,
                "amount": "0.1",
                "slippage_bps": 500,
            }
        }),
        "{accepted}"
    );
    // One button's consent, never a saved bypass.
    let mut changed_wallet = control["args"].clone();
    changed_wallet["reviewed_wallet"] = json!(TOKEN);
    assert!(buy(responses(), changed_wallet).get("tx").is_none());
}

#[test]
fn consent_cannot_create_a_route_or_a_balance() {
    let args =
        buy(responses(), json!({"token": TOKEN, "amount": "0.1"}))["controls"][0]["args"].clone();
    let mut no_route = responses();
    no_route.insert(
        format!("jupiter:{SOL}>{TOKEN}:100000000:500"),
        json!({"$fail": {"code": "NO_ROUTE", "message": "none", "source": "jupiter"}}),
    );
    assert!(buy(no_route, args.clone()).get("tx").is_none());
    let out = buy(responses(), json!({"token": TOKEN, "amount": "5 sol"}));
    assert!(out.get("tx").is_none());
    assert!(
        out["text"].as_str().unwrap().starts_with("Not enough SOL"),
        "{out}"
    );
}

#[test]
fn a_percent_sell_stages_the_exact_token_amount_for_sol() {
    let app = DottieApp::replaying(responses());
    let out = run_tool::<Sell>(
        &app,
        json!({"token": TOKEN, "amount": "50%"}),
        ctx("dottie_sell"),
    )
    .unwrap()
    .value;
    assert_eq!(out["tx"]["tool"], "stage_jupiter_swap", "{out}");
    assert_eq!(out["tx"]["args"]["input_token"], TOKEN);
    assert_eq!(out["tx"]["args"]["output_token"], "SOL");
    assert_eq!(out["tx"]["args"]["amount"], "2500");
    assert_eq!(out["tx"]["args"]["slippage_bps"], 500);
}
