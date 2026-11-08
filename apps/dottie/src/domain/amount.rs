//! Exact token amounts, and sizes as people type them. A decimal the token
//! can't represent is an error, never a rounding.
/// Whole-unit decimal string ("0.05") to atomic units. SPL amounts are
/// u64; anything past that is an error.
pub fn from_decimal(value: &str, decimals: u8) -> Result<u128, String> {
    let value = value.trim();
    let (whole, fractional) = value.split_once('.').unwrap_or((value, ""));
    let plain = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty()
        || value.len() > 48
        || !plain(whole)
        || !plain(fractional)
        || (value.contains('.') && fractional.is_empty())
    {
        return Err("expected a plain non-negative decimal such as 0.05".into());
    }
    if fractional.len() > decimals as usize {
        return Err(format!("this token supports at most {decimals} decimals"));
    }
    let digits = format!("{whole}{fractional:0<width$}", width = decimals as usize);
    let digits = digits.trim_start_matches('0');
    digits
        .parse::<u128>()
        .ok()
        .or(digits.is_empty().then_some(0))
        .filter(|v| *v <= u128::from(u64::MAX))
        .ok_or_else(|| "amount is too large for a Solana token".to_string())
}

/// Atomic units back to a whole-unit decimal string.
pub fn format(value: u128, decimals: u8) -> String {
    if decimals == 0 {
        return value.to_string();
    }
    let scale = 10u128.pow(decimals as u32);
    let (whole, rem) = (value / scale, value % scale);
    if rem == 0 {
        return whole.to_string();
    }
    let frac = format!("{:0>width$}", rem, width = decimals as usize);
    format!("{whole}.{}", frac.trim_end_matches('0'))
}

/// A size as a person types it after /buy or /sell, or in a reply.
#[derive(Clone, Debug, PartialEq)]
pub enum Amount {
    /// "$5", "5 usd".
    Usd(f64),
    /// "0.5 sol".
    Sol(String),
    /// "5 usdc".
    Usdc(String),
    /// "25%"; "all" is 100.
    Pct(f64),
    /// "1000 bonk": whole units of the token named, lowercased.
    Tokens(String, String),
    /// A plain number: SOL on a buy, unclear on a sell.
    Bare(String),
}

fn base58ish(word: &str) -> bool {
    word.len() >= 32
        && word
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() && !matches!(b, b'0' | b'O' | b'I' | b'l'))
}

/// Splits command text into an optional mint and an optional size:
/// "Dez… 0.5", "$20", "0.5 sol", "25%", "1000 bonk".
pub fn order(text: &str) -> Result<(Option<String>, Option<Amount>), String> {
    let mut token = None;
    let mut rest = String::new();
    for word in text.split(|c: char| c.is_whitespace() || c == ',') {
        if base58ish(word) || word.len() > 30 {
            let mint = crate::providers::pubkey(word)
                .map_err(|_| "That's not a full Solana mint address.".to_string())?;
            if token.replace(mint.to_string()).is_some() {
                return Err("One token at a time.".into());
            }
        } else {
            rest.push_str(word);
        }
    }
    let size = (!rest.is_empty()).then(|| parse(&rest)).transpose()?;
    Ok((token, size))
}

/// One size, spaces ignored: "$5", "5$", "0.5sol", "25%", "all".
pub fn parse(text: &str) -> Result<Amount, String> {
    let bad = || "Say an amount like 0.5 SOL, $5 or 25%.".to_string();
    let t: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    if t == "all" || t == "max" {
        return Ok(Amount::Pct(100.0));
    }
    let (n, unit) = match t.strip_prefix('$') {
        Some(n) => (n, "usd"),
        None => t.split_at(
            t.find(|c: char| !c.is_ascii_digit() && c != '.')
                .unwrap_or(t.len()),
        ),
    };
    let n = if n.starts_with('.') {
        format!("0{n}")
    } else {
        n.to_string()
    };
    let value: f64 = n.parse().map_err(|_| bad())?;
    let plain = n.bytes().all(|b| b.is_ascii_digit() || b == b'.') && n.matches('.').count() <= 1;
    if !plain || !value.is_finite() || value <= 0.0 {
        return Err(bad());
    }
    Ok(match unit {
        "" => Amount::Bare(n),
        "$" | "usd" | "dollar" | "dollars" => Amount::Usd(value),
        "%" if value <= 100.0 => Amount::Pct(value),
        "sol" => Amount::Sol(n),
        "usdc" => Amount::Usdc(n),
        symbol
            if symbol.len() <= 16
                && symbol.starts_with(|c: char| c.is_ascii_alphabetic())
                && symbol.bytes().all(|b| b.is_ascii_alphanumeric()) =>
        {
            Amount::Tokens(n, symbol.to_string())
        }
        _ => return Err(bad()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_convert_exactly() {
        assert_eq!(
            from_decimal("0.05", 18).unwrap().to_string(),
            "50000000000000000"
        );
        assert_eq!(from_decimal("1.5", 6).unwrap().to_string(), "1500000");
        assert_eq!(from_decimal("0", 6).unwrap().to_string(), "0");
        assert!(from_decimal("0.1234567", 6).is_err());
        assert!(from_decimal("-1", 6).is_err());
        assert!(from_decimal("1e5", 6).is_err());
        assert!(from_decimal("1.", 6).is_err());
        assert_eq!(format(from_decimal("1.2345", 6).unwrap(), 6), "1.2345");
        assert!(from_decimal("99999999999999999999", 9).is_err());
    }

    #[test]
    fn sizes_read_like_people_type_them() {
        let bonk = "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263";
        assert_eq!(
            order(&format!(" {bonk} 0.5 ")).unwrap(),
            (Some(bonk.into()), Some(Amount::Bare("0.5".into())))
        );
        assert_eq!(order("").unwrap(), (None, None));
        assert_eq!(parse("$20").unwrap(), Amount::Usd(20.0));
        assert_eq!(parse("2.5 usd").unwrap(), Amount::Usd(2.5));
        assert_eq!(parse("0.1 SOL").unwrap(), Amount::Sol("0.1".into()));
        assert_eq!(parse(".5sol").unwrap(), Amount::Sol("0.5".into()));
        assert_eq!(parse("5 usdc").unwrap(), Amount::Usdc("5".into()));
        assert_eq!(parse("25%").unwrap(), Amount::Pct(25.0));
        assert_eq!(parse("All").unwrap(), Amount::Pct(100.0));
        assert_eq!(
            order("1,000 BONK").unwrap().1,
            Some(Amount::Tokens("1000".into(), "bonk".into()))
        );
        for bad in ["0", "$0", "150%", "-5", "five", "5 $$", "0.1.2 sol"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        assert!(order("DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7Ya 5").is_err());
        assert!(order("0x5fc5360d0400a0fd4f2af552add042d716f1d168 5").is_err());
        assert!(order(&format!("{bonk} {bonk}")).is_err());
    }
}
