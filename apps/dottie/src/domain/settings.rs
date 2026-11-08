//! A user's quick-action settings: the buy and sell buttons, what pays
//! for a buy, and their default slippage. Saved per host account; guests
//! and unsaved accounts get the defaults.
use super::amount::{Amount, parse};
use crate::providers::settings::Settings as Store;
use crate::runtime::{Fail, Runtime, Source, Turn};
use aomi_sdk::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What pays for a buy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Pay {
    /// USDC for a dollar-sized buy when it covers it, else SOL.
    #[default]
    Auto,
    Sol,
    Usdc,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Quick-buy sizes: "0.1 SOL" or "$20".
    pub buy: Vec<String>,
    /// Sell buttons: "25%".
    pub sell: Vec<String>,
    pub pay: Pay,
    /// `None`: Dottie's per-coin defaults.
    pub slippage_bps: Option<u32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            buy: ["0.1 SOL", "0.5 SOL", "1 SOL"].map(String::from).to_vec(),
            sell: ["25%", "50%", "100%"].map(String::from).to_vec(),
            pay: Pay::Auto,
            slippage_bps: None,
        }
    }
}

impl Settings {
    /// Checked and spelled one way: 1–3 buys of 0.001–100 SOL or
    /// $1–$10,000, 1–3 sell percents, slippage up to 10%.
    pub fn checked(self) -> Result<Self, String> {
        let list = |items: &[String], what: &str| {
            if (1..=3).contains(&items.len()) {
                Ok(())
            } else {
                Err(format!("Give 1 to 3 {what}."))
            }
        };
        list(&self.buy, "buy amounts")?;
        list(&self.sell, "sell percents")?;
        let buy = self
            .buy
            .iter()
            .map(|b| match parse(b) {
                Ok(Amount::Usd(d)) if (1.0..=10_000.0).contains(&d) => Ok(format!("${d}")),
                Ok(Amount::Bare(n) | Amount::Sol(n))
                    if (0.001..=100.0).contains(&n.parse().unwrap_or(0.0)) =>
                {
                    Ok(format!("{n} SOL"))
                }
                _ => Err(format!(
                    "\"{b}\" isn't a buy size: 0.001 to 100 SOL, or $1 to $10,000."
                )),
            })
            .collect::<Result<_, _>>()?;
        let sell = self
            .sell
            .iter()
            .map(|s| match parse(s) {
                Ok(Amount::Pct(p)) if p >= 1.0 => Ok(format!("{p}%")),
                Ok(Amount::Bare(n)) if (1.0..=100.0).contains(&n.parse().unwrap_or(0.0)) => {
                    Ok(format!("{n}%"))
                }
                _ => Err(format!("\"{s}\" isn't a sell percent from 1% to 100%.")),
            })
            .collect::<Result<_, _>>()?;
        if self.slippage_bps.is_some_and(|b| !(1..=1000).contains(&b)) {
            return Err("Slippage goes from 0.01% to 10%.".into());
        }
        Ok(Self { buy, sell, ..self })
    }
}

/// The saved settings; defaults for guests or an empty saved document.
/// A signed-in account requires its store to be configured and readable.
pub async fn saved(rt: &Runtime, turn: &Turn) -> Result<Settings, Fail> {
    let Some(account) = &turn.account else {
        return Ok(Settings::default());
    };
    if turn.settings_store().is_none() {
        return Err(Fail::Unconfigured(Source::Settings));
    }
    let doc = Store::new(rt, turn).get(account).await?;
    Settings::from_saved(doc)
}

impl Settings {
    fn from_saved(doc: serde_json::Value) -> Result<Self, Fail> {
        serde_json::from_value::<Self>(doc)
            .map_err(|_| {
                Fail::Invalid("Saved settings are invalid; review /settings before trading.".into())
            })?
            .checked()
            .map_err(Fail::Invalid)
    }
}

/// As [`saved`], falling back to the defaults (said as a gap) when the
/// store can't be read.
pub async fn load(rt: &Runtime, turn: &Turn) -> Settings {
    saved(rt, turn).await.unwrap_or_else(|fail| {
        turn.gap(format!("saved settings unavailable: {}", fail.message()));
        Settings::default()
    })
}

/// Saves checked settings for the chat's account.
pub async fn save(rt: &Runtime, turn: &Turn, settings: Settings) -> Result<Settings, Fail> {
    let account = turn
        .account
        .as_deref()
        .ok_or_else(|| Fail::Invalid("Sign in to save settings.".into()))?;
    let settings = settings.checked().map_err(Fail::Invalid)?;
    let doc = serde_json::to_value(&settings).map_err(|e| Fail::Invalid(e.to_string()))?;
    Store::new(rt, turn).put(account, &doc).await?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_authenticated_trade_requires_its_settings_store() {
        use std::collections::BTreeMap;
        let rt = Runtime::replaying(BTreeMap::new()).unwrap();
        let mut turn = Turn::default();
        turn.account = Some("settings-test".into());
        assert!(matches!(
            rt.tokio.block_on(saved(&rt, &turn)),
            Err(Fail::Unconfigured(Source::Settings))
        ));
        assert_eq!(
            rt.tokio.block_on(saved(&rt, &Turn::default())).unwrap(),
            Settings::default()
        );
    }

    #[test]
    fn unavailable_saved_settings_do_not_become_default_trade_settings() {
        use crate::runtime::{Lane, SETTINGS_TOKEN, SETTINGS_URL, Source};
        use aomi_sdk::testing::TestCtxBuilder;
        use serde_json::json;
        use std::collections::BTreeMap;

        let ctx = TestCtxBuilder::new("dottie_buy")
            .secret(SETTINGS_URL, "https://settings.test")
            .secret(SETTINGS_TOKEN, "local-test-token")
            .attribute("hosted", json!({"account_id": "settings-test"}))
            .build();
        let turn = Turn::new(&ctx, Lane::Chat);
        let fail = Fail::unavailable(Source::Settings, "offline");
        let rt = Runtime::replaying(BTreeMap::from([(
            "settings:settings-test".into(),
            fail.recorded(Source::Settings),
        )]))
        .unwrap();
        assert!(rt.tokio.block_on(saved(&rt, &turn)).is_err());
    }

    #[test]
    fn invalid_saved_slippage_cannot_be_replaced_by_a_wider_default() {
        use serde_json::json;
        assert!(Settings::from_saved(json!({"slippage_bps": 1001})).is_err());
        assert!(Settings::from_saved(json!({"slippage_bps": "50"})).is_err());
        assert!(Settings::from_saved(json!(null)).is_err());
        assert_eq!(
            Settings::from_saved(json!({})).unwrap(),
            Settings::default()
        );
        assert_eq!(
            Settings::from_saved(json!({"slippage_bps": 50}))
                .unwrap()
                .slippage_bps,
            Some(50)
        );
    }

    #[test]
    fn settings_are_checked_and_spelled_one_way() {
        let s = Settings {
            buy: vec!["0.25".into(), "$25".into(), "2 sol".into()],
            sell: vec!["50".into(), "all".into()],
            slippage_bps: Some(300),
            ..Settings::default()
        };
        let s = s.checked().unwrap();
        assert_eq!(s.buy, ["0.25 SOL", "$25", "2 SOL"]);
        assert_eq!(s.sell, ["50%", "100%"]);
        assert_eq!(Settings::default().checked().unwrap(), Settings::default());
        let bad = |f: fn(&mut Settings)| {
            let mut s = Settings::default();
            f(&mut s);
            s.checked().is_err()
        };
        assert!(bad(|s| s.buy.clear()));
        assert!(bad(|s| s.buy = vec!["$0.5".into()]));
        assert!(bad(|s| s.buy = vec!["500 sol".into()]));
        assert!(bad(|s| s.buy = vec!["25%".into()]));
        assert!(bad(|s| s.sell = vec!["$5".into()]));
        assert!(bad(|s| s.slippage_bps = Some(1500)));
        let doc: Settings = serde_json::from_value(serde_json::json!({ "pay": "usdc" })).unwrap();
        assert_eq!((doc.pay, doc.buy.len()), (Pay::Usdc, 3));
    }
}
