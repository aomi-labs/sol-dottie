//! Jupiter read-only quotes: the router the host trades through
//! (`stage_jupiter_swap`), so a quote here is the best proof a size can get
//! out. Keyless works at about one request per two seconds; the operator's
//! `JUPITER_API_KEY` lifts that.
use crate::providers::{SOL, USDC, num};
use crate::runtime::{Cost, Fail, Request, Runtime, Source, Turn};
use serde_json::Value;
use std::time::Duration;

const URL: &str = "https://api.jup.ag/swap/v1/quote";
const TTL: Duration = Duration::from_secs(15);
const PRICE_TTL: Duration = Duration::from_secs(30);
/// Keyless Jupiter allows 0.5 requests a second per IP.
const KEYLESS_GAP: Duration = Duration::from_millis(2200);

#[derive(Debug, Clone, PartialEq)]
pub struct Quote {
    /// Atomic units of the input and output tokens.
    pub in_amount: String,
    pub out_amount: String,
    pub min_out: String,
    pub price_impact_pct: Option<f64>,
    /// The venues of each hop (`Whirlpool`, `Pump.fun Amm`…).
    pub route: Vec<String>,
    /// Jupiter's dollar value of the swap.
    pub usd_value: Option<f64>,
}

#[derive(Clone, Copy)]
pub struct Jupiter<'a> {
    rt: &'a Runtime,
    turn: &'a Turn,
}

impl<'a> Jupiter<'a> {
    pub fn new(rt: &'a Runtime, turn: &'a Turn) -> Self {
        Self { rt, turn }
    }

    /// An exact-in quote for `amount` atomic units of `input`.
    pub async fn quote(
        self,
        input: &str,
        output: &str,
        amount: &str,
        slippage_bps: u32,
    ) -> Result<Quote, Fail> {
        self.quote_ttl(input, output, amount, slippage_bps, TTL)
            .await
    }

    /// SOL's dollar price from a 1 SOL → USDC quote.
    pub async fn sol_usd(self) -> Result<f64, Fail> {
        let q = self
            .quote_ttl(SOL, USDC, "1000000000", 50, PRICE_TTL)
            .await?;
        num(&Value::String(q.out_amount))
            .map(|out| out / 1e6)
            .filter(|p| *p > 0.0)
            .ok_or_else(|| Fail::bad(Source::Jupiter, "SOL quote without an amount"))
    }

    async fn quote_ttl(
        self,
        input: &str,
        output: &str,
        amount: &str,
        slippage_bps: u32,
        ttl: Duration,
    ) -> Result<Quote, Fail> {
        let key = self.turn.key(Source::Jupiter).map(str::to_string);
        let cost = match key {
            Some(_) => Cost::Free,
            None => Cost::Paced(Source::Jupiter, KEYLESS_GAP),
        };
        let request = Request::new(
            Source::Jupiter,
            format!("jupiter:{input}>{output}:{amount}:{slippage_bps}"),
            cost,
            ttl,
        )
        .live_only();
        let query = [
            ("inputMint", input.to_string()),
            ("outputMint", output.to_string()),
            ("amount", amount.to_string()),
            ("slippageBps", slippage_bps.to_string()),
        ];
        let value = self
            .rt
            .fetch(self.turn, request, || async {
                let mut builder = self
                    .rt
                    .http
                    .get(URL)
                    .query(&query)
                    .timeout(Duration::from_secs(12));
                if let Some(key) = &key {
                    builder = builder.header("x-api-key", key);
                }
                let response = builder
                    .send()
                    .await
                    .map_err(|_| Fail::unavailable(Source::Jupiter, "request failed"))?;
                let status = response.status().as_u16();
                let body: Value = response.json().await.unwrap_or(Value::Null);
                unwrap_response(status, body)
            })
            .await?;
        parse_quote(&value)
    }
}

/// Jupiter answers 400 with an error code when no route exists; that is an
/// answer (`NoRoute`), not an outage.
fn unwrap_response(status: u16, body: Value) -> Result<Value, Fail> {
    match status {
        200..=299 => Ok(body),
        429 => Err(Fail::RateLimited {
            source: Source::Jupiter,
            retry_after_s: 10,
        }),
        401 | 403 => Err(Fail::Unconfigured(Source::Jupiter)),
        _ => {
            let code = body["errorCode"]
                .as_str()
                .or(body["error"].as_str())
                .unwrap_or("")
                .to_string();
            let upper = code.to_ascii_uppercase();
            if upper.contains("ROUTE")
                || upper.contains("NOT_TRADABLE")
                || upper.contains("NO_ROUTES")
            {
                Err(Fail::NoRoute("Jupiter found no route at this size".into()))
            } else {
                Err(Fail::unavailable(
                    Source::Jupiter,
                    format!(
                        "HTTP {status} {}",
                        code.chars().take(80).collect::<String>()
                    ),
                ))
            }
        }
    }
}

pub fn parse_quote(value: &Value) -> Result<Quote, Fail> {
    let amount = |field: &str| {
        value[field]
            .as_str()
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .map(str::to_string)
            .ok_or_else(|| Fail::bad(Source::Jupiter, format!("quote without {field}")))
    };
    let out_amount = amount("outAmount")?;
    if out_amount.trim_start_matches('0').is_empty() {
        return Err(Fail::NoRoute("Jupiter quoted zero output".into()));
    }
    let route = value["routePlan"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|step| step.pointer("/swapInfo/label").and_then(Value::as_str))
        .fold(Vec::<String>::new(), |mut labels, label| {
            if !labels.iter().any(|l| l == label) {
                labels.push(label.to_string());
            }
            labels
        });
    Ok(Quote {
        in_amount: amount("inAmount")?,
        out_amount,
        min_out: amount("otherAmountThreshold")?,
        // Jupiter reports impact as a fraction ("0.0002" = 0.02%).
        price_impact_pct: num(&value["priceImpactPct"]).map(|f| f * 100.0),
        route,
        usd_value: num(&value["swapUsdValue"]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn quotes_parse_and_missing_routes_are_answers() {
        let q = parse_quote(&json!({
            "inAmount": "100000000", "outAmount": "321329633106",
            "otherAmountThreshold": "319722984941", "priceImpactPct": "0.0002306",
            "swapUsdValue": "8.92",
            "routePlan": [{"swapInfo": {"label": "Whirlpool"}}, {"swapInfo": {"label": "Whirlpool"}}]
        }))
        .unwrap();
        assert_eq!(q.route, ["Whirlpool"]);
        assert!((q.price_impact_pct.unwrap() - 0.02306).abs() < 1e-9);
        assert_eq!(q.usd_value, Some(8.92));
        let none = unwrap_response(400, json!({"errorCode": "COULD_NOT_FIND_ANY_ROUTE"}));
        assert!(matches!(none, Err(Fail::NoRoute(_))));
        assert!(matches!(
            unwrap_response(429, Value::Null),
            Err(Fail::RateLimited { .. })
        ));
        assert!(
            parse_quote(&json!({"inAmount": "1", "outAmount": "0", "otherAmountThreshold": "0"}))
                .is_err()
        );
    }
}
