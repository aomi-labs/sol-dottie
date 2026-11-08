//! Card artwork: the brand's linocut cats and SVG props in the brand's
//! flat-fill, ink-stroke style. Each piece is a self-contained `<svg>` that
//! [`place`] drops into an art box, which clips it.
use super::layout::INK;
use std::sync::OnceLock;

fn data_uri(mime: &str, bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = format!("data:{mime};base64,");
    for c in bytes.chunks(3) {
        let n = u32::from(c[0]) << 16
            | u32::from(*c.get(1).unwrap_or(&0)) << 8
            | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            out.push(if i <= c.len() {
                T[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

macro_rules! image {
    ($name:ident, $file:literal) => {
        pub fn $name() -> &'static str {
            static URI: OnceLock<String> = OnceLock::new();
            URI.get_or_init(|| {
                data_uri(
                    "image/jpeg",
                    include_bytes!(concat!("../../assets/card/art/", $file)),
                )
            })
        }
    };
}
image!(logo, "logo.jpg");
image!(hero_cat, "hero.jpg");
image!(sword_cat, "sword.jpg");
image!(sad_cat, "hero_sad.jpg");

/// Puts a self-contained `<svg>` into the box `r`.
pub fn place(r: [f32; 4], svg: &str) -> String {
    let [x, y, w, h] = r;
    svg.replacen(
        "<svg",
        &format!(r#"<svg x="{x}" y="{y}" width="{w}" height="{h}""#),
        1,
    )
}

pub fn money() -> &'static str {
    include_str!("../../assets/card/art/money.svg")
}

pub fn coins() -> String {
    let stack: String = (0..4)
        .map(|i| {
            format!(
                r##"<ellipse cx="80" cy="{}" rx="54" ry="18" fill="#e8a93c"/>"##,
                118 - i * 22
            )
        })
        .collect();
    format!(
        r##"<svg viewBox="0 0 160 150"><g stroke="{INK}" stroke-width="5">{stack}<ellipse cx="80" cy="52" rx="54" ry="18" fill="#f2c063"/><path d="M68 46 h24 M80 40 v24"/></g></svg>"##
    )
}

/// An arrow along +x: tip at 0, tail at `len`. The metal head is left off
/// when the tip is buried in a target.
fn arrow(len: f32, fletch: &str, head: bool) -> String {
    let l = len;
    let start = if head { 20.0 } else { -6.0 };
    let tip = if head {
        r##"<path d="M0 0 L28 -14 L21 0 L28 14 Z" fill="#cfd3dc" stroke-width="5"/><path d="M6 0 L24 -8" stroke="#fff" stroke-width="3" opacity=".8"/>"##
    } else {
        ""
    };
    format!(
        r##"<g stroke="{INK}" stroke-linejoin="round" stroke-linecap="round" fill="none">
<path d="M{start} 0 L{l} 0" stroke-width="11"/><path d="M{start} 0 L{l} 0" stroke="#8a5a2b" stroke-width="5"/>
<path d="M{a} -4 L{b} -23 L{c} -23 L{d} -4 Z" fill="{fletch}" stroke-width="5"/>
<path d="M{a} 4 L{b} 23 L{c} 23 L{d} 4 Z" fill="{fletch}" stroke-width="5"/>
<path d="M{e} -10 L{f} -10 M{e} 10 L{f} 10" stroke-width="3" opacity=".55"/>
<path d="M{c} -7 L{g} -7 L{g} 7 L{c} 7" fill="{INK}" stroke-width="3"/>{tip}</g>"##,
        a = l - 74.0,
        b = l - 50.0,
        c = l + 2.0,
        d = l - 18.0,
        e = l - 64.0,
        f = l - 40.0,
        g = l + 10.0,
    )
}

pub fn target() -> String {
    format!(
        r##"<svg viewBox="0 0 440 440"><g stroke="{INK}" stroke-width="7">
<circle cx="210" cy="230" r="196" fill="#f4ecd9"/><circle cx="210" cy="230" r="152" fill="#f3a39e"/><circle cx="210" cy="230" r="108" fill="#f4ecd9"/><circle cx="210" cy="230" r="66" fill="#2b4fb4"/><circle cx="210" cy="230" r="28" fill="#e8a93c"/></g>
<g transform="translate(210 230) rotate(-45)">{}</g></svg>"##,
        arrow(250.0, "#f3a39e", false)
    )
}

pub fn arrows() -> String {
    let three: String = [
        (70, -9, "#f3a39e"),
        (180, 0, "#8ed1aa"),
        (290, 9, "#e8a93c"),
    ]
    .iter()
    .map(|(x, t, c)| {
        format!(
            r#"<g transform="translate({x} 12) rotate({})">{}</g>"#,
            90 + t,
            arrow(156.0, c, true)
        )
    })
    .collect();
    format!(r#"<svg viewBox="0 0 360 180">{three}</svg>"#)
}

pub fn nail() -> String {
    format!(
        r##"<svg viewBox="0 0 40 40"><circle cx="20" cy="20" r="12" fill="#9aa0ad" stroke="{INK}" stroke-width="4"/><circle cx="16" cy="16" r="3" fill="#f4ecd9"/></svg>"##
    )
}

/// A photo of a cat in a `w`×`h` ink frame on green.
pub fn framed(src: &str, w: f32, h: f32, border: f32) -> String {
    format!(
        r##"<svg viewBox="0 0 {w} {h}"><rect width="{w}" height="{h}" fill="#2f8b57"/><image width="{w}" height="{h}" href="{src}" preserveAspectRatio="xMidYMid slice"/><rect x="{o}" y="{o}" width="{iw}" height="{ih}" fill="none" stroke="{INK}" stroke-width="{border}"/></svg>"##,
        o = border / 2.0,
        iw = w - border,
        ih = h - border
    )
}

/// The receipt's torn top or bottom edge.
pub fn zigzag(down: bool) -> String {
    let (base, peak) = if down { (0, 24) } else { (24, 0) };
    let mut d = format!("M0 {base}");
    for x in (0..=720).step_by(30) {
        d += &format!(" L{x} {base} L{} {peak}", x + 15);
    }
    format!(
        r##"<svg viewBox="0 0 720 24" preserveAspectRatio="none"><path d="{d} L720 {base} Z" fill="#fffaf0"/></svg>"##
    )
}

pub fn barcode(seed: &str) -> String {
    let mut h: u32 = 0;
    for c in seed.chars() {
        h = h.wrapping_mul(31).wrapping_add(c as u32);
    }
    let (mut x, mut bars) = (0u32, String::new());
    while x < 286 {
        h = h.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let w = 2 + h % 7;
        if !(h >> 8).is_multiple_of(3) {
            bars += &format!(r#"<rect x="{x}" width="{w}" height="70" fill="{INK}"/>"#);
        }
        x += w + 2 + (h >> 4) % 4;
    }
    format!(
        r#"<svg viewBox="0 0 290 90" preserveAspectRatio="none">{bars}<text x="145" y="88" font-family="Space Mono" font-size="14" text-anchor="middle" fill="{INK}">{}</text></svg>"#,
        super::text::escape(seed)
    )
}

pub fn coin_strip(flip: bool) -> String {
    let coins: String = (0..9)
        .map(|i| {
            let x = if flip { 40 + (i % 2) * 60 } else { 70 - (i % 2) * 40 };
            let y = 70 + i * 118;
            format!(
                r##"<g transform="rotate({} {x} {y})"><circle cx="{x}" cy="{y}" r="38" fill="#e8a93c"/><circle cx="{x}" cy="{y}" r="24" fill="none" stroke="#d18f24" stroke-width="4"/><path d="M{} {y} h18 M{x} {} v22"/></g>"##,
                (i * 37) % 50 - 25,
                x - 9,
                y - 11
            )
        })
        .collect();
    format!(
        r#"<svg viewBox="0 0 170 1080" preserveAspectRatio="xMidYMid slice"><g stroke="{INK}" stroke-width="5">{coins}</g></svg>"#
    )
}

/// Certificate border: interlaced waves on cobalt.
pub fn guilloche(w: f32, h: f32, vertical: bool) -> String {
    let (len, amp) = if vertical { (h, w / 2.0) } else { (w, h / 2.0) };
    let (mut d1, mut d2) = (String::new(), String::new());
    let mut i = 0.0;
    while i <= len {
        let y1 = amp + (i / 18.0).sin() * amp * 0.55;
        let y2 = amp + (i / 14.0).cos() * amp * 0.45;
        let cmd = if i == 0.0 { "M" } else { "L" };
        if vertical {
            d1 += &format!("{cmd}{y1:.1} {i}");
            d2 += &format!("{cmd}{y2:.1} {i}");
        } else {
            d1 += &format!("{cmd}{i} {y1:.1}");
            d2 += &format!("{cmd}{i} {y2:.1}");
        }
        i += 6.0;
    }
    let shift = if vertical {
        "translate(0 30)"
    } else {
        "translate(30 0)"
    };
    format!(
        r##"<svg viewBox="0 0 {w} {h}" preserveAspectRatio="none"><rect width="{w}" height="{h}" fill="#2b4fb4"/><path d="{d1}" fill="none" stroke="#f4ecd9" stroke-width="3" opacity=".9"/><path d="{d2}" fill="none" stroke="#e8a93c" stroke-width="3"/><path d="{d1}" transform="{shift}" fill="none" stroke="#8ed1aa" stroke-width="2" opacity=".8"/></svg>"##
    )
}

pub fn seal() -> String {
    let mut star = String::new();
    for i in 0..=24 {
        let a = i as f32 / 24.0 * std::f32::consts::TAU;
        let r = if i % 2 == 1 { 92.0 } else { 104.0 };
        star += &format!(
            "{}{:.1} {:.1}",
            if i == 0 { "M" } else { "L" },
            110.0 + a.cos() * r,
            100.0 + a.sin() * r
        );
    }
    format!(
        r##"<svg viewBox="0 0 220 220"><defs><clipPath id="sealclip"><circle cx="110" cy="100" r="74"/></clipPath></defs><g stroke="{INK}" stroke-width="5" stroke-linejoin="round"><path d="M72 150 L50 208 L68 200 L80 214 L102 160 Z" fill="#b4473f"/><path d="M148 150 L170 208 L152 200 L140 214 L118 160 Z" fill="#b4473f"/><path d="{star}Z" fill="#e8a93c"/></g><image href="{}" x="36" y="26" width="148" height="148" clip-path="url(#sealclip)" preserveAspectRatio="xMidYMid slice"/><circle cx="110" cy="100" r="74" fill="none" stroke="{INK}" stroke-width="6"/></svg>"##,
        hero_cat()
    )
}

/// A straight sword drawn along one axis, then turned 45°.
pub fn sword() -> String {
    format!(
        r##"<svg viewBox="0 0 100 100"><g transform="translate(17 83) rotate(-45)" stroke="{INK}" stroke-linejoin="round" stroke-linecap="round">
<path d="M28 -7 L98 -7 L112 0 L98 7 L28 7 Z" fill="#dfe3ea" stroke-width="5"/><path d="M32 -2 L100 -2" stroke="#fff" stroke-width="3" opacity=".9"/>
<path d="M0 0 L22 0" stroke-width="12"/><path d="M0 0 L22 0" stroke="#2b4fb4" stroke-width="6"/>
<rect x="21" y="-15" width="8" height="30" rx="3" fill="#e8a93c" stroke-width="4"/>
<circle cx="-3" cy="0" r="5" fill="#e8a93c" stroke-width="4"/></g></svg>"##
    )
}

/// The trading card window: sunburst, framed hooded cat, holo sheen.
pub fn portrait() -> String {
    let mut rays = String::new();
    for i in 0..20 {
        let a0 = (i as f32 * 18.0).to_radians();
        let a1 = (i as f32 * 18.0 + 9.0).to_radians();
        let (cx, cy, r) = (460.0, 210.0, 900.0);
        rays += &format!(
            "M{cx} {cy} L{:.0} {:.0} L{:.0} {:.0} Z ",
            cx + a0.cos() * r,
            cy + a0.sin() * r,
            cx + a1.cos() * r,
            cy + a1.sin() * r
        );
    }
    format!(
        r##"<svg viewBox="0 0 920 420"><defs><linearGradient id="holo" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="0"/><stop offset=".45" stop-color="#8ed1aa" stop-opacity=".28"/><stop offset=".55" stop-color="#f3a39e" stop-opacity=".3"/><stop offset=".65" stop-color="#2b4fb4" stop-opacity=".18"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></linearGradient></defs>
<rect width="920" height="420" fill="#e8a93c"/><path d="{rays}" fill="#f3a39e"/>
<rect x="274" y="24" width="392" height="392" fill="{INK}"/>
<rect x="264" y="14" width="392" height="392" fill="#2f8b57"/><image x="264" y="14" width="392" height="392" href="{}" preserveAspectRatio="xMidYMid slice"/><rect x="267.5" y="17.5" width="385" height="385" fill="none" stroke="{INK}" stroke-width="7"/>
<rect width="920" height="420" fill="url(#holo)"/><rect x="3.5" y="3.5" width="913" height="413" fill="none" stroke="{INK}" stroke-width="7"/></svg>"##,
        hero_cat()
    )
}

pub fn cloud() -> String {
    format!(
        r##"<svg viewBox="0 0 420 240"><path d="M70 200 Q20 200 24 156 Q28 112 78 116 Q84 52 150 54 Q186 10 246 38 Q300 18 330 74 Q392 74 396 134 Q400 200 340 200 Z" fill="#d8dbe4" stroke="{INK}" stroke-width="7"/><path d="M96 168 Q150 150 196 168" stroke="#a9aebb" stroke-width="6" fill="none"/></svg>"##
    )
}

pub fn rain() -> String {
    let drops: String = [
        (40, 20),
        (120, 60),
        (200, 14),
        (280, 70),
        (340, 24),
        (80, 110),
        (240, 120),
        (320, 130),
        (160, 124),
    ]
    .iter()
    .map(|(x, y)| {
        format!(
            "<path d=\"M{x} {y} Q{} {} {x} {} Q{} {} {x} {y}Z\"/>",
            x + 12,
            y + 24,
            y + 30,
            x - 12,
            y + 24
        )
    })
    .collect();
    format!(
        r##"<svg viewBox="0 0 380 170"><g fill="#2b4fb4" stroke="{INK}" stroke-width="4">{drops}</g></svg>"##
    )
}

pub fn puddle() -> String {
    format!(
        r##"<svg viewBox="0 0 440 60"><ellipse cx="220" cy="30" rx="210" ry="24" fill="#8fa4d8" stroke="{INK}" stroke-width="5"/><path d="M110 30 Q160 20 210 30" stroke="#f4ecd9" stroke-width="5" fill="none"/></svg>"##
    )
}

pub fn wet_cat() -> String {
    format!(
        r##"<svg viewBox="0 0 380 380"><defs><clipPath id="wet"><circle cx="190" cy="190" r="186"/></clipPath></defs><circle cx="190" cy="190" r="186" fill="#8a8fa8"/><image width="380" height="380" href="{}" clip-path="url(#wet)" preserveAspectRatio="xMidYMid slice"/><circle cx="190" cy="190" r="182" fill="none" stroke="{INK}" stroke-width="8"/></svg>"##,
        sad_cat()
    )
}

pub fn rule() -> String {
    format!(
        r#"<svg viewBox="0 0 620 6" preserveAspectRatio="none"><path d="M0 2 H620" stroke="{INK}" stroke-width="4" stroke-dasharray="10 6"/></svg>"#
    )
}

pub fn bar() -> String {
    format!(
        r#"<svg viewBox="0 0 10 10" preserveAspectRatio="none"><rect width="10" height="10" fill="{INK}"/></svg>"#
    )
}
