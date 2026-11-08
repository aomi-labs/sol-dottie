//! How numbers read on a card. One formatter for the image, the caption
//! and the studio mockup. A tiny price keeps its digits with a subscript
//! zero count, written `{8}` here (`$0.0{8}442` = 0.00000000442); the
//! renderer draws it as a subscript and [`plain`] turns it into `₈`.

const MINUS: char = '\u{2212}';

fn trim_zeros(s: String) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// `digits` significant figures, never in exponent form.
fn precision(v: f64, digits: i32) -> String {
    let e = v.abs().log10().floor() as i32;
    format!("{:.*}", (digits - 1 - e).max(0) as usize, v)
}

fn sig_fix(v: f64, digits: i32) -> String {
    let int_digits = if v >= 100.0 {
        3
    } else if v >= 10.0 {
        2
    } else {
        1
    };
    trim_zeros(format!("{:.*}", (digits - int_digits).max(0) as usize, v))
}

pub fn compact(a: f64, digits: i32) -> String {
    const UNITS: [(f64, &str); 4] = [(1e12, "T"), (1e9, "B"), (1e6, "M"), (1e3, "K")];
    for (i, (n, unit)) in UNITS.iter().enumerate() {
        if a < *n {
            continue;
        }
        let s = sig_fix(a / n, digits);
        // 999.96K rounds to 1000K: step up a unit instead.
        if s.parse::<f64>().unwrap_or(0.0) >= 1000.0 && i > 0 {
            let (up, up_unit) = UNITS[i - 1];
            return format!("{}{up_unit}", sig_fix(a / up, digits));
        }
        return format!("{s}{unit}");
    }
    sig_fix(a, digits)
}

pub fn thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn sign(v: f64, plus: bool) -> &'static str {
    if v < 0.0 {
        "\u{2212}"
    } else if plus && v > 0.0 {
        "+"
    } else {
        ""
    }
}

/// Dollars: compact from $100K, whole from $1K, cents from $1, two
/// significant figures below, subscript zeros below $0.0001.
pub fn usd(v: f64, plus: bool) -> String {
    let (s, a) = (sign(v, plus), v.abs());
    if a == 0.0 {
        return "$0.00".into();
    }
    if a < 0.0001 {
        return format!("{s}{}", price(a));
    }
    if a < 1.0 {
        let p = trim_zeros(precision(a, 2));
        // "0.5" reads as cents: "0.50".
        let p = if p.len() == 3 { format!("{p}0") } else { p };
        return format!("{s}${p}");
    }
    if a < 1000.0 {
        return format!("{s}${a:.2}");
    }
    if a < 100_000.0 {
        return format!("{s}${}", thousands(a.round() as i64));
    }
    format!("{s}${}", compact(a, 3))
}

/// A token price: subscript-zero form once there are 4+ zeros after the point.
pub fn price(p: f64) -> String {
    if p == 0.0 {
        return "$0".into();
    }
    if p >= 1000.0 {
        return format!("${}", thousands(p.round() as i64));
    }
    if p >= 1.0 {
        return format!("${p:.2}");
    }
    if p >= 0.0001 {
        return format!("${}", trim_zeros(precision(p, 4)));
    }
    let mut e = p.log10().floor() as i32;
    let mut m = format!("{:.3}", p / 10f64.powi(e));
    if m.starts_with("10") {
        e += 1;
        m = "1.000".into();
    }
    let digits = m.replace('.', "");
    let digits = digits.trim_end_matches('0');
    format!(
        "$0.0{{{}}}{}",
        -e - 1,
        if digits.is_empty() { "0" } else { digits }
    )
}

pub fn pct(v: f64) -> String {
    let (s, a) = (sign(v, true), v.abs());
    match a {
        0.0 => "0%".into(),
        a if a < 0.01 => format!("{s}<0.01%"),
        a if a < 1.0 => format!("{s}{a:.2}%"),
        a if a < 100.0 => format!("{s}{a:.1}%"),
        a if a < 1000.0 => format!("{s}{}%", a.round()),
        a if a < 100_000.0 => format!("{s}{}%", thousands(a.round() as i64)),
        a => format!("{s}{}%", compact(a, 2)),
    }
}

pub fn mult(m: f64) -> String {
    match m {
        m if m < 0.01 => "0×".into(),
        m if m < 10.0 => format!("{m:.2}×"),
        m if m < 100.0 => format!("{m:.1}×"),
        m if m < 1000.0 => format!("{}×", m.round()),
        m => format!("{}×", compact(m, 2)),
    }
}

pub fn held(s: i64) -> String {
    let (m, h, d) = (s / 60, s / 3600, s / 86_400);
    match s {
        s if s < 60 => format!("{}s", s.max(1)),
        s if s < 3600 => format!("{m}m"),
        s if s < 86_400 => format!("{h}h {:02}m", m % 60),
        s if s < 30 * 86_400 => format!("{d}d {}h", h % 24),
        _ => format!("{}mo {}d", d / 30, d % 30),
    }
}

/// (year, month 1–12, day) of a unix time, UTC.
fn civil(ts: i64) -> (i64, u32, u32) {
    let z = ts.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

pub fn date(ts: i64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (y, m, d) = civil(ts);
    format!("{d} {} {y}", MONTHS[m as usize - 1])
}

pub fn time(ts: i64) -> String {
    let s = ts.rem_euclid(86_400);
    format!("{:02}:{:02} UTC", s / 3600, s % 3600 / 60)
}

/// Card markup to plain text: `{8}` becomes `₈`.
pub fn plain(s: &str) -> String {
    let mut out = String::new();
    let mut sub = false;
    for c in s.chars() {
        match c {
            '{' => sub = true,
            '}' => sub = false,
            c if sub => out.push(char::from_u32(0x2080 + c.to_digit(10).unwrap_or(0)).unwrap_or(c)),
            c => out.push(c),
        }
    }
    out
}

/// Trims to `n` characters with an ellipsis.
pub fn clamp(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        return s.to_string();
    }
    let mut out: String = chars[..n - 1].iter().collect();
    out.push('…');
    out
}

pub fn minus(s: &str) -> String {
    s.replace('-', &MINUS.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_prices_use_subscript_zeros() {
        assert_eq!(price(0.000_000_004_42), "$0.0{8}442");
        assert_eq!(plain(&price(0.000_000_004_42)), "$0.0₈442");
        assert_eq!(price(0.000_000_023_1), "$0.0{7}231");
        assert_eq!(price(3e-12), "$0.0{11}3");
        assert_eq!(price(0.000_120_03), "$0.00012");
        assert_eq!(price(0.00543), "$0.00543");
        assert_eq!(price(243.17), "$243.17");
        assert_eq!(price(0.000_099_999_99), "$0.0{3}1");
    }

    #[test]
    fn dollars_percents_and_multiples() {
        assert_eq!(usd(1_290_840.0, false), "$1.29M");
        assert_eq!(usd(999_960.0, false), "$1M");
        assert_eq!(usd(50_840.0, true), "+$50,840");
        assert_eq!(usd(2.4136, true), "+$2.41");
        assert_eq!(usd(0.004, true), "+$0.004");
        assert_eq!(usd(0.5, false), "$0.50");
        assert_eq!(usd(0.000_04, true), "+$0.0{4}4");
        assert_eq!(usd(-265.7, true), "\u{2212}$265.70");
        assert_eq!(usd(0.0, true), "$0.00");
        assert_eq!(pct(80.2413), "+80.2%");
        assert_eq!(pct(422.6), "+423%");
        assert_eq!(pct(0.8), "+0.80%");
        assert_eq!(pct(0.004), "+<0.01%");
        assert_eq!(pct(12_840.0), "+12,840%");
        assert_eq!(pct(250_000.0), "+250K%");
        assert_eq!(pct(-100.0), "\u{2212}100%");
        assert_eq!(mult(1.802), "1.80×");
        assert_eq!(mult(12.94), "12.9×");
        assert_eq!(mult(129.4), "129×");
        assert_eq!(mult(2500.0), "2.5K×");
        assert_eq!(mult(0.0), "0×");
    }

    #[test]
    fn times_and_dates() {
        assert_eq!(held(4165), "1h 09m");
        assert_eq!(held(2 * 86_400 + 4 * 3600), "2d 4h");
        assert_eq!(held(95 * 86_400), "3mo 5d");
        assert_eq!(date(1_792_495_294), "20 Oct 2026");
        assert_eq!(time(1_792_495_294), "11:21 UTC");
        assert_eq!(
            clamp("SUPERCALIFRAGILISTICEXPIALIDOCIOUS", 20)
                .chars()
                .count(),
            20
        );
    }
}
