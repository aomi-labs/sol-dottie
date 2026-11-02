//! Real app-tool read smoke. No signing, mutation or host deployment.
use aomi_sdk::{DynAomiApp, DynToolCallCtx};
use dottie::runtime::DottieApp;
use serde_json::{Value, json};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let tool = args.get(1).map(String::as_str).unwrap_or("dottie_assets");
    let input: Value = serde_json::from_str(args.get(2).map(String::as_str).unwrap_or("{}"))?;
    let ctx = DynToolCallCtx {
        session_id: "local-read-smoke".into(),
        tool_name: tool.into(),
        call_id: "read-1".into(),
        state_attributes: Default::default(),
        secrets: Default::default(),
    };
    let app = DottieApp;
    if tool == "manifest" {
        println!("{}", serde_json::to_string_pretty(&app.manifest())?);
        return Ok(());
    }
    let sink = aomi_sdk::DynAsyncSink::__from_queue(std::sync::Arc::default());
    let value = match app.start_tool(
        tool,
        &input.to_string(),
        &serde_json::to_string(&ctx)?,
        sink,
    ) {
        aomi_sdk::DynToolDispatch::Ready(aomi_sdk::DynToolResult::Ok(value)) => value,
        aomi_sdk::DynToolDispatch::Ready(aomi_sdk::DynToolResult::Err(message)) => {
            return Err(message.into());
        }
        _ => return Err("Unexpected asynchronous tool".into()),
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    if value.get("status") != Some(&json!("ok")) {
        std::process::exit(2);
    }
    Ok(())
}
