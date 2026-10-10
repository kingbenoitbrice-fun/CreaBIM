//! Drawings on paper. A [`Canvas`] takes linework in millimetres with y up;
//! SVG, PDF and the app's sheet view all implement it, so a sheet looks the
//! same on screen as in its exports.

use crate::plan::{Hatch, Pen, PlanItem, PlanView};
use glam::DVec2;
use std::fmt::Write;

/// Line style on paper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub width: f64,
    /// 0 is black, 1 is white.
    pub gray: f64,
    /// Dash and gap lengths in mm, for dashed lines.
    pub dash: Option<[f64; 2]>,
}

impl Stroke {
    pub const fn solid(width: f64, gray: f64) -> Self {
        Self {
            width,
            gray,
            dash: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

/// Something that draws in paper millimetres, y up.
pub trait Canvas {
    /// An open polyline.
    fn line(&mut self, pts: &[DVec2], stroke: Stroke);
    /// A filled polygon with no outline.
    fn fill(&mut self, pts: &[DVec2], gray: f64);
    /// Text with its baseline centred vertically on `at`.
    fn text(&mut self, at: DVec2, size: f64, text: &str, anchor: Anchor);

    fn polygon(&mut self, pts: &[DVec2], stroke: Stroke) {
        if let Some(first) = pts.first() {
            let mut closed = pts.to_vec();
            closed.push(*first);
            self.line(&closed, stroke);
        }
    }

    fn rect(&mut self, lo: DVec2, hi: DVec2, stroke: Stroke) {
        self.polygon(
            &[lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)],
            stroke,
        );
    }

    fn circle(&mut self, c: DVec2, r: f64, stroke: Stroke) {
        let pts: Vec<DVec2> = (0..32)
            .map(|i| c + DVec2::from_angle(i as f64 / 32.0 * std::f64::consts::TAU) * r)
            .collect();
        self.polygon(&pts, stroke);
    }
}

/// Maps drawing coordinates (model metres) to paper millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaperMap {
    /// Drawing point that lands on `paper`.
    pub model: DVec2,
    pub paper: DVec2,
    /// Scale denominator: 100 means 1:100.
    pub scale: f64,
}

impl PaperMap {
    /// Millimetres on paper per metre in the model.
    pub fn k(&self) -> f64 {
        1000.0 / self.scale
    }

    pub fn to_paper(&self, p: DVec2) -> DVec2 {
        self.paper + (p - self.model) * self.k()
    }

    pub fn to_model(&self, p: DVec2) -> DVec2 {
        self.model + (p - self.paper) / self.k()
    }
}

pub const CUT: Stroke = Stroke::solid(0.5, 0.0);
pub const PROJECTION: Stroke = Stroke::solid(0.18, 0.25);
pub const SYMBOL: Stroke = Stroke::solid(0.18, 0.0);
pub const GRID: Stroke = Stroke {
    width: 0.13,
    gray: 0.5,
    dash: Some([4.0, 1.0]),
};
const HATCH: Stroke = Stroke::solid(0.1, 0.2);
/// Hatch line spacing on paper, mm.
pub const HATCH_SPACING: f64 = 1.2;

pub fn pen_stroke(pen: Pen) -> Stroke {
    match pen {
        Pen::Cut => CUT,
        Pen::Projection => PROJECTION,
        Pen::Symbol => SYMBOL,
        Pen::Grid => GRID,
    }
}

/// Fill shade for a cut hatch: solid cuts are darker, hatched ones light.
pub fn hatch_fill(h: Hatch) -> f64 {
    match h {
        Hatch::Solid => 0.6,
        Hatch::Diagonal | Hatch::Cross => 0.88,
    }
}

/// Draw a generated drawing onto a canvas.
pub fn draw(canvas: &mut dyn Canvas, drawing: &PlanView, map: &PaperMap) {
    let to = |p: DVec2| map.to_paper(p);
    for it in &drawing.items {
        match it {
            PlanItem::Fill { points, hatch, .. } => {
                let pts: Vec<DVec2> = points.iter().map(|p| to(*p)).collect();
                canvas.fill(&pts, hatch_fill(*hatch));
                for (a, b) in hatch.lines(points, HATCH_SPACING / map.k()) {
                    canvas.line(&[to(a), to(b)], HATCH);
                }
            }
            PlanItem::Polygon { pen, points, .. } => {
                let pts: Vec<DVec2> = points.iter().map(|p| to(*p)).collect();
                if *pen == Pen::Cut {
                    canvas.fill(&pts, hatch_fill(Hatch::Solid));
                }
                canvas.polygon(&pts, pen_stroke(*pen));
            }
            PlanItem::Line { pen, a, b, .. } => canvas.line(&[to(*a), to(*b)], pen_stroke(*pen)),
            PlanItem::Arc {
                pen,
                center,
                radius,
                start,
                end,
                ..
            } => {
                let pts: Vec<DVec2> = (0..=24)
                    .map(|i| {
                        let t = start + (end - start) * i as f64 / 24.0;
                        to(*center + DVec2::from_angle(t) * *radius)
                    })
                    .collect();
                canvas.line(&pts, pen_stroke(*pen));
            }
            PlanItem::Label { at, text, .. } => {
                let p = to(*at);
                // short tags (grids, section marks) sit in a bubble
                if text.chars().count() <= 3 {
                    canvas.circle(p, 2.5, Stroke::solid(0.18, 0.0));
                }
                canvas.text(p, 2.5, text, Anchor::Middle);
            }
        }
    }
}

/// Rough width of Helvetica text, mm.
pub fn text_width(text: &str, size: f64) -> f64 {
    text.chars()
        .map(|c| match c {
            'i' | 'j' | 'l' | '.' | ',' | ':' | ';' | '\'' | '|' | '!' => 0.25,
            ' ' | 'f' | 't' | 'r' | 'I' | '(' | ')' | '-' => 0.33,
            'm' | 'w' | 'M' | 'W' => 0.85,
            c if c.is_uppercase() || c.is_ascii_digit() => 0.63,
            _ => 0.54,
        })
        .sum::<f64>()
        * size
}

// ---- SVG -----------------------------------------------------------------

/// An SVG page; `finish` returns the document.
pub struct SvgCanvas {
    size: DVec2,
    body: String,
}

impl SvgCanvas {
    pub fn new(size: DVec2) -> Self {
        Self {
            size,
            body: String::new(),
        }
    }

    fn y(&self, p: DVec2) -> DVec2 {
        DVec2::new(p.x, self.size.y - p.y)
    }

    pub fn finish(self) -> String {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.1}mm\" height=\"{h:.1}mm\" viewBox=\"0 0 {w:.3} {h:.3}\">\n\
             <rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n{}</svg>\n",
            self.body,
            w = self.size.x,
            h = self.size.y
        )
    }
}

fn svg_gray(g: f64) -> String {
    let v = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{v:02x}{v:02x}{v:02x}")
}

fn points(pts: &[DVec2]) -> String {
    pts.iter()
        .map(|p| format!("{:.3},{:.3}", p.x, p.y))
        .collect::<Vec<_>>()
        .join(" ")
}

impl Canvas for SvgCanvas {
    fn line(&mut self, pts: &[DVec2], s: Stroke) {
        let pts: Vec<DVec2> = pts.iter().map(|p| self.y(*p)).collect();
        let dash = s
            .dash
            .map(|[a, b]| format!(" stroke-dasharray=\"{a} {b}\""))
            .unwrap_or_default();
        let _ = writeln!(
            self.body,
            "<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"{dash}/>",
            points(&pts),
            svg_gray(s.gray),
            s.width
        );
    }

    fn fill(&mut self, pts: &[DVec2], gray: f64) {
        let pts: Vec<DVec2> = pts.iter().map(|p| self.y(*p)).collect();
        let _ = writeln!(
            self.body,
            "<polygon points=\"{}\" fill=\"{}\" stroke=\"none\"/>",
            points(&pts),
            svg_gray(gray)
        );
    }

    fn text(&mut self, at: DVec2, size: f64, text: &str, anchor: Anchor) {
        let p = self.y(at);
        let anchor = match anchor {
            Anchor::Start => "start",
            Anchor::Middle => "middle",
            Anchor::End => "end",
        };
        let _ = writeln!(
            self.body,
            "<text x=\"{:.3}\" y=\"{:.3}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{size}\" text-anchor=\"{anchor}\" dominant-baseline=\"middle\">{}</text>",
            p.x,
            p.y,
            xml_escape(text)
        );
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ---- PDF -----------------------------------------------------------------

/// A multi-page vector PDF. Text uses the built-in Helvetica font.
#[derive(Default)]
pub struct PdfWriter {
    pages: Vec<(DVec2, String)>,
}

/// One page of a [`PdfWriter`], drawn in millimetres.
pub struct PdfPage<'a> {
    content: &'a mut String,
}

const PT: f64 = 72.0 / 25.4;

impl PdfWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a page of `size` mm; draw on the returned canvas.
    pub fn page(&mut self, size: DVec2) -> PdfPage<'_> {
        // millimetre units, round line ends
        self.pages
            .push((size, format!("{PT:.5} 0 0 {PT:.5} 0 0 cm 1 J 1 j\n")));
        PdfPage {
            content: &mut self.pages.last_mut().expect("just pushed").1,
        }
    }

    pub fn finish(self) -> Vec<u8> {
        let mut out: Vec<u8> = b"%PDF-1.4\n".to_vec();
        let mut offsets: Vec<usize> = Vec::new();
        let n = self.pages.len();
        // objects: 1 catalog, 2 pages, 3 font, then (page, content) pairs
        let mut obj = |out: &mut Vec<u8>, body: &[u8]| {
            offsets.push(out.len());
            let id = offsets.len();
            out.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        };
        obj(&mut out, b"<< /Type /Catalog /Pages 2 0 R >>");
        let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 4 + i * 2)).collect();
        obj(
            &mut out,
            format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")).as_bytes(),
        );
        obj(
            &mut out,
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        );
        for (i, (size, content)) in self.pages.iter().enumerate() {
            obj(
                &mut out,
                format!(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                    size.x * PT,
                    size.y * PT,
                    5 + i * 2
                )
                .as_bytes(),
            );
            let mut body = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
            body.extend_from_slice(content.as_bytes());
            body.extend_from_slice(b"\nendstream");
            obj(&mut out, &body);
        }
        let xref = out.len();
        let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1);
        for o in &offsets {
            let _ = writeln!(tail, "{o:010} 00000 n ");
        }
        let _ = write!(
            tail,
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len() + 1
        );
        out.extend_from_slice(tail.as_bytes());
        out
    }
}

fn pdf_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('(');
    for c in s.chars() {
        match c {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            ' '..='~' => out.push(c),
            _ => out.push('?'),
        }
    }
    out.push(')');
    out
}

impl Canvas for PdfPage<'_> {
    fn line(&mut self, pts: &[DVec2], s: Stroke) {
        if pts.len() < 2 {
            return;
        }
        let c = &mut *self.content;
        let _ = write!(c, "{:.3} G {:.3} w ", s.gray, s.width);
        match s.dash {
            Some([a, b]) => {
                let _ = write!(c, "[{a} {b}] 0 d ");
            }
            None => c.push_str("[] 0 d "),
        }
        for (i, p) in pts.iter().enumerate() {
            let _ = write!(
                c,
                "{:.3} {:.3} {} ",
                p.x,
                p.y,
                if i == 0 { "m" } else { "l" }
            );
        }
        c.push_str("S\n");
    }

    fn fill(&mut self, pts: &[DVec2], gray: f64) {
        if pts.len() < 3 {
            return;
        }
        let c = &mut *self.content;
        let _ = write!(c, "{gray:.3} g ");
        for (i, p) in pts.iter().enumerate() {
            let _ = write!(
                c,
                "{:.3} {:.3} {} ",
                p.x,
                p.y,
                if i == 0 { "m" } else { "l" }
            );
        }
        c.push_str("h f\n");
    }

    fn text(&mut self, at: DVec2, size: f64, text: &str, anchor: Anchor) {
        let w = text_width(text, size);
        let x = match anchor {
            Anchor::Start => at.x,
            Anchor::Middle => at.x - w / 2.0,
            Anchor::End => at.x - w,
        };
        // baseline sits a little below the middle of capital letters
        let y = at.y - size * 0.35;
        let _ = writeln!(
            self.content,
            "0 g BT /F1 {size:.3} Tf {x:.3} {y:.3} Td {} Tj ET",
            pdf_string(text)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_has_a_valid_xref() {
        let mut pdf = PdfWriter::new();
        {
            let mut p = pdf.page(DVec2::new(420.0, 297.0));
            p.rect(DVec2::splat(10.0), DVec2::new(410.0, 287.0), CUT);
            p.text(DVec2::new(20.0, 20.0), 3.0, "Plan (1:100)", Anchor::Start);
        }
        pdf.page(DVec2::new(210.0, 297.0));
        let bytes = pdf.finish();
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.starts_with("%PDF-1.4"));
        assert!(s.contains("/Count 2"));
        assert!(s.contains("(Plan \\(1:100\\)) Tj"));
        // every xref offset points at its object
        let xref_at: usize = s
            .rsplit("startxref\n")
            .next()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let table = &s[xref_at..];
        for (i, line) in table.lines().skip(3).take(7).enumerate() {
            let off: usize = line[..10].parse().unwrap();
            assert!(
                s[off..].starts_with(&format!("{} 0 obj", i + 1)),
                "object {}",
                i + 1
            );
        }
    }
}
