//! OpenCascade BREP support: text parser, tessellator and polyhedral writer.

pub mod geom;
pub mod parse;
pub mod tess;
pub mod tokens;
pub mod uvmesh;
pub mod write;

/// Detects OpenCascade's binary BREP ("BinBRep"), which FreeCAD writes
/// instead of text when the `SaveBinaryBrep` preference is on.
pub fn is_binary(data: &[u8]) -> bool {
    let head = &data[..data.len().min(64)];
    head.windows(20).any(|w| w == b"Open CASCADE Topolog")
}
