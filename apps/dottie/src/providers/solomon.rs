use crate::{
    domain::{USDV, pubkey},
    runtime::{Failure, Runtime},
    view,
};
use serde_json::{Value, json};
use std::time::Duration;
pub fn product() -> Value {
    json!({"status":"ok","source":"https://docs.solomonlabs.org/usdv/how-it-works","verified_on":"2026-11-02","data":{"name":"Solomon USDv","mint":USDV,"decimals":6,"model":"Hold USDv and opt in to eligible rewards; no staking, wrapping or deposit into the old sUSDv vault.","apr":null,"enrollment":"One-time ownership proof and current terms; the current app uses challenge/proof APIs.","product_url":"https://app.solomonlabs.org/","direct_mint":"Approved counterparties only; not an app action.","supported_actions":["product_info","wallet_status"],"write_status":"disabled_pending_verified_proof_flow"}})
}
pub async fn position(rt: &Runtime, wallet: &str) -> Result<Value, Failure> {
    pubkey(wallet)?;
    let url = format!("https://yaas.solomonlabs.io/api/yaas/wallet/{wallet}?view=v2");
    let read = rt.get(2, &url, Duration::from_secs(10)).await?;
    Ok(view::sourced(&read, &url, normalize(&read.value, wallet)?))
}
pub fn normalize(raw: &Value, wallet: &str) -> Result<Value, Failure> {
    if raw["walletAddress"].as_str() != Some(wallet)
        || !raw["observation"].is_object()
        || !raw["programPositions"].is_array()
        || !raw["holdings"].is_array()
        || raw.get("account").is_none()
    {
        return Err(Failure::new(
            "MALFORMED_RESPONSE",
            "Solomon returned an invalid or mismatched wallet response",
        ));
    }
    let account = &raw["account"];
    if !account.is_null() && account["address"].as_str() != Some(wallet) {
        return Err(Failure::new(
            "MALFORMED_RESPONSE",
            "Solomon account does not match requested wallet",
        ));
    }
    let summary = if account.is_null() {
        Value::Null
    } else {
        json!({"status":account["status"],"is_rewards_eligible":account["isRewardsEligible"],"distribution_mode":account["distributionMode"],"rewards_destination":account["rewardsDestinationAddress"],"total_earned_rewards":account["totalEarnedRewards"],"total_positions_value":account["totalPositionsValue"]})
    };
    Ok(
        json!({"wallet":wallet,"account":summary,"holdings":raw["holdings"].as_array().unwrap().iter().take(10).collect::<Vec<_>>(),"positions_count":raw["programPositions"].as_array().unwrap().len(),"positions":raw["programPositions"].as_array().unwrap().iter().take(5).collect::<Vec<_>>(),"observation":raw["observation"],"note":"Null amounts and partial observations are unknown, not zero. Having USDv does not prove rewards eligibility."}),
    )
}
pub fn prepare() -> Value {
    view::unavailable(
        "ENROLLMENT_PROOF_UNVERIFIED",
        "Solomon enrollment is disabled: the current challenge, ownership-proof, terms and submission contract has not been rehearsed with Aomi. Use Solomon's official app. No old vault deposit or signature request is substituted.",
    )
}
