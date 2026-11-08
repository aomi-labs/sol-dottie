//! USD candles from GMGN (curve phase included) and facts computed from
//! the same bars the model receives.
use crate::providers::gmgn::{Gmgn, types as gmgn_types};
use crate::runtime::{Fail, Runtime, Turn};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Range {
    H1,
    H6,
    H24,
    D7,
    D30,
    /// The whole life since launch, curve included.
    Life,
}

impl Range {
    pub fn label(self) -> &'static str {
        match self {
            Range::H1 => "1h",
            Range::H6 => "6h",
            Range::H24 => "24h",
            Range::D7 => "7d",
            Range::D30 => "30d",
            Range::Life => "life",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Candle {
    M1,
    M5,
    M15,
    M30,
    H1,
    H4,
    H12,
    D1,
}

impl Candle {
    /// Seconds per bar.
    fn secs(self) -> i64 {
        match self {
            Candle::M1 => 60,
            Candle::M5 => 300,
            Candle::M15 => 900,
            Candle::M30 => 1800,
            Candle::H1 => 3600,
            Candle::H4 => 14_400,
            Candle::H12 => 43_200,
            Candle::D1 => 86_400,
        }
    }

    /// The nearest candle GMGN serves at or below this one, and its name there.
    fn gmgn(self) -> (Candle, &'static str) {
        match self {
            Candle::M1 => (Candle::M1, "1m"),
            Candle::M5 => (Candle::M5, "5m"),
            Candle::M15 | Candle::M30 => (Candle::M15, "15m"),
            Candle::H1 => (Candle::H1, "1h"),
            Candle::H4 | Candle::H12 => (Candle::H4, "4h"),
            Candle::D1 => (Candle::D1, "1d"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Candle::M1 => "1m",
            Candle::M5 => "5m",
            Candle::M15 => "15m",
            Candle::M30 => "30m",
            Candle::H1 => "1h",
            Candle::H4 => "4h",
            Candle::H12 => "12h",
            Candle::D1 => "1d",
        }
    }
}

/// (candle, bars) giving 24–60 bars for a range in sizes GMGN serves;
/// `life` uses the age.
pub fn plan(range: Range, age_s: i64) -> (Candle, i64) {
    match range {
        Range::H1 => (Candle::M1, 60),
        Range::H6 => (Candle::M15, 24),
        Range::H24 => (Candle::H1, 24),
        Range::D7 => (Candle::H4, 42),
        Range::D30 => (Candle::D1, 30),
        Range::Life => {
            let candle = match age_s {
                s if s <= 6 * 3600 => Candle::M5,
                s if s <= 2 * 86_400 => Candle::H1,
                s if s <= 10 * 86_400 => Candle::H4,
                _ => Candle::D1,
            };
            (candle, (age_s / candle.secs() + 1).clamp(2, 60))
        }
    }
}

pub struct Query {
    pub range: Range,
    pub context: Option<Range>,
    pub candle: Option<Candle>,
    pub bars: Option<i64>,
    /// Unix seconds the main range ends at; `None` = now.
    pub to: Option<i64>,
}

pub struct Bars {
    pub t: Vec<i64>,
    pub o: Vec<Option<f64>>,
    pub h: Vec<Option<f64>>,
    pub l: Vec<Option<f64>>,
    pub c: Vec<Option<f64>>,
    pub v: Vec<Option<f64>>,
    pub buy_usd: Option<f64>,
    pub sell_usd: Option<f64>,
}

pub struct Context {
    pub range: Range,
    pub t0: Option<i64>,
    pub step_min: i64,
    pub closes: Vec<Option<f64>>,
}

pub struct Chart {
    pub range: Range,
    pub candle: Candle,
    pub step_s: i64,
    pub bars: Bars,
    pub context: Option<Context>,
    pub facts: Facts,
}

pub async fn chart(rt: &Runtime, turn: &Turn, token: &str, q: &Query) -> Result<Chart, Fail> {
    let now = crate::view::now();
    let needs_age = q.range == Range::Life || q.context == Some(Range::Life);
    let age_s = if needs_age {
        age(rt, turn, token).await
    } else {
        0
    };
    let (candle, bars) = match q.candle {
        Some(c) => (c, q.bars.unwrap_or(48).clamp(10, 60)),
        None => plan(q.range, age_s),
    };
    // GMGN serves fewer sizes; report the candle it actually gave.
    let (candle, name) = candle.gmgn();
    let step = candle.secs();
    // Rounded up to the bar so repeats within one bar share a cache entry.
    let to = (q.to.unwrap_or(now).min(now) / step + 1) * step;
    let from = to - step * (bars + 1);
    let gmgn = Gmgn::new(rt, turn);
    let context = q.context.map(|range| {
        let (c, b) = plan(range, age_s);
        let (c, name) = c.gmgn();
        let step = c.secs();
        let to = (now / step + 1) * step;
        (range, step, name, to - step * b.min(40), to)
    });
    let (main, raw_context) = tokio::join!(gmgn.kline(token, name, from, to), async {
        match &context {
            Some((_, _, name, from, to)) => gmgn.kline(token, name, *from, *to).await.ok(),
            None => None,
        }
    });
    let raw = Raw::from_gmgn(main?);
    // Keep only bars inside the asked window, at most 60.
    let inside: Vec<usize> = (0..raw.t.len()).filter(|&i| raw.t[i] >= from).collect();
    let keep = &inside[inside.len().saturating_sub(60)..];
    if keep.is_empty() {
        return Err(Fail::NoData(match raw.t.last() {
            Some(last) => format!(
                "no trades in this range; the last candle with trades was {} minutes ago",
                (now - last) / 60
            ),
            None => "no trades in this range; the token may be inactive or not indexed".into(),
        }));
    }
    let pick = |series: &[Option<f64>]| -> Vec<Option<f64>> {
        keep.iter()
            .map(|&i| series.get(i).copied().flatten())
            .collect()
    };
    let sum = |series: &[Option<f64>]| -> Option<f64> {
        let picked: Vec<f64> = pick(series).into_iter().flatten().collect();
        (!picked.is_empty()).then(|| picked.iter().sum())
    };
    let bars = Bars {
        t: keep.iter().map(|&i| raw.t[i]).collect(),
        o: pick(&raw.o),
        h: pick(&raw.h),
        l: pick(&raw.l),
        c: pick(&raw.c),
        v: pick(&raw.v),
        buy_usd: sum(&raw.buy),
        sell_usd: sum(&raw.sell),
    };
    let context = context.zip(raw_context).map(|((range, step, ..), x)| {
        let x = Raw::from_gmgn(x);
        let closes = x.c;
        // Skip the empty bars before launch, then keep the last 40.
        let first = closes
            .iter()
            .position(Option::is_some)
            .unwrap_or(closes.len());
        let start = first.max(closes.len().saturating_sub(40));
        Context {
            range,
            t0: x.t.get(start).copied(),
            step_min: step / 60,
            closes: closes[start..].to_vec(),
        }
    });
    let facts = facts(&bars);
    Ok(Chart {
        range: q.range,
        candle,
        step_s: step,
        bars,
        context,
        facts,
    })
}

/// Seconds since launch, for `life`; 30 days when unknown.
async fn age(rt: &Runtime, turn: &Turn, token: &str) -> i64 {
    let created = Gmgn::new(rt, turn)
        .token_info(token)
        .await
        .ok()
        .and_then(|info| info.creation_timestamp.ts().or(info.open_timestamp.ts()));
    created
        .map(|c| crate::view::now() - c)
        .unwrap_or(30 * 86_400)
}

/// Bars as parallel series, one entry per bar.
struct Raw {
    t: Vec<i64>,
    o: Vec<Option<f64>>,
    h: Vec<Option<f64>>,
    l: Vec<Option<f64>>,
    c: Vec<Option<f64>>,
    v: Vec<Option<f64>>,
    buy: Vec<Option<f64>>,
    sell: Vec<Option<f64>>,
}

impl Raw {
    /// GMGN candles carry millisecond times; one without a time is dropped
    /// so the series stay aligned.
    fn from_gmgn(candles: Vec<gmgn_types::Candle>) -> Self {
        let candles: Vec<_> = candles
            .into_iter()
            .filter(|c| c.time.get().is_some())
            .collect();
        let f = |get: fn(&gmgn_types::Candle) -> Option<f64>| candles.iter().map(get).collect();
        Self {
            t: candles
                .iter()
                .filter_map(|c| c.time.get())
                .map(|ms| (ms / 1000.0) as i64)
                .collect(),
            o: f(|c| c.open.get()),
            h: f(|c| c.high.get()),
            l: f(|c| c.low.get()),
            c: f(|c| c.close.get()),
            v: f(|c| c.volume.get()),
            buy: vec![],
            sell: vec![],
        }
    }
}

/// Facts the model can quote, computed from the bars it receives.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    pub open: Option<f64>,
    pub last: Option<f64>,
    pub change_pct: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub from_high_pct: Option<f64>,
    pub from_low_pct: Option<f64>,
    pub volume_usd: Option<f64>,
    /// Volume in the last third of bars over the first third.
    pub volume_trend_x: Option<f64>,
    /// Lowest low in the last third over the first third; > 1 is higher lows.
    pub lows_trend_x: Option<f64>,
}

pub fn facts(b: &Bars) -> Facts {
    let first = b.o.iter().flatten().next().copied();
    let last = b.c.iter().rev().flatten().next().copied();
    let high = b.h.iter().flatten().copied().fold(f64::NAN, f64::max);
    let low = b.l.iter().flatten().copied().fold(f64::NAN, f64::min);
    let (high, low) = (
        high.is_finite().then_some(high),
        low.is_finite().then_some(low),
    );
    let third = (b.v.len() / 3).max(1);
    let sum = |s: &[Option<f64>]| s.iter().flatten().sum::<f64>();
    let early = sum(&b.v[..third.min(b.v.len())]);
    let late = sum(&b.v[b.v.len().saturating_sub(third)..]);
    let lows = |s: &[Option<f64>]| s.iter().flatten().copied().fold(f64::NAN, f64::min);
    let n = b.l.len();
    let ratio = |a: Option<f64>, b: Option<f64>| match (a, b) {
        (Some(a), Some(b)) if b > 0.0 => Some((a / b - 1.0) * 100.0),
        _ => None,
    };
    Facts {
        open: first,
        last,
        change_pct: ratio(last, first),
        high,
        low,
        from_high_pct: ratio(last, high),
        from_low_pct: ratio(last, low),
        volume_usd: Some(sum(&b.v)),
        volume_trend_x: (early > 0.0).then(|| late / early),
        lows_trend_x: if n >= 6 {
            let (a, z) = (lows(&b.l[..n / 3]), lows(&b.l[n - n / 3..]));
            (a > 0.0 && z.is_finite()).then(|| z / a)
        } else {
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_give_24_to_60_bars() {
        for r in [Range::H1, Range::H6, Range::H24, Range::D7, Range::D30] {
            let (_, bars) = plan(r, 0);
            assert!((24..=60).contains(&bars));
        }
        assert_eq!(plan(Range::Life, 3 * 3600).0, Candle::M5);
        assert_eq!(plan(Range::Life, 40 * 86_400).0, Candle::D1);
        assert_eq!(Candle::M30.gmgn().0, Candle::M15);
    }

    #[test]
    fn gmgn_candles_parse_with_second_times() {
        let text = std::fs::read_to_string(format!(
            "{}/src/fixtures/gmgn/kline_bonk.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let candles: Vec<gmgn_types::Candle> =
            serde_json::from_value(v["data"]["list"].clone()).unwrap();
        let raw = Raw::from_gmgn(candles);
        assert!(raw.t.len() >= 12 && raw.t[0] > 1_700_000_000);
        assert!(raw.c.iter().all(Option::is_some));
    }

    #[test]
    fn facts_from_bars() {
        let s = |v: &[f64]| v.iter().map(|x| Some(*x)).collect::<Vec<_>>();
        let f = facts(&Bars {
            t: vec![0, 1, 2],
            o: s(&[1.0, 2.0, 3.0]),
            h: s(&[2.0, 4.0, 3.5]),
            l: s(&[0.9, 1.8, 2.5]),
            c: s(&[2.0, 3.0, 3.0]),
            v: s(&[10.0, 10.0, 30.0]),
            buy_usd: None,
            sell_usd: None,
        });
        assert_eq!(f.change_pct, Some(200.0));
        assert_eq!(f.from_high_pct, Some(-25.0));
        assert_eq!(f.volume_trend_x, Some(3.0));
    }
}
