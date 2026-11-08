//! One token snapshot, built the same way for every tool: GMGN token info
//! and security (flow, liquidity, ATH, holder mix and Solana launch facts)
//! merged with RugCheck's authorities and Token-2022 transfer fee.
use super::market::{Stage, is_base_asset, known, launchpad, stage};
use crate::providers::gmgn::{Gmgn, types as gm};
use crate::providers::rpc::{Mint, Rpc};
use crate::providers::rugcheck::{self, Report};
use crate::providers::text;
use crate::runtime::{Fail, Runtime, Source, Turn};
use crate::view::safe;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    M5,
    H1,
    H4,
    H6,
    H24,
}

impl Window {
    pub fn label(self) -> &'static str {
        match self {
            Window::M5 => "5m",
            Window::H1 => "1h",
            Window::H4 => "4h",
            Window::H6 => "6h",
            Window::H24 => "24h",
        }
    }
}

/// Buy and sell dollars and trade counts over one window.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flow {
    pub buy_usd: Option<f64>,
    pub sell_usd: Option<f64>,
    pub buys: Option<f64>,
    pub sells: Option<f64>,
}

/// Share of supply or trading (percent 0–100) held or done by each kind of
/// wallet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HolderMix {
    pub dev_pct: Option<f64>,
    pub dev_team_pct: Option<f64>,
    pub snipers_pct: Option<f64>,
    pub bundler_trading_pct: Option<f64>,
    pub rat_trading_pct: Option<f64>,
    pub bot_degen_pct: Option<f64>,
    pub fresh_wallet_pct: Option<f64>,
    /// Trading by wallets GMGN flags as bait.
    pub entrapment_trading_pct: Option<f64>,
}

/// How many holders carry each GMGN wallet tag.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TaggedWallets {
    pub smart: Option<f64>,
    pub kol: Option<f64>,
    pub snipers: Option<f64>,
    pub bundlers: Option<f64>,
    pub whales: Option<f64>,
    pub fresh: Option<f64>,
    pub rats: Option<f64>,
}

/// The contract screen: GMGN's, with RugCheck filling authorities and
/// adding the Token-2022 fee. `None` fields are unknown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Security {
    pub honeypot: Option<bool>,
    pub buy_tax_pct: Option<f64>,
    pub sell_tax_pct: Option<f64>,
    /// Mint authority revoked: nobody can print more.
    pub mint_renounced: Option<bool>,
    /// Freeze authority revoked: nobody can freeze holders.
    pub freeze_renounced: Option<bool>,
    /// GMGN says the pool's liquidity is locked or burnt.
    pub lp_locked: Option<bool>,
    /// Token-2022 transfer fee; it comes off the exit too.
    pub transfer_fee_pct: Option<f64>,
    /// Token-2022 extensions that let someone move, block or tax holders'
    /// tokens.
    pub risky_extensions: Vec<String>,
    pub flags: Vec<String>,
}

impl Security {
    /// A red flag that rules out a buy on its own.
    pub fn red_flag(&self) -> bool {
        self.honeypot == Some(true)
            || self.sell_tax_pct.is_some_and(|t| t > 10.0)
            || self.transfer_fee_pct.is_some_and(|t| t > 10.0)
    }

    /// The mint account is the chain's own answer on authorities and
    /// Token-2022 fees, so it overrides GMGN's screen.
    pub fn merge(&mut self, mint: &Mint) {
        self.mint_renounced = Some(mint.mint_authority.is_none());
        self.freeze_renounced = Some(mint.freeze_authority.is_none());
        self.transfer_fee_pct = Some(mint.transfer_fee_bps / 100.0);
        self.risky_extensions = mint.risky_extensions.clone();
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Socials {
    pub twitter: Option<String>,
    pub website: Option<String>,
    pub telegram: Option<String>,
    /// Tokens launched from the same X account: a serial launcher.
    pub x_launches: Option<f64>,
    /// Times the linked X account was renamed.
    pub x_renames: Option<usize>,
    /// The community took the coin over from its dev.
    pub community_takeover: Option<bool>,
}

/// What the X link points at: the project's own account, a single post the
/// meme is about, or a community.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum XLink {
    Account,
    Post,
    Community,
}

impl Socials {
    /// The X link as (kind, handle or path), from whatever shape GMGN stored.
    pub fn x(&self) -> Option<(XLink, String)> {
        let raw = self.twitter.as_deref()?.trim();
        let path = raw
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("www.")
            .trim_start_matches("x.com/")
            .trim_start_matches("twitter.com/")
            .trim_matches('/');
        if path.is_empty() {
            return None;
        }
        let kind = if path.contains("/status/") {
            XLink::Post
        } else if path.contains("communities") {
            XLink::Community
        } else {
            XLink::Account
        };
        Some((kind, path.split('?').next().unwrap_or(path).to_string()))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TokenSnapshot {
    pub token: String,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub decimals: Option<u8>,
    pub launchpad: Option<String>,
    pub stage: Stage,
    /// The quote asset of its main pool, when it is a catalog asset.
    pub pair: Option<String>,
    /// The DEX a graduated coin moved to (`pump_amm`, `raydium`…).
    pub migrated_to: Option<String>,
    pub created_at: Option<i64>,
    pub price_usd: Option<f64>,
    pub fdv_usd: Option<f64>,
    pub liquidity_usd: Option<f64>,
    pub volume_24h_usd: Option<f64>,
    pub change_pct: Vec<(Window, f64)>,
    pub flow: Vec<(Window, Flow)>,
    pub holders: Option<f64>,
    /// Top 10 wallets; pools, curves and lockers excluded.
    pub top10_pct: Option<f64>,
    pub mix: HolderMix,
    pub tagged: TaggedWallets,
    pub ath_usd: Option<f64>,
    /// Launchpad curve progress 0–100 while on a curve.
    pub curve_pct: Option<f64>,
    pub creator: Option<String>,
    /// GMGN's `creator_token_status`: `creator_hold`, `creator_close`…
    pub dev_status: Option<String>,
    pub socials: Socials,
    pub security: Option<Security>,
    /// RugCheck's score and named risks (card only).
    pub rugcheck: Option<Report>,
    pub source: Source,
}

impl TokenSnapshot {
    pub fn from_gmgn(info: gm::TokenInfo) -> Self {
        let p = &info.price;
        let now = p.price.get();
        let change = |then: crate::providers::Num| match (now, then.get()) {
            (Some(n), Some(t)) if t > 0.0 => Some((n / t - 1.0) * 100.0),
            _ => None,
        };
        let change_pct = [
            (Window::M5, change(p.price_5m)),
            (Window::H1, change(p.price_1h)),
            (Window::H6, change(p.price_6h)),
            (Window::H24, change(p.price_24h)),
        ]
        .into_iter()
        .filter_map(|(w, v)| v.map(|v| (w, v)))
        .collect();
        let flow = vec![
            (
                Window::M5,
                flow(p.buy_volume_5m, p.sell_volume_5m, p.buys_5m, p.sells_5m),
            ),
            (
                Window::H1,
                flow(p.buy_volume_1h, p.sell_volume_1h, p.buys_1h, p.sells_1h),
            ),
            (
                Window::H6,
                flow(p.buy_volume_6h, p.sell_volume_6h, p.buys_6h, p.sells_6h),
            ),
            (
                Window::H24,
                flow(p.buy_volume_24h, p.sell_volume_24h, p.buys_24h, p.sells_24h),
            ),
        ];
        let launchpad = launchpad(info.launchpad_platform.as_deref());
        let progress = info.launchpad_progress.get();
        let stage = stage(launchpad.is_some(), info.launchpad_status.get(), progress);
        let s = &info.stat;
        let t = &info.wallet_tags_stat;
        let supply = info.total_supply.get();
        Self {
            token: info.address.trim().to_string(),
            symbol: info.symbol.clone(),
            name: info.name.clone(),
            decimals: info.decimals,
            launchpad,
            stage,
            pair: text(&info.pool.quote_address)
                .and_then(known)
                .map(|a| a.symbol.clone()),
            migrated_to: (stage == Stage::Graduated)
                .then(|| text(&info.pool.exchange).map(str::to_string))
                .flatten(),
            created_at: info.creation_timestamp.ts().or(info.open_timestamp.ts()),
            price_usd: now,
            fdv_usd: now.zip(supply).map(|(p, s)| p * s),
            liquidity_usd: info.liquidity.get(),
            volume_24h_usd: p.volume_24h.get(),
            change_pct,
            flow,
            holders: info.holder_count.get(),
            top10_pct: s.top_10_holder_rate.pct(),
            mix: HolderMix {
                dev_pct: s.creator_hold_rate.pct(),
                dev_team_pct: s.dev_team_hold_rate.pct(),
                snipers_pct: s.top70_sniper_hold_rate.pct(),
                bundler_trading_pct: s.top_bundler_trader_percentage.pct(),
                rat_trading_pct: s.top_rat_trader_percentage.pct(),
                bot_degen_pct: s.bot_degen_rate.pct(),
                fresh_wallet_pct: s.fresh_wallet_rate.pct(),
                entrapment_trading_pct: s.top_entrapment_trader_percentage.pct(),
            },
            tagged: TaggedWallets {
                smart: t.smart_wallets.get(),
                kol: t.renowned_wallets.get(),
                snipers: t.sniper_wallets.get(),
                bundlers: t.bundler_wallets.get(),
                whales: t.whale_wallets.get(),
                fresh: t.fresh_wallets.get(),
                rats: t.rat_trader_wallets.get(),
            },
            ath_usd: info.ath_price.get(),
            curve_pct: (stage == Stage::Curve)
                .then(|| progress.map(|p| p * 100.0))
                .flatten(),
            creator: text(&info.dev.creator_address).map(str::to_string),
            // A GMGN status code such as `creator_close`, not free text.
            dev_status: text(&info.dev.creator_token_status).map(|s| {
                s.chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .take(24)
                    .collect()
            }),
            socials: Socials {
                twitter: text(&info.link.twitter_username).map(str::to_string),
                website: text(&info.link.website).map(str::to_string),
                telegram: text(&info.link.telegram).map(str::to_string),
                x_launches: info.dev.twitter_create_token_count.get(),
                x_renames: Some(info.dev.twitter_name_change_history.len()),
                community_takeover: info.dev.cto_flag.get().map(|f| f == 1.0),
            },
            security: None,
            rugcheck: None,
            source: Source::Gmgn,
        }
    }

    pub fn flow(&self, window: Window) -> Option<&Flow> {
        self.flow.iter().find(|(w, _)| *w == window).map(|(_, f)| f)
    }
}

fn flow(
    buy: crate::providers::Num,
    sell: crate::providers::Num,
    buys: crate::providers::Num,
    sells: crate::providers::Num,
) -> Flow {
    Flow {
        buy_usd: buy.get(),
        sell_usd: sell.get(),
        buys: buys.get(),
        sells: sells.get(),
    }
}

pub fn security(raw: gm::Security) -> Security {
    Security {
        honeypot: raw.honeypot.get(),
        buy_tax_pct: raw.buy_tax.pct(),
        sell_tax_pct: raw.sell_tax.pct(),
        mint_renounced: raw.renounced_mint.get(),
        freeze_renounced: raw.renounced_freeze_account.get(),
        lp_locked: raw.lock_summary.is_locked.get(),
        transfer_fee_pct: None,
        risky_extensions: vec![],
        flags: raw.flags.into_iter().take(6).collect(),
    }
}

/// What a watcher polls: curve progress and whether the coin graduated.
/// Progress reads 100 once graduated, so "ping me at 90%" never misses a
/// graduation between polls.
pub fn watch_progress(snap: &TokenSnapshot) -> (Option<f64>, Option<bool>) {
    match snap.stage {
        Stage::Curve => (snap.curve_pct, Some(false)),
        Stage::Graduated => (Some(100.0), Some(true)),
        Stage::Pool => (None, None),
    }
}

/// How much of the snapshot to fill in.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// Market numbers and stage (watch polls).
    Basic,
    /// Live buy facts: security and the transfer fee.
    Trade,
    /// Everything on the card: security, RugCheck's risks.
    Card,
}

/// Reads one token: GMGN info, and for a card or a trade GMGN's security
/// screen and RugCheck's report.
pub async fn snapshot(
    rt: &Runtime,
    turn: &Turn,
    token: &str,
    depth: Depth,
) -> Result<TokenSnapshot, Fail> {
    let gmgn = if depth == Depth::Trade {
        Gmgn::new(rt, turn).live()
    } else {
        Gmgn::new(rt, turn)
    };
    let deep = depth != Depth::Basic;
    let card = depth == Depth::Card;
    let (info, screen, mint, report) = tokio::join!(
        gmgn.token_info(token),
        async {
            if deep {
                Some(gmgn.security(token).await)
            } else {
                None
            }
        },
        async {
            if deep {
                Some(Rpc::new(rt, turn).mint(token).await)
            } else {
                None
            }
        },
        async {
            if card {
                Some(rugcheck::report(rt, turn, token).await)
            } else {
                None
            }
        }
    );
    let mut snap = match info? {
        info if info.address.trim() == token => TokenSnapshot::from_gmgn(info),
        info if !info.address.is_empty() => {
            return Err(Fail::Invalid("provider returned a different mint".into()));
        }
        _ => {
            return Err(Fail::NotFound(
                "no market data for this exact mint on Solana".into(),
            ));
        }
    };
    match screen {
        Some(Ok(s)) => snap.security = Some(self::security(s)),
        Some(Err(_)) => turn.gap("contract security screen unavailable"),
        None => {}
    }
    match mint {
        Some(Ok(mint)) => snap
            .security
            .get_or_insert_with(Security::default)
            .merge(&mint),
        Some(Err(fail)) => turn.gap(format!(
            "mint account unreadable ({}); authorities are gmgn's and the transfer fee is unknown",
            fail.code()
        )),
        None => {}
    }
    match report {
        Some(Ok(report)) => snap.rugcheck = Some(report),
        Some(Err(fail)) => turn.gap(format!("rugcheck unavailable ({})", fail.code())),
        None => {}
    }
    Ok(snap)
}

/// Who a token is, cheaply, for quick actions: the mint account from chain
/// (it proves the address is a token) and GMGN's market numbers when it
/// lists the coin. No security screen or exit probe. Symbol and name are
/// already safe to put in Markdown.
#[derive(Clone, Debug, PartialEq)]
pub struct Identity {
    pub token: String,
    pub symbol: String,
    pub name: Option<String>,
    pub decimals: u8,
    pub stage: Stage,
    pub price_usd: Option<f64>,
    pub fdv_usd: Option<f64>,
    pub liquidity_usd: Option<f64>,
    /// SOL, a stable, an LST, a major or a stock token: the quote is the
    /// check, no meme rules.
    pub base: bool,
}

/// Reads a token's identity. An address that isn't a mint (a wallet or a
/// program) is `NotFound`, said for the user.
pub async fn identity(rt: &Runtime, turn: &Turn, token: &str) -> Result<Identity, Fail> {
    let (mint, info) = tokio::join!(
        Rpc::new(rt, turn).mint(token),
        Gmgn::new(rt, turn).token_info(token)
    );
    let catalog = known(token);
    let decimals = match (mint, catalog) {
        (Ok(m), _) => m.decimals,
        (Err(_), Some(asset)) => asset.decimals,
        (Err(fail), None) => return Err(fail),
    };
    let snap = info
        .ok()
        .filter(|i| i.address.trim() == token)
        .map(TokenSnapshot::from_gmgn);
    let snap = snap.as_ref();
    Ok(Identity {
        token: token.to_string(),
        symbol: catalog
            .map(|a| a.symbol.clone())
            .or_else(|| snap.and_then(|s| s.symbol.clone()))
            .map(|s| safe(&s, 16))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "?".into()),
        name: catalog
            .map(|a| a.name.clone())
            .or_else(|| snap.and_then(|s| s.name.clone()))
            .map(|n| safe(&n, 32))
            .filter(|n| !n.is_empty()),
        decimals,
        stage: snap.map_or(Stage::Pool, |s| s.stage),
        price_usd: snap.and_then(|s| s.price_usd),
        fdv_usd: snap.and_then(|s| s.fdv_usd),
        liquidity_usd: snap.and_then(|s| s.liquidity_usd),
        base: is_base_asset(token),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str) -> gm::TokenInfo {
        let text = std::fs::read_to_string(format!(
            "{}/src/fixtures/gmgn/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let body: serde_json::Value = serde_json::from_str(&text).unwrap();
        serde_json::from_value(body["data"].clone()).unwrap()
    }

    #[test]
    fn gmgn_snapshot_reads_flow_mix_and_solana_fields() {
        let bonk = TokenSnapshot::from_gmgn(info("info_bonk"));
        assert_eq!(bonk.symbol.as_deref(), Some("Bonk"));
        assert_eq!(bonk.decimals, Some(5));
        assert_eq!(bonk.stage, Stage::Pool);
        assert_eq!(bonk.pair.as_deref(), Some("SOL"));
        assert!((bonk.top10_pct.unwrap() - 31.25).abs() < 1e-9);
        assert!(bonk.flow(Window::H1).unwrap().buy_usd.is_some());
        assert_eq!(bonk.dev_status.as_deref(), Some("creator_close"));
        let fresh = TokenSnapshot::from_gmgn(info("info_fresh"));
        assert_eq!(fresh.launchpad.as_deref(), Some("pump.fun"));
        assert!(fresh.fdv_usd.is_some_and(|f| f > 0.0));
    }

    #[test]
    fn the_mint_account_sets_authorities_and_the_fee() {
        let mut s = Security {
            mint_renounced: Some(true),
            ..Security::default()
        };
        s.merge(&Mint {
            decimals: 6,
            token_2022: true,
            mint_authority: Some("M".into()),
            freeze_authority: None,
            transfer_fee_bps: 300.0,
            risky_extensions: vec!["transferHook".into()],
        });
        assert_eq!(s.mint_renounced, Some(false));
        assert_eq!(s.freeze_renounced, Some(true));
        assert_eq!(s.transfer_fee_pct, Some(3.0));
        assert_eq!(s.risky_extensions, ["transferHook"]);
        s.transfer_fee_pct = Some(12.0);
        assert!(s.red_flag());
    }

    #[test]
    fn x_links_are_classified() {
        let x = |raw: &str| {
            Socials {
                twitter: Some(raw.into()),
                ..Socials::default()
            }
            .x()
        };
        assert_eq!(x("VraxERC"), Some((XLink::Account, "VraxERC".into())));
        assert_eq!(
            x("https://x.com/sighduck"),
            Some((XLink::Account, "sighduck".into()))
        );
        assert_eq!(
            x("BenitoMuss5gzu/status/207500801012083930").map(|x| x.0),
            Some(XLink::Post)
        );
        assert_eq!(
            x("https://x.com/i/communities/123").map(|x| x.0),
            Some(XLink::Community)
        );
        assert_eq!(x(" "), None);
    }

    #[test]
    fn security_red_flags() {
        let mut s = Security::default();
        assert!(!s.red_flag());
        s.sell_tax_pct = Some(12.0);
        assert!(s.red_flag());
    }
}
