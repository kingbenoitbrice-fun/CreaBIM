//! IFC export for CreaBIM.
//!
//! The model's elements already use IFC classes, so export is close to a
//! direct write. The output is an IFC4 STEP physical file (ISO 10303-21).

mod export;
mod step;

pub use export::{export_ifc4, ExportOptions};
