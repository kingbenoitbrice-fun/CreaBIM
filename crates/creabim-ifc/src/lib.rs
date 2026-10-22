//! IFC exchange for CreaBIM.
//!
//! - Export: the model's elements already use IFC classes, so export is
//!   close to a direct write of an IFC4 STEP physical file (ISO 10303-21).
//! - Import: IFC2X3, IFC4 and IFC4X3 STEP files are read back into native
//!   levels, walls, slabs, doors and windows where they fit CreaBIM's
//!   patterns, and into free meshes that keep their IFC class otherwise.
//! - IFC5 (experimental): the alpha IFCX JSON format, both ways.

mod classes;
mod export;
mod ifcx;
mod import;
mod params;
mod shape;
mod step;
pub mod step_read;
mod tess;

pub use export::{export_ifc4, ExportOptions};
pub use ifcx::{export_ifcx, import_ifcx};
pub use import::{import_ifc, import_ifc_materials, ImportError, ImportResult};
