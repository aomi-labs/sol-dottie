use super::{address, run};
use crate::domain::exit::{Exits, Size};
use crate::domain::market::{Stage, is_base_asset};
use crate::domain::token::{Depth, TokenSnapshot, snapshot};
use crate::providers::fomo::Fomo;
use crate::runtime::{DottieApp, Fail, Lane};
use aomi_sdk::schemars::JsonSchema;
use aomi_sdk::{DynAomiTool, DynToolCallCtx};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Research card with an indicative 0.1 SOL exit probe.
    Full,
    /// Buy checks with one exit quote at the requested size; no research enrichment.
    Trade,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TokenArgs {
    /// Exact Solana mint, copied from a tool result or the user's text. Resolve tickers with dottie_find first.
    pub token: String,
    /// trade for a requested buy; full for research. null or omitted = full.
    pub mode: Option<Mode>,
    /// Exact SOL input for trade mode, e.g. "0.5". Give this or usd_amount, never both.
    pub sol_amount: Option<String>,
    /// USD budget for trade mode, converted at the current SOL price.
    pub usd_amount: Option<f64>,
    /// Trade quote slippage in percent. Omitted = 0.5 for base assets, 3 above $50k liquidity, else 5 (also 5 on curves). Above 5 needs user consent; maximum 10.
    pub slippage_pct: Option<f64>,
}

impl TokenArgs {
    fn slippage_bps(&self, snap: &TokenSnapshot) -> Result<u32, Fail> {
        if let Some(slippage) = self.slippage_pct {
            return Ok((slippage * 100.0).round() as u32);
        }
        if is_base_asset(&snap.token) {
            return Ok(50);
        }
        match snap.liquidity_usd.filter(|l| *l >= 0.0) {
            Some(liquidity) if liquidity > 50_000.0 && snap.stage != Stage::Curve => Ok(300),
            Some(_) => Ok(500),
            None => Err(Fail::Invalid("liquidity unknown; give the intended slippage_pct without widening existing bounds".into())),
        }
    }

    fn size(&self) -> Result<Option<Size>, Fail> {
        let invalid = |why: &str| Fail::Invalid(why.into());
        if self.mode != Some(Mode::Trade) {
            return if self.sol_amount.is_some()
                || self.usd_amount.is_some()
                || self.slippage_pct.is_some()
            {
                Err(invalid("size and slippage require mode trade"))
            } else {
                Ok(None)
            };
        }
        if self
            .slippage_pct
            .is_some_and(|s| !s.is_finite() || s <= 0.0 || s > 10.0)
        {
            return Err(invalid(
                "slippage_pct must be greater than 0 and at most 10; never silently widen it",
            ));
        }
        match (&self.sol_amount, self.usd_amount) {
            (Some(sol), None) => {
                let lamports =
                    crate::domain::amount::from_decimal(sol, 9).map_err(Fail::Invalid)?;
                if lamports == 0 {
                    return Err(invalid("sol_amount must be positive"));
                }
                Ok(Some(Size::Sol(sol.clone())))
            }
            (None, Some(usd)) if usd.is_finite() && usd > 0.0 => Ok(Some(Size::Usd(usd))),
            _ => Err(invalid(
                "trade requires exactly one positive sol_amount or usd_amount",
            )),
        }
    }
}

pub struct Token;

impl DynAomiTool for Token {
    type App = DottieApp;
    type Args = TokenArgs;
    const NAME: &'static str = "dottie_token";
    const DESCRIPTION: &'static str = "One Solana token by exact mint. mode trade: security, Token-2022 fee, liquidity and one Jupiter buy/sell quote at sol_amount or usd_amount, with slippage_pct or the trade default; do not also call dottie_exit. checks_passed is advisory: disclose warnings and let the user explicitly proceed. Unknown never becomes a pass. mode full (default): the research card (launchpad and curve, authorities, holder mix, dev, socials, RugCheck, Fomo warnings, links) with an indicative 0.1 SOL exit. Quotes are not fills.";

    fn run(app: &DottieApp, args: TokenArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let token = address(&args.token)?;
            if let Some(size) = args.size()? {
                let snap = snapshot(rt, turn, &token, Depth::Trade).await?;
                let slippage = args.slippage_bps(&snap)?;
                let (trip, sol_usd) = Exits::new(rt, turn)
                    .round_trip_sized(&token, size, slippage)
                    .await?;
                return Ok(crate::view::token::trade(&snap, &trip, sol_usd, slippage));
            }
            // The exit probe runs alongside the reads; it is the only test
            // that catches traps the security screen calls clean.
            let (snap, probe, fomo) = tokio::join!(
                snapshot(rt, turn, &token, Depth::Card),
                Exits::new(rt, turn).probe(&token),
                async {
                    if is_base_asset(&token) {
                        return None;
                    }
                    match Fomo::new(rt, turn).warnings(&token).await {
                        Ok(w) => Some(crate::view::people::fomo_warnings(&w)),
                        Err(Fail::Unconfigured(_)) => None,
                        Err(fail) => {
                            turn.gap(format!("fomo warnings unavailable: {}", fail.message()));
                            None
                        }
                    }
                }
            );
            Ok(crate::view::token::card(&snap?, Some(&probe), fomo))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_slippage_preserves_liquidity_and_curve_policy() {
        let args: TokenArgs = serde_json::from_value(
            serde_json::json!({"token":"unused","mode":"trade","sol_amount":"0.1"}),
        )
        .unwrap();
        let mut snap = TokenSnapshot::from_gmgn(Default::default());
        assert!(args.slippage_bps(&snap).is_err());
        snap.liquidity_usd = Some(50_000.0);
        assert_eq!(args.slippage_bps(&snap).unwrap(), 500);
        snap.liquidity_usd = Some(50_001.0);
        assert_eq!(args.slippage_bps(&snap).unwrap(), 300);
        snap.stage = Stage::Curve;
        assert_eq!(args.slippage_bps(&snap).unwrap(), 500);
        snap.token = crate::providers::USDC.into();
        assert_eq!(args.slippage_bps(&snap).unwrap(), 50);
    }
}
