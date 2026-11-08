//! The seven card styles. Every style is a fixed map on the 1080 grid:
//! art boxes, value slots and fixed lettering, the same boxes as the card
//! studio mockup. Art and text never share a box (see the test in `mod`).
use super::art;
use super::format::{self as f, clamp};
use super::layout::{Body, Boxed, INK, Line, Piece, Slot, run, slot};
use super::text::Font;
use super::{Card, Hero, Kind, Look, Show};

pub enum Item {
    /// Background shapes text may sit on.
    Panel(String),
    Art(&'static str, [f32; 4], String),
    Slot(Slot),
    /// Fixed template words.
    Label(Slot),
}

pub struct Design {
    pub bg: String,
    pub items: Vec<Item>,
}

const CREAM: &str = "#f4ecd9";
const GREEN: &str = "#23724a";
const RED: &str = "#b4473f";
const COBALT: &str = "#2b4fb4";

fn rect(r: [f32; 4], fill: &str) -> String {
    let [x, y, w, h] = r;
    format!(r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}"/>"#)
}

// ---- the values every style draws ----

/// Which number leads, after falling back past unknowns.
#[derive(Clone, Copy, PartialEq)]
pub enum Lead {
    Pct,
    Usd,
    Mult,
    /// No cost basis: dollars out.
    Out,
}

pub fn lead(c: &Card, look: &Look) -> (Lead, String) {
    let order = [look.hero, Hero::Pct, Hero::Usd, Hero::Mult];
    for h in order {
        match h {
            Hero::Pct if c.pct().is_some() => return (Lead::Pct, f::pct(c.pct().unwrap_or(0.0))),
            Hero::Usd if c.pnl().is_some() => {
                return (Lead::Usd, f::usd(c.pnl().unwrap_or(0.0), true));
            }
            Hero::Mult if c.mult().is_some() => {
                return (Lead::Mult, f::mult(c.mult().unwrap_or(0.0)));
            }
            _ => {}
        }
    }
    (Lead::Out, f::usd(c.out, false))
}

/// The other two of $, % and ×, as chosen.
pub fn sub(c: &Card, look: &Look, lead: Lead) -> String {
    let mut parts = vec![];
    if lead != Lead::Usd
        && look.shows(Show::Usd)
        && let Some(p) = c.pnl()
    {
        parts.push(f::usd(p, true));
    }
    if lead != Lead::Pct
        && look.shows(Show::Pct)
        && let Some(p) = c.pct()
    {
        parts.push(f::pct(p));
    }
    if lead != Lead::Mult
        && look.shows(Show::Mult)
        && let Some(m) = c.mult()
    {
        parts.push(f::mult(m));
    }
    if c.cost.is_none() {
        parts.push("cost unknown".into());
    }
    parts.join(" · ")
}

pub fn stats(c: &Card, look: &Look) -> Vec<(String, String)> {
    let mut out = vec![];
    if look.shows(Show::Inout) {
        if let Some(cost) = c.cost {
            out.push((
                if c.open { "Cost" } else { "In" }.into(),
                f::usd(cost, false),
            ));
        }
        out.push((
            if c.open && c.sells > 0 {
                "Sold + value now"
            } else if c.open {
                "Value now"
            } else {
                "Out"
            }
            .into(),
            f::usd(c.out, false),
        ));
    }
    if look.shows(Show::Prices) {
        if let Some(p) = c.entry {
            out.push(("Entry".into(), f::price(p)));
        }
        if let Some(p) = c.exit {
            out.push((
                if c.open { "Price now" } else { "Exit" }.into(),
                f::price(p),
            ));
        }
    }
    if look.shows(Show::Held)
        && let Some(s) = c.held_s
    {
        out.push(("Held".into(), f::held(s)));
    }
    if look.shows(Show::Fills) {
        let plural = |n: u32, w: &str| format!("{n} {w}{}", if n == 1 { "" } else { "s" });
        out.push((
            "Fills".into(),
            format!("{} · {}", plural(c.buys, "buy"), plural(c.sells, "sell")),
        ));
    }
    if look.shows(Show::Date) {
        out.push(("Date".into(), f::date(c.ts)));
    }
    if look.shows(Show::Fees)
        && let Some(g) = c.fee_sol
    {
        out.push(("Fees paid".into(), format!("{g:.6} SOL")));
    }
    out
}

/// The look minus the multiple, for styles that already show it in a stamp.
fn without_mult(look: &Look) -> Look {
    let mut look = look.clone();
    look.show.remove(&Show::Mult);
    look
}

fn name(look: &Look) -> Option<String> {
    look.name
        .as_deref()
        .map(str::trim)
        .filter(|n| look.shows(Show::Name) && !n.is_empty())
        .map(|n| clamp(n, 20))
}

fn symbol(c: &Card) -> String {
    format!("${}", clamp(&c.symbol, 20))
}

fn hero_color(k: Kind, dark: bool) -> &'static str {
    match (k, dark) {
        (Kind::Open, false) => COBALT,
        (Kind::Open, true) => CREAM,
        (Kind::Loss, false) => RED,
        (Kind::Loss, true) => "#f3a39e",
        (Kind::Unknown, false) => INK,
        (Kind::Unknown, true) => CREAM,
        (Kind::Win, false) => GREEN,
        (Kind::Win, true) => "#8ed1aa",
    }
}

fn tag_text(k: Kind) -> &'static str {
    match k {
        Kind::Win => "Closed · in profit",
        Kind::Loss => "Closed · loss",
        Kind::Open => "Open · PnL so far",
        Kind::Unknown => "Sold · cost unknown",
    }
}

// ---- line builders ----

fn display(text: &str, fill: &'static str) -> Line {
    run(text, Font::Display).ls(-0.03).fill(fill).markup()
}

fn mono(text: &str, fill: &'static str, ls: f32) -> Line {
    run(text, Font::MonoBold).ls(ls).fill(fill).markup()
}

fn symbol_line(c: &Card, fill: &'static str) -> Line {
    vec![Piece::Run(
        run(symbol(c).to_uppercase(), Font::Display)
            .ls(-0.03)
            .fill(fill)
            .ell(),
    )]
}

fn brand(fill: &'static str) -> Line {
    vec![
        Piece::Logo(1.15),
        Piece::Gap(0.32),
        Piece::Run(run("DOTTIE", Font::Display).ls(-0.02).fill(fill)),
    ]
}

fn chip(text: &str, color: &'static str) -> Body {
    Body::Boxed(
        mono(&text.to_uppercase(), color, 0.1),
        Boxed {
            fill: "none",
            stroke: color,
            pad: (0.7, 0.42),
            border: 0.14,
            radius: 14.0,
            shadow: 0.0,
        },
    )
}

fn tag(text: &str, bg: &'static str) -> Body {
    Body::Boxed(
        mono(&text.to_uppercase(), INK, 0.12),
        Boxed {
            fill: bg,
            stroke: INK,
            pad: (0.6, 0.38),
            border: 0.14,
            radius: 8.0,
            shadow: 0.0,
        },
    )
}

fn stamp(line: Line, bg: &'static str, stroke: &'static str, shadow: f32) -> Body {
    Body::Boxed(
        line,
        Boxed {
            fill: bg,
            stroke,
            pad: (0.34, 0.16),
            border: 0.09,
            radius: 16.0,
            shadow,
        },
    )
}

fn stat_body(c: &Card, look: &Look, fill: &'static str, center: bool) -> Body {
    Body::Stats {
        items: stats(c, look)
            .into_iter()
            .map(|(k, v)| (k, mono(&v, fill, 0.0)))
            .collect(),
        fill,
        center,
    }
}

fn line(text: &str, font: Font, fill: &'static str) -> Body {
    Body::Line(run(text, font).fill(fill).markup())
}

/// The ink band: url left, tagline right.
fn footer(url_fill: &'static str, tagline_fill: &'static str) -> [Item; 2] {
    [
        Item::Slot(slot(
            "url",
            [64.0, 996.0, 480.0, 48.0],
            Body::Line(mono("DOTTIE.AOMI.DEV", url_fill, 0.08)),
            30.0,
            16.0,
        )),
        Item::Label(
            slot(
                "tagline",
                [560.0, 996.0, 456.0, 48.0],
                Body::Line(mono("TALK IT. TRADE IT.", tagline_fill, 0.08)),
                30.0,
                16.0,
            )
            .right(),
        ),
    ]
}

// ---- styles ----

pub fn bag(c: &Card, look: &Look) -> Design {
    let (l, hero) = lead(c, look);
    let k = c.kind();
    let tag_bg = match k {
        Kind::Loss => "#f3a39e",
        Kind::Open => "#c9d6f7",
        _ => "#8ed1aa",
    };
    let mut items = vec![
        Item::Panel(rect([0.0, 980.0, 1080.0, 100.0], INK)),
        Item::Art("money", [610.0, 600.0, 440.0, 300.0], art::money().into()),
        Item::Art("coins", [890.0, 400.0, 160.0, 150.0], art::coins()),
        Item::Slot(slot(
            "brand",
            [64.0, 56.0, 520.0, 72.0],
            Body::Line(brand(INK)),
            54.0,
            30.0,
        )),
        Item::Slot(
            slot(
                "chain",
                [700.0, 64.0, 316.0, 56.0],
                chip("Solana", INK),
                22.0,
                12.0,
            )
            .right(),
        ),
        Item::Slot(slot(
            "symbol",
            [64.0, 170.0, 660.0, 140.0],
            Body::Line(symbol_line(c, INK)),
            132.0,
            44.0,
        )),
        Item::Slot(slot(
            "tag",
            [64.0, 322.0, 620.0, 52.0],
            tag(tag_text(k), tag_bg),
            26.0,
            14.0,
        )),
        Item::Slot(slot(
            "hero",
            [64.0, 385.0, 800.0, 205.0],
            Body::Line(display(&hero, hero_color(k, false))),
            220.0,
            56.0,
        )),
        Item::Slot(slot(
            "sub",
            [64.0, 606.0, 520.0, 64.0],
            Body::Line(mono(&sub(c, &without_mult(look), l), INK, 0.0)),
            46.0,
            20.0,
        )),
        Item::Slot(
            slot(
                "stats",
                [64.0, 688.0, 520.0, 200.0],
                stat_body(c, look, INK, false),
                40.0,
                16.0,
            )
            .top(),
        ),
    ];
    if l != Lead::Mult
        && look.shows(Show::Mult)
        && let Some(m) = c.mult()
    {
        items.push(Item::Slot(
            slot(
                "multiple",
                [740.0, 190.0, 280.0, 150.0],
                stamp(display(&f::mult(m), INK), "#e8a93c", INK, 0.1),
                76.0,
                30.0,
            )
            .center()
            .rot(8.0),
        ));
    }
    if let Some(n) = name(look) {
        items.push(Item::Slot(slot(
            "name",
            [64.0, 900.0, 520.0, 52.0],
            Body::Line(vec![Piece::Run(
                run(format!("by {n}"), Font::MonoBold).ell(),
            )]),
            32.0,
            16.0,
        )));
    }
    items.extend(footer(CREAM, "#e8a93c"));
    Design {
        bg: rect([0.0, 0.0, 1080.0, 1080.0], "#f4ecd9"),
        items,
    }
}

pub fn bullseye(c: &Card, look: &Look) -> Design {
    let (l, hero) = lead(c, look);
    let k = c.kind();
    let hero_fill = if k == Kind::Loss { "#f3a39e" } else { CREAM };
    let hero_line: Line = run(&hero, Font::Display)
        .ls(-0.03)
        .fill(hero_fill)
        .shadow(0.045)
        .markup();
    let mut items = vec![
        Item::Panel(rect([0.0, 980.0, 1080.0, 100.0], INK)),
        Item::Art("target", [590.0, 230.0, 450.0, 450.0], art::target()),
        Item::Art("arrows", [640.0, 720.0, 380.0, 180.0], art::arrows()),
        Item::Slot(slot(
            "brand",
            [64.0, 56.0, 520.0, 72.0],
            Body::Line(brand(CREAM)),
            54.0,
            30.0,
        )),
        Item::Slot(
            slot(
                "chain",
                [700.0, 64.0, 316.0, 56.0],
                chip("Solana", CREAM),
                22.0,
                12.0,
            )
            .right(),
        ),
        Item::Slot(slot(
            "symbol",
            [64.0, 170.0, 500.0, 130.0],
            Body::Line(symbol_line(c, CREAM)),
            124.0,
            40.0,
        )),
        Item::Slot(slot(
            "tag",
            [64.0, 312.0, 500.0, 50.0],
            tag(
                if k == Kind::Open {
                    "On target · open"
                } else {
                    tag_text(k)
                },
                CREAM,
            ),
            26.0,
            13.0,
        )),
        Item::Slot(slot(
            "hero",
            [64.0, 380.0, 510.0, 200.0],
            Body::Line(hero_line),
            196.0,
            48.0,
        )),
        Item::Slot(slot(
            "sub",
            [64.0, 595.0, 500.0, 60.0],
            Body::Line(mono(&sub(c, look, l), CREAM, 0.0)),
            44.0,
            18.0,
        )),
        Item::Slot(
            slot(
                "stats",
                [64.0, 670.0, 500.0, 230.0],
                stat_body(c, look, CREAM, false),
                38.0,
                15.0,
            )
            .top(),
        ),
    ];
    if let Some(n) = name(look) {
        items.push(Item::Slot(slot(
            "name",
            [64.0, 912.0, 500.0, 50.0],
            Body::Line(vec![Piece::Run(
                run(format!("by {n}"), Font::MonoBold).fill(CREAM).ell(),
            )]),
            30.0,
            14.0,
        )));
    }
    items.extend(footer(CREAM, "#8ed1aa"));
    Design {
        bg: rect([0.0, 0.0, 1080.0, 1080.0], "#2f8b57"),
        items,
    }
}

pub fn wanted(c: &Card, look: &Look) -> Design {
    let (l, hero) = lead(c, look);
    let k = c.kind();
    let tagline = match k {
        Kind::Win => "Dead or alive · for robbing the market",
        Kind::Loss => "For donating to the market",
        Kind::Open => "Still at large · position open",
        Kind::Unknown => "No questions asked",
    };
    let mut details: Vec<Line> = vec![];
    let s = sub(c, look, l);
    if !s.is_empty() {
        details.push(mono(&s, INK, 0.0));
    }
    for (a, b) in stats(c, look) {
        details.push(mono(&format!("{a} {b}"), INK, 0.0));
    }
    if let Some(n) = name(look) {
        details.push(mono(&format!("last seen: {n}"), INK, 0.0));
    }
    let nails = [[56.0, 56.0], [984.0, 56.0], [56.0, 984.0], [984.0, 984.0]];
    let mut items: Vec<Item> = vec![Item::Panel(format!(
        r##"<rect x="43" y="43" width="994" height="994" fill="#ebe0c7" stroke="{INK}" stroke-width="6"/><rect x="59" y="59" width="962" height="962" fill="none" stroke="{INK}" stroke-width="4"/>"##
    ))];
    items.extend(
        nails
            .iter()
            .map(|[x, y]| Item::Art("nail", [*x, *y, 40.0, 40.0], art::nail())),
    );
    items.extend([
        Item::Label(
            slot(
                "WANTED",
                [110.0, 78.0, 860.0, 170.0],
                Body::Line(vec![Piece::Run(run("WANTED", Font::Display).ls(0.02))]),
                200.0,
                80.0,
            )
            .center(),
        ),
        Item::Slot(
            slot(
                "tag",
                [110.0, 252.0, 860.0, 50.0],
                Body::Line(mono(&tagline.to_uppercase(), INK, 0.12)),
                30.0,
                14.0,
            )
            .center(),
        ),
        Item::Art(
            "sword cat",
            [330.0, 312.0, 420.0, 316.0],
            art::framed(art::sword_cat(), 420.0, 316.0, 8.0),
        ),
        Item::Slot(
            slot(
                "symbol",
                [110.0, 640.0, 860.0, 90.0],
                Body::Line(symbol_line(c, INK)),
                92.0,
                36.0,
            )
            .center(),
        ),
        Item::Label(
            slot(
                "REWARD",
                [110.0, 736.0, 860.0, 34.0],
                Body::Line(mono(
                    if k == Kind::Loss { "DAMAGES" } else { "REWARD" },
                    INK,
                    0.3,
                )),
                28.0,
                16.0,
            )
            .center(),
        ),
        Item::Slot(
            slot(
                "hero",
                [110.0, 772.0, 860.0, 110.0],
                Body::Line(display(&hero, hero_color(k, false))),
                116.0,
                40.0,
            )
            .center(),
        ),
        Item::Slot(
            slot(
                "details",
                [110.0, 888.0, 860.0, 92.0],
                Body::Wrap(details),
                28.0,
                12.0,
            )
            .center(),
        ),
        Item::Slot(
            slot(
                "url",
                [110.0, 986.0, 860.0, 30.0],
                Body::Line(mono("DOTTIE.AOMI.DEV · SOLANA", INK, 0.14)),
                20.0,
                10.0,
            )
            .center(),
        ),
    ]);
    Design {
        bg: rect([0.0, 0.0, 1080.0, 1080.0], "#3b2a1e"),
        items,
    }
}

pub fn receipt(c: &Card, look: &Look) -> Design {
    let (l, hero) = lead(c, look);
    let k = c.kind();
    let mut rows: Vec<(String, Line)> = vec![];
    let mut row = |label: &str, value: String| rows.push((label.into(), mono(&value, INK, 0.0)));
    if look.shows(Show::Inout) {
        row(
            if c.open { "COST" } else { "BOUGHT" },
            c.cost.map_or("UNKNOWN".into(), |v| f::usd(v, false)),
        );
        row(
            if c.open && c.sells > 0 {
                "SOLD + VALUE NOW"
            } else if c.open {
                "VALUE NOW"
            } else {
                "SOLD"
            },
            f::usd(c.out, false),
        );
    }
    if look.shows(Show::Prices) {
        if let Some(p) = c.entry {
            row("ENTRY", f::price(p));
        }
        if let Some(p) = c.exit {
            row(if c.open { "PRICE NOW" } else { "EXIT" }, f::price(p));
        }
    }
    if look.shows(Show::Held)
        && let Some(s) = c.held_s
    {
        row("HELD", f::held(s));
    }
    if look.shows(Show::Fills) {
        row("FILLS", format!("{}B / {}S", c.buys, c.sells));
    }
    if look.shows(Show::Fees)
        && let Some(g) = c.fee_sol
    {
        row("FEES PAID", format!("{g:.6} SOL"));
    }
    let stamp_text = match k {
        Kind::Win => "PAID",
        Kind::Loss => "PAID · OUCH",
        Kind::Open => "OPEN TAB",
        Kind::Unknown => "SOLD",
    };
    let stamp_color = if k == Kind::Loss { RED } else { COBALT };
    let mut hero_line = mono(if l == Lead::Out { "TOTAL" } else { "PNL" }, INK, 0.0);
    if let Some(Piece::Run(r)) = hero_line.first_mut() {
        r.scale = 0.32;
    }
    hero_line.push(Piece::Gap(0.3));
    hero_line.extend(display(&hero, hero_color(k, false)));
    let meta = if look.shows(Show::Date) {
        format!("{} · {}", f::date(c.ts).to_uppercase(), f::time(c.ts))
    } else {
        String::new()
    };
    let mut items = vec![
        Item::Art("coins", [0.0, 0.0, 170.0, 1044.0], art::coin_strip(false)),
        Item::Art("coins", [910.0, 0.0, 170.0, 1044.0], art::coin_strip(true)),
        Item::Panel(format!(
            r##"<rect x="180" y="60" width="720" height="960" fill="#fffaf0"/><path d="M182.5 60 V1020 M897.5 60 V1020" stroke="{INK}" stroke-width="5"/>"##
        )),
        Item::Art("torn edge", [180.0, 36.0, 720.0, 24.0], art::zigzag(false)),
        Item::Art("torn edge", [180.0, 1020.0, 720.0, 24.0], art::zigzag(true)),
        Item::Slot(
            slot(
                "brand",
                [230.0, 76.0, 620.0, 72.0],
                Body::Line(brand(INK)),
                60.0,
                30.0,
            )
            .center(),
        ),
        Item::Label(
            slot(
                "title",
                [230.0, 152.0, 620.0, 36.0],
                Body::Line(mono("TRADE RECEIPT · SOLANA", INK, 0.14)),
                24.0,
                12.0,
            )
            .center(),
        ),
        Item::Slot(
            slot(
                "symbol",
                [230.0, 272.0, 620.0, 58.0],
                Body::Line(symbol_line(c, INK)),
                52.0,
                24.0,
            )
            .center(),
        ),
        Item::Art("rule", [230.0, 336.0, 620.0, 6.0], art::rule()),
        Item::Slot(
            slot(
                "items",
                [230.0, 352.0, 620.0, 250.0],
                Body::Rows(rows),
                34.0,
                13.0,
            )
            .top(),
        ),
        Item::Art("rule", [230.0, 612.0, 620.0, 6.0], art::rule()),
        Item::Slot(
            slot(
                "hero",
                [230.0, 628.0, 620.0, 124.0],
                Body::Line(hero_line),
                116.0,
                40.0,
            )
            .right(),
        ),
        Item::Slot(
            slot(
                "sub",
                [230.0, 756.0, 620.0, 50.0],
                Body::Line(mono(&sub(c, look, l), INK, 0.0)),
                36.0,
                14.0,
            )
            .right(),
        ),
        Item::Slot(
            slot(
                "stamp",
                [236.0, 818.0, 300.0, 130.0],
                stamp(display(stamp_text, stamp_color), "none", stamp_color, 0.0),
                64.0,
                22.0,
            )
            .center()
            .rot(-7.0),
        ),
        Item::Art("barcode", [560.0, 828.0, 290.0, 100.0], art::barcode(&c.tx)),
        Item::Label(
            slot(
                "thanks",
                [230.0, 954.0, 620.0, 40.0],
                Body::Line(mono("THANK YOU · DOTTIE.AOMI.DEV", INK, 0.12)),
                22.0,
                11.0,
            )
            .center(),
        ),
    ];
    if !meta.is_empty() {
        items.push(Item::Slot(
            slot(
                "meta",
                [230.0, 192.0, 620.0, 34.0],
                line(&meta, Font::Mono, INK),
                22.0,
                11.0,
            )
            .center(),
        ));
    }
    if let Some(n) = name(look) {
        items.push(Item::Slot(
            slot(
                "name",
                [230.0, 228.0, 620.0, 34.0],
                Body::Line(vec![Piece::Run(
                    run(format!("CUSTOMER: {}", n.to_uppercase()), Font::Mono).ell(),
                )]),
                22.0,
                11.0,
            )
            .center(),
        ));
    }
    Design {
        bg: rect([0.0, 0.0, 1080.0, 1080.0], "#2f8b57"),
        items,
    }
}

pub fn certificate(c: &Card, look: &Look) -> Design {
    let (l, hero) = lead(c, look);
    let k = c.kind();
    let title = match k {
        Kind::Win => "Certificate of gains",
        Kind::Loss => "Certificate of participation",
        Kind::Open if l == Lead::Out => "Certificate of position",
        Kind::Open => "Certificate of paper gains",
        Kind::Unknown => "Certificate of sale",
    };
    let verb = match k {
        Kind::Win | Kind::Loss => "closed on Solana with a return of",
        Kind::Open if l == Lead::Out => "holds on Solana, with estimated value of",
        Kind::Open => "holds on Solana, with PnL so far of",
        Kind::Unknown => "was sold on Solana for",
    };
    let signer = match name(look) {
        Some(n) => vec![Piece::Run(run(n, Font::SemiItalic).ell())],
        None => vec![Piece::Run(run("the bearer", Font::Italic).opacity(0.55))],
    };
    let date = if look.shows(Show::Date) {
        format!("{} · DOTTIE.AOMI.DEV", f::date(c.ts).to_uppercase())
    } else {
        "DOTTIE.AOMI.DEV".into()
    };
    let items = vec![
        Item::Art(
            "border",
            [0.0, 0.0, 1080.0, 84.0],
            art::guilloche(1080.0, 84.0, false),
        ),
        Item::Art(
            "border",
            [0.0, 996.0, 1080.0, 48.0],
            art::guilloche(1080.0, 48.0, false),
        ),
        Item::Art(
            "border",
            [0.0, 84.0, 84.0, 912.0],
            art::guilloche(84.0, 912.0, true),
        ),
        Item::Art(
            "border",
            [996.0, 84.0, 84.0, 912.0],
            art::guilloche(84.0, 912.0, true),
        ),
        Item::Panel(format!(
            r#"<rect x="106" y="106" width="868" height="868" fill="none" stroke="{INK}" stroke-width="4"/><rect x="119" y="119" width="842" height="842" fill="none" stroke="{INK}" stroke-width="2"/>"#
        )),
        Item::Label(
            slot(
                "title",
                [130.0, 132.0, 820.0, 84.0],
                Body::Line(display(&title.to_uppercase(), INK)),
                64.0,
                30.0,
            )
            .center(),
        ),
        Item::Label(
            slot(
                "bearer",
                [130.0, 222.0, 820.0, 44.0],
                line("This certifies that the bearer of", Font::Italic, INK),
                34.0,
                18.0,
            )
            .center(),
        ),
        Item::Slot(
            slot(
                "symbol",
                [130.0, 268.0, 820.0, 118.0],
                Body::Line(symbol_line(c, "#1f3d94")),
                116.0,
                40.0,
            )
            .center(),
        ),
        Item::Slot(
            slot(
                "tag",
                [130.0, 392.0, 820.0, 44.0],
                line(verb, Font::Italic, INK),
                32.0,
                16.0,
            )
            .center(),
        ),
        Item::Slot(
            slot(
                "hero",
                [130.0, 440.0, 820.0, 166.0],
                Body::Line(display(&hero, hero_color(k, false))),
                170.0,
                50.0,
            )
            .center(),
        ),
        Item::Slot(
            slot(
                "sub",
                [130.0, 612.0, 820.0, 54.0],
                Body::Line(mono(&sub(c, look, l), INK, 0.0)),
                38.0,
                16.0,
            )
            .center(),
        ),
        Item::Slot(
            slot(
                "stats",
                [130.0, 680.0, 820.0, 88.0],
                stat_body(c, look, INK, true),
                32.0,
                13.0,
            )
            .center()
            .top(),
        ),
        Item::Art("seal", [150.0, 772.0, 176.0, 176.0], art::seal()),
        Item::Slot(
            slot(
                "name",
                [380.0, 812.0, 560.0, 70.0],
                Body::Line(signer),
                52.0,
                20.0,
            )
            .center()
            .bottom(),
        ),
        Item::Art("signature line", [380.0, 888.0, 560.0, 4.0], art::bar()),
        Item::Slot(
            slot(
                "url",
                [380.0, 898.0, 560.0, 40.0],
                Body::Line(mono(&date, INK, 0.12)),
                22.0,
                11.0,
            )
            .center(),
        ),
    ];
    Design {
        bg: rect([0.0, 0.0, 1080.0, 1080.0], "#fbf3df"),
        items,
    }
}

pub fn tcg(c: &Card, look: &Look) -> Design {
    let (l, hero) = lead(c, look);
    let k = c.kind();
    let m = c.mult();
    let rarity = match (k, m) {
        (Kind::Unknown, _) | (_, None) => "Mystery",
        (_, Some(m)) if m < 1.0 => "Cursed",
        (_, Some(m)) if m < 1.2 => "Common",
        (_, Some(m)) if m < 2.0 => "Rare",
        (_, Some(m)) if m < 5.0 => "Epic",
        _ => "Legendary",
    };
    let rarity_bg = match rarity {
        "Legendary" => "#e8a93c",
        "Epic" => "#c9b6f2",
        "Rare" => "#8ed1aa",
        "Cursed" => "#f3a39e",
        _ => "#ebe0c7",
    };
    let mv = match k {
        Kind::Win => "Take profit",
        Kind::Loss => "Paper hands",
        Kind::Open => "Diamond hold",
        Kind::Unknown => "Mystery sell",
    };
    let mut hero_line = vec![
        Piece::Run(run(mv, Font::BodyBold).scale(0.36)),
        Piece::Gap(0.35),
    ];
    hero_line.extend(display(&hero, hero_color(k, false)));
    let mut stat_items: Vec<(String, Line)> = vec![];
    let s = sub(c, &without_mult(look), l);
    if !s.is_empty() {
        stat_items.push(("Also".into(), mono(&s, INK, 0.0)));
    }
    stat_items.extend(
        stats(c, look)
            .into_iter()
            .map(|(a, b)| (a, mono(&b, INK, 0.0))),
    );
    let footer_text = match name(look) {
        Some(n) => format!("DOTTIE.AOMI.DEV · TRAINER {}", n.to_uppercase()),
        None => "DOTTIE.AOMI.DEV".into(),
    };
    let mut items = vec![
        Item::Panel(format!(
            r##"<rect x="44" y="44" width="992" height="992" rx="36" fill="#f4ecd9" stroke="{INK}" stroke-width="8"/>"##
        )),
        Item::Slot(slot(
            "symbol",
            [80.0, 70.0, 640.0, 100.0],
            Body::Line(symbol_line(c, INK)),
            88.0,
            34.0,
        )),
        Item::Art("portrait", [80.0, 180.0, 920.0, 420.0], art::portrait()),
        Item::Slot(slot(
            "tag",
            [80.0, 612.0, 620.0, 48.0],
            Body::Line(mono(
                &format!("{} · Solana", tag_text(k)).to_uppercase(),
                INK,
                0.08,
            )),
            24.0,
            11.0,
        )),
        Item::Slot(
            slot(
                "rarity",
                [710.0, 612.0, 290.0, 48.0],
                tag(rarity, rarity_bg),
                24.0,
                12.0,
            )
            .right(),
        ),
        Item::Art("sword", [80.0, 694.0, 90.0, 90.0], art::sword()),
        Item::Slot(slot(
            "hero",
            [184.0, 670.0, 816.0, 114.0],
            Body::Line(hero_line),
            110.0,
            36.0,
        )),
        Item::Slot(
            slot(
                "stats",
                [80.0, 796.0, 920.0, 150.0],
                Body::Stats {
                    items: stat_items,
                    fill: INK,
                    center: false,
                },
                34.0,
                13.0,
            )
            .top(),
        ),
        Item::Slot(slot(
            "footer",
            [80.0, 956.0, 920.0, 50.0],
            Body::Line(vec![Piece::Run(
                run(footer_text, Font::MonoBold).ls(0.06).ell(),
            )]),
            24.0,
            11.0,
        )),
    ];
    if let Some(m) = m.filter(|_| look.shows(Show::Mult) || l == Lead::Mult) {
        let mut pwr = mono("PWR", INK, 0.0);
        if let Some(Piece::Run(r)) = pwr.first_mut() {
            r.scale = 0.38;
        }
        pwr.push(Piece::Gap(0.15));
        pwr.extend(display(&f::mult(m), RED));
        items.push(Item::Slot(
            slot(
                "multiple",
                [730.0, 70.0, 270.0, 100.0],
                Body::Line(pwr),
                76.0,
                28.0,
            )
            .right(),
        ));
    }
    let bg = r##"<defs><linearGradient id="foil" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#e8a93c"/><stop offset=".4" stop-color="#f3a39e"/><stop offset=".7" stop-color="#8ed1aa"/><stop offset="1" stop-color="#2b4fb4"/></linearGradient></defs><rect width="1080" height="1080" fill="url(#foil)"/>"##.to_string();
    Design { bg, items }
}

pub fn cope(c: &Card, look: &Look) -> Design {
    let (l, hero) = lead(c, look);
    let k = c.kind();
    let quote = match k {
        Kind::Win => "“Should've held longer.”",
        Kind::Loss => "“Lesson paid. Next one.”",
        Kind::Open => "“Still holding. Pray.”",
        Kind::Unknown => "“No idea what I paid.”",
    };
    let mut items = vec![
        Item::Panel(rect([0.0, 985.0, 1080.0, 95.0], INK)),
        Item::Art("cloud", [600.0, 50.0, 420.0, 240.0], art::cloud()),
        Item::Art("rain", [620.0, 296.0, 380.0, 170.0], art::rain()),
        Item::Art("wet cat", [630.0, 480.0, 380.0, 380.0], art::wet_cat()),
        Item::Art("puddle", [600.0, 880.0, 440.0, 60.0], art::puddle()),
        Item::Slot(slot(
            "brand",
            [64.0, 56.0, 500.0, 72.0],
            Body::Line(brand(INK)),
            54.0,
            30.0,
        )),
        Item::Slot(slot(
            "symbol",
            [64.0, 170.0, 520.0, 130.0],
            Body::Line(symbol_line(c, INK)),
            124.0,
            40.0,
        )),
        Item::Slot(slot(
            "tag",
            [64.0, 312.0, 520.0, 50.0],
            tag(tag_text(k), "#f3a39e"),
            26.0,
            13.0,
        )),
        Item::Slot(slot(
            "hero",
            [64.0, 380.0, 520.0, 200.0],
            Body::Line(display(&hero, hero_color(k, false))),
            200.0,
            48.0,
        )),
        Item::Slot(slot(
            "sub",
            [64.0, 595.0, 500.0, 60.0],
            Body::Line(mono(&sub(c, look, l), INK, 0.0)),
            44.0,
            18.0,
        )),
        Item::Slot(
            slot(
                "stats",
                [64.0, 670.0, 500.0, 216.0],
                stat_body(c, look, INK, false),
                38.0,
                15.0,
            )
            .top(),
        ),
        Item::Slot(slot(
            "url",
            [64.0, 1000.0, 420.0, 44.0],
            Body::Line(mono("DOTTIE.AOMI.DEV", CREAM, 0.08)),
            30.0,
            14.0,
        )),
        Item::Label(
            slot(
                "quote",
                [500.0, 1000.0, 516.0, 44.0],
                line(quote, Font::SemiItalic, "#f3a39e"),
                32.0,
                14.0,
            )
            .right(),
        ),
    ];
    if let Some(n) = name(look) {
        items.push(Item::Slot(slot(
            "name",
            [64.0, 900.0, 500.0, 56.0],
            Body::Line(vec![Piece::Run(
                run(format!("by {n}"), Font::MonoBold).ell(),
            )]),
            30.0,
            14.0,
        )));
    }
    Design {
        bg: rect([0.0, 0.0, 1080.0, 1080.0], "#f6ddd3"),
        items,
    }
}
