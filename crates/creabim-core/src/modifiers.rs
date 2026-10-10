//! Blender-style modifier stack: non-destructive operations evaluated on a
//! free mesh, top to bottom, every time its geometry is needed. The mesh
//! itself stays as modelled, so a modifier can be changed, switched off,
//! reordered or applied later.

use crate::EditMesh;
use glam::DVec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];

    pub fn unit(self) -> DVec3 {
        match self {
            Axis::X => DVec3::X,
            Axis::Y => DVec3::Y,
            Axis::Z => DVec3::Z,
        }
    }

    pub fn index(self) -> usize {
        match self {
            Axis::X => 0,
            Axis::Y => 1,
            Axis::Z => 2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Axis::X => "X",
            Axis::Y => "Y",
            Axis::Z => "Z",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ModifierKind {
    /// `count` copies, each `offset` further on (in the mesh's own axes).
    Array { count: u32, offset: DVec3 },
    /// A mirrored copy across the plane normal to `axis` at `plane` along it.
    /// Vertices on the plane are shared and faces lying in it dropped, so a
    /// half model mirrors into one closed mesh.
    Mirror { axis: Axis, plane: f64 },
    /// Thickness for surfaces: an inner skin `thickness` behind the faces,
    /// joined to them along open edges.
    Solidify { thickness: f64 },
    /// Catmull-Clark smoothing, `levels` times.
    Subdivision { levels: u32 },
}

impl ModifierKind {
    pub fn label(&self) -> &'static str {
        match self {
            ModifierKind::Array { .. } => "Array",
            ModifierKind::Mirror { .. } => "Mirror",
            ModifierKind::Solidify { .. } => "Solidify",
            ModifierKind::Subdivision { .. } => "Subdivision",
        }
    }

    /// The new modifiers a user can add, with sensible starting values.
    pub fn presets() -> [ModifierKind; 4] {
        [
            ModifierKind::Array {
                count: 3,
                offset: DVec3::new(3.0, 0.0, 0.0),
            },
            ModifierKind::Mirror {
                axis: Axis::X,
                plane: 0.0,
            },
            ModifierKind::Solidify { thickness: 0.2 },
            ModifierKind::Subdivision { levels: 1 },
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Modifier {
    pub enabled: bool,
    pub kind: ModifierKind,
}

impl Modifier {
    pub fn new(kind: ModifierKind) -> Self {
        Self {
            enabled: true,
            kind,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match &self.kind {
            ModifierKind::Array { count, offset } => {
                if !(1..=200).contains(count) {
                    return Err(format!("array count must be 1 to 200, got {count}"));
                }
                if !offset.is_finite() {
                    return Err("array offset must be finite".into());
                }
            }
            ModifierKind::Mirror { plane, .. } => {
                if !plane.is_finite() {
                    return Err("mirror plane must be finite".into());
                }
            }
            ModifierKind::Solidify { thickness } => {
                if !thickness.is_finite() || thickness.abs() < 1e-6 {
                    return Err("solidify thickness must not be zero".into());
                }
            }
            ModifierKind::Subdivision { levels } => {
                if *levels > 4 {
                    return Err(format!("subdivision levels must be 0 to 4, got {levels}"));
                }
            }
        }
        Ok(())
    }
}

/// A mesh after its modifiers, with the face of the modelled mesh each face
/// came from (`None` for faces a modifier made up, like solidify's rims).
#[derive(Clone, Debug, Default)]
pub struct Evaluated {
    pub mesh: EditMesh,
    pub source: Vec<Option<usize>>,
}

/// Run the enabled modifiers over `mesh`, top to bottom.
pub fn evaluate(mesh: &EditMesh, stack: &[Modifier]) -> Evaluated {
    let mut out = Evaluated {
        mesh: mesh.clone(),
        source: (0..mesh.faces.len()).map(Some).collect(),
    };
    for m in stack.iter().filter(|m| m.enabled && m.validate().is_ok()) {
        out = match &m.kind {
            ModifierKind::Array { count, offset } => array(&out, *count, *offset),
            ModifierKind::Mirror { axis, plane } => mirror(&out, *axis, *plane),
            ModifierKind::Solidify { thickness } => solidify(&out, *thickness),
            ModifierKind::Subdivision { levels } => {
                let mut e = out;
                for _ in 0..*levels {
                    // Catmull-Clark turns face f into len(f) quads, in order
                    let source = e
                        .mesh
                        .faces
                        .iter()
                        .zip(&e.source)
                        .flat_map(|(f, s)| std::iter::repeat_n(*s, f.len()))
                        .collect();
                    e = Evaluated {
                        mesh: e.mesh.subdivide(),
                        source,
                    };
                }
                e
            }
        };
    }
    out
}

fn array(e: &Evaluated, count: u32, offset: DVec3) -> Evaluated {
    let mut out = Evaluated::default();
    for i in 0..count {
        let base = out.mesh.vertices.len() as u32;
        out.mesh
            .vertices
            .extend(e.mesh.vertices.iter().map(|v| *v + offset * i as f64));
        out.mesh.faces.extend(
            e.mesh
                .faces
                .iter()
                .map(|f| f.iter().map(|v| v + base).collect()),
        );
        out.source.extend_from_slice(&e.source);
    }
    out
}

fn mirror(e: &Evaluated, axis: Axis, plane: f64) -> Evaluated {
    const ON: f64 = 1e-5;
    let k = axis.index();
    let on_plane = |v: &DVec3| (v[k] - plane).abs() < ON;
    let mut out = Evaluated {
        mesh: EditMesh {
            vertices: e.mesh.vertices.clone(),
            faces: vec![],
        },
        source: vec![],
    };
    // vertices on the plane are shared by both halves
    let map: Vec<u32> = e
        .mesh
        .vertices
        .iter()
        .enumerate()
        .map(|(i, v)| {
            if on_plane(v) {
                i as u32
            } else {
                let mut m = *v;
                m[k] = 2.0 * plane - m[k];
                out.mesh.vertices.push(m);
                (out.mesh.vertices.len() - 1) as u32
            }
        })
        .collect();
    // faces lying in the mirror plane would end up inside the result
    let kept: Vec<usize> = (0..e.mesh.faces.len())
        .filter(|&f| {
            !e.mesh.faces[f]
                .iter()
                .all(|v| on_plane(&e.mesh.vertices[*v as usize]))
        })
        .collect();
    for &f in &kept {
        out.mesh.faces.push(e.mesh.faces[f].clone());
        out.source.push(e.source[f]);
    }
    for &f in &kept {
        // a reflection turns faces inside out, so reverse each loop
        let face = e.mesh.faces[f].iter().rev().map(|v| map[*v as usize]);
        out.mesh.faces.push(face.collect());
        out.source.push(e.source[f]);
    }
    out
}

fn solidify(e: &Evaluated, thickness: f64) -> Evaluated {
    let mesh = &e.mesh;
    let nv = mesh.vertices.len();
    let normals: Vec<DVec3> = (0..mesh.faces.len()).map(|f| mesh.face_normal(f)).collect();
    let mut vn = vec![DVec3::ZERO; nv];
    let mut v_faces: Vec<Vec<usize>> = vec![vec![]; nv];
    for (fi, f) in mesh.faces.iter().enumerate() {
        for &v in f {
            vn[v as usize] += normals[fi];
            v_faces[v as usize].push(fi);
        }
    }
    let mut out = e.clone();
    // the inner skin: each vertex pushed back far enough that every face it
    // touches ends up `thickness` behind itself (even thickness at corners)
    for v in 0..nv {
        let n = vn[v].normalize_or_zero();
        let fit = v_faces[v]
            .iter()
            .map(|&f| n.dot(normals[f]))
            .fold(1.0f64, f64::min)
            .max(0.25);
        out.mesh
            .vertices
            .push(mesh.vertices[v] - n * (thickness / fit));
    }
    let inner = |v: u32| v + nv as u32;
    for f in &mesh.faces {
        out.mesh
            .faces
            .push(f.iter().rev().map(|&v| inner(v)).collect());
        out.source.push(None);
    }
    // rims along open edges, where a surface has a boundary
    let mut uses: HashMap<(u32, u32), Vec<(u32, u32)>> = HashMap::new();
    for f in &mesh.faces {
        for i in 0..f.len() {
            let (a, b) = (f[i], f[(i + 1) % f.len()]);
            uses.entry((a.min(b), a.max(b))).or_default().push((a, b));
        }
    }
    let mut open: Vec<(u32, u32)> = uses
        .values()
        .filter(|u| u.len() == 1)
        .map(|u| u[0])
        .collect();
    open.sort_unstable();
    for (a, b) in open {
        out.mesh.faces.push(vec![b, a, inner(a), inner(b)]);
        out.source.push(None);
    }
    if thickness < 0.0 {
        // a negative thickness grows outwards: the skins swap roles
        for f in &mut out.mesh.faces {
            f.reverse();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> EditMesh {
        EditMesh::cuboid(DVec3::ONE)
    }

    #[test]
    fn array_repeats_the_mesh() {
        let e = evaluate(
            &cube(),
            &[Modifier::new(ModifierKind::Array {
                count: 4,
                offset: DVec3::new(2.0, 0.0, 0.0),
            })],
        );
        assert_eq!(e.mesh.faces.len(), 24);
        assert!((e.mesh.volume() - 4.0).abs() < 1e-9);
        assert_eq!(e.source[6], Some(0));
        e.mesh.validate().unwrap();
    }

    #[test]
    fn mirror_welds_a_half_model_into_one_closed_mesh() {
        // a box from x = 0 to 1, mirrored at x = 0, is a closed 2 x 1 x 1 box
        let mut half = cube();
        for v in &mut half.vertices {
            v.x += 0.5;
        }
        let e = evaluate(
            &half,
            &[Modifier::new(ModifierKind::Mirror {
                axis: Axis::X,
                plane: 0.0,
            })],
        );
        // the face on the mirror plane is dropped from both halves
        assert_eq!(e.mesh.faces.len(), 10);
        assert_eq!(e.mesh.vertices.len(), 12);
        assert!((e.mesh.volume() - 2.0).abs() < 1e-9);
        e.mesh.validate().unwrap();
    }

    #[test]
    fn solidify_hollows_a_box_with_even_walls() {
        let big = EditMesh::cuboid(DVec3::new(4.0, 4.0, 4.0));
        let e = evaluate(
            &big,
            &[Modifier::new(ModifierKind::Solidify { thickness: 0.5 })],
        );
        // 4³ minus the 3³ hole left by 0.5 m walls all round
        assert!((e.mesh.volume() - (64.0 - 27.0)).abs() < 1e-6);
        assert_eq!(e.mesh.faces.len(), 12);
    }

    #[test]
    fn solidify_gives_a_surface_thickness_with_rims() {
        let sheet = EditMesh {
            vertices: vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(2.0, 0.0, 0.0),
                DVec3::new(2.0, 3.0, 0.0),
                DVec3::new(0.0, 3.0, 0.0),
            ],
            faces: vec![vec![0, 1, 2, 3]],
        };
        let e = evaluate(
            &sheet,
            &[Modifier::new(ModifierKind::Solidify { thickness: 0.1 })],
        );
        assert_eq!(e.mesh.faces.len(), 6);
        assert!((e.mesh.volume() - 0.6).abs() < 1e-9);
        assert_eq!(e.source, vec![Some(0), None, None, None, None, None]);
    }

    #[test]
    fn subdivision_tracks_where_faces_came_from_and_disabled_modifiers_do_nothing() {
        let mut stack = vec![Modifier::new(ModifierKind::Subdivision { levels: 2 })];
        let e = evaluate(&cube(), &stack);
        assert_eq!(e.mesh.faces.len(), 96);
        assert!(e.source[..16].iter().all(|s| *s == Some(0)));
        stack[0].enabled = false;
        assert_eq!(evaluate(&cube(), &stack).mesh, cube());
    }
}
