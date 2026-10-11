//! CreaBIM model core: the element store, edit operations and undo history.
//!
//! Every view, drawing and export reads from a [`Model`]. Edits never mutate
//! the model directly; they are expressed as a [`Transaction`] of [`Op`]s and
//! committed through a [`Document`], which records them for undo and redo.

pub mod csg;
pub mod document;
pub mod editmesh;
pub mod element;
pub mod id;
pub mod joins;
pub mod marks;
pub mod model;
pub mod modifiers;
pub mod ops;
pub mod polygon;
pub mod sample;
pub mod transform;

pub use csg::BoolOp;
pub use document::{Document, Step, StepId};
pub use editmesh::EditMesh;
pub use element::{
    Cut, Dimension, Element, ElementKind, FreeMesh, Grid, Guide, Hatch, HostPoint, Level, Material,
    Opening, OpeningKind, ScheduleCategory, Sheet, Sketch, Slab, Source, Tag, View, ViewKind,
    ViewStyle, Viewport, Wall, Workplane,
};
pub use id::ElementId;
pub use joins::{wall_joins, WallEnds};
pub use marks::{marks, material_of};
pub use model::Model;
pub use modifiers::{Axis, Modifier, ModifierKind};
pub use ops::{Op, Transaction};
pub use transform::Xform;

/// Errors raised when an edit would leave the model inconsistent.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ModelError {
    #[error("element {0} already exists")]
    Duplicate(ElementId),
    #[error("element {0} not found")]
    NotFound(ElementId),
    #[error("element {element} references missing element {missing}")]
    DanglingReference {
        element: ElementId,
        missing: ElementId,
    },
    #[error("element {element} is still referenced by {by}")]
    StillReferenced { element: ElementId, by: ElementId },
    #[error("invalid element {0}: {1}")]
    Invalid(ElementId, String),
}
