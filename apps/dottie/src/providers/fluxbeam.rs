use crate::{
    domain::{self, FLUX_POOL, FLUX_PROGRAM},
    runtime::{Failure, Runtime},
    view,
};
use serde_json::{Value, json};
use std::time::Duration;
pub async fn quote(
    rt: &Runtime,
    input: &str,
    output: &str,
    amount: &str,
    slippage: u16,
) -> Result<Value, Failure> {
    domain::pair(input, output)?;
    let amount = domain::amount(amount)?;
    if !(1..=100).contains(&slippage) {
        return Err(Failure::new(
            "INVALID_ARGUMENT",
            "Slippage must be 1–100 basis points",
        ));
    }
    let url = format!(
        "https://api.fluxbeam.xyz/v1/quote?inputMint={input}&outputMint={output}&amount={amount}&slippageBps={slippage}"
    );
    let read = rt.get(1, &url, Duration::ZERO).await?;
    let q = normalize(&read.value, input, output, amount, slippage)?;
    Ok(view::sourced(&read, "https://api.fluxbeam.xyz/v1/quote", q))
}
pub fn normalize(
    raw: &Value,
    input: &str,
    output: &str,
    amount: u64,
    slippage: u16,
) -> Result<Value, Failure> {
    let q = &raw["quote"];
    if q["inputMint"].as_str() != Some(input)
        || q["outputMint"].as_str() != Some(output)
        || q["amountIn"].as_u64() != Some(amount)
        || q["program"].as_str() != Some(FLUX_PROGRAM)
        || q["pool"].as_str() != Some(FLUX_POOL)
    {
        return Err(Failure::new(
            "MALFORMED_RESPONSE",
            "FluxBeam quote does not match the requested pair, amount, verified pool and program",
        ));
    }
    let out = q["outAmount"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| Failure::new("NO_LIQUIDITY", "No positive output quote"))?;
    let min = q["minimumOut"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= out)
        .ok_or_else(|| Failure::new("MALFORMED_RESPONSE", "Invalid minimum output"))?;
    let floor = (u128::from(out) * u128::from(10_000 - slippage)) / 10_000;
    if u128::from(min) < floor {
        return Err(Failure::new(
            "MALFORMED_RESPONSE",
            "Provider minimum output exceeds requested slippage",
        ));
    }
    Ok(
        json!({"venue":"FluxBeam","input_mint":input,"output_mint":output,"amount_in_base_units":amount.to_string(),"quoted_out_base_units":out.to_string(),"minimum_out_base_units":min.to_string(),"slippage_bps":slippage,"pool":FLUX_POOL,"program":FLUX_PROGRAM,"fees":null,"executable":false,"note":"Indicative direct quote. FLUXB has Token-2022 transfer fees; net wallet output and full transaction payload are not verified. Preparation is disabled; never label this a completed trade."}),
    )
}
pub fn prepare() -> Value {
    view::unavailable(
        "PAYLOAD_VERIFICATION_REQUIRED",
        "Direct quotes work. FluxBeam write preparation remains disabled until ATA setup, SOL wrapping, Token-2022 fees and the full payload are independently validated and rehearsed through the host. No transaction was built or sent.",
    )
}

/// Offline contract check for the documented single swap instruction.
/// This does NOT validate token-account ownership, ATA setup, wrapping or a full
/// transaction. Consequently it cannot enable the public prepare tool by itself.
pub fn validate_instruction(
    raw: &Value,
    quote: &Value,
    payer: &str,
    observed_at: u64,
    now: u64,
) -> Result<(), Failure> {
    use base64::Engine;
    domain::pubkey(payer)?;
    if now < observed_at || now - observed_at > 20 {
        return Err(Failure::new(
            "STALE_QUOTE",
            "Quote must be refreshed before preparation",
        ));
    }
    let q = &quote["quote"];
    let ix = &raw["instruction"];
    if ix["programId"].as_str() != Some(FLUX_PROGRAM) || q["program"].as_str() != Some(FLUX_PROGRAM)
    {
        return Err(Failure::new("PAYLOAD_MISMATCH", "Unexpected swap program"));
    }
    let accounts = ix["accounts"]
        .as_array()
        .filter(|a| a.len() == 14)
        .ok_or_else(|| Failure::new("PAYLOAD_MISMATCH", "Expected 14 swap accounts"))?;
    for (index, expected) in [
        (0, Some(FLUX_POOL)),
        (2, Some(payer)),
        (9, q["inputMint"].as_str()),
        (10, q["outputMint"].as_str()),
    ] {
        if expected.is_none() || accounts[index]["PublicKey"].as_str() != expected {
            return Err(Failure::new(
                "PAYLOAD_MISMATCH",
                "Wrong pool, payer or mint in swap instruction",
            ));
        }
    }
    for (i, account) in accounts.iter().enumerate() {
        domain::pubkey(account["PublicKey"].as_str().unwrap_or(""))?;
        if account["IsSigner"].as_bool() != Some(i == 2) {
            return Err(Failure::new("PAYLOAD_MISMATCH", "Unexpected signer"));
        }
    }
    let bytes = ix["data"]
        .as_str()
        .and_then(|s| base64::engine::general_purpose::STANDARD.decode(s).ok())
        .filter(|b| b.len() == 17 && b[0] == 1)
        .ok_or_else(|| Failure::new("PAYLOAD_MISMATCH", "Unexpected swap instruction encoding"))?;
    let amount = u64::from_le_bytes(bytes[1..9].try_into().unwrap());
    let minimum = u64::from_le_bytes(bytes[9..17].try_into().unwrap());
    if q["amountIn"].as_u64() != Some(amount) || q["minimumOut"].as_u64() != Some(minimum) {
        return Err(Failure::new(
            "PAYLOAD_MISMATCH",
            "Swap amounts differ from quote",
        ));
    }
    Ok(())
}
