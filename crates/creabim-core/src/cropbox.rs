//! Crop boxes: Revit's section boxes. A crop box belongs to a 3D view (or
//! to the 3D window itself) and cuts away everything outside it, so the
//! inside of a building shows. It crops every element in the view, or only
//! the ones picked for it; the rest are left whole. A view can have as many
//! as wanted: an element several of them crop keeps only what is inside
//! all of them.
//!
//! Crop boxes are elements like any other, so they are saved, undone,
//! renamed and deleted the usual way. They are documentation: nothing in
//! the building changes and they are never exported.

use crate::{Element, ElementId, ElementKind, Model};
use glam::DVec3;
use serde::{Deserialize, Serialize};

/// What a crop box cuts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum CropScope {
    /// Everything the view shows.
    #[default]
    All,
    /// Only these elements (and the doors and windows in them); everything
    /// else shows whole. Not hard references: a deleted element just drops
    /// out of the list.
    Only(Vec<ElementId>),
}

/// An axis-aligned box, in model metres, that crops a 3D view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CropBox {
    /// The 3D view it crops; `None` for the 3D window's own crop boxes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<ElementId>,
    pub min: DVec3,
    pub max: DVec3,
    #[serde(default, skip_serializing_if = "is_all")]
    pub scope: CropScope,
    /// Switched off, the box keeps its place but crops nothing.
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    pub enabled: bool,
    /// Hide the box's frame while it still crops; it shows again while
    /// the box is selected, to edit it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub frame_hidden: bool,
}

fn yes() -> bool {
    true
}

fn is_yes(v: &bool) -> bool {
    *v
}

fn is_all(s: &CropScope) -> bool {
    *s == CropScope::All
}

/// The smallest a crop box gets along any side, metres.
pub const MIN_SIZE: f64 = 0.05;

impl CropBox {
    pub fn new(view: Option<ElementId>, min: DVec3, max: DVec3) -> Self {
        Self {
            view,
            min: min.min(max),
            max: min.max(max),
            scope: CropScope::All,
            enabled: true,
            frame_hidden: false,
        }
    }

    pub fn size(&self) -> DVec3 {
        self.max - self.min
    }

    pub fn center(&self) -> DVec3 {
        (self.min + self.max) / 2.0
    }

    /// Does this box crop element `id`? `hosts` maps a door or window to
    /// its wall, so picking a wall takes its openings along.
    pub fn crops(&self, id: ElementId, host: Option<ElementId>) -> bool {
        match &self.scope {
            CropScope::All => true,
            CropScope::Only(ids) => ids.contains(&id) || host.is_some_and(|h| ids.contains(&h)),
        }
    }

    /// The middle of face `i` (0 −X, 1 +X, 2 −Y, 3 +Y, 4 −Z, 5 +Z) and its
    /// outward direction.
    pub fn face(&self, i: usize) -> (DVec3, DVec3) {
        let axis = i / 2;
        let mut n = DVec3::ZERO;
        n[axis] = if i % 2 == 0 { -1.0 } else { 1.0 };
        let mut at = self.center();
        at[axis] = if i % 2 == 0 {
            self.min[axis]
        } else {
            self.max[axis]
        };
        (at, n)
    }

    /// Move face `i` to `value` along its axis, keeping the box at least
    /// [`MIN_SIZE`] across.
    pub fn set_face(&mut self, i: usize, value: f64) {
        let axis = i / 2;
        if i % 2 == 0 {
            self.min[axis] = value.min(self.max[axis] - MIN_SIZE);
        } else {
            self.max[axis] = value.max(self.min[axis] + MIN_SIZE);
        }
    }

    /// The twelve edges of the box.
    pub fn edges(&self) -> [(DVec3, DVec3); 12] {
        let c = |x: usize, y: usize, z: usize| {
            DVec3::new(
                [self.min.x, self.max.x][x],
                [self.min.y, self.max.y][y],
                [self.min.z, self.max.z][z],
            )
        };
        [
            (c(0, 0, 0), c(1, 0, 0)),
            (c(0, 1, 0), c(1, 1, 0)),
            (c(0, 0, 1), c(1, 0, 1)),
            (c(0, 1, 1), c(1, 1, 1)),
            (c(0, 0, 0), c(0, 1, 0)),
            (c(1, 0, 0), c(1, 1, 0)),
            (c(0, 0, 1), c(0, 1, 1)),
            (c(1, 0, 1), c(1, 1, 1)),
            (c(0, 0, 0), c(0, 0, 1)),
            (c(1, 0, 0), c(1, 0, 1)),
            (c(0, 1, 0), c(0, 1, 1)),
            (c(1, 1, 0), c(1, 1, 1)),
        ]
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(self.min.is_finite() && self.max.is_finite()) {
            return Err("crop box corners must be finite".into());
        }
        if !(self.max - self.min)
            .cmpge(DVec3::splat(MIN_SIZE * 0.999))
            .all()
        {
            return Err("a crop box needs some size along every side".into());
        }
        Ok(())
    }
}

/// The crop boxes of a 3D view (`None`: the 3D window's own), in model
/// order, switched off ones too.
pub fn crop_boxes(model: &Model, view: Option<ElementId>) -> Vec<(&Element, &CropBox)> {
    model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::CropBox(c) if c.view == view => Some((e, c)),
            _ => None,
        })
        .collect()
}

/// A name for the next crop box of `view`: "Crop box 1", "Crop box 2"...
pub fn next_name(model: &Model, view: Option<ElementId>) -> String {
    let taken: Vec<&str> = crop_boxes(model, view)
        .into_iter()
        .map(|(e, _)| e.name.as_str())
        .collect();
    (1..)
        .map(|i| format!("Crop box {i}"))
        .find(|n| !taken.contains(&n.as_str()))
        .expect("an unused number")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Transaction, View};

    #[test]
    fn a_crop_box_with_only_its_corners_reads_with_the_defaults() {
        let text = r#"{"min":[0.0,0.0,0.0],"max":[4.0,3.0,2.5]}"#;
        let c: CropBox = serde_json::from_str(text).unwrap();
        assert_eq!(c.view, None);
        assert_eq!(c.scope, CropScope::All);
        assert!(c.enabled && !c.frame_hidden);
        // the defaults are not written out, a changed box round-trips
        let plain = serde_json::to_string(&c).unwrap();
        assert!(!plain.contains("enabled") && !plain.contains("scope"));
        let mut c = c;
        c.enabled = false;
        c.frame_hidden = true;
        c.scope = CropScope::Only(vec![ElementId::new()]);
        let text = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<CropBox>(&text).unwrap(), c);
    }

    #[test]
    fn crop_boxes_save_undo_and_go_with_their_view() {
        let mut doc = Document::new();
        let view = Element::new("3D", ElementKind::View(View::axonometric(30.0, 30.0)));
        let view_id = view.id;
        doc.commit(Transaction::new("view").insert(view)).unwrap();
        let name = next_name(doc.model(), Some(view_id));
        assert_eq!(name, "Crop box 1");
        let b = Element::new(
            name,
            ElementKind::CropBox(CropBox::new(Some(view_id), DVec3::ONE, DVec3::ZERO)),
        );
        doc.commit(Transaction::new("Add crop box").insert(b))
            .unwrap();
        assert_eq!(crop_boxes(doc.model(), Some(view_id)).len(), 1);
        assert_eq!(crop_boxes(doc.model(), None).len(), 0);
        assert_eq!(next_name(doc.model(), Some(view_id)), "Crop box 2");
        // the corners were put in order
        let (_, c) = crop_boxes(doc.model(), Some(view_id))[0];
        assert_eq!((c.min, c.max), (DVec3::ZERO, DVec3::ONE));
        // saved and read back
        let back = Model::from_json(&doc.model().to_json()).unwrap();
        assert_eq!(&back, doc.model());
        assert!(doc.undo());
        assert!(crop_boxes(doc.model(), Some(view_id)).is_empty());
        assert!(doc.redo());
        // deleting the view takes its crop boxes with it
        let tx = Transaction::new("Delete view").remove_all_cascade(doc.model(), &[view_id]);
        doc.commit(tx).unwrap();
        assert!(doc
            .model()
            .iter()
            .all(|e| !matches!(e.kind, ElementKind::CropBox(_))));
    }

    #[test]
    fn faces_move_but_never_turn_the_box_inside_out() {
        let mut c = CropBox::new(None, DVec3::ZERO, DVec3::splat(2.0));
        c.set_face(5, 3.0);
        assert_eq!(c.max.z, 3.0);
        c.set_face(1, -5.0);
        assert!((c.max.x - MIN_SIZE).abs() < 1e-12);
        assert!(c.validate().is_ok());
        let (at, n) = c.face(2);
        assert_eq!(n, DVec3::NEG_Y);
        assert_eq!(at.y, 0.0);
        let wall = ElementId::new();
        let door = ElementId::new();
        c.scope = CropScope::Only(vec![wall]);
        assert!(c.crops(wall, None) && c.crops(door, Some(wall)));
        assert!(!c.crops(ElementId::new(), None));
        // the Move tool moves it; turning it is refused
        let e = Element::new("Crop box 1", ElementKind::CropBox(c.clone()));
        let model = Model::new();
        let moved = crate::transform::transformed(&model, &e, &crate::Xform::Move(DVec3::X))
            .expect("a crop box moves");
        let ElementKind::CropBox(m) = moved.kind else {
            unreachable!()
        };
        assert_eq!(m.min, c.min + DVec3::X);
        assert!((m.size() - c.size()).abs().max_element() < 1e-12);
        let turn = crate::Xform::Rotate {
            center: glam::DVec2::ZERO,
            degrees: 30.0,
        };
        assert!(crate::transform::transformed(&model, &e, &turn).is_none());
    }
}
