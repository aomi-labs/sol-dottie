//! Solomon USDv: the verified rewards model and a wallet's enrollment and
//! holdings. Enrollment stays disabled until the proof flow is rehearsed.
use crate::providers::{fluxbeam::unavailable, get_json, pubkey};
use crate::runtime::{Fail, Runtime, Source, Turn};
use serde_json::{Value, json};
use std::time::Duration;

pub const USDV: &str = "USDvUSpnhCr9yBgj3UyVrD239HRUv4RsHwH2FxsWuMk";
pub fn product() -> Value {
    json!({"source":"https://docs.solomonlabs.org/usdv/how-it-works","verified_on":"2026-11-02","product":{"name":"Solomon USDv","mint":USDV,"decimals":6,"model":"Hold USDv and opt in to eligible rewards; no staking, wrapping or deposit into the old sUSDv vault.","apr":null,"enrollment":"One-time ownership proof and current terms; the current app uses challenge/proof APIs.","product_url":"https://app.solomonlabs.org/","direct_mint":"Approved counterparties only; not an app action.","supported_actions":["product_info","wallet_status"],"write_status":"disabled_pending_verified_proof_flow"}})
}
pub async fn position(rt: &Runtime, turn: &Turn, wallet: &str) -> Result<Value, Fail> {
    let wallet = pubkey(wallet)?;
    let url = format!("https://yaas.solomonlabs.io/api/yaas/wallet/{wallet}?view=v2");
    let raw = get_json(rt, turn, Source::Solomon, url, Duration::from_secs(10)).await?;
    normalize(&raw, wallet)
}
pub fn normalize(raw: &Value, wallet: &str) -> Result<Value, Fail> {
    if raw["walletAddress"].as_str() != Some(wallet)
        || !raw["observation"].is_object()
        || !raw["programPositions"].is_array()
        || !raw["holdings"].is_array()
        || raw.get("account").is_none()
    {
        return Err(Fail::bad(
            Source::Solomon,
            "invalid or mismatched wallet response",
        ));
    }
    let account = &raw["account"];
    if !account.is_null() && account["address"].as_str() != Some(wallet) {
        return Err(Fail::bad(
            Source::Solomon,
            "account does not match requested wallet",
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
    unavailable(
        "ENROLLMENT_PROOF_UNVERIFIED",
        "Solomon enrollment is disabled: the current challenge, ownership-proof, terms and submission contract has not been rehearsed with Aomi. Use Solomon's official app. No old vault deposit or signature request is substituted.",
    )
}
