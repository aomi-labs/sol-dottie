//! FluxBeam direct quotes for the verified WSOL/FLUXB pair. Preparation
//! stays disabled until the full payload is validated through the host.
use crate::providers::{SOL, get_json, pubkey};
use crate::runtime::{Fail, Runtime, Source, Turn};
use serde_json::{Value, json};
use std::time::Duration;

pub const FLUXB: &str = "FLUXBmPhT3Fd1EDVFdg46YREqHBeNypn1h4EbnTzWERX";
pub const FLUX_PROGRAM: &str = "FLUXubRmkEi2q6K3Y9kBPg9248ggaZVsoSFhtJHSrm1X";
pub const FLUX_POOL: &str = "GeNc51GBjhXiYruWrx6VPh6xMNeTjYb5hUBpFJi9cW27";

pub async fn quote(
    rt: &Runtime,
    turn: &Turn,
    input: &str,
    output: &str,
    amount: &str,
    slippage: u16,
) -> Result<Value, Fail> {
    pair(input, output)?;
    let amount = base_units(amount)?;
    if !(1..=100).contains(&slippage) {
        return Err(Fail::Invalid("Slippage must be 1–100 basis points".into()));
    }
    let url = format!(
        "https://api.fluxbeam.xyz/v1/quote?inputMint={input}&outputMint={output}&amount={amount}&slippageBps={slippage}"
    );
    let raw = get_json(rt, turn, Source::Fluxbeam, url, Duration::ZERO).await?;
    normalize(&raw, input, output, amount, slippage)
}

/// An integer amount in base units, bounded for the demo.
pub fn base_units(value: &str) -> Result<u64, Fail> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Fail::Invalid(
            "Amount must be an unsigned integer string in base units".into(),
        ));
    }
    value
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0 && *n <= 100_000_000)
        .ok_or_else(|| {
            Fail::Invalid("Demo amount must be positive and at most 100000000 base units".into())
        })
}

/// Only the verified pair; never a silent venue substitute.
pub fn pair(input: &str, output: &str) -> Result<(), Fail> {
    if (input == SOL && output == FLUXB) || (input == FLUXB && output == SOL) {
        Ok(())
    } else {
        Err(Fail::Invalid(
            "Direct FluxBeam supports only the verified WSOL/FLUXB pair; never silently substitute a venue".into(),
        ))
    }
}
pub fn normalize(
    raw: &Value,
    input: &str,
    output: &str,
    amount: u64,
    slippage: u16,
) -> Result<Value, Fail> {
    let q = &raw["quote"];
    if q["inputMint"].as_str() != Some(input)
        || q["outputMint"].as_str() != Some(output)
        || q["amountIn"].as_u64() != Some(amount)
        || q["program"].as_str() != Some(FLUX_PROGRAM)
        || q["pool"].as_str() != Some(FLUX_POOL)
    {
        return Err(Fail::bad(
            Source::Fluxbeam,
            "quote does not match the requested pair, amount, verified pool and program",
        ));
    }
    let out = q["outAmount"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| Fail::NoRoute("No positive output quote".into()))?;
    let min = q["minimumOut"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= out)
        .ok_or_else(|| Fail::bad(Source::Fluxbeam, "invalid minimum output"))?;
    let floor = (u128::from(out) * u128::from(10_000 - slippage)) / 10_000;
    if u128::from(min) < floor {
        return Err(Fail::bad(
            Source::Fluxbeam,
            "minimum output exceeds requested slippage",
        ));
    }
    Ok(
        json!({"venue":"FluxBeam","input_mint":input,"output_mint":output,"amount_in_base_units":amount.to_string(),"quoted_out_base_units":out.to_string(),"minimum_out_base_units":min.to_string(),"slippage_bps":slippage,"pool":FLUX_POOL,"program":FLUX_PROGRAM,"fees":null,"executable":false,"note":"Indicative direct quote. FLUXB has Token-2022 transfer fees; net wallet output and full transaction payload are not verified. Preparation is disabled; never label this a completed trade."}),
    )
}
pub fn prepare() -> Value {
    unavailable(
        "PAYLOAD_VERIFICATION_REQUIRED",
        "Direct quotes work. FluxBeam write preparation remains disabled until ATA setup, SOL wrapping, Token-2022 fees and the full payload are independently validated and rehearsed through the host. No transaction was built or sent.",
    )
}

/// A disabled write path: what is missing, and no transaction.
pub fn unavailable(code: &str, reason: &str) -> Value {
    json!({"status":"unavailable","error":{"code":code,"message":reason},"transaction":null})
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
) -> Result<(), Fail> {
    use base64::Engine;
    pubkey(payer)?;
    if now < observed_at || now - observed_at > 20 {
        return Err(Fail::Invalid(
            "Quote must be refreshed before preparation".into(),
        ));
    }
    let q = &quote["quote"];
    let ix = &raw["instruction"];
    if ix["programId"].as_str() != Some(FLUX_PROGRAM) || q["program"].as_str() != Some(FLUX_PROGRAM)
    {
        return Err(Fail::Invalid("Unexpected swap program".into()));
    }
    let accounts = ix["accounts"]
        .as_array()
        .filter(|a| a.len() == 14)
        .ok_or_else(|| Fail::Invalid("Expected 14 swap accounts".into()))?;
    for (index, expected) in [
        (0, Some(FLUX_POOL)),
        (2, Some(payer)),
        (9, q["inputMint"].as_str()),
        (10, q["outputMint"].as_str()),
    ] {
        if expected.is_none() || accounts[index]["PublicKey"].as_str() != expected {
            return Err(Fail::Invalid(
                "Wrong pool, payer or mint in swap instruction".into(),
            ));
        }
    }
    for (i, account) in accounts.iter().enumerate() {
        pubkey(account["PublicKey"].as_str().unwrap_or(""))?;
        if account["IsSigner"].as_bool() != Some(i == 2) {
            return Err(Fail::Invalid("Unexpected signer".into()));
        }
    }
    let bytes = ix["data"]
        .as_str()
        .and_then(|s| base64::engine::general_purpose::STANDARD.decode(s).ok())
        .filter(|b| b.len() == 17 && b[0] == 1)
        .ok_or_else(|| Fail::Invalid("Unexpected swap instruction encoding".into()))?;
    let amount = u64::from_le_bytes(bytes[1..9].try_into().unwrap());
    let minimum = u64::from_le_bytes(bytes[9..17].try_into().unwrap());
    if q["amountIn"].as_u64() != Some(amount) || q["minimumOut"].as_u64() != Some(minimum) {
        return Err(Fail::Invalid("Swap amounts differ from quote".into()));
    }
    Ok(())
}
