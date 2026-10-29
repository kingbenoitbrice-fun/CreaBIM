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
    /// A spot level: the height of `a` (a slab's top in a plan, or the
    /// plan's own level: a level marker), written at `b` from the
    /// dimension's pattern, such as "+0.45 FFL".
    Spot,
}

impl DimensionKind {
    pub const ALL: [DimensionKind; 6] = [
        DimensionKind::Linear,
        DimensionKind::Angular,
        DimensionKind::Radius,
        DimensionKind::Diameter,
        DimensionKind::ArcLength,
        DimensionKind::Spot,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DimensionKind::Linear => "Linear",
            DimensionKind::Angular => "Angle",
            DimensionKind::Radius => "Radius",
            DimensionKind::Diameter => "Diameter",
            DimensionKind::ArcLength => "Arc length",
            DimensionKind::Spot => "Spot level",
        }
    }

    /// Does this kind measure about a centre point?
    pub fn has_center(self) -> bool {
        !matches!(self, DimensionKind::Linear | DimensionKind::Spot)
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

/// The mark at the end of a dimension line or a leader.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Terminator {
    /// A short heavy stroke at 45°: the architectural tick.
    #[default]
    Tick,
    /// A thin 45° slash.
    Slash,
    /// A solid arrowhead.
    Arrow,
    /// A slender solid arrowhead.
    ArrowSlim,
    /// An open arrowhead: two strokes.
    ArrowOpen,
    /// An arrowhead drawn in outline.
    ArrowClosed,
    /// A solid dot.
    Dot,
    /// A small circle.
    DotOpen,
    /// A small solid square.
    Square,
    /// A tick with a dot on the point.
    TickDot,
    /// Nothing: the line just stops.
    None,
}

impl Terminator {
    pub const ALL: [Terminator; 11] = [
        Terminator::Tick,
        Terminator::Slash,
        Terminator::Arrow,
        Terminator::ArrowSlim,
        Terminator::ArrowOpen,
        Terminator::ArrowClosed,
        Terminator::Dot,
        Terminator::DotOpen,
        Terminator::Square,
        Terminator::TickDot,
        Terminator::None,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Terminator::Tick => "Architectural tick",
            Terminator::Slash => "Thin slash",
            Terminator::Arrow => "Arrow, filled",
            Terminator::ArrowSlim => "Arrow, slender",
            Terminator::ArrowOpen => "Arrow, open",
            Terminator::ArrowClosed => "Arrow, outline",
            Terminator::Dot => "Dot",
            Terminator::DotOpen => "Open dot",
            Terminator::Square => "Square",
            Terminator::TickDot => "Tick and dot",
            Terminator::None => "None",
        }
    }
}

/// The outline of a tag's bubble.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TagShape {
    /// Round for doors and windows, a box for the rest: the look tags
    /// had before shapes could be picked.
    #[default]
    Auto,
    Circle,
    Box,
    RoundedBox,
    /// A box with round ends.
    Pill,
    Hexagon,
    Octagon,
    Diamond,
    /// Just the text, underlined.
    Underline,
    /// Just the text.
    TextOnly,
}

impl TagShape {
    pub const ALL: [TagShape; 10] = [
        TagShape::Auto,
        TagShape::Circle,
        TagShape::Box,
        TagShape::RoundedBox,
        TagShape::Pill,
        TagShape::Hexagon,
        TagShape::Octagon,
        TagShape::Diamond,
        TagShape::Underline,
        TagShape::TextOnly,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TagShape::Auto => "Auto (round for openings)",
            TagShape::Circle => "Circle",
            TagShape::Box => "Box",
            TagShape::RoundedBox => "Rounded box",
            TagShape::Pill => "Pill",
            TagShape::Hexagon => "Hexagon",
            TagShape::Octagon => "Octagon",
            TagShape::Diamond => "Diamond",
            TagShape::Underline => "Underlined text",
            TagShape::TextOnly => "Text only",
        }
    }

    pub fn is_auto(&self) -> bool {
        *self == TagShape::Auto
    }
}

/// What kind of annotation a style is meant for. It sorts the style
/// manager and the pickers; any style can still be used by anything.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StyleUse {
    #[default]
    Any,
    Dimensions,
    Notes,
    Tags,
}

impl StyleUse {
    pub const ALL: [StyleUse; 4] = [
        StyleUse::Any,
        StyleUse::Dimensions,
        StyleUse::Notes,
        StyleUse::Tags,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StyleUse::Any => "Any annotation",
            StyleUse::Dimensions => "Dimensions",
            StyleUse::Notes => "Notes and leaders",
            StyleUse::Tags => "Tags",
        }
    }

    pub fn is_any(&self) -> bool {
        *self == StyleUse::Any
    }
}

/// Which way a linear dimension measures: along the line between its
/// points, or only across the drawing's X or Y (the projected distance).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DimAxis {
    #[default]
    Aligned,
    /// The distance along X, with a horizontal dimension line.
    Horizontal,
    /// The distance along Y, with a vertical dimension line.
    Vertical,
}

impl DimAxis {
    pub const ALL: [DimAxis; 3] = [DimAxis::Aligned, DimAxis::Horizontal, DimAxis::Vertical];

    pub fn label(self) -> &'static str {
        match self {
            DimAxis::Aligned => "Aligned",
            DimAxis::Horizontal => "Horizontal",
            DimAxis::Vertical => "Vertical",
        }
    }

    pub fn is_aligned(&self) -> bool {
        *self == DimAxis::Aligned
    }

    /// The direction its line runs for points `a` to `b`, pointing from
    /// `a` towards `b`; none when they coincide along it.
    pub fn direction(self, a: DVec2, b: DVec2) -> Option<DVec2> {
        let d = b - a;
        let u = match self {
            DimAxis::Aligned => d,
            DimAxis::Horizontal => DVec2::new(d.x, 0.0),
            DimAxis::Vertical => DVec2::new(0.0, d.y),
        };
        (u.length() > 1e-9).then(|| u.normalize())
    }

    /// The axis a dimension line placed at `at` reads as, for points `a`
    /// and `b`, the way most CAD programs pick it: past both points to the
    /// left or right it measures up the drawing, above or below both it
    /// measures across, and between them it is aligned. Two points already
    /// level or plumb measure that way.
    pub fn pick(a: DVec2, b: DVec2, at: DVec2) -> DimAxis {
        let d = b - a;
        if d.y.abs() < 1e-9 {
            return DimAxis::Horizontal;
        }
        if d.x.abs() < 1e-9 {
            return DimAxis::Vertical;
        }
        let (lo, hi) = (a.min(b), a.max(b));
        let beside = at.x < lo.x || at.x > hi.x;
        let over = at.y < lo.y || at.y > hi.y;
        match (beside, over) {
            (true, false) => DimAxis::Vertical,
            (false, true) => DimAxis::Horizontal,
            _ => DimAxis::Aligned,
        }
    }
}

/// How annotation text (and its linework) looks on paper. The element's
/// name is the style's name. Beyond the text it sets the ends of
/// dimension lines and leaders and the shape of tags, so one style serves
/// as a dimension style, a note style or a tag style.
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
    /// The ends of dimension lines; none is the tick of linear dimensions
    /// and the arrow of angles, radii and arcs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminator: Option<Terminator>,
    /// The end of a note's leader; none is a filled arrow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leader_end: Option<Terminator>,
    /// Size of those ends on paper, mm; none is the usual 2.5 mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_size: Option<f64>,
    #[serde(default, skip_serializing_if = "TagShape::is_auto")]
    pub tag_shape: TagShape,
    #[serde(default, skip_serializing_if = "StyleUse::is_any")]
    pub purpose: StyleUse,
    /// Dimension lines in this style leave a gap where a tag or a note
    /// sits across them.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub break_at_tags: bool,
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
            terminator: None,
            leader_end: None,
            end_size: None,
            tag_shape: TagShape::Auto,
            purpose: StyleUse::Any,
            break_at_tags: false,
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
        if self
            .end_size
            .is_some_and(|v| !(v.is_finite() && v > 0.1 && v < 50.0))
        {
            return Err("terminator size must be between 0.1 and 50 mm".into());
        }
        Ok(())
    }
}

/// The pattern a spot level is written with when it has none of its own.
pub const SPOT_PATTERN: &str = "{elevation}";

/// A spot level's text: `pattern` with `{elevation}` (signed, in the
/// project's units), `{m}` (signed metres to two places, "+0.45"), `{m3}`
/// (to three), `{mm}` (signed millimetres) and `{level}` (the name of the
/// level at or below it) filled in for the height `z`.
pub fn spot_text(model: &Model, pattern: &str, z: f64) -> String {
    let signed = |v: f64, places: usize| {
        let t = format!("{:.*}", places, v.abs());
        if t.trim_start_matches(['0', '.']).is_empty() {
            format!("±{t}")
        } else if v < 0.0 {
            format!("-{t}")
        } else {
            format!("+{t}")
        }
    };
    let level = model
        .levels()
        .filter(|(_, l)| l.elevation <= z + 1e-6)
        .max_by(|a, b| a.1.elevation.total_cmp(&b.1.elevation))
        .map(|(e, _)| e.name.clone())
        .unwrap_or_default();
    pattern
        .replace("{elevation}", &model.units().elevation(z))
        .replace("{m3}", &signed(z, 3))
        .replace("{m}", &signed(z, 2))
        .replace("{mm}", &signed(z * 1000.0, 0))
        .replace("{level}", &level)
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
    /// A bend in its leader, in the same units as `at`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elbow: Option<DVec2>,
    /// The mark at the leader's end; none is its style's (an arrow unless
    /// the style says otherwise).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leader_end: Option<Terminator>,
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
            elbow: None,
            leader_end: None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let leader_ok = self
            .leader
            .is_none_or(|p| p.local.is_finite() && p.body.is_none_or(|b| b.is_finite()));
        if self.at.is_finite() && leader_ok && self.elbow.is_none_or(|p| p.is_finite()) {
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
        assert_eq!(d.axis, DimAxis::Aligned);
        // and a linear dimension still writes nothing new
        let text = serde_json::to_string(&d).unwrap();
        for field in ["kind", "center", "style", "color", "axis"] {
            assert!(!text.contains(field), "{field} in {text}");
        }
        // a newer kind round-trips
        let mut r = d.clone();
        r.kind = DimensionKind::Radius;
        r.center = Some(HostPoint::free(DVec2::ONE));
        r.color = Some([1.0, 0.0, 0.0]);
        let back: Dimension = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(back, r);
        // so does a horizontal one
        let mut h = d.clone();
        h.axis = DimAxis::Horizontal;
        let back: Dimension = serde_json::from_str(&serde_json::to_string(&h).unwrap()).unwrap();
        assert_eq!(back.axis, DimAxis::Horizontal);
    }

    #[test]
    fn styles_from_older_files_read_with_the_old_ends_and_shapes() {
        let old = r#"{"font":"Serif","size":3.5,"bold":true}"#;
        let s: TextStyle = serde_json::from_str(old).unwrap();
        assert_eq!((s.terminator, s.leader_end, s.end_size), (None, None, None));
        assert_eq!((s.tag_shape, s.purpose), (TagShape::Auto, StyleUse::Any));
        // a default style writes none of the new fields
        let text = serde_json::to_string(&TextStyle::default()).unwrap();
        for field in [
            "terminator",
            "leader_end",
            "end_size",
            "tag_shape",
            "purpose",
        ] {
            assert!(!text.contains(field), "{field} in {text}");
        }
        let dots = TextStyle {
            terminator: Some(Terminator::Dot),
            leader_end: Some(Terminator::ArrowOpen),
            end_size: Some(1.5),
            tag_shape: TagShape::Hexagon,
            purpose: StyleUse::Tags,
            ..TextStyle::default()
        };
        let back: TextStyle = serde_json::from_str(&serde_json::to_string(&dots).unwrap()).unwrap();
        assert_eq!(back, dots);
        assert!(TextStyle {
            end_size: Some(0.0),
            ..dots
        }
        .validate()
        .is_err());
    }

    #[test]
    fn dimension_axes_are_picked_from_where_the_line_goes() {
        let (a, b) = (DVec2::ZERO, DVec2::new(4.0, 1.0));
        // above both points: across; beside them: up; between: aligned
        assert_eq!(
            DimAxis::pick(a, b, DVec2::new(2.0, 3.0)),
            DimAxis::Horizontal
        );
        assert_eq!(DimAxis::pick(a, b, DVec2::new(6.0, 0.5)), DimAxis::Vertical);
        assert_eq!(DimAxis::pick(a, b, DVec2::new(2.0, 0.6)), DimAxis::Aligned);
        assert_eq!(
            DimAxis::Horizontal.direction(a, b),
            Some(DVec2::X),
            "points the way a goes to b"
        );
        assert_eq!(DimAxis::Vertical.direction(b, a), Some(-DVec2::Y));
        assert_eq!(DimAxis::Vertical.direction(a, DVec2::X), None);
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
                ..TextStyle::default()
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
        // a view can leave notes out, like dimensions and tags
        let mut hidden = crate::visibility::ViewHidden::default();
        assert!(!hidden.hidden_set(doc.model()).contains(&id));
        hidden.set_category(crate::visibility::Category::Notes, true);
        assert!(hidden.hidden_set(doc.model()).contains(&id));
    }
}
