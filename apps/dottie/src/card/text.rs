//! The bundled brand fonts: measured here with `ttf-parser`, drawn by
//! resvg from the same bytes, so a width that fits here fits on the card.
use std::sync::{Arc, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Font {
    /// Archivo Black: symbols and big numbers.
    Display,
    Mono,
    MonoBold,
    BodyBold,
    Italic,
    SemiItalic,
}

const FILES: [(&[u8], &str, u16, &str); 6] = [
    (
        include_bytes!("../../assets/card/fonts/ArchivoBlack-Regular.ttf"),
        "Archivo Black",
        400,
        "normal",
    ),
    (
        include_bytes!("../../assets/card/fonts/SpaceMono-Regular.ttf"),
        "Space Mono",
        400,
        "normal",
    ),
    (
        include_bytes!("../../assets/card/fonts/SpaceMono-Bold.ttf"),
        "Space Mono",
        700,
        "normal",
    ),
    (
        include_bytes!("../../assets/card/fonts/WorkSans-Bold.ttf"),
        "Work Sans",
        700,
        "normal",
    ),
    (
        include_bytes!("../../assets/card/fonts/WorkSans-MediumItalic.ttf"),
        "Work Sans",
        500,
        "italic",
    ),
    (
        include_bytes!("../../assets/card/fonts/WorkSans-SemiBoldItalic.ttf"),
        "Work Sans",
        600,
        "italic",
    ),
];

impl Font {
    fn index(self) -> usize {
        self as usize
    }

    /// `font-family`, `font-weight` and `font-style` for SVG.
    pub fn attrs(self) -> String {
        let (_, family, weight, style) = FILES[self.index()];
        format!(r#"font-family="{family}" font-weight="{weight}" font-style="{style}""#)
    }
}

struct Metrics {
    face: ttf_parser::Face<'static>,
    upem: f32,
    /// Fallback advance for a glyph the font lacks.
    missing: f32,
}

struct Fonts {
    metrics: Vec<Metrics>,
    db: Arc<resvg::usvg::fontdb::Database>,
}

fn fonts() -> &'static Fonts {
    static FONTS: OnceLock<Fonts> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        let metrics = FILES
            .iter()
            .map(|(bytes, ..)| {
                db.load_font_data(bytes.to_vec());
                let face = ttf_parser::Face::parse(bytes, 0).expect("bundled font parses");
                let upem = f32::from(face.units_per_em());
                Metrics {
                    face,
                    upem,
                    missing: upem * 0.6,
                }
            })
            .collect();
        // Glyphs the brand fonts lack (CJK, emoji) fall back to Work Sans
        // so they render as a box rather than vanish.
        db.set_sans_serif_family("Work Sans");
        Fonts {
            metrics,
            db: Arc::new(db),
        }
    })
}

pub fn database() -> Arc<resvg::usvg::fontdb::Database> {
    fonts().db.clone()
}

/// Advance width of `text` at `size` px with `ls` em of letter spacing.
pub fn width(font: Font, text: &str, size: f32, ls: f32) -> f32 {
    let m = &fonts().metrics[font.index()];
    let units: f32 = text
        .chars()
        .map(|c| {
            m.face
                .glyph_index(c)
                .and_then(|g| m.face.glyph_hor_advance(g))
                .map_or(m.missing, f32::from)
        })
        .sum();
    units * size / m.upem + ls * size * text.chars().count() as f32
}

/// Ascender and descender (positive) as fractions of the font size.
pub fn extents(font: Font) -> (f32, f32) {
    let m = &fonts().metrics[font.index()];
    (
        f32::from(m.face.ascender()) / m.upem,
        -f32::from(m.face.descender()) / m.upem,
    )
}

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_is_monospaced_and_display_is_wide() {
        let a = width(Font::MonoBold, "1111", 100.0, 0.0);
        let b = width(Font::MonoBold, "WWWW", 100.0, 0.0);
        assert!((a - b).abs() < 0.01);
        assert!(
            width(Font::Display, "TADA", 100.0, 0.0) > width(Font::BodyBold, "TADA", 100.0, 0.0)
        );
        assert!(width(Font::Mono, "AB", 10.0, 0.1) > width(Font::Mono, "AB", 10.0, 0.0));
    }
}
