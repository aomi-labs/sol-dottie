//! Units live in field names: `_usd` dollars, `_pct` percent 0–100, `_x`
//! ratios, `_min`/`_h` ages. These turn typed numbers into the JSON the
//! model reads; `null` means unknown, never zero.
use serde_json::{Map, Value, json};
use std::time::Duration;

/// Set only while replaying recorded responses, so time windows ("the
/// last hour") read the same as when they were recorded.
static FROZEN_NOW: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

pub fn now() -> i64 {
    match FROZEN_NOW.load(std::sync::atomic::Ordering::Relaxed) {
        0 => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        frozen => frozen,
    }
}

/// Freezes [`now`] for the rest of the process (replay tests only).
pub fn freeze_clock(at: i64) {
    FROZEN_NOW.store(at, std::sync::atomic::Ordering::Relaxed);
}

/// Rounds to `sig` significant figures so tiny prices survive and big ones
/// stay short.
pub fn sig(value: f64, sig: i32) -> f64 {
    if value == 0.0 || !value.is_finite() {
        return value;
    }
    let mag = value.abs().log10().floor() as i32;
    let scale = 10f64.powi(sig - 1 - mag);
    (value * scale).round() / scale
}

/// A count: always an integer.
pub fn int(value: Option<f64>) -> Value {
    value
        .filter(|v| v.is_finite())
        .map(|v| json!(v.round() as i64))
        .unwrap_or(Value::Null)
}

/// Dollars: whole dollars from $100, cents from $1, else 3 significant figures.
pub fn usd(value: Option<f64>) -> Value {
    match value.filter(|v| v.is_finite()) {
        Some(v) if v.abs() >= 100.0 => json!(v.round() as i64),
        Some(v) if v.abs() >= 1.0 => json!((v * 100.0).round() / 100.0),
        Some(v) => json!(sig(v, 3)),
        None => Value::Null,
    }
}

/// Percent on a 0–100 scale. Small values keep two significant figures so
/// 0.04% never renders as 0.
pub fn pct(value: Option<f64>) -> Value {
    match value.filter(|v| v.is_finite()) {
        Some(v) if v.abs() >= 1.0 => json!((v * 10.0).round() / 10.0),
        Some(v) => json!(sig(v, 2)),
        None => Value::Null,
    }
}

pub fn price(value: Option<f64>) -> Value {
    value
        .filter(|v| v.is_finite())
        .map(|v| json!(sig(v, 4)))
        .unwrap_or(Value::Null)
}

/// A ratio such as "late volume vs early volume".
pub fn ratio(value: Option<f64>) -> Value {
    value
        .filter(|v| v.is_finite())
        .map(|v| json!(sig(v, 2)))
        .unwrap_or(Value::Null)
}

/// Whole minutes since a unix timestamp.
pub fn min_ago(ts: Option<i64>) -> Value {
    ts.filter(|t| *t > 0)
        .map(|t| json!(((now() - t) as f64 / 60.0).max(0.0).round() as i64))
        .unwrap_or(Value::Null)
}

/// Sets `<prefix>_min` (whole minutes) for spans up to 3 h, else
/// `<prefix>_h` with 2 significant figures.
pub fn put_span(out: &mut Map<String, Value>, prefix: &str, since: Option<i64>) {
    let Some(t) = since.filter(|t| *t > 0) else {
        return;
    };
    let minutes = ((now() - t) as f64 / 60.0).max(0.0);
    if minutes <= 180.0 {
        out.insert(format!("{prefix}_min"), json!(minutes.round() as i64));
    } else {
        let hours = sig(minutes / 60.0, 2);
        let hours = if hours >= 10.0 {
            json!(hours as i64)
        } else {
            json!(hours)
        };
        out.insert(format!("{prefix}_h"), hours);
    }
}

/// "45 s", "12 min", "3.5 h": for gap notes.
pub fn duration(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..=89 => format!("{s} s"),
        90..=5399 => format!("{} min", (s as f64 / 60.0).round() as u64),
        _ => format!("{} h", sig(s as f64 / 3600.0, 2)),
    }
}

/// A number series as one space-separated string, so pretty-printing
/// doesn't spend a line per number. Unknown values are `-`.
pub fn series(values: impl IntoIterator<Item = Option<f64>>, digits: i32) -> Value {
    let text: Vec<String> = values
        .into_iter()
        .map(|v| match v {
            Some(x) if x.is_finite() => {
                let x = sig(x, digits);
                if x.fract() == 0.0 && x.abs() < 1e15 {
                    format!("{}", x as i64)
                } else {
                    format!("{x}")
                }
            }
            _ => "-".into(),
        })
        .collect();
    json!(text.join(" "))
}

/// Checks a Solana address or mint (base58, case kept).
pub fn address(value: &str) -> Result<String, String> {
    crate::providers::pubkey(value)
        .map(str::to_string)
        .map_err(|_| {
            format!(
                "`{}` is not a Solana mint or address; resolve tickers with dottie_find",
                value.trim().chars().take(48).collect::<String>()
            )
        })
}

/// Untrusted token metadata, cut short and stripped of control characters.
pub fn label(value: Option<&str>, max: usize) -> Value {
    match value {
        Some(s) => {
            let clean: String = s.chars().filter(|c| !c.is_control()).take(max).collect();
            let clean = clean.trim();
            if clean.is_empty() {
                Value::Null
            } else {
                json!(clean)
            }
        }
        None => Value::Null,
    }
}

/// Untrusted text for Markdown: letters, digits and a little punctuation,
/// so a token name can't open a link or a code span.
pub fn safe(value: &str, max: usize) -> String {
    let clean: String = value
        .chars()
        .filter(|c| c.is_alphanumeric() || " $.-&'".contains(*c))
        .take(max)
        .collect();
    clean.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_percentages_survive() {
        assert_eq!(pct(Some(0.04)), json!(0.04));
        assert_eq!(pct(Some(49.83)), json!(49.8));
        assert_eq!(pct(None), Value::Null);
        assert_eq!(pct(Some(f64::NAN)), Value::Null);
    }

    #[test]
    fn money_and_counts() {
        assert_eq!(usd(Some(26736.61)), json!(26737));
        assert_eq!(usd(Some(3.456)), json!(3.46));
        assert_eq!(price(Some(0.000339119645)), json!(0.0003391));
        assert_eq!(serde_json::to_string(&int(Some(14021.0))).unwrap(), "14021");
    }

    #[test]
    fn spans_switch_to_hours() {
        let mut out = Map::new();
        put_span(&mut out, "age", Some(now() - 11743 * 60));
        assert_eq!(Value::Object(out), json!({ "age_h": 200 }));
        let mut out = Map::new();
        put_span(&mut out, "age", Some(now() - 51 * 60));
        assert_eq!(Value::Object(out), json!({ "age_min": 51 }));
        assert_eq!(duration(Duration::from_secs(600)), "10 min");
    }

    #[test]
    fn series_are_one_line() {
        assert_eq!(
            series([Some(0.00033312), None, Some(1606.4)], 3),
            json!("0.000333 - 1610")
        );
    }

    #[test]
    fn metadata_is_cut_and_cleaned() {
        assert_eq!(label(Some("ab\ncd"), 3), json!("abc"));
        assert_eq!(label(Some(" "), 3), Value::Null);
        assert!(address("PRIORS").is_err());
        assert_eq!(safe("[PONS](https://x.y) `a`", 20), "PONShttpsx.y a");
    }
}
