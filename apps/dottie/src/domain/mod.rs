use crate::runtime::Failure;
pub const SOL: &str = "So11111111111111111111111111111111111111112";
pub const FLUXB: &str = "FLUXBmPhT3Fd1EDVFdg46YREqHBeNypn1h4EbnTzWERX";
pub const FLUX_PROGRAM: &str = "FLUXubRmkEi2q6K3Y9kBPg9248ggaZVsoSFhtJHSrm1X";
pub const FLUX_POOL: &str = "GeNc51GBjhXiYruWrx6VPh6xMNeTjYb5hUBpFJi9cW27";
pub const USDV: &str = "USDvUSpnhCr9yBgj3UyVrD239HRUv4RsHwH2FxsWuMk";
pub fn pubkey(value: &str) -> Result<&str, Failure> {
    if value.len() > 44
        || bs58::decode(value)
            .into_vec()
            .map_or(true, |b| b.len() != 32)
    {
        return Err(Failure::new(
            "INVALID_ARGUMENT",
            "Expected a 32-byte Solana base58 address",
        ));
    }
    Ok(value)
}
pub fn amount(value: &str) -> Result<u64, Failure> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Failure::new(
            "INVALID_ARGUMENT",
            "Amount must be an unsigned integer string in base units",
        ));
    }
    value
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0 && *n <= 100_000_000)
        .ok_or_else(|| {
            Failure::new(
                "INVALID_ARGUMENT",
                "Demo amount must be positive and at most 100000000 base units",
            )
        })
}
pub fn pair(input: &str, output: &str) -> Result<(), Failure> {
    if (input == SOL && output == FLUXB) || (input == FLUXB && output == SOL) {
        Ok(())
    } else {
        Err(Failure::new(
            "UNSUPPORTED_PAIR",
            "Direct FluxBeam currently supports only the verified WSOL/FLUXB pair; never silently substitute a venue",
        ))
    }
}
