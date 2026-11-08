//! Slots on the 1080 grid: each value sits in a fixed box, and its text is
//! drawn at the largest size (between the slot's min and max) whose width
//! and height fit the box. A symbol or name that still doesn't fit at the
//! min is cut with "…". Numbers are never cut; the formatter keeps them short.
use super::text::{Font, escape, extents, width};
use std::fmt::Write;

pub const INK: &str = "#15161c";

#[derive(Clone)]
pub struct Run {
    pub text: String,
    pub font: Font,
    /// Size relative to the slot's font size.
    pub scale: f32,
    /// Letter spacing in em.
    pub ls: f32,
    pub fill: &'static str,
    pub opacity: f32,
    /// A subscript zero count inside a price.
    pub sub: bool,
    /// The run "…" may shorten when nothing else fits.
    pub ell: bool,
    /// Hard ink shadow offset in em.
    pub shadow: f32,
}

pub fn run(text: impl Into<String>, font: Font) -> Run {
    Run {
        text: text.into(),
        font,
        scale: 1.0,
        ls: 0.0,
        fill: INK,
        opacity: 1.0,
        sub: false,
        ell: false,
        shadow: 0.0,
    }
}

impl Run {
    pub fn fill(mut self, fill: &'static str) -> Self {
        self.fill = fill;
        self
    }
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }
    pub fn ls(mut self, ls: f32) -> Self {
        self.ls = ls;
        self
    }
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
    pub fn ell(mut self) -> Self {
        self.ell = true;
        self
    }
    pub fn shadow(mut self, em: f32) -> Self {
        self.shadow = em;
        self
    }
    /// Splits card markup (`$0.0{8}442`) into runs with a subscript count.
    pub fn markup(self) -> Line {
        let mut out = vec![];
        let mut rest = self.text.as_str();
        while let Some(open) = rest.find('{') {
            let close = rest[open..].find('}').map_or(rest.len(), |c| open + c);
            if open > 0 {
                out.push(Piece::Run(Run {
                    text: rest[..open].into(),
                    ..self.clone()
                }));
            }
            out.push(Piece::Run(Run {
                text: rest[open + 1..close].into(),
                sub: true,
                ..self.clone()
            }));
            rest = rest.get(close + 1..).unwrap_or("");
        }
        if !rest.is_empty() {
            out.push(Piece::Run(Run {
                text: rest.into(),
                ..self
            }));
        }
        out
    }
}

#[derive(Clone)]
pub enum Piece {
    Run(Run),
    /// The round logo, `em` wide.
    Logo(f32),
    Gap(f32),
}

pub type Line = Vec<Piece>;

impl Piece {
    fn width(&self, size: f32) -> f32 {
        match self {
            Piece::Run(r) => width(
                r.font,
                &r.text,
                size * r.scale * if r.sub { 0.6 } else { 1.0 },
                r.ls,
            ),
            Piece::Logo(em) | Piece::Gap(em) => em * size,
        }
    }
}

fn line_width(line: &Line, size: f32) -> f32 {
    line.iter().map(|p| p.width(size)).sum()
}

/// Line box height: the tallest piece, at line-height 1.
fn line_height(line: &Line, size: f32) -> f32 {
    line.iter()
        .map(|p| match p {
            Piece::Run(r) if !r.sub => r.scale,
            Piece::Logo(em) => *em,
            _ => 0.0,
        })
        .fold(0.0, f32::max)
        * size
}

/// Draws `line` with its box's top-left at (x, top).
fn draw_line(out: &mut String, line: &Line, size: f32, x: f32, top: f32) {
    let h = line_height(line, size);
    // The baseline of the biggest run, centred in the line box the way CSS
    // centres a line-height: 1 box.
    let main = line
        .iter()
        .filter_map(|p| match p {
            Piece::Run(r) if !r.sub => Some(r),
            _ => None,
        })
        .max_by(|a, b| a.scale.total_cmp(&b.scale));
    let baseline = match main {
        Some(r) => {
            let (asc, desc) = extents(r.font);
            let s = size * r.scale;
            top + (h - (asc + desc) * s) / 2.0 + asc * s
        }
        None => top + h * 0.8,
    };
    let mut cx = x;
    for piece in line {
        let w = piece.width(size);
        match piece {
            Piece::Run(r) => {
                let s = size * r.scale * if r.sub { 0.6 } else { 1.0 };
                let y = baseline + if r.sub { 0.3 * size * r.scale } else { 0.0 };
                let common = format!(
                    r#"{} font-size="{s:.1}" letter-spacing="{:.2}""#,
                    r.font.attrs(),
                    r.ls * s
                );
                if r.shadow > 0.0 {
                    let d = r.shadow * s;
                    let _ = write!(
                        out,
                        r#"<text x="{:.1}" y="{:.1}" {common} fill="{INK}">{}</text>"#,
                        cx + d,
                        y + d,
                        escape(&r.text)
                    );
                }
                let _ = write!(
                    out,
                    r#"<text x="{cx:.1}" y="{y:.1}" {common} fill="{}" fill-opacity="{}">{}</text>"#,
                    r.fill,
                    r.opacity,
                    escape(&r.text)
                );
            }
            Piece::Logo(em) => {
                let d = em * size;
                let _ = write!(
                    out,
                    r#"<image x="{cx:.1}" y="{:.1}" width="{d:.1}" height="{d:.1}" href="{}" clip-path="url(#round)"/>"#,
                    top + (h - d) / 2.0,
                    super::art::logo()
                );
            }
            Piece::Gap(_) => {}
        }
        cx += w;
    }
}

#[derive(Clone)]
pub struct Boxed {
    pub fill: &'static str,
    pub stroke: &'static str,
    /// Padding (x, y) and border width in em.
    pub pad: (f32, f32),
    pub border: f32,
    pub radius: f32,
    pub shadow: f32,
}

#[derive(Clone)]
pub enum Body {
    Line(Line),
    /// A chip, tag or stamp: one line inside a bordered box.
    Boxed(Line, Boxed),
    /// Label over value, wrapping left to right (centred when `center`).
    Stats {
        items: Vec<(String, Line)>,
        fill: &'static str,
        center: bool,
    },
    /// Receipt lines: label, dotted leader, value.
    Rows(Vec<(String, Line)>),
    /// Short items joined by " · ", wrapped and centred.
    Wrap(Vec<Line>),
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq)]
pub enum VAlign {
    Middle,
    Top,
    Bottom,
}

#[derive(Clone)]
pub struct Slot {
    pub id: &'static str,
    pub r: [f32; 4],
    pub body: Body,
    pub max: f32,
    pub min: f32,
    pub align: Align,
    pub valign: VAlign,
    /// Degrees; a rotated slot keeps its content inside 86% of the box.
    pub rot: f32,
}

pub fn slot(id: &'static str, r: [f32; 4], body: Body, max: f32, min: f32) -> Slot {
    Slot {
        id,
        r,
        body,
        max,
        min,
        align: Align::Left,
        valign: VAlign::Middle,
        rot: 0.0,
    }
}

impl Slot {
    pub fn center(mut self) -> Self {
        self.align = Align::Center;
        self
    }
    pub fn right(mut self) -> Self {
        self.align = Align::Right;
        self
    }
    pub fn top(mut self) -> Self {
        self.valign = VAlign::Top;
        self
    }
    pub fn bottom(mut self) -> Self {
        self.valign = VAlign::Bottom;
        self
    }
    pub fn rot(mut self, deg: f32) -> Self {
        self.rot = deg;
        self
    }

    fn room(&self) -> (f32, f32) {
        let k = if self.rot != 0.0 { 0.86 } else { 1.0 };
        (self.r[2] * k, self.r[3] * k)
    }

    /// Width and height the body needs at `size`.
    fn need(&self, size: f32) -> (f32, f32) {
        let (w, _) = self.room();
        match &self.body {
            Body::Line(line) => (line_width(line, size), line_height(line, size)),
            Body::Boxed(line, b) => {
                let edge = 2.0 * (b.border * size);
                (
                    line_width(line, size) + 2.0 * b.pad.0 * size + edge + b.shadow * size,
                    line_height(line, size) + 2.0 * b.pad.1 * size + edge + b.shadow * size,
                )
            }
            Body::Stats { items, .. } => {
                let rows = stat_rows(items, size, w);
                let widest = rows.iter().map(|r| r.1).fold(0.0, f32::max);
                let n = rows.len() as f32;
                (widest, n * STAT_H * size + (n - 1.0).max(0.0) * 0.5 * size)
            }
            Body::Rows(rows) => {
                let widest = rows
                    .iter()
                    .map(|(l, v)| {
                        width(Font::MonoBold, l, size, 0.0) + line_width(v, size) + 1.6 * size
                    })
                    .fold(0.0, f32::max);
                let n = rows.len() as f32;
                (widest, n * 1.15 * size + (n - 1.0).max(0.0) * 0.32 * size)
            }
            Body::Wrap(items) => {
                let lines = wrap_lines(items, size, w);
                let widest = lines.iter().map(|l| l.1).fold(0.0, f32::max);
                (widest, lines.len() as f32 * 1.3 * size)
            }
        }
    }

    fn over(&self, size: f32) -> bool {
        let (w, h) = self.room();
        let (nw, nh) = self.need(size);
        nw > w + 0.5 || nh > h + 0.5
    }

    /// Picks the size and trims with "…" if needed; false when even that fails.
    pub fn fit(&mut self) -> (f32, bool) {
        let (mut lo, mut hi) = (self.min as i32, self.max as i32);
        if self.over(hi as f32) {
            while hi - lo > 1 {
                let mid = (lo + hi) / 2;
                if self.over(mid as f32) {
                    hi = mid
                } else {
                    lo = mid
                }
            }
        } else {
            lo = hi;
        }
        let size = lo as f32;
        if self.over(size) {
            self.ellipsize(size);
        }
        (size, !self.over(size))
    }

    fn ellipsize(&mut self, size: f32) {
        let full: Vec<char> = match self.ell_run() {
            Some(r) => r.text.chars().collect(),
            None => return,
        };
        let (mut a, mut b) = (1, full.len());
        while b - a > 1 {
            let m = (a + b) / 2;
            self.set_ell(full[..m].iter().collect::<String>() + "…");
            if self.over(size) { b = m } else { a = m }
        }
        self.set_ell(full[..a].iter().collect::<String>() + "…");
    }

    fn ell_run(&mut self) -> Option<&mut Run> {
        let line = match &mut self.body {
            Body::Line(l) | Body::Boxed(l, _) => l,
            _ => return None,
        };
        line.iter_mut().find_map(|p| match p {
            Piece::Run(r) if r.ell => Some(r),
            _ => None,
        })
    }

    fn set_ell(&mut self, text: String) {
        if let Some(r) = self.ell_run() {
            r.text = text;
        }
    }

    /// Fits and draws the slot; returns whether it fit.
    pub fn draw(mut self, out: &mut String) -> bool {
        let (size, ok) = self.fit();
        let [x, y, w, h] = self.r;
        let (nw, nh) = self.need(size);
        let x0 = match self.align {
            Align::Left => x,
            Align::Center => x + (w - nw) / 2.0,
            Align::Right => x + w - nw,
        };
        let y0 = match self.valign {
            VAlign::Middle => y + (h - nh) / 2.0,
            VAlign::Top => y,
            VAlign::Bottom => y + h - nh,
        };
        if self.rot != 0.0 {
            let _ = write!(
                out,
                r#"<g transform="rotate({} {} {})">"#,
                self.rot,
                x + w / 2.0,
                y + h / 2.0
            );
        }
        match &self.body {
            Body::Line(line) => draw_line(out, line, size, x0, y0),
            Body::Boxed(line, b) => {
                let bw = b.border * size;
                let (bx, by) = (nw - b.shadow * size, nh - b.shadow * size);
                if b.shadow > 0.0 {
                    let d = b.shadow * size;
                    let _ = write!(
                        out,
                        r#"<rect x="{:.1}" y="{:.1}" width="{bx:.1}" height="{by:.1}" rx="{:.1}" fill="{INK}"/>"#,
                        x0 + d,
                        y0 + d,
                        b.radius
                    );
                }
                let _ = write!(
                    out,
                    r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="{:.1}" fill="{}" stroke="{}" stroke-width="{bw:.1}"/>"#,
                    x0 + bw / 2.0,
                    y0 + bw / 2.0,
                    bx - bw,
                    by - bw,
                    b.radius,
                    b.fill,
                    b.stroke
                );
                draw_line(
                    out,
                    line,
                    size,
                    x0 + bw + b.pad.0 * size,
                    y0 + bw + b.pad.1 * size,
                );
            }
            Body::Stats {
                items,
                fill,
                center,
            } => {
                let rows = stat_rows(items, size, self.room().0);
                let mut top = y0;
                for (row, row_w) in rows {
                    let mut cx = if *center { x + (w - row_w) / 2.0 } else { x0 };
                    for i in row {
                        let (label, value) = &items[i];
                        let label = vec![Piece::Run(
                            run(label.to_uppercase(), Font::MonoBold)
                                .scale(0.56)
                                .ls(0.1)
                                .fill(fill)
                                .opacity(0.78),
                        )];
                        draw_line(out, &label, size, cx, top);
                        draw_line(out, value, size, cx, top + 0.78 * size);
                        cx += stat_width(&items[i], size) + 1.3 * size;
                    }
                    top += (STAT_H + 0.5) * size;
                }
            }
            Body::Rows(rows) => {
                let mut top = y0;
                for (label, value) in rows {
                    let lw = width(Font::MonoBold, label, size, 0.0);
                    let vw = line_width(value, size);
                    draw_line(
                        out,
                        &vec![Piece::Run(run(label.clone(), Font::MonoBold))],
                        size,
                        x,
                        top,
                    );
                    draw_line(out, value, size, x + w - vw, top);
                    let ly = top + 0.82 * size;
                    let _ = write!(
                        out,
                        r#"<line x1="{:.1}" y1="{ly:.1}" x2="{:.1}" y2="{ly:.1}" stroke="{INK}" stroke-opacity=".55" stroke-width="{:.1}" stroke-dasharray="{:.1} {:.1}" stroke-linecap="round"/>"#,
                        x + lw + 0.4 * size,
                        x + w - vw - 0.4 * size,
                        0.12 * size,
                        0.01 * size,
                        0.24 * size
                    );
                    top += (1.15 + 0.32) * size;
                }
            }
            Body::Wrap(items) => {
                let mut top = y0;
                for (line, lw) in wrap_lines(items, size, self.room().0) {
                    draw_line(out, &line, size, x + (w - lw) / 2.0, top + 0.15 * size);
                    top += 1.3 * size;
                }
            }
        }
        if self.rot != 0.0 {
            out.push_str("</g>");
        }
        ok
    }
}

/// Label (0.56em) + gap + value (1.05em).
const STAT_H: f32 = 0.56 + 0.22 + 1.05;

fn stat_width(item: &(String, Line), size: f32) -> f32 {
    let label = width(Font::MonoBold, &item.0.to_uppercase(), size * 0.56, 0.1);
    label.max(line_width(&item.1, size))
}

/// Greedy wrap: rows of item indices with their widths.
fn stat_rows(items: &[(String, Line)], size: f32, max_w: f32) -> Vec<(Vec<usize>, f32)> {
    let gap = 1.3 * size;
    let mut rows: Vec<(Vec<usize>, f32)> = vec![];
    for (i, item) in items.iter().enumerate() {
        let w = stat_width(item, size);
        match rows.last_mut() {
            Some((row, row_w)) if *row_w + gap + w <= max_w => {
                row.push(i);
                *row_w += gap + w;
            }
            _ => rows.push((vec![i], w)),
        }
    }
    rows
}

fn wrap_lines(items: &[Line], size: f32, max_w: f32) -> Vec<(Line, f32)> {
    let sep: Line = vec![Piece::Run(run("  ·  ", Font::MonoBold))];
    let sep_w = line_width(&sep, size);
    let mut lines: Vec<(Line, f32)> = vec![];
    for item in items {
        let w = line_width(item, size);
        match lines.last_mut() {
            Some((line, lw)) if *lw + sep_w + w <= max_w => {
                line.extend(sep.iter().cloned());
                line.extend(item.iter().cloned());
                *lw += sep_w + w;
            }
            _ => lines.push((item.clone(), w)),
        }
    }
    lines
}
