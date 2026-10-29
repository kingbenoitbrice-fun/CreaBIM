//! CreaBIM model core: the element store, edit operations and undo history.
//!
//! Every view, drawing and export reads from a [`Model`]. Edits never mutate
//! the model directly; they are expressed as a [`Transaction`] of [`Op`]s and
//! committed through a [`Document`], which records them for undo and redo.

pub mod annotation;
pub mod assembly;
pub mod catalogue;
pub mod constraint;
pub mod cropbox;
pub mod csg;
pub mod curtain;
pub mod document;
pub mod editmesh;
pub mod element;
pub mod footing;
pub mod graphics;
pub mod grids;
pub mod id;
pub mod joins;
pub mod levels;
pub mod library;
pub mod marks;
pub mod members;
pub mod model;
pub mod modifiers;
pub mod objects;
pub mod ops;
pub mod polygon;
pub mod pushpull;
pub mod sample;
pub mod sketch;
pub mod slab;
pub mod starter;
pub mod titleblock;
pub mod transform;
pub mod types;
pub mod units;
pub mod visibility;
pub mod walls;

pub use annotation::{
    text_style, DimAxis, DimensionKind, Font, Note, StyleUse, TagShape, Terminator, TextStyle,
};
pub use assembly::{assembly_of, Assembly, AssemblyUse, Layer, LayerFunction};
pub use constraint::{Constraint, SolveError};
pub use cropbox::{CropBox, CropScope};
pub use csg::BoolOp;
pub use curtain::{CurtainGrid, GridRule, PanelKind};
pub use document::{Document, Step, StepId};
pub use editmesh::EditMesh;
pub use element::{
    Cut, Dimension, Element, ElementKind, FreeMesh, Grid, GridGroup, Guide, Hatch, HostPoint,
    Level, LevelHead, Material, Opening, OpeningFrame, OpeningKind, ScheduleCategory, Sheet,
    Sketch, Slab, Source, Tag, Underlay, View, ViewKind, ViewStyle, Viewport, Wall, Workplane,
};
pub use element::{LocationLine, WallTop};
pub use footing::{Footing, FootingKind};
pub use graphics::{LineWeights, PlanGraphics};
pub use id::ElementId;
pub use joins::{wall_joins, Gap, WallEnds};
pub use marks::{marks, material_of, MarkClass, MarkOrder, MarkRule, MarkRules};
pub use members::{Beam, Column, Landing, Profile, Railing, Roof, RoofShape, Stair, StairSide};
pub use model::Model;
pub use modifiers::{Axis, Modifier, ModifierKind};
pub use objects::{CatalogueObject, Object, ObjectCategory, ObjectType};
pub use ops::{Op, Transaction};
pub use sketch::{bulge_through, Circle, Curve, SegmentKind};
pub use slab::{room_outline, slab_from_sketch, Shaft};
pub use titleblock::SheetTemplate;
pub use transform::Xform;
pub use types::{ElementType, TypeCategory, TypeParams, TypeRef};
pub use units::{LengthUnit, Units};
pub use visibility::{Category, ViewHidden};

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
