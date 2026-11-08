//! Ticker or name → mints. The same ticker on different mints is a
//! different token; the catalog answers majors, stables and xStocks first,
//! and KOL counts and honeypot flags separate the real coin from
//! lookalikes better than holders alone.
use super::market::{Asset, Stage, is_base_asset, known_symbol, launchpad, stage};
use super::token::{Depth, snapshot};
use crate::providers::gmgn::Gmgn;
use crate::runtime::{Fail, Runtime, Turn};

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub token: String,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub launchpad: Option<String>,
    pub stage: Stage,
    pub created_at: Option<i64>,
    pub fdv_usd: Option<f64>,
    pub liquidity_usd: Option<f64>,
    pub volume_1h_usd: Option<f64>,
    pub holders: Option<f64>,
    pub kols: Option<f64>,
    pub honeypot: Option<bool>,
    pub exact_symbol: bool,
    /// From Dottie's verified catalog, not a search.
    pub verified: bool,
}

impl Candidate {
    fn from_asset(a: &Asset) -> Self {
        Candidate {
            token: a.mint.clone(),
            symbol: Some(a.symbol.clone()),
            name: Some(a.name.clone()),
            launchpad: None,
            stage: Stage::Pool,
            created_at: None,
            fdv_usd: None,
            liquidity_usd: None,
            volume_1h_usd: None,
            holders: None,
            kols: None,
            honeypot: None,
            exact_symbol: true,
            verified: true,
        }
    }
}

/// A catalog asset answers alone; otherwise every candidate GMGN has,
/// exact ticker first, then deepest liquidity: holder counts are easy to
/// farm with airdrops.
pub async fn search(rt: &Runtime, turn: &Turn, query: &str) -> Result<Vec<Candidate>, Fail> {
    if let Some(asset) = known_symbol(query) {
        return Ok(vec![Candidate::from_asset(asset)]);
    }
    let upper = query.trim().trim_start_matches('$').to_ascii_uppercase();
    let exact = |symbol: &Option<String>| {
        symbol
            .as_deref()
            .is_some_and(|s| s.to_ascii_uppercase() == upper)
    };
    let mut out: Vec<Candidate> = Gmgn::new(rt, turn)
        .search(query.trim().trim_start_matches('$'))
        .await?
        .into_iter()
        .map(|c| {
            let pad = launchpad(c.launchpad_platform.as_deref());
            Candidate {
                stage: stage(pad.is_some(), c.launchpad_status.get(), c.progress.get()),
                exact_symbol: exact(&c.symbol),
                token: c.address.trim().to_string(),
                symbol: c.symbol,
                name: c.name,
                launchpad: pad,
                created_at: c.created_at.ts(),
                fdv_usd: c.mcp.get(),
                liquidity_usd: c.liquidity.get(),
                volume_1h_usd: c.volume_1h.get(),
                holders: c.holder_count.get(),
                kols: c.kol_count.get(),
                honeypot: c.is_honeypot.get(),
                verified: false,
            }
        })
        .collect();
    out.retain(|c| !is_base_asset(&c.token) || c.exact_symbol);
    out.sort_by(|a, b| {
        b.exact_symbol
            .cmp(&a.exact_symbol)
            .then(
                b.liquidity_usd
                    .unwrap_or(0.0)
                    .total_cmp(&a.liquidity_usd.unwrap_or(0.0)),
            )
            .then(
                b.holders
                    .unwrap_or(0.0)
                    .total_cmp(&a.holders.unwrap_or(0.0)),
            )
    });
    Ok(out)
}

/// Exactly this mint, never a substitute.
pub async fn exact(rt: &Runtime, turn: &Turn, token: &str) -> Result<Candidate, Fail> {
    if let Some(asset) = super::market::known(token) {
        return Ok(Candidate::from_asset(asset));
    }
    let s = snapshot(rt, turn, token, Depth::Basic).await?;
    Ok(Candidate {
        token: s.token,
        symbol: s.symbol,
        name: s.name,
        launchpad: s.launchpad,
        stage: s.stage,
        created_at: s.created_at,
        fdv_usd: s.fdv_usd,
        liquidity_usd: s.liquidity_usd,
        volume_1h_usd: None,
        holders: s.holders,
        kols: s.tagged.kol,
        honeypot: None,
        exact_symbol: true,
        verified: false,
    })
}

/// Copies with under $100 of liquidity and no trades in the last hour.
pub fn is_dust(c: &Candidate) -> bool {
    c.liquidity_usd.is_some_and(|l| l < 100.0) && c.volume_1h_usd.is_none_or(|v| v <= 0.0)
}
