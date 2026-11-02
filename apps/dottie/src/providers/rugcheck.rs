use crate::{
    domain::pubkey,
    runtime::{Failure, Runtime},
    view,
};
use serde_json::{Value, json};
use std::time::Duration;
pub async fn report(rt: &Runtime, mint: &str) -> Result<Value, Failure> {
    pubkey(mint)?;
    let url = format!("https://api.rugcheck.xyz/v1/tokens/{mint}/report");
    let read = rt.get(0, &url, Duration::from_secs(30)).await?;
    Ok(view::sourced(&read, &url, normalize(&read.value, mint)?))
}
pub fn normalize(raw: &Value, mint: &str) -> Result<Value, Failure> {
    if raw["mint"].as_str() != Some(mint) || !raw["risks"].is_array() || !raw["token"].is_object() {
        return Err(Failure::new(
            "MALFORMED_RESPONSE",
            "RugCheck report is missing expected data or names a different mint",
        ));
    }
    let risks: Vec<Value> = raw["risks"].as_array().unwrap().iter().take(12).map(|r| json!({"name":r["name"],"level":r["level"],"description":r["description"],"score":r["score"]})).collect();
    let holders = raw["topHolders"].as_array().map(|h| {
        h.iter()
            .take(5)
            .map(|v| json!({"address":v["address"],"pct":v["pct"],"insider":v["insider"]}))
            .collect::<Vec<_>>()
    });
    Ok(
        json!({"mint":mint,"score":raw["score"],"score_normalised":raw["score_normalised"],
        "risks":risks,"risks_total":raw["risks"].as_array().unwrap().len(),
        "mint_authority":raw["token"]["mintAuthority"],"mint_authority_known":raw["token"].get("mintAuthority").is_some(),
        "freeze_authority":raw["token"]["freezeAuthority"],"freeze_authority_known":raw["token"].get("freezeAuthority").is_some(),
        "top_holders":holders,"liquidity_usd":raw["totalMarketLiquidity"],"token_extensions":raw["token_extensions"],
        "token_detected_at":raw["detectedAt"],"report_generated_at":null,
        "interpretation":"Risk signals, not a guarantee. Missing data is unknown. Token detection time is not report freshness."}),
    )
}
