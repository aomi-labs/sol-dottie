//! Solana market vocabulary: the curated catalog (SOL, stables, LSTs,
//! majors, xStocks), launchpad names and a coin's stage.
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Native,
    Stable,
    Lst,
    Major,
    Stock,
    /// FluxBeam and Solomon tokens: catalogued, but coins like any other.
    Partner,
}

/// One catalog entry (`domain/assets.json`).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Asset {
    pub symbol: String,
    pub name: String,
    pub mint: String,
    pub decimals: u8,
    pub kind: Kind,
    pub program: String,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

pub fn catalog() -> &'static [Asset] {
    static CATALOG: OnceLock<Vec<Asset>> = OnceLock::new();
    CATALOG.get_or_init(|| serde_json::from_str(include_str!("assets.json")).unwrap_or_default())
}

/// The catalog entry for this exact mint.
pub fn known(mint: &str) -> Option<&'static Asset> {
    catalog().iter().find(|a| a.mint == mint.trim())
}

/// The catalog entry with this ticker (`USDC`, `JitoSOL`, `NVDAx`…).
/// `WSOL` and `SOL` are the same entry.
pub fn known_symbol(symbol: &str) -> Option<&'static Asset> {
    let symbol = symbol.trim().trim_start_matches('$');
    let symbol = if symbol.eq_ignore_ascii_case("wsol") {
        "SOL"
    } else {
        symbol
    };
    catalog()
        .iter()
        .find(|a| a.symbol.eq_ignore_ascii_case(symbol))
}

/// SOL, stables, LSTs, majors and stock tokens: priced and routed as base
/// assets, never a meme pick and never under memecoin rules.
pub fn is_base_asset(mint: &str) -> bool {
    known(mint).is_some_and(|a| a.kind != Kind::Partner)
}

/// Two different mints with the same first and last four characters: the
/// newer one is almost always address poisoning.
pub fn same_ends(a: &str, b: &str) -> bool {
    let ends = |x: &str| {
        (
            x.get(..4).map(str::to_string),
            x.get(x.len().saturating_sub(4)..).map(str::to_string),
        )
    };
    a != b && a.len() > 8 && ends(a) == ends(b)
}

/// The catalog asset this mint imitates, if any.
pub fn lookalike(mint: &str) -> Option<&'static str> {
    catalog()
        .iter()
        .find(|a| same_ends(&a.mint, mint))
        .map(|a| a.symbol.as_str())
}

/// One spelling per Solana launchpad, whichever field named it: GMGN's
/// `Pump.fun`, `pump_mayhem`, `ray_launchpad`, `letsbonk`… Plain pools
/// aren't launchpads.
pub fn launchpad(raw: Option<&str>) -> Option<String> {
    let raw = raw?.trim().to_ascii_lowercase();
    if raw.is_empty() || raw.starts_with("pool") {
        return None;
    }
    let flat: String = raw.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let name = match flat.as_str() {
        "pumpfun" | "pump" | "pumpmayhem" => "pump.fun",
        "raylaunchpad" | "launchlab" | "raydiumlaunchlab" => "launchlab",
        "letsbonk" | "bonk" | "bonkfun" | "letsbonkfun" => "letsbonk",
        "stonkfun" | "stonk" => "stonk.fun",
        "meteoravirtualcurve" | "meteoradbc" | "dbc" => "meteora_dbc",
        "moonshot" => "moonshot",
        "believe" | "launchcoin" => "believe",
        "bags" | "bagsfm" => "bags",
        "boop" | "boopfun" => "boop",
        "heaven" | "heavenxyz" => "heaven",
        other => return Some(other.to_string()),
    };
    Some(name.to_string())
}

/// Where a coin is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// On a launchpad bonding curve.
    Curve,
    /// Graduated from a curve to a pool.
    Graduated,
    /// Launched straight into a pool, no launchpad.
    Pool,
}

/// GMGN's `launchpad_status` is 1 (or 2) once graduated; progress 0–1 on
/// a curve.
pub fn stage(on_launchpad: bool, status: Option<f64>, progress: Option<f64>) -> Stage {
    if !on_launchpad {
        return Stage::Pool;
    }
    if status.is_some_and(|s| s >= 1.0) || progress.is_some_and(|p| p >= 1.0) {
        Stage::Graduated
    } else {
        Stage::Curve
    }
}

/// Links built in code; the model never builds a URL.
pub fn links(mint: &str) -> serde_json::Value {
    serde_json::json!({
        "gmgn": format!("https://gmgn.ai/sol/token/{mint}"),
        "dexscreener": format!("https://dexscreener.com/solana/{mint}"),
        "solscan": format!("https://solscan.io/token/{mint}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{SOL, USDC};

    #[test]
    fn catalog_resolves_exact_mints_and_symbols() {
        assert!(catalog().len() > 20);
        assert_eq!(known_symbol("usdc").map(|a| a.mint.as_str()), Some(USDC));
        assert_eq!(known_symbol("WSOL").map(|a| a.mint.as_str()), Some(SOL));
        assert_eq!(known(SOL).map(|a| a.kind), Some(Kind::Native));
        assert_eq!(known_symbol("NVDAx").map(|a| a.kind), Some(Kind::Stock));
        assert!(is_base_asset(USDC));
        assert!(!is_base_asset(
            "FLUXBmPhT3Fd1EDVFdg46YREqHBeNypn1h4EbnTzWERX"
        ));
        assert!(!is_base_asset(
            "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263"
        ));
    }

    #[test]
    fn spoofed_mints_are_caught() {
        assert_eq!(
            lookalike("EPjFxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxDt1v"),
            Some("USDC")
        );
        assert_eq!(lookalike(USDC), None);
        assert_eq!(
            lookalike("DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263"),
            None
        );
    }

    #[test]
    fn launchpads_have_one_spelling_and_stages_follow_status() {
        assert_eq!(launchpad(Some("Pump.fun")).as_deref(), Some("pump.fun"));
        assert_eq!(launchpad(Some("pump_mayhem")).as_deref(), Some("pump.fun"));
        assert_eq!(
            launchpad(Some("ray_launchpad")).as_deref(),
            Some("launchlab")
        );
        assert_eq!(launchpad(Some("")), None);
        assert_eq!(stage(false, None, None), Stage::Pool);
        assert_eq!(stage(true, Some(0.0), Some(0.4)), Stage::Curve);
        assert_eq!(stage(true, Some(1.0), None), Stage::Graduated);
        assert_eq!(stage(true, Some(2.0), None), Stage::Graduated);
    }
}
