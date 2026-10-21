//! Sheet templates: a named title block (and, if it has one, its own
//! border) that any number of sheets share. A template's linework and text
//! are in paper millimetres measured from one corner of the sheet, so the
//! same template fits every paper size. Text can hold fields in braces,
//! `{sheet}`, `{number}`, `{project}`, `{scale}`, `{date}` or a field of
//! the template's own, filled in per sheet.
//!
//! Templates are elements like text styles: saved, undone and listed in
//! the model. A sheet without one draws the built-in title block.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use glam::DVec2;

/// The sheet corner a template's coordinates are measured from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Corner {
    BottomLeft,
    /// Where title blocks usually sit.
    #[default]
    BottomRight,
    TopLeft,
    TopRight,
}

impl Corner {
    pub const ALL: [Corner; 4] = [
        Corner::BottomLeft,
        Corner::BottomRight,
        Corner::TopLeft,
        Corner::TopRight,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Corner::BottomLeft => "Bottom left",
            Corner::BottomRight => "Bottom right",
            Corner::TopLeft => "Top left",
            Corner::TopRight => "Top right",
        }
    }

    /// The corner of a `width` × `height` sheet, mm from its lower left.
    pub fn at(self, width: f64, height: f64) -> DVec2 {
        match self {
            Corner::BottomLeft => DVec2::ZERO,
            Corner::BottomRight => DVec2::new(width, 0.0),
            Corner::TopLeft => DVec2::new(0.0, height),
            Corner::TopRight => DVec2::new(width, height),
        }
    }
}

/// How a piece of title block text lines up with its point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextAlign {
    #[default]
    Start,
    Middle,
    End,
}

/// One piece of a title block, in paper mm from the template's corner.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TitleItem {
    /// An open polyline (closed when its last point is its first).
    Line {
        points: Vec<DVec2>,
        /// Pen width, mm.
        width: f64,
    },
    /// A filled polygon; 0 is black, 1 white.
    Fill { points: Vec<DVec2>, gray: f64 },
    /// Text centred vertically on `at`; may hold `{field}`s.
    Text {
        at: DVec2,
        /// Letter height, mm.
        size: f64,
        text: String,
        #[serde(default)]
        align: TextAlign,
        /// Radians counter-clockwise.
        #[serde(default)]
        angle: f64,
    },
}

/// A named, reusable title block. The element's name is the template's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SheetTemplate {
    #[serde(default)]
    pub corner: Corner,
    pub items: Vec<TitleItem>,
    /// Draw the usual border inside the sheet's margins as well. Off for
    /// a template that brings its own frame.
    #[serde(default = "yes")]
    pub border: bool,
    /// Fields of its own (client, drawn by, revision…) and the value a
    /// sheet shows until it is given one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<(String, String)>,
    /// The paper size new sheets made from it get, mm; none keeps A3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paper: Option<DVec2>,
}

fn yes() -> bool {
    true
}

/// The fields every sheet fills in itself.
pub const BUILT_IN_FIELDS: [(&str, &str); 5] = [
    ("sheet", "The sheet's title"),
    ("number", "The sheet number"),
    ("project", "The project name"),
    ("scale", "The scale of its views"),
    ("date", "Today's date"),
];

impl SheetTemplate {
    pub fn validate(&self) -> Result<(), String> {
        let finite = |p: &DVec2| p.is_finite();
        for it in &self.items {
            let ok = match it {
                TitleItem::Line { points, width } => {
                    points.iter().all(finite) && width.is_finite() && *width >= 0.0
                }
                TitleItem::Fill { points, gray } => {
                    points.iter().all(finite) && (0.0..=1.0).contains(gray)
                }
                TitleItem::Text {
                    at, size, angle, ..
                } => at.is_finite() && size.is_finite() && *size > 0.0 && angle.is_finite(),
            };
            if !ok {
                return Err("title block items must be finite, with positive text sizes".into());
            }
        }
        if self
            .paper
            .is_some_and(|p| !(p.is_finite() && p.min_element() > 10.0))
        {
            return Err("a template's paper must be over 10 mm each way".into());
        }
        Ok(())
    }

    /// The corners of everything in it, from its corner, mm.
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        let mut pts = vec![];
        for it in &self.items {
            match it {
                TitleItem::Line { points, .. } | TitleItem::Fill { points, .. } => {
                    pts.extend(points.iter().copied())
                }
                TitleItem::Text { at, size, .. } => pts.push(*at + DVec2::Y * size * 0.5),
            }
        }
        let first = *pts.first()?;
        Some(
            pts.iter()
                .fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))),
        )
    }

    /// Move everything so its outline's far corner sits on the template's
    /// corner: imported drawings come at any position, and a title block
    /// belongs in its corner.
    pub fn fit_to_corner(&mut self) {
        let Some((lo, hi)) = self.bounds() else {
            return;
        };
        let target = match self.corner {
            Corner::BottomLeft => lo,
            Corner::BottomRight => DVec2::new(hi.x, lo.y),
            Corner::TopLeft => DVec2::new(lo.x, hi.y),
            Corner::TopRight => hi,
        };
        let shift = -target;
        for it in &mut self.items {
            match it {
                TitleItem::Line { points, .. } | TitleItem::Fill { points, .. } => {
                    points.iter_mut().for_each(|p| *p += shift)
                }
                TitleItem::Text { at, .. } => *at += shift,
            }
        }
    }

    /// The names of the fields its text uses, in order, once each.
    pub fn used_fields(&self) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for it in &self.items {
            if let TitleItem::Text { text, .. } = it {
                for f in fields_in(text) {
                    if !out.contains(&f) {
                        out.push(f);
                    }
                }
            }
        }
        out
    }
}

/// The `{name}`s in `text`.
pub fn fields_in(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut rest = text;
    while let Some(i) = rest.find('{') {
        let after = &rest[i + 1..];
        let Some(j) = after.find('}') else { break };
        let name = after[..j].trim();
        if !name.is_empty() && !name.contains('{') {
            out.push(name.to_string());
        }
        rest = &after[j + 1..];
    }
    out
}

/// `text` with each `{name}` replaced by its value; unknown fields are
/// left as written so a missing value shows.
pub fn fill_fields(text: &str, values: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('}') {
            Some(j) => {
                let name = after[..j].trim();
                match values.get(name) {
                    Some(v) => out.push_str(v),
                    None => {
                        out.push('{');
                        out.push_str(&after[..=j]);
                    }
                }
                rest = &after[j + 1..];
            }
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Element, ElementKind, Sheet, Transaction};

    fn block() -> SheetTemplate {
        SheetTemplate {
            corner: Corner::BottomRight,
            items: vec![
                TitleItem::Line {
                    points: vec![
                        DVec2::new(-120.0, 10.0),
                        DVec2::new(-10.0, 10.0),
                        DVec2::new(-10.0, 40.0),
                        DVec2::new(-120.0, 40.0),
                        DVec2::new(-120.0, 10.0),
                    ],
                    width: 0.35,
                },
                TitleItem::Text {
                    at: DVec2::new(-115.0, 30.0),
                    size: 5.0,
                    text: "{sheet} for {client}".into(),
                    align: TextAlign::Start,
                    angle: 0.0,
                },
            ],
            border: true,
            fields: vec![("client".into(), "Client name".into())],
            paper: None,
        }
    }

    #[test]
    fn fields_fill_in_and_unknown_ones_show() {
        let mut v = BTreeMap::new();
        v.insert("sheet".to_string(), "Ground floor".to_string());
        assert_eq!(
            fill_fields("{sheet} for {client}", &v),
            "Ground floor for {client}"
        );
        v.insert("client".to_string(), "Ana".to_string());
        assert_eq!(
            fill_fields("{ sheet } / {client}", &v),
            "Ground floor / Ana"
        );
        assert_eq!(fill_fields("a { b", &v), "a { b");
        assert_eq!(block().used_fields(), vec!["sheet", "client"]);
    }

    #[test]
    fn templates_round_trip_and_sheets_from_older_files_have_none() {
        let mut doc = Document::new();
        let t = Element::new("Office A3", ElementKind::SheetTemplate(block()));
        let tid = t.id;
        let mut sheet = Sheet::a("A101", "A3");
        sheet.template = Some(tid);
        sheet.fields.insert("client".into(), "Ana".into());
        let s = Element::new("Plans", ElementKind::Sheet(sheet.clone()));
        doc.commit(Transaction::new("Template").insert(t).insert(s))
            .unwrap();
        let text = serde_json::to_string(doc.model()).unwrap();
        let back: crate::Model = serde_json::from_str(&text).unwrap();
        assert_eq!(
            back.get(tid).unwrap().kind,
            ElementKind::SheetTemplate(block())
        );
        // a sheet from before templates reads with none, and writes none
        let old: Sheet =
            serde_json::from_str(r#"{"number":"A1","width":420.0,"height":297.0}"#).unwrap();
        assert_eq!(old, Sheet::a("A1", "A3"));
        let text = serde_json::to_string(&old).unwrap();
        assert!(
            !text.contains("template") && !text.contains("fields"),
            "{text}"
        );
        // a template without a border flag draws the border
        let t: SheetTemplate = serde_json::from_str(r#"{"items":[]}"#).unwrap();
        assert!(t.border);
    }

    #[test]
    fn imported_blocks_move_into_their_corner() {
        let mut t = block();
        t.fit_to_corner();
        let (lo, hi) = t.bounds().unwrap();
        assert!(hi.x.abs() < 1e-9 && lo.y.abs() < 1e-9, "{lo} {hi}");
        let mut bad = block();
        bad.items.push(TitleItem::Text {
            at: DVec2::ZERO,
            size: 0.0,
            text: "x".into(),
            align: TextAlign::Start,
            angle: 0.0,
        });
        assert!(bad.validate().is_err());
    }
}
