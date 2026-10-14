//! Annotation data: the kinds of dimension, text notes with leaders, and
//! named text styles that dimensions, tags and notes refer to.
//!
//! Styles are elements like materials, so they are saved, undone and listed
//! in the model. A missing or unset style falls back to [`TextStyle::default`],
//! which is how annotations looked before styles existed.

use crate::{element::HostPoint, ElementId, ElementKind, Model};
use glam::DVec2;
use serde::{Deserialize, Serialize};

/// What a [`crate::Dimension`] measures. Older files have no kind and read
/// as [`DimensionKind::Linear`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DimensionKind {
    /// The distance from `a` to `b`, its line `offset` to the left.
    #[default]
    Linear,
    /// The angle at `center` turning counter-clockwise from `a` to `b`, its
    /// arc `offset` from the vertex.
    Angular,
    /// The radius of the circle about `center` through `a`; its line runs
    /// on `offset` past the circle.
    Radius,
    /// The diameter of the circle about `center` through `a`.
    Diameter,
    /// The length of the arc about `center` from `a` counter-clockwise to
    /// `b`, its line `offset` outside the arc.
    ArcLength,
}

impl DimensionKind {
    pub const ALL: [DimensionKind; 5] = [
        DimensionKind::Linear,
        DimensionKind::Angular,
        DimensionKind::Radius,
        DimensionKind::Diameter,
        DimensionKind::ArcLength,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DimensionKind::Linear => "Linear",
            DimensionKind::Angular => "Angle",
            DimensionKind::Radius => "Radius",
            DimensionKind::Diameter => "Diameter",
            DimensionKind::ArcLength => "Arc length",
        }
    }

    /// Does this kind measure about a centre point?
    pub fn has_center(self) -> bool {
        self != DimensionKind::Linear
    }

    pub fn is_linear(&self) -> bool {
        *self == DimensionKind::Linear
    }
}

/// Typeface families every exporter can name: PDF has them built in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Font {
    /// Helvetica or Arial.
    #[default]
    Sans,
    /// Times.
    Serif,
    /// Courier.
    Mono,
}

impl Font {
    pub const ALL: [Font; 3] = [Font::Sans, Font::Serif, Font::Mono];

    pub fn label(self) -> &'static str {
        match self {
            Font::Sans => "Sans (Helvetica)",
            Font::Serif => "Serif (Times)",
            Font::Mono => "Mono (Courier)",
        }
    }
}

/// How annotation text (and its linework) looks on paper. The element's
/// name is the style's name.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    #[serde(default)]
    pub font: Font,
    /// Letter height on paper, mm.
    pub size: f64,
    /// sRGB, 0..1. `None` uses the view's ink: black on paper, the usual
    /// annotation blue on screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 3]>,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
}

/// Letter height of dimensions, tags and notes without a style, paper mm.
pub const DEFAULT_TEXT_SIZE: f64 = 2.5;

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: Font::Sans,
            size: DEFAULT_TEXT_SIZE,
            color: None,
            bold: false,
            italic: false,
        }
    }
}

impl TextStyle {
    pub fn validate(&self) -> Result<(), String> {
        if !(self.size.is_finite() && self.size > 0.1 && self.size < 100.0) {
            return Err(format!(
                "text size must be between 0.1 and 100 mm, got {}",
                self.size
            ));
        }
        if self
            .color
            .is_some_and(|c| c.iter().any(|v| !(0.0..=1.0).contains(v)))
        {
            return Err("text colour components must be within 0..1".into());
        }
        Ok(())
    }
}

/// The style an annotation refers to, or the default when it refers to
/// none (or to one since deleted), with the annotation's own colour on top.
pub fn text_style(model: &Model, style: Option<ElementId>, color: Option<[f32; 3]>) -> TextStyle {
    let mut s = style
        .and_then(|id| match &model.get(id)?.kind {
            ElementKind::TextStyle(s) => Some(*s),
            _ => None,
        })
        .unwrap_or_default();
    if color.is_some() {
        s.color = color;
    }
    s
}

/// A text note in a view (drawing units) or on a sheet (paper mm), with an
/// optional leader ending in an arrow at `leader`. A leader point on an
/// element follows it, like a dimension point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// The view, or the sheet, it is written in.
    pub view: ElementId,
    /// Where the first line of text starts (its left end, mid-height).
    pub at: DVec2,
    /// The text; line breaks start new lines.
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leader: Option<HostPoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<ElementId>,
    /// Overrides the style's colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 3]>,
}

impl Note {
    pub fn new(view: ElementId, at: DVec2, text: impl Into<String>) -> Self {
        Self {
            view,
            at,
            text: text.into(),
            leader: None,
            style: None,
            color: None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let leader_ok = self
            .leader
            .is_none_or(|p| p.local.is_finite() && p.body.is_none_or(|b| b.is_finite()));
        if self.at.is_finite() && leader_ok {
            Ok(())
        } else {
            Err("note position must be finite".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Dimension, Element, Transaction};

    #[test]
    fn linear_dimensions_from_older_files_still_read() {
        let view = ElementId::new();
        let old = format!(
            r#"{{"view":"{}","a":{{"host":null,"local":[0.0,0.0],"from_end":false}},"b":{{"host":null,"local":[3.0,0.0],"from_end":false}},"offset":-1.5}}"#,
            view.0
        );
        let d: Dimension = serde_json::from_str(&old).unwrap();
        assert_eq!(d.kind, DimensionKind::Linear);
        assert_eq!(d.center, None);
        assert_eq!(d.style, None);
        assert_eq!(d.color, None);
        assert_eq!(d.offset, -1.5);
        // and a linear dimension still writes nothing new
        let text = serde_json::to_string(&d).unwrap();
        for field in ["kind", "center", "style", "color"] {
            assert!(!text.contains(field), "{field} in {text}");
        }
        // a newer kind round-trips
        let mut r = d.clone();
        r.kind = DimensionKind::Radius;
        r.center = Some(HostPoint::free(DVec2::ONE));
        r.color = Some([1.0, 0.0, 0.0]);
        let back: Dimension = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn tags_from_older_files_still_read() {
        let (view, host) = (ElementId::new(), ElementId::new());
        let old = format!(
            r#"{{"view":"{}","host":"{}","offset":[1.0,2.0]}}"#,
            view.0, host.0
        );
        let t: crate::Tag = serde_json::from_str(&old).unwrap();
        assert_eq!(t, crate::Tag::new(view, host, DVec2::new(1.0, 2.0)));
    }

    #[test]
    fn styles_resolve_with_fallbacks_and_undo() {
        let mut doc = crate::Document::new();
        let style = Element::new(
            "Titles",
            ElementKind::TextStyle(TextStyle {
                font: Font::Serif,
                size: 5.0,
                color: Some([0.8, 0.1, 0.1]),
                bold: true,
                italic: false,
            }),
        );
        let id = style.id;
        doc.commit(Transaction::new("New text style").insert(style))
            .unwrap();
        let s = text_style(doc.model(), Some(id), None);
        assert_eq!((s.font, s.size, s.bold), (Font::Serif, 5.0, true));
        // an element's colour wins over its style's
        let blue = [0.0, 0.0, 1.0];
        assert_eq!(
            text_style(doc.model(), Some(id), Some(blue)).color,
            Some(blue)
        );
        // no style, or a deleted one, is the old look
        assert_eq!(text_style(doc.model(), None, None), TextStyle::default());
        doc.undo();
        assert_eq!(
            text_style(doc.model(), Some(id), None),
            TextStyle::default()
        );
        doc.redo();
        assert_eq!(text_style(doc.model(), Some(id), None).size, 5.0);
        // sizes are checked
        let bad = Element::new(
            "Bad",
            ElementKind::TextStyle(TextStyle {
                size: 0.0,
                ..TextStyle::default()
            }),
        );
        assert!(doc.commit(Transaction::new("Bad").insert(bad)).is_err());
    }

    #[test]
    fn notes_follow_their_view_and_round_trip() {
        let mut doc = crate::Document::from_model(crate::sample::sample_house());
        let plan = doc
            .model()
            .iter()
            .find(|e| matches!(e.kind, ElementKind::View(_)))
            .unwrap()
            .id;
        let mut note = Note::new(plan, DVec2::new(1.0, 2.0), "Line one\nLine two");
        note.leader = Some(HostPoint::free(DVec2::ZERO));
        let e = Element::new("Note", ElementKind::Note(note.clone()));
        let id = e.id;
        doc.commit(Transaction::new("Add note").insert(e)).unwrap();
        let text = serde_json::to_string(doc.model().get(id).unwrap()).unwrap();
        let back: Element = serde_json::from_str(&text).unwrap();
        assert_eq!(back.kind, ElementKind::Note(note));
        // it depends on its view: deleting the view takes the note with it
        let tx = Transaction::new("Delete view").remove_cascade(doc.model(), plan);
        doc.commit(tx).unwrap();
        assert!(doc.model().get(id).is_none());
        doc.undo();
        assert!(doc.model().get(id).is_some());
    }
}
