//! Sheets: views laid out on paper with a border and title block, exported
//! as SVG or a multi-page PDF. The same layout drives the app's sheet view.

use crate::paper::{self, Anchor, Canvas, PaperMap, PdfWriter, Stroke, SvgCanvas};
use crate::section::{view_drawing, Drawing};
use creabim_core::{ElementId, ElementKind, Model, Sheet, View, Viewport};
use glam::DVec2;

/// Text for the title block that does not live on the sheet itself.
#[derive(Clone, Debug, Default)]
pub struct SheetInfo {
    pub project: String,
    pub date: String,
}

/// A viewport resolved for drawing: its view's live drawing and where it lands.
#[derive(Clone, Debug)]
pub struct PlacedView {
    pub viewport: ElementId,
    pub view: ElementId,
    /// Position on the sheet, 1-based, in model order.
    pub number: usize,
    pub title: String,
    pub map: PaperMap,
    pub drawing: Drawing,
    /// The drawing's extent on paper, mm.
    pub bounds: (DVec2, DVec2),
}

impl PlacedView {
    /// Where the view title sits, below the drawing.
    pub fn title_at(&self) -> DVec2 {
        DVec2::new(self.bounds.0.x, self.bounds.0.y - 7.0)
    }
}

/// Today's date as YYYY-MM-DD (UTC), for title blocks.
pub fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400) as i64;
    // days since 1970-01-01 to a civil date (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

pub const MARGIN: f64 = 10.0;
/// Extra margin on the binding (left) edge.
pub const BINDING: f64 = 20.0;
pub const TITLE_BLOCK: DVec2 = DVec2::new(170.0, 45.0);

pub fn sheet_of(model: &Model, id: ElementId) -> Option<&Sheet> {
    match &model.get(id)?.kind {
        ElementKind::Sheet(s) => Some(s),
        _ => None,
    }
}

fn view_of(model: &Model, id: ElementId) -> Option<&View> {
    match &model.get(id)?.kind {
        ElementKind::View(v) => Some(v),
        _ => None,
    }
}

/// Every sheet in the model, ordered by sheet number.
pub fn sheets(model: &Model) -> Vec<(ElementId, &Sheet)> {
    let mut v: Vec<(ElementId, &Sheet)> = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Sheet(s) => Some((e.id, s)),
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| a.1.number.cmp(&b.1.number));
    v
}

/// The viewports on a sheet with their drawings. `drawing` supplies each
/// view's drawing, so callers can cache them.
pub fn placed_views(
    model: &Model,
    sheet: ElementId,
    drawing: &mut dyn FnMut(ElementId) -> Option<Drawing>,
) -> Vec<PlacedView> {
    let mut out = Vec::new();
    for e in model.iter() {
        let ElementKind::Viewport(vp) = &e.kind else {
            continue;
        };
        if vp.sheet != sheet {
            continue;
        }
        let (Some(view), Some(d)) = (view_of(model, vp.view), drawing(vp.view)) else {
            continue;
        };
        let map = PaperMap {
            model: vp.target,
            paper: vp.center,
            scale: view.scale,
        };
        let bounds = d
            .bounds()
            .map(|(lo, hi)| (map.to_paper(lo), map.to_paper(hi)))
            .unwrap_or((vp.center, vp.center));
        out.push(PlacedView {
            viewport: e.id,
            view: vp.view,
            number: out.len() + 1,
            title: model
                .get(vp.view)
                .map(|v| v.name.clone())
                .unwrap_or_default(),
            map,
            drawing: d,
            bounds,
        });
    }
    out
}

/// A viewport for `view` centred on `at` (sheet mm), pinning the middle of
/// the view's current drawing there.
pub fn new_viewport(model: &Model, sheet: ElementId, view: ElementId, at: DVec2) -> Viewport {
    let target = view_drawing(model, view)
        .and_then(|d| d.bounds())
        .map_or(DVec2::ZERO, |(lo, hi)| (lo + hi) / 2.0);
    Viewport {
        sheet,
        view,
        center: at,
        target,
    }
}

/// A spot on the sheet where `view` fits without covering the views
/// already there or the title block; the middle of the sheet if none does.
pub fn free_spot(model: &Model, sheet: ElementId, view: ElementId) -> DVec2 {
    let Some(s) = sheet_of(model, sheet) else {
        return DVec2::ZERO;
    };
    let middle = DVec2::new(s.width, s.height) / 2.0;
    let (Some(v), Some((lo, hi))) = (
        view_of(model, view),
        view_drawing(model, view).and_then(|d| d.bounds()),
    ) else {
        return middle;
    };
    let k = 1000.0 / v.scale;
    // half size on paper, with room for the title below
    let half = (hi - lo) * k / 2.0 + DVec2::new(4.0, 6.0);
    let mut taken: Vec<(DVec2, DVec2)> =
        placed_views(model, sheet, &mut |v| view_drawing(model, v))
            .iter()
            .map(|p| {
                (
                    p.bounds.0 - DVec2::new(4.0, 14.0),
                    p.bounds.1 + DVec2::splat(4.0),
                )
            })
            .collect();
    let tb_lo = DVec2::new(s.width - MARGIN - TITLE_BLOCK.x, MARGIN);
    taken.push((tb_lo, tb_lo + TITLE_BLOCK));
    let (x0, x1) = (BINDING + half.x + 5.0, s.width - MARGIN - half.x - 5.0);
    let (y0, y1) = (MARGIN + half.y + 10.0, s.height - MARGIN - half.y - 5.0);
    let mut y = y1;
    while y >= y0 {
        let mut x = x0;
        while x <= x1 {
            let c = DVec2::new(x, y);
            let (a, b) = (c - half - DVec2::new(0.0, 8.0), c + half);
            if taken
                .iter()
                .all(|(p, q)| b.x < p.x || a.x > q.x || b.y < p.y || a.y > q.y)
            {
                return c;
            }
            x += 10.0;
        }
        y -= 10.0;
    }
    middle
}

/// Border and title block.
pub fn draw_frame(c: &mut dyn Canvas, sheet: &Sheet, title: &str, scales: &str, info: &SheetInfo) {
    let (w, h) = (sheet.width, sheet.height);
    let lo = DVec2::new(BINDING, MARGIN);
    let hi = DVec2::new(w - MARGIN, h - MARGIN);
    c.rect(lo, hi, Stroke::solid(0.5, 0.0));

    let thin = Stroke::solid(0.18, 0.0);
    let tb_lo = DVec2::new(hi.x - TITLE_BLOCK.x, lo.y);
    let tb_hi = DVec2::new(hi.x, lo.y + TITLE_BLOCK.y);
    c.rect(tb_lo, tb_hi, Stroke::solid(0.35, 0.0));
    // rows from the top: project, sheet title, then number / scale / date
    let row = |k: f64| tb_hi.y - k;
    c.line(
        &[
            DVec2::new(tb_lo.x, row(12.0)),
            DVec2::new(tb_hi.x, row(12.0)),
        ],
        thin,
    );
    c.line(
        &[
            DVec2::new(tb_lo.x, row(27.0)),
            DVec2::new(tb_hi.x, row(27.0)),
        ],
        thin,
    );
    let split = tb_lo.x + 70.0;
    c.line(
        &[DVec2::new(split, tb_lo.y), DVec2::new(split, row(27.0))],
        thin,
    );
    let x = tb_lo.x + 3.0;
    let small = 2.0;
    c.text(DVec2::new(x, row(3.5)), small, "PROJECT", Anchor::Start);
    c.text(DVec2::new(x, row(8.5)), 4.0, &info.project, Anchor::Start);
    c.text(DVec2::new(x, row(15.5)), small, "SHEET", Anchor::Start);
    c.text(DVec2::new(x, row(21.5)), 5.0, title, Anchor::Start);
    c.text(DVec2::new(x, row(30.5)), small, "NUMBER", Anchor::Start);
    c.text(DVec2::new(x, row(38.5)), 8.0, &sheet.number, Anchor::Start);
    let x2 = split + 3.0;
    c.text(DVec2::new(x2, row(30.5)), small, "SCALE", Anchor::Start);
    c.text(DVec2::new(x2, row(35.5)), 3.0, scales, Anchor::Start);
    c.text(DVec2::new(x2, row(40.5)), small, &info.date, Anchor::Start);
    c.text(
        DVec2::new(tb_hi.x - 3.0, row(40.5)),
        small,
        "CreaBIM",
        Anchor::End,
    );
}

/// The scale text for a title block: one scale, or "As indicated".
pub fn scale_text(model: &Model, views: &[PlacedView]) -> String {
    let mut scales: Vec<i64> = views
        .iter()
        .filter_map(|v| view_of(model, v.view))
        .map(|v| v.scale.round() as i64)
        .collect();
    scales.sort();
    scales.dedup();
    match scales.as_slice() {
        [] => "-".into(),
        [s] => format!("1:{s}"),
        _ => "As indicated".into(),
    }
}

/// A view's number bubble, title and scale under its drawing.
pub fn draw_view_title(c: &mut dyn Canvas, v: &PlacedView) {
    let at = v.title_at();
    let thin = Stroke::solid(0.18, 0.0);
    c.circle(at + DVec2::new(3.5, 0.0), 3.5, thin);
    c.text(
        at + DVec2::new(3.5, 0.0),
        3.0,
        &v.number.to_string(),
        Anchor::Middle,
    );
    c.text(at + DVec2::new(9.0, 1.2), 3.5, &v.title, Anchor::Start);
    let width = paper::text_width(&v.title, 3.5).max(30.0);
    c.line(
        &[
            at + DVec2::new(9.0, -1.0),
            at + DVec2::new(9.0 + width, -1.0),
        ],
        Stroke::solid(0.35, 0.0),
    );
    c.text(
        at + DVec2::new(9.0, -3.5),
        2.5,
        &format!("1:{:.0}", v.map.scale),
        Anchor::Start,
    );
}

/// Draw a whole sheet: frame, every view and its title.
pub fn draw_sheet(
    c: &mut dyn Canvas,
    model: &Model,
    sheet: ElementId,
    views: &[PlacedView],
    info: &SheetInfo,
) {
    let Some(s) = sheet_of(model, sheet) else {
        return;
    };
    let title = model.get(sheet).map(|e| e.name.as_str()).unwrap_or("");
    draw_frame(c, s, title, &scale_text(model, views), info);
    for v in views {
        paper::draw(c, &v.drawing, &v.map);
        draw_view_title(c, v);
    }
}

pub fn sheet_svg(model: &Model, sheet: ElementId, info: &SheetInfo) -> Option<String> {
    let s = sheet_of(model, sheet)?;
    let views = placed_views(model, sheet, &mut |v| view_drawing(model, v));
    let mut svg = SvgCanvas::new(DVec2::new(s.width, s.height));
    draw_sheet(&mut svg, model, sheet, &views, info);
    Some(svg.finish())
}

/// All sheets as one PDF, a page per sheet in sheet-number order.
pub fn sheets_pdf(model: &Model, info: &SheetInfo) -> Vec<u8> {
    let mut pdf = PdfWriter::new();
    for (id, s) in sheets(model) {
        let views = placed_views(model, id, &mut |v| view_drawing(model, v));
        let mut page = pdf.page(DVec2::new(s.width, s.height));
        draw_sheet(&mut page, model, id, &views, info);
    }
    pdf.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::sample::sample_house;

    #[test]
    fn sample_sheets_place_their_views_inside_the_border() {
        let m = sample_house();
        let all = sheets(&m);
        let numbers: Vec<&str> = all.iter().map(|(_, s)| s.number.as_str()).collect();
        assert_eq!(numbers, ["A001", "A101", "A201"]);
        for (id, s) in all {
            let views = placed_views(&m, id, &mut |v| view_drawing(&m, v));
            assert!(!views.is_empty());
            for v in &views {
                let (lo, hi) = v.bounds;
                assert!(
                    lo.x >= BINDING
                        && lo.y >= MARGIN
                        && hi.x <= s.width - MARGIN
                        && hi.y <= s.height - MARGIN,
                    "{} on {} spills off the sheet: {lo} {hi}",
                    v.title,
                    s.number
                );
            }
        }
    }

    #[test]
    fn free_spot_avoids_views_already_placed() {
        let mut doc = creabim_core::Document::from_model(sample_house());
        let plan = |name: &str, m: &Model| m.iter().find(|e| e.name == name).unwrap().id;
        let (g, f) = (
            plan("Ground Floor plan", doc.model()),
            plan("First Floor plan", doc.model()),
        );
        let sheet = creabim_core::Element::new("Empty", ElementKind::Sheet(Sheet::a("X1", "A1")));
        let sid = sheet.id;
        let vp = new_viewport(doc.model(), sid, g, DVec2::new(200.0, 400.0));
        doc.commit(
            creabim_core::Transaction::new("t")
                .insert(sheet)
                .insert(creabim_core::Element::new("vp", ElementKind::Viewport(vp))),
        )
        .unwrap();
        let at = free_spot(doc.model(), sid, f);
        let placed = &placed_views(doc.model(), sid, &mut |v| view_drawing(doc.model(), v))[0];
        let (lo, hi) = placed.bounds;
        // the new plan's own half width keeps it clear of the first
        let half = 90.0;
        assert!(
            at.x - half > hi.x || at.x + half < lo.x || at.y + 70.0 < lo.y || at.y - 70.0 > hi.y,
            "{at} overlaps the plan at {lo} {hi}"
        );
    }

    #[test]
    fn pdf_has_a_page_per_sheet() {
        let m = sample_house();
        let pdf = sheets_pdf(
            &m,
            &SheetInfo {
                project: "Sample house".into(),
                date: "2026-10-10".into(),
            },
        );
        let s = String::from_utf8(pdf).unwrap();
        assert!(s.contains("/Count 3"));
        assert!(s.contains("(Floor Plans) Tj"));
        assert!(s.contains("(Ground Floor plan) Tj"));
    }

    #[test]
    fn viewports_follow_model_edits() {
        let mut doc = creabim_core::Document::from_model(sample_house());
        let (sheet, _) = sheets(doc.model())[1];
        let before = placed_views(doc.model(), sheet, &mut |v| view_drawing(doc.model(), v));
        let wall = doc
            .model()
            .iter()
            .find(|e| e.name == "GF Wall 1")
            .unwrap()
            .id;
        doc.edit(wall, "Thicken", |e| {
            if let ElementKind::Wall(w) = &mut e.kind {
                w.thickness = 0.5;
            }
        })
        .unwrap();
        let after = placed_views(doc.model(), sheet, &mut |v| view_drawing(doc.model(), v));
        assert_ne!(before[0].drawing, after[0].drawing);
        // the viewport stays pinned: same mapping
        assert_eq!(before[0].map, after[0].map);
    }
}
