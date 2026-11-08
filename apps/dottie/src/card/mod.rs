//! Share cards: square 1080 × 1080 PNGs drawn inside the plugin. The bot
//! sends a tool result's `image_path` as a Telegram photo. Numbers come
//! only from trade records; the model picks at most a style and which
//! details show. Layout lives in `styles` (the same boxes as the card
//! studio), drawing in `layout`, artwork in `art`.
mod art;
pub mod format;
mod layout;
mod styles;
mod text;

use aomi_sdk::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt::Write;

/// One trade round's figures, frozen when the card is first made so a
/// redraw shows the same numbers.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Card {
    pub symbol: String,
    /// Still holding: `out` includes value now and `exit` is the price now.
    pub open: bool,
    /// Wallet spend including route deductions, before network fees and
    /// token-account rent.
    pub cost: Option<f64>,
    /// Sell proceeds, plus value now when open.
    pub out: f64,
    /// Net sale proceeds alone. Absent on cards made before it existed.
    #[serde(default)]
    pub proceeds: Option<f64>,
    /// Moving-average PnL on what was sold, and on what is still held.
    /// Together they equal `pnl()` when the basis is known.
    #[serde(default)]
    pub realized: Option<f64>,
    #[serde(default)]
    pub unrealized: Option<f64>,
    pub entry: Option<f64>,
    pub exit: Option<f64>,
    pub held_s: Option<i64>,
    pub buys: u32,
    pub sells: u32,
    /// Network fees the wallet paid, in SOL. Kept separate: a historical
    /// USD conversion is not implied.
    #[serde(default)]
    pub fee_sol: Option<f64>,
    #[serde(default)]
    pub fee_transactions: u32,
    /// Mandatory on the shared image, including redraws.
    #[serde(default)]
    pub accounting_note: String,
    /// When the round closed, or the card was made.
    pub ts: i64,
    /// The closing (or latest) transaction.
    pub tx: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Win,
    Loss,
    Open,
    Unknown,
}

/// What a result feels like; styles declare which moods they fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mood {
    BigWin,
    Win,
    Loss,
    Rug,
    OpenUp,
    OpenDown,
    Unknown,
}

impl Card {
    pub fn pnl(&self) -> Option<f64> {
        self.cost.map(|c| self.out - c)
    }
    pub fn pct(&self) -> Option<f64> {
        self.cost
            .filter(|c| *c > 0.0)
            .map(|c| (self.out - c) / c * 100.0)
    }
    pub fn mult(&self) -> Option<f64> {
        self.cost.filter(|c| *c > 0.0).map(|c| self.out / c)
    }
    pub fn kind(&self) -> Kind {
        if self.open {
            return Kind::Open;
        }
        match self.pnl() {
            None => Kind::Unknown,
            Some(p) if p < 0.0 => Kind::Loss,
            Some(_) => Kind::Win,
        }
    }
    pub fn mood(&self) -> Mood {
        let Some(pnl) = self.pnl() else {
            return Mood::Unknown;
        };
        match () {
            _ if self.open && pnl >= 0.0 => Mood::OpenUp,
            _ if self.open => Mood::OpenDown,
            _ if self.pct().is_some_and(|p| p <= -90.0) => Mood::Rug,
            _ if pnl < 0.0 => Mood::Loss,
            _ if self.mult().is_some_and(|m| m >= 2.0) => Mood::BigWin,
            _ => Mood::Win,
        }
    }
}

/// The big number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Hero {
    /// Return %.
    #[default]
    Pct,
    /// Profit in dollars.
    Usd,
    /// Multiple, e.g. 1.80×.
    Mult,
}

/// Details a card can show.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Show {
    Usd,
    Pct,
    Mult,
    /// Cost in and dollars out.
    Inout,
    /// Average entry and exit price.
    Prices,
    Held,
    Date,
    /// Number of buys and sells.
    Fills,
    /// Network fees paid, in SOL.
    Fees,
    /// The user's name, only when they asked for it.
    Name,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Look {
    pub hero: Hero,
    pub show: BTreeSet<Show>,
    pub name: Option<String>,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            hero: Hero::Pct,
            show: [Show::Usd, Show::Pct, Show::Mult, Show::Inout, Show::Held].into(),
            name: None,
        }
    }
}

impl Look {
    pub fn shows(&self, s: Show) -> bool {
        self.show.contains(&s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    /// Money pile. Wins and open positions in profit.
    Bag,
    /// Robin Hood's target. Wins and open positions in profit.
    Bullseye,
    /// Old West poster. Any result.
    Wanted,
    /// Till receipt. Any result.
    Receipt,
    /// Stock certificate. Any result.
    Certificate,
    /// Holo trading card. Any result.
    Tcg,
    /// Rain cloud and a wet cat. Losses, rugs and open positions in the red.
    Cope,
}

impl Style {
    pub const ALL: [Style; 7] = [
        Style::Bag,
        Style::Bullseye,
        Style::Wanted,
        Style::Receipt,
        Style::Certificate,
        Style::Tcg,
        Style::Cope,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Style::Bag => "Bag",
            Style::Bullseye => "Bullseye",
            Style::Wanted => "Wanted poster",
            Style::Receipt => "Receipt",
            Style::Certificate => "Certificate",
            Style::Tcg => "Trading card",
            Style::Cope => "Cope",
        }
    }

    /// Styles that fit any mood change their wording instead.
    fn universal(self) -> bool {
        matches!(
            self,
            Style::Wanted | Style::Receipt | Style::Certificate | Style::Tcg
        )
    }

    pub fn fits(self, mood: Mood) -> bool {
        use Mood::*;
        match self {
            Style::Bag | Style::Bullseye => matches!(mood, BigWin | Win | OpenUp),
            Style::Cope => matches!(mood, Loss | Rug | OpenDown),
            _ => true,
        }
    }

    fn design(self, c: &Card, look: &Look) -> styles::Design {
        let mut design = match self {
            Style::Bag => styles::bag(c, look),
            Style::Bullseye => styles::bullseye(c, look),
            Style::Wanted => styles::wanted(c, look),
            Style::Receipt => styles::receipt(c, look),
            Style::Certificate => styles::certificate(c, look),
            Style::Tcg => styles::tcg(c, look),
            Style::Cope => styles::cope(c, look),
        };
        design.items.push(styles::Item::Panel(
            r##"<rect x="0" y="1048" width="1080" height="32" fill="#20251f"/>"##.into(),
        ));
        design.items.push(styles::Item::Label(
            layout::slot(
                "accounting",
                [16.0, 1048.0, 1048.0, 32.0],
                layout::Body::Line(vec![layout::Piece::Run(
                    layout::run(&c.accounting_note, text::Font::MonoBold).fill("#ffffff"),
                )]),
                20.0,
                16.0,
            )
            .center(),
        ));
        design
    }
}

/// The asked style when it fits; otherwise a weighted random fitting one
/// (mood-specific styles count double), with a note to pass on.
pub fn pick(
    card: &Card,
    asked: Option<Style>,
    avoid: Option<Style>,
    seed: u64,
) -> (Style, Option<String>) {
    let mood = card.mood();
    if let Some(s) = asked
        && s.fits(mood)
    {
        return (s, None);
    }
    let pool: Vec<Style> = Style::ALL
        .into_iter()
        .filter(|s| s.fits(mood) && Some(*s) != avoid)
        .flat_map(|s| if s.universal() { vec![s] } else { vec![s, s] })
        .collect();
    let mut h = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 31;
    let chosen = pool[(h % pool.len() as u64) as usize];
    let note = asked.map(|s| {
        let for_ = if s == Style::Cope { "losses" } else { "wins" };
        format!("{} is for {for_}; used {}", s.name(), chosen.name())
    });
    (chosen, note)
}

/// The card as SVG, plus the slots that didn't fit (should be none).
pub fn svg(card: &Card, look: &Look, style: Style) -> (String, Vec<&'static str>) {
    let design = style.design(card, look);
    let mut out = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1080" height="1080" viewBox="0 0 1080 1080" xml:space="preserve"><defs><clipPath id="round" clipPathUnits="objectBoundingBox"><circle cx=".5" cy=".5" r=".5"/></clipPath><filter id="grain" x="0" y="0" width="1" height="1"><feTurbulence type="fractalNoise" baseFrequency=".85" numOctaves="2" seed="7"/><feColorMatrix type="saturate" values="0"/></filter></defs>"##,
    );
    out.push_str(&design.bg);
    let mut clipped = vec![];
    for item in design.items {
        match item {
            styles::Item::Panel(svg) => out.push_str(&svg),
            styles::Item::Art(name, r, svg) => {
                let _ = write!(
                    out,
                    r#"<g id="art-{}">{}</g>"#,
                    name.replace(' ', "-"),
                    art::place(r, &svg)
                );
            }
            styles::Item::Slot(s) | styles::Item::Label(s) => {
                let id = s.id;
                if !s.draw(&mut out) {
                    clipped.push(id);
                }
            }
        }
    }
    let _ = write!(
        out,
        r#"<rect width="1080" height="1080" filter="url(#grain)" opacity=".1" style="mix-blend-mode:multiply"/></svg>"#
    );
    (out, clipped)
}

pub fn png(card: &Card, look: &Look, style: Style) -> Result<Vec<u8>, String> {
    let (svg, _) = svg(card, look, style);
    let opt = resvg::usvg::Options {
        fontdb: text::database(),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(&svg, &opt).map_err(|e| format!("card svg: {e}"))?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(1080, 1080).ok_or("card pixmap")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|e| format!("card png: {e}"))
}

/// Edge-case trades for tests and the render example.
#[doc(hidden)]
pub mod samples {
    use super::Card;

    fn card(
        symbol: &str,
        open: bool,
        cost: Option<f64>,
        out: f64,
        entry: Option<f64>,
        exit: Option<f64>,
    ) -> Card {
        Card {
            symbol: symbol.into(),
            open,
            cost,
            out,
            proceeds: (!open).then_some(out),
            realized: cost.filter(|_| !open).map(|c| out - c),
            unrealized: None,
            entry,
            exit,
            held_s: Some(4165),
            buys: 1,
            sells: 1,
            fee_sol: Some(0.000023337432484),
            fee_transactions: 4,
            accounting_note: "BEFORE FEES · USDC AT $1".into(),
            ts: 1_792_495_294,
            tx: "5wwb5qnfXZieuPmw".into(),
        }
    }

    /// The studio's edge cases: TADA (real), tiny price, 129×, rug, $1.2M
    /// stock, dust, a 20-char symbol, an open position, unknown cost.
    pub fn all() -> Vec<Card> {
        vec![
            card(
                "TADA",
                false,
                Some(3.0),
                4.667558,
                Some(3.0 / 24_413.514_952),
                Some(4.667558 / 24_413.514_952),
            ),
            card(
                "NANOCAT",
                false,
                Some(12.5),
                65.33,
                Some(0.000_000_004_42),
                Some(0.000_000_023_1),
            ),
            card(
                "HOOD69",
                false,
                Some(50.0),
                6470.0,
                Some(0.000_073_1),
                Some(0.009_459),
            ),
            card(
                "RUGME",
                false,
                Some(40.0),
                0.0,
                Some(0.000_022_1),
                Some(3e-12),
            ),
            card(
                "AAPL",
                false,
                Some(1_240_000.0),
                1_290_840.0,
                Some(241.18),
                Some(251.07),
            ),
            card(
                "DUST",
                false,
                Some(0.5),
                0.504,
                Some(0.0412),
                Some(0.041_53),
            ),
            card(
                "SUPERCALIFRAGILISTIC",
                false,
                Some(820.0),
                554.3,
                Some(0.000_41),
                Some(0.000_277),
            ),
            card(
                "CRADLE",
                true,
                Some(703.94),
                833.34,
                Some(0.000_325_06),
                Some(0.000_384_8),
            ),
            card("GIFT", false, None, 18.2, None, Some(0.0731)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::styles::Item;
    use super::*;

    fn worst_look(hero: Hero) -> Look {
        Look {
            hero,
            show: [
                Show::Usd,
                Show::Pct,
                Show::Mult,
                Show::Inout,
                Show::Prices,
                Show::Held,
                Show::Date,
                Show::Fills,
                Show::Fees,
                Show::Name,
            ]
            .into(),
            name: Some("WWWWWWWWWWWWWWWWWWWW".into()),
        }
    }

    fn overlap(a: [f32; 4], b: [f32; 4]) -> bool {
        a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
    }

    /// Every style × edge-case trade × big number, with every detail on and
    /// a 20-character name: art never touches text, text never touches
    /// text, nothing leaves the card and every slot fits.
    #[test]
    fn no_overlaps_and_everything_fits() {
        let mut problems = vec![];
        for style in Style::ALL {
            for card in samples::all() {
                for hero in [Hero::Pct, Hero::Usd, Hero::Mult] {
                    let look = worst_look(hero);
                    let design = style.design(&card, &look);
                    let (mut arts, mut texts) = (vec![], vec![]);
                    for item in &design.items {
                        match item {
                            Item::Art(name, r, _) => arts.push((*name, *r)),
                            Item::Slot(s) | Item::Label(s) => texts.push((s.id, s.r)),
                            Item::Panel(_) => {}
                        }
                    }
                    for (name, r) in arts.iter().chain(&texts) {
                        if r[0] < 0.0 || r[1] < 0.0 || r[0] + r[2] > 1080.0 || r[1] + r[3] > 1080.0
                        {
                            problems.push(format!("{style:?}: {name} leaves the card"));
                        }
                    }
                    for (a, ar) in &arts {
                        for (t, tr) in &texts {
                            if overlap(*ar, *tr) {
                                problems.push(format!("{style:?}: art {a} overlaps {t}"));
                            }
                        }
                    }
                    for (i, (a, ar)) in texts.iter().enumerate() {
                        for (b, br) in &texts[i + 1..] {
                            if overlap(*ar, *br) {
                                problems.push(format!("{style:?}: {a} overlaps {b}"));
                            }
                        }
                    }
                    let (_, clipped) = svg(&card, &look, style);
                    for id in clipped {
                        problems.push(format!(
                            "{style:?} · {} · {hero:?}: {id} doesn't fit",
                            card.symbol
                        ));
                    }
                }
            }
        }
        problems.dedup();
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    #[test]
    fn styles_never_clash_with_the_result() {
        let cards = samples::all();
        let (win, rug, open, unknown) = (&cards[0], &cards[3], &cards[7], &cards[8]);
        for seed in 0..50 {
            assert_ne!(pick(win, None, None, seed).0, Style::Cope);
            assert!(!matches!(
                pick(rug, None, None, seed).0,
                Style::Bag | Style::Bullseye
            ));
            assert!(pick(unknown, None, None, seed).0.universal());
            assert_ne!(pick(open, None, Some(Style::Bag), seed).0, Style::Bag);
        }
        assert_eq!(
            pick(win, Some(Style::Wanted), None, 1),
            (Style::Wanted, None)
        );
        let (used, note) = pick(rug, Some(Style::Bag), None, 1);
        assert!(used.fits(Mood::Rug));
        assert!(note.unwrap().starts_with("Bag is for wins; used"));
    }

    #[test]
    fn moods() {
        let c = samples::all();
        let moods: Vec<Mood> = c.iter().map(Card::mood).collect();
        use Mood::*;
        assert_eq!(
            moods,
            [Win, BigWin, BigWin, Rug, Win, Win, Loss, OpenUp, Unknown]
        );
    }
}
