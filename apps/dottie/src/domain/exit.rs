//! Can a size get out? Live Jupiter quotes: a round trip (buy with SOL,
//! then sell the tokens straight back) or a sell, with one loss definition
//! and one verdict scale for both and for the small probe on every card.
use super::amount;
use crate::providers::SOL;
use crate::providers::gmgn::Gmgn;
use crate::providers::jupiter::{Jupiter, Quote};
use crate::providers::rpc::Rpc;
use crate::runtime::{Fail, Runtime, Source, Turn};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const THIN_PCT: f64 = 10.0;
const TRAP_PCT: f64 = 50.0;
/// The round trip quoted on every card.
pub const PROBE_SOL: &str = "0.1";
const PROBE_TTL: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitResult {
    Ok,
    /// Loses more than 10%: size down.
    Thin,
    /// More than half the quoted input is lost.
    Trap,
    /// The router returned no sell quote; this does not prove a honeypot.
    NoSellRoute,
    NoBuyRoute,
    Unknown,
}

impl ExitResult {
    pub fn of(sell: Option<&Result<Quote, Fail>>, loss_pct: Option<f64>) -> Self {
        match (sell, loss_pct) {
            (Some(Err(Fail::NoRoute(_))), _) => ExitResult::NoSellRoute,
            (_, Some(l)) if l > TRAP_PCT => ExitResult::Trap,
            (_, Some(l)) if l > THIN_PCT => ExitResult::Thin,
            (_, Some(_)) => ExitResult::Ok,
            _ => ExitResult::Unknown,
        }
    }
}

pub struct RoundTrip {
    pub sol_in: String,
    pub buy: Result<Quote, Fail>,
    pub decimals: u8,
    pub sell: Option<Result<Quote, Fail>>,
    pub sol_back: Option<f64>,
    pub loss_pct: Option<f64>,
    pub result: ExitResult,
}

pub struct Sale {
    pub tokens_in: u128,
    pub decimals: u8,
    pub sell: Result<Quote, Fail>,
    pub sol_out: Option<f64>,
    pub sol_min_out: Option<f64>,
    pub value_usd: Option<f64>,
    /// SOL out vs the tokens' value at the current price.
    pub loss_pct: Option<f64>,
    pub result: ExitResult,
}

/// The small round trip on every card, cached per token.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Probe {
    pub sol_in: String,
    pub loss_pct: Option<f64>,
    pub no_sell_route: bool,
    pub route: Vec<String>,
    pub result: ExitResult,
}

/// A size as the user gave it.
pub enum Size {
    Sol(String),
    /// Whole token units.
    Tokens(String),
    Usd(f64),
}

#[derive(Clone, Copy)]
pub struct Exits<'a> {
    rt: &'a Runtime,
    turn: &'a Turn,
}

impl<'a> Exits<'a> {
    pub fn new(rt: &'a Runtime, turn: &'a Turn) -> Self {
        Self { rt, turn }
    }

    pub async fn probe(self, token: &str) -> Probe {
        let key = format!("probe:{token}");
        if let Some(hit) = self.rt.cache.fresh(&key)
            && let Ok(probe) = serde_json::from_value::<Probe>(hit)
        {
            return probe;
        }
        let unknown = Probe {
            sol_in: PROBE_SOL.into(),
            loss_pct: None,
            no_sell_route: false,
            route: vec![],
            result: ExitResult::Unknown,
        };
        let probe = match self.round_trip(token, PROBE_SOL, 500).await {
            Ok(trip) => Probe {
                loss_pct: trip.loss_pct,
                no_sell_route: matches!(trip.sell, Some(Err(Fail::NoRoute(_)))),
                route: route(&trip),
                result: trip.result,
                ..unknown
            },
            Err(_) => unknown,
        };
        if probe.result != ExitResult::Unknown
            && let Ok(value) = serde_json::to_value(&probe)
        {
            self.rt.cache.put(key, value, PROBE_TTL);
        }
        probe
    }

    /// Buys with `sol` SOL and quotes selling every token straight back.
    pub async fn round_trip(
        self,
        token: &str,
        sol: &str,
        slippage_bps: u32,
    ) -> Result<RoundTrip, Fail> {
        let lamports = amount::from_decimal(sol, 9).map_err(Fail::Invalid)?;
        if lamports == 0 {
            return Err(Fail::Invalid("round-trip input must be positive".into()));
        }
        let decimals = self.decimals(token).await?;
        let jupiter = Jupiter::new(self.rt, self.turn);
        let buy = jupiter
            .quote(SOL, token, &lamports.to_string(), slippage_bps)
            .await;
        let sell = match &buy {
            Ok(q) => Some(jupiter.quote(token, SOL, &q.out_amount, slippage_bps).await),
            Err(_) => None,
        };
        let sol_in = units(lamports, 9);
        let sol_back = sell
            .as_ref()
            .and_then(|s| s.as_ref().ok())
            .and_then(|q| units_str(&q.out_amount, 9));
        let loss_pct = match sol_back {
            Some(b) if sol_in > 0.0 => Some((1.0 - b / sol_in) * 100.0),
            _ => None,
        };
        let result = match &buy {
            Err(Fail::NoRoute(_)) => ExitResult::NoBuyRoute,
            _ => ExitResult::of(sell.as_ref(), loss_pct),
        };
        Ok(RoundTrip {
            sol_in: sol.to_string(),
            buy,
            decimals,
            sell,
            sol_back,
            loss_pct,
            result,
        })
    }

    /// Sells `tokens` (raw units) for SOL. The loss compares SOL out with
    /// the tokens' value at the current price, so it needs both prices.
    pub async fn sell(self, token: &str, tokens: u128, slippage_bps: u32) -> Result<Sale, Fail> {
        let decimals = self.decimals(token).await?;
        let raw = tokens.to_string();
        let ((sol_usd, token_usd), sell) = tokio::join!(
            self.prices(token),
            Jupiter::new(self.rt, self.turn).quote(token, SOL, &raw, slippage_bps)
        );
        let sol_out = sell.as_ref().ok().and_then(|q| units_str(&q.out_amount, 9));
        let sol_min_out = sell.as_ref().ok().and_then(|q| units_str(&q.min_out, 9));
        let value_usd = token_usd.map(|p| p * units(tokens, decimals));
        let loss_pct = match (sol_out, sol_usd, value_usd) {
            (Some(o), Some(p), Some(v)) if v > 0.0 => Some((1.0 - o * p / v) * 100.0),
            _ => None,
        };
        if loss_pct.is_none() && sell.is_ok() {
            self.turn
                .gap("prices unavailable, so the sell's loss is unknown");
        }
        let result = ExitResult::of(Some(&sell), loss_pct);
        Ok(Sale {
            tokens_in: tokens,
            decimals,
            sell,
            sol_out,
            sol_min_out,
            value_usd,
            loss_pct,
            result,
        })
    }

    /// A round trip at a size in SOL or USD; also returns the SOL price
    /// when it was read.
    pub async fn round_trip_sized(
        self,
        token: &str,
        size: Size,
        slippage_bps: u32,
    ) -> Result<(RoundTrip, Option<f64>), Fail> {
        let (sol, sol_usd) = match size {
            Size::Sol(sol) => (sol, None),
            Size::Usd(usd) => {
                let price = Jupiter::new(self.rt, self.turn).sol_usd().await?;
                (format!("{:.9}", usd / price), Some(price))
            }
            Size::Tokens(_) => {
                return Err(Fail::Invalid("trade needs sol_amount or usd_amount".into()));
            }
        };
        Ok((self.round_trip(token, &sol, slippage_bps).await?, sol_usd))
    }

    /// A sell of a size in tokens or USD.
    pub async fn sell_sized(
        self,
        token: &str,
        size: Size,
        slippage_bps: u32,
    ) -> Result<Sale, Fail> {
        let decimals = self.decimals(token).await?;
        let tokens = match size {
            Size::Tokens(t) => t,
            Size::Usd(usd) => {
                let (_, token_usd) = self.prices(token).await;
                let price = token_usd.filter(|p| *p > 0.0).ok_or_else(|| {
                    Fail::unavailable(Source::Gmgn, "token price unavailable for USD sizing")
                })?;
                format!("{:.*}", decimals.min(6) as usize, usd / price)
            }
            Size::Sol(_) => {
                return Err(Fail::Invalid(
                    "sell needs token_amount or usd_amount".into(),
                ));
            }
        };
        let tokens = amount::from_decimal(&tokens, decimals).map_err(Fail::Invalid)?;
        self.sell(token, tokens, slippage_bps).await
    }

    /// SOL and token prices in USD.
    pub async fn prices(self, token: &str) -> (Option<f64>, Option<f64>) {
        let (sol, info) = tokio::join!(
            Jupiter::new(self.rt, self.turn).sol_usd(),
            Gmgn::new(self.rt, self.turn).token_info(token)
        );
        (
            sol.ok(),
            info.ok()
                .filter(|i| i.address.trim() == token)
                .and_then(|i| i.price.price.get()),
        )
    }

    async fn decimals(self, token: &str) -> Result<u8, Fail> {
        if let Some(asset) = super::market::known(token) {
            return Ok(asset.decimals);
        }
        Ok(Rpc::new(self.rt, self.turn).mint(token).await?.decimals)
    }
}

/// The venues of both legs, buy first.
pub fn route(trip: &RoundTrip) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for q in [
        trip.buy.as_ref().ok(),
        trip.sell.as_ref().and_then(|s| s.as_ref().ok()),
    ]
    .into_iter()
    .flatten()
    {
        for label in &q.route {
            if !out.contains(label) {
                out.push(label.clone());
            }
        }
    }
    out.truncate(4);
    out
}

pub fn units(raw: u128, decimals: u8) -> f64 {
    raw as f64 / 10f64.powi(i32::from(decimals))
}

pub fn units_str(raw: &str, decimals: u8) -> Option<f64> {
    raw.parse::<u128>().ok().map(|v| units(v, decimals))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quote() -> Result<Quote, Fail> {
        Ok(Quote {
            in_amount: "1".into(),
            out_amount: "1".into(),
            min_out: "1".into(),
            price_impact_pct: None,
            route: vec!["Whirlpool".into()],
            usd_value: None,
        })
    }

    #[test]
    fn one_verdict_scale() {
        let no_route: Result<Quote, Fail> = Err(Fail::NoRoute("none".into()));
        assert_eq!(
            ExitResult::of(Some(&no_route), None),
            ExitResult::NoSellRoute
        );
        assert_eq!(ExitResult::of(Some(&quote()), Some(95.0)), ExitResult::Trap);
        assert_eq!(ExitResult::of(Some(&quote()), Some(12.0)), ExitResult::Thin);
        assert_eq!(ExitResult::of(Some(&quote()), Some(3.0)), ExitResult::Ok);
        assert_eq!(ExitResult::of(None, None), ExitResult::Unknown);
    }

    #[test]
    fn probe_round_trips_through_the_cache() {
        let probe = Probe {
            sol_in: PROBE_SOL.into(),
            loss_pct: Some(4.2),
            no_sell_route: false,
            route: vec!["Whirlpool".into()],
            result: ExitResult::Ok,
        };
        let back: Probe = serde_json::from_value(serde_json::to_value(&probe).unwrap()).unwrap();
        assert_eq!(back, probe);
        assert_eq!(units_str("1500000000", 9), Some(1.5));
    }
}
