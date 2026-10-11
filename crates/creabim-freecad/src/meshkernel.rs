//! FreeCAD's binary mesh kernel format (`MeshKernel.bms`, `Mesh::Feature`).
//!
//! Layout (little endian): magic `0xA0B0C0D0`, version `0x00010000`, a
//! 256-byte info string, point and facet counts, points as three `f32`,
//! facets as three point indices plus three neighbour indices (`u32`), then
//! the bounding box.

use crate::FcMesh;

pub fn read(data: &[u8]) -> Result<FcMesh, String> {
    let u32_at = |o: usize| -> Option<u32> {
        data.get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let f32_at = |o: usize| u32_at(o).map(f32::from_bits);
    let magic = u32_at(0).ok_or("mesh file too short")?;
    let version = u32_at(4).ok_or("mesh file too short")?;
    if magic != 0xA0B0C0D0 || version != 0x0001_0000 {
        return Err("unsupported (pre-0.13 or big-endian) mesh kernel format".into());
    }
    let mut o = 8 + 256;
    let np = u32_at(o).ok_or("mesh file truncated")? as usize;
    let nf = u32_at(o + 4).ok_or("mesh file truncated")? as usize;
    o += 8;
    let need = np
        .checked_mul(12)
        .and_then(|p| nf.checked_mul(24).map(|f| p + f))
        .ok_or("mesh counts overflow")?;
    if data.len() < o + need {
        return Err("mesh file truncated".into());
    }
    let mut vertices = Vec::with_capacity(np);
    for i in 0..np {
        let b = o + i * 12;
        let p = [f32_at(b), f32_at(b + 4), f32_at(b + 8)];
        vertices.push(p.map(|c| c.unwrap_or(0.0) as f64));
    }
    o += np * 12;
    let mut faces = Vec::with_capacity(nf);
    for i in 0..nf {
        let b = o + i * 24;
        let f = [u32_at(b), u32_at(b + 4), u32_at(b + 8)].map(|v| v.unwrap_or(u32::MAX));
        if f.iter().all(|&v| (v as usize) < np) {
            faces.push(f.to_vec());
        }
    }
    Ok(FcMesh { vertices, faces })
}
