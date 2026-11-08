//! Real app-tool read smoke. No signing, mutation or host deployment.
//!
//!   cargo run -p dottie --example read -- <tool> '<args json>'
//!   cargo run -p dottie --example read -- --plan tests/fixtures/replay-plan.json
//!
//! Secrets come from the environment (GMGN_API_KEY, FOMO_API_KEY,
//! JUPITER_API_KEY, SOLANA_RPC_URL), never from the repo. A plan runs in one
//! process, so every call shares one provider gate. With DOTTIE_RECORD=<file>
//! every provider response is saved for the offline replay test.
use aomi_sdk::{DynAomiApp, DynToolCallCtx};
use dottie::runtime::DottieApp;
use dottie::view::MAX_REPLY;
use serde_json::{Value, json};

fn ctx(tool: &str, step: &Value) -> DynToolCallCtx {
    let mut attributes = serde_json::Map::new();
    let wallet = step["wallet"]
        .as_str()
        .map(str::to_string)
        .or_else(|| std::env::var("DOTTIE_WALLET").ok());
    if let Some(w) = wallet {
        attributes.insert("domain".into(), json!({"svm": {"address": w}}));
    }
    DynToolCallCtx {
        session_id: "local-read-smoke".into(),
        tool_name: tool.into(),
        call_id: "read-1".into(),
        state_attributes: attributes,
        secrets: [
            "GMGN_API_KEY",
            "FOMO_API_KEY",
            "JUPITER_API_KEY",
            "SOLANA_RPC_URL",
        ]
        .into_iter()
        .filter_map(|k| Some((k.to_string(), std::env::var(k).ok()?)))
        .collect(),
    }
}

fn call(app: &DottieApp, tool: &str, args: &Value, step: &Value) -> Result<Value, String> {
    let sink = aomi_sdk::DynAsyncSink::__from_queue(std::sync::Arc::default());
    let ctx = serde_json::to_string(&ctx(tool, step)).map_err(|e| e.to_string())?;
    match app.start_tool(tool, &args.to_string(), &ctx, sink) {
        aomi_sdk::DynToolDispatch::Ready(aomi_sdk::DynToolResult::Ok(value)) => Ok(value),
        aomi_sdk::DynToolDispatch::Ready(aomi_sdk::DynToolResult::Err(message)) => Err(message),
        _ => Err("Unexpected asynchronous tool".into()),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let app = DottieApp::default();
    let first = args.get(1).map(String::as_str).unwrap_or("dottie_assets");
    if first == "manifest" {
        println!("{}", serde_json::to_string_pretty(&app.manifest())?);
        return Ok(());
    }
    if first == "--plan" {
        let plan: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(args.get(2).ok_or("plan file")?)?)?;
        let mut failed = vec![];
        for step in &plan {
            let tool = step["tool"].as_str().ok_or("step without tool")?;
            let out = call(&app, tool, &step["args"], step)?;
            let text = serde_json::to_string_pretty(&out)?;
            println!(
                "=== {tool} {} ({} chars)\n{text}\n",
                step["args"],
                text.len()
            );
            if text.len() > MAX_REPLY || out["status"] == "error" {
                failed.push(format!("{tool} {}", step["args"]));
            }
        }
        if let Ok(file) = std::env::var("DOTTIE_RECORD") {
            let recorded = app.runtime()?.recorded();
            std::fs::write(&file, serde_json::to_string_pretty(&recorded)?)?;
            eprintln!("recorded {} responses to {file}", recorded.len());
        }
        if !failed.is_empty() {
            eprintln!("errors or oversized replies:\n{}", failed.join("\n"));
            std::process::exit(2);
        }
        return Ok(());
    }
    let input: Value = serde_json::from_str(args.get(2).map(String::as_str).unwrap_or("{}"))?;
    let value = call(&app, first, &input, &Value::Null)?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    if value.get("status") != Some(&json!("ok")) {
        std::process::exit(2);
    }
    Ok(())
}
