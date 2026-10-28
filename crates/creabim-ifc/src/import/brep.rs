//! Native shapes in meshes: a faceted B-rep, a polygonal or triangulated
//! face set (what FreeCAD and many other tools write instead of extrusions)
//! that is really a straight wall, a flat slab, a column or a beam.
//!
//! Each test says why a shape is not one, so the import can tell the user
//! which element stayed a mesh and what about it did not fit.

use crate::tess::{area2, bounds, inside, newell, weld};
use creabim_core::EditMesh;
use glam::{DVec2, DVec3};
use std::collections::HashMap;

/// How far a corner may stray from the plane it should be on (0.1 mm).
const TOL: f64 = 1e-4;

/// A straight wall found in a mesh, in world coordinates, with the
/// rectangular holes through it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WallShape {
    pub start: DVec2,
    pub end: DVec2,
    pub thickness: f64,
    pub z0: f64,
    pub z1: f64,
    pub voids: Vec<Void>,
}

/// A rectangular hole through a wall: from `u0` to `u1` along it, measured
/// from its start, and from `z0` to `z1` in world heights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Void {
    pub u0: f64,
    pub u1: f64,
    pub z0: f64,
    pub z1: f64,
}

impl Void {
    /// How much of this hole a rectangle covers, 0..1.
    pub fn covered_by(&self, u0: f64, u1: f64, z0: f64, z1: f64) -> f64 {
        let du = (self.u1.min(u1) - self.u0.max(u0)).max(0.0);
        let dz = (self.z1.min(z1) - self.z0.max(z0)).max(0.0);
        du * dz / ((self.u1 - self.u0) * (self.z1 - self.z0)).max(1e-12)
    }
}

/// A flat slab (or any upright prism) found in a mesh: its outline, counter-
/// clockwise, and its holes, between two heights.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SlabShape {
    pub outline: Vec<DVec2>,
    pub holes: Vec<Vec<DVec2>>,
    pub z0: f64,
    pub z1: f64,
}

fn welded(mesh: &EditMesh) -> Result<EditMesh, String> {
    let mut m = mesh.clone();
    weld(&mut m, 1e-6);
    if m.faces.is_empty() {
        return Err("has no faces CreaBIM can read".into());
    }
    Ok(m)
}

fn face_normals(m: &EditMesh) -> Vec<DVec3> {
    m.faces
        .iter()
        .map(|f| {
            let pts: Vec<DVec3> = f.iter().map(|&v| m.vertices[v as usize]).collect();
            newell(&pts).normalize_or_zero()
        })
        .collect()
}

/// Faces that are neither upright nor level.
fn sloped_faces(normals: &[DVec3]) -> usize {
    normals
        .iter()
        .filter(|n| n.z.abs() > 1e-4 && n.z.abs() < 1.0 - 1e-4)
        .count()
}

/// A shape's volume should match what its native element would hold.
fn volume_matches(m: &EditMesh, expected: f64) -> Result<(), String> {
    let v = m.volume().abs();
    if (v - expected).abs() > 0.01 * expected.max(1e-9) + 1e-9 {
        return Err(format!(
            "has a volume of {v:.3} m³ where its outline holds {expected:.3} m³ (it has recesses or is not closed)"
        ));
    }
    Ok(())
}

/// The direction of the longest level edge of a mesh.
fn longest_level_edge(m: &EditMesh) -> Option<DVec2> {
    let mut best = (0.0, DVec2::ZERO);
    for f in &m.faces {
        for i in 0..f.len() {
            let (a, b) = (
                m.vertices[f[i] as usize],
                m.vertices[f[(i + 1) % f.len()] as usize],
            );
            let d = (b - a).truncate();
            if (b.z - a.z).abs() < TOL && d.length() > best.0 {
                best = (d.length(), d / d.length());
            }
        }
    }
    (best.0 > 1e-3).then_some(best.1)
}

/// A straight wall in a mesh: every corner on one of two upright planes
/// parallel to the wall, a level bottom and top, and only rectangular
/// holes through it (cut doors and windows). With `axis`, the wall runs
/// that way and ends where the axis does, as for an extruded wall.
pub(crate) fn wall_of_mesh(
    mesh: &EditMesh,
    axis: Option<(DVec2, DVec2)>,
) -> Result<WallShape, String> {
    let m = welded(mesh)?;
    let (lo, hi) = bounds(m.vertices.iter().copied()).ok_or("has no corners")?;
    let (z0, z1) = (lo.z, hi.z);
    if z1 - z0 < 1e-3 {
        return Err("is flat".into());
    }
    let normals = face_normals(&m);
    let sloped = sloped_faces(&normals);
    if sloped > 0 {
        return Err(format!(
            "has {sloped} sloped faces (a raked or gable top, or a battered face)"
        ));
    }
    let d = match axis {
        Some((a, b)) if a.distance(b) > 1e-6 => (b - a).normalize(),
        _ => longest_level_edge(&m).ok_or("has no level edges")?,
    };
    let pts: Vec<DVec2> = m.vertices.iter().map(|v| v.truncate()).collect();
    let (start, end, thickness) = crate::import::wall_from_points(&pts, pts[0], d, axis)
        .ok_or_else(|| {
            format!(
                "is not a straight wall: its {} corners do not all lie on two parallel faces",
                pts.len()
            )
        })?;
    let d = (end - start).normalize_or_zero();
    let n = d.perp();
    let half = thickness / 2.0;
    // the holes, from the wall's face on its left
    let on_face = |v: DVec3| ((v.truncate() - start).dot(n) - half).abs() < 1e-3;
    let uz = |v: DVec3| DVec2::new((v.truncate() - start).dot(d), v.z);
    let faces: Vec<Vec<DVec2>> = m
        .faces
        .iter()
        .zip(&normals)
        .filter(|(f, nm)| {
            nm.dot(n.extend(0.0)).abs() > 1.0 - 1e-4
                && f.iter().all(|&v| on_face(m.vertices[v as usize]))
        })
        .map(|(f, _)| f.iter().map(|&v| uz(m.vertices[v as usize])).collect())
        .collect();
    if faces.is_empty() {
        return Err("has no flat face along its length".into());
    }
    let voids = holes_in_face(&faces, z0, z1)?;
    // its plan: the two faces' extents (square or mitred ends)
    let (mut a, mut b) = (
        (f64::INFINITY, f64::NEG_INFINITY),
        (f64::INFINITY, f64::NEG_INFINITY),
    );
    for v in &m.vertices {
        let (u, s) = ((v.truncate() - start).dot(d), (v.truncate() - start).dot(n));
        let r = if s > 0.0 { &mut a } else { &mut b };
        *r = (r.0.min(u), r.1.max(u));
    }
    let plan = ((a.1 - a.0) + (b.1 - b.0)) / 2.0 * thickness;
    let cut: f64 = voids.iter().map(|v| (v.u1 - v.u0) * (v.z1 - v.z0)).sum();
    volume_matches(&m, plan * (z1 - z0) - cut * thickness)?;
    Ok(WallShape {
        start,
        end,
        thickness,
        z0,
        z1,
        voids,
    })
}

/// The rectangular holes in a wall's face, given as polygons in (along,
/// up) coordinates: the face is cut into cells on its corners' lines, and
/// the cells no polygon covers make up the holes.
fn holes_in_face(faces: &[Vec<DVec2>], z0: f64, z1: f64) -> Result<Vec<Void>, String> {
    let lines = |pick: fn(&DVec2) -> f64, extra: &[f64]| {
        let mut v: Vec<f64> = faces
            .iter()
            .flatten()
            .map(pick)
            .chain(extra.iter().copied())
            .collect();
        v.sort_by(f64::total_cmp);
        v.dedup_by(|a, b| (*a - *b).abs() < TOL);
        v
    };
    let us = lines(|p| p.x, &[]);
    let zs = lines(|p| p.y, &[z0, z1]);
    if us.len() < 2 || zs.len() < 2 || us.len() * zs.len() > 250_000 {
        return Err("has a face too finely cut to read".into());
    }
    let (nu, nz) = (us.len() - 1, zs.len() - 1);
    let mut empty = vec![false; nu * nz];
    for i in 0..nu {
        for j in 0..nz {
            let c = DVec2::new((us[i] + us[i + 1]) / 2.0, (zs[j] + zs[j + 1]) / 2.0);
            empty[j * nu + i] = !faces.iter().any(|f| inside(c, f));
        }
    }
    // each group of empty cells must be a rectangle clear of the edges
    let mut seen = vec![false; nu * nz];
    let mut out = vec![];
    for start in 0..nu * nz {
        if !empty[start] || seen[start] {
            continue;
        }
        let (mut i0, mut i1, mut j0, mut j1) = (nu, 0, nz, 0);
        let mut stack = vec![start];
        let mut count = 0;
        seen[start] = true;
        while let Some(k) = stack.pop() {
            count += 1;
            let (i, j) = (k % nu, k / nu);
            (i0, i1, j0, j1) = (i0.min(i), i1.max(i), j0.min(j), j1.max(j));
            let mut push = |k: usize| {
                if empty[k] && !seen[k] {
                    seen[k] = true;
                    stack.push(k);
                }
            };
            if i > 0 {
                push(k - 1);
            }
            if i + 1 < nu {
                push(k + 1);
            }
            if j > 0 {
                push(k - nu);
            }
            if j + 1 < nz {
                push(k + nu);
            }
        }
        if count != (i1 - i0 + 1) * (j1 - j0 + 1) {
            return Err("has a hole that is not rectangular".into());
        }
        if i0 == 0 || i1 == nu - 1 {
            return Err("has a notch at its end".into());
        }
        if j1 == nz - 1 {
            return Err("has a stepped or notched top".into());
        }
        out.push(Void {
            u0: us[i0],
            u1: us[i1 + 1],
            z0: zs[j0],
            z1: zs[j1 + 1],
        });
    }
    out.sort_by(|a, b| a.u0.total_cmp(&b.u0).then(a.z0.total_cmp(&b.z0)));
    Ok(out)
}

/// An upright prism in a mesh: every corner at its bottom or its top,
/// upright sides, and a top made of one outline with any holes in it.
pub(crate) fn slab_of_mesh(mesh: &EditMesh) -> Result<SlabShape, String> {
    let m = welded(mesh)?;
    let (lo, hi) = bounds(m.vertices.iter().copied()).ok_or("has no corners")?;
    let (z0, z1) = (lo.z, hi.z);
    if z1 - z0 < 1e-4 {
        return Err("is flat (no thickness)".into());
    }
    let between = m
        .vertices
        .iter()
        .filter(|v| (v.z - z0).abs() > TOL && (v.z - z1).abs() > TOL)
        .count();
    if between > 0 {
        return Err(format!(
            "is not an upright prism: {between} of its corners lie between its top and bottom (sloped or stepped)"
        ));
    }
    let normals = face_normals(&m);
    let sloped = sloped_faces(&normals);
    if sloped > 0 {
        return Err(format!(
            "has {sloped} faces that are neither level nor upright"
        ));
    }
    // the outline of the top faces: their edges no other top face shares
    let mut edges: HashMap<(u32, u32), i32> = HashMap::new();
    for (f, nm) in m.faces.iter().zip(&normals) {
        if nm.z > 0.5
            && f.iter()
                .all(|&v| (m.vertices[v as usize].z - z1).abs() <= TOL)
        {
            for i in 0..f.len() {
                let (a, b) = (f[i], f[(i + 1) % f.len()]);
                if edges.remove(&(b, a)).is_none() {
                    *edges.entry((a, b)).or_insert(0) += 1;
                }
            }
        }
    }
    let mut next: HashMap<u32, u32> = HashMap::new();
    for &(a, b) in edges.keys() {
        if next.insert(a, b).is_some() {
            return Err("has a top whose outline touches itself".into());
        }
    }
    if next.len() < 3 {
        return Err("has no level top face".into());
    }
    let mut loops: Vec<Vec<DVec2>> = vec![];
    let mut keys: Vec<u32> = next.keys().copied().collect();
    keys.sort_unstable();
    let mut used = std::collections::HashSet::new();
    for s in keys {
        if used.contains(&s) {
            continue;
        }
        let mut lp = vec![];
        let mut cur = s;
        loop {
            if !used.insert(cur) {
                return Err("has a top whose outline does not close".into());
            }
            lp.push(m.vertices[cur as usize].truncate());
            match next.get(&cur) {
                Some(&n) if n == s => break,
                Some(&n) => cur = n,
                None => return Err("has a top whose outline does not close".into()),
            }
        }
        loops.push(remove_collinear(&lp));
    }
    loops.sort_by(|a, b| area2(b).abs().total_cmp(&area2(a).abs()));
    let mut outline = loops.remove(0);
    if area2(&outline) < 0.0 {
        outline.reverse();
    }
    let mut holes = vec![];
    let mut pieces = 1;
    for mut h in loops {
        if inside(h[0], &outline) && area2(&h) < 0.0 {
            h.reverse();
            holes.push(h);
        } else {
            pieces += 1;
        }
    }
    if pieces > 1 {
        return Err(format!("is made of {pieces} separate pieces"));
    }
    let area = area2(&outline) / 2.0 - holes.iter().map(|h| area2(h) / 2.0).sum::<f64>();
    volume_matches(&m, area * (z1 - z0))?;
    Ok(SlabShape {
        outline,
        holes,
        z0,
        z1,
    })
}

/// A beam in a mesh: a straight bar lying level, with a rectangular
/// section, as its start, end, width and depth, and its top height.
pub(crate) fn beam_of_mesh(mesh: &EditMesh) -> Result<(DVec2, DVec2, f64, f64, f64), String> {
    let w = wall_of_mesh(mesh, None)?;
    if !w.voids.is_empty() {
        return Err("has holes through it".into());
    }
    Ok((w.start, w.end, w.thickness, w.z1 - w.z0, w.z1))
}

/// A column's section from its outline: a rectangle (width, depth, turn in
/// degrees) or a circle (diameter), about its centre.
pub(crate) fn column_section(
    outline: &[DVec2],
) -> Result<(DVec2, creabim_core::Profile, f64), String> {
    use creabim_core::Profile;
    let n = outline.len();
    let c = outline.iter().copied().sum::<DVec2>() / n as f64;
    if n == 4 {
        let e: Vec<DVec2> = (0..4).map(|i| outline[(i + 1) % 4] - outline[i]).collect();
        let square = (0..4).all(|i| {
            e[i].normalize_or_zero()
                .dot(e[(i + 1) % 4].normalize_or_zero())
                .abs()
                < 1e-4
        });
        if square {
            let rotation = e[0].y.atan2(e[0].x).to_degrees();
            let profile = Profile::Rectangle {
                width: e[0].length(),
                depth: e[1].length(),
            };
            return Ok((c, profile, rotation));
        }
    }
    if n >= 8 {
        let r: Vec<f64> = outline.iter().map(|p| p.distance(c)).collect();
        let mean = r.iter().sum::<f64>() / n as f64;
        if r.iter().all(|x| (x - mean).abs() < 0.01 * mean) {
            return Ok((
                c,
                Profile::Round {
                    diameter: 2.0 * mean,
                },
                0.0,
            ));
        }
    }
    Err(format!(
        "has a section of {n} corners that is neither a rectangle nor a circle"
    ))
}

/// Drop corners that lie on the line through their neighbours.
pub(crate) fn remove_collinear(poly: &[DVec2]) -> Vec<DVec2> {
    let n = poly.len();
    let keep: Vec<DVec2> = (0..n)
        .filter(|&i| {
            let (a, b, c) = (poly[(i + n - 1) % n], poly[i], poly[(i + 1) % n]);
            (b - a).perp_dot(c - b).abs() > 1e-9 * (1.0 + (c - a).length_squared())
        })
        .map(|i| poly[i])
        .collect();
    if keep.len() >= 3 {
        keep
    } else {
        poly.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An upright prism between two heights: `outer` counter-clockwise,
    /// `holes` clockwise.
    fn prism(outer: &[DVec2], holes: &[Vec<DVec2>], z0: f64, z1: f64) -> EditMesh {
        let mut m = EditMesh::default();
        add_prism(&mut m, outer, holes, z0, z1);
        m
    }

    fn add_prism(m: &mut EditMesh, outer: &[DVec2], holes: &[Vec<DVec2>], z0: f64, z1: f64) {
        let at = |p: &DVec2, z: f64| p.extend(z);
        let top: Vec<DVec3> = outer.iter().map(|p| at(p, z1)).collect();
        let mut bottom: Vec<DVec3> = outer.iter().map(|p| at(p, z0)).collect();
        bottom.reverse();
        let lift = |h: &Vec<DVec2>, z: f64| h.iter().map(|p| at(p, z)).collect::<Vec<_>>();
        let th: Vec<Vec<DVec3>> = holes.iter().map(|h| lift(h, z1)).collect();
        let bh: Vec<Vec<DVec3>> = holes.iter().map(|h| lift(h, z0)).collect();
        crate::tess::add_face(m, &top, &th);
        crate::tess::add_face(m, &bottom, &bh);
        for lp in std::iter::once(outer).chain(holes.iter().map(Vec::as_slice)) {
            for i in 0..lp.len() {
                let (a, b) = (lp[i], lp[(i + 1) % lp.len()]);
                crate::tess::add_face(m, &[at(&a, z0), at(&b, z0), at(&b, z1), at(&a, z1)], &[]);
            }
        }
    }

    /// A box as six quads, outward.
    fn cuboid(lo: DVec3, hi: DVec3) -> EditMesh {
        let mut m = EditMesh::cuboid(hi - lo);
        let shift = DVec3::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, lo.z);
        for v in &mut m.vertices {
            *v += shift;
        }
        m
    }

    /// A wall 5 m long, 0.3 thick, 2.8 high along x, with a window hole
    /// 1..2 along at 0.9..2.1 and a door hole 3..4 from the floor, built
    /// from boxes the way an exporter writes a cut wall.
    fn cut_wall() -> EditMesh {
        let (y0, y1) = (-0.15, 0.15);
        let mut m = EditMesh::default();
        let mut add = |lo: DVec3, hi: DVec3| {
            let b = cuboid(lo, hi);
            let base = m.vertices.len() as u32;
            m.vertices.extend(b.vertices);
            m.faces.extend(
                b.faces
                    .into_iter()
                    .map(|f| f.into_iter().map(|v| v + base).collect()),
            );
        };
        let p = |x, y, z| DVec3::new(x, y, z);
        add(p(0.0, y0, 0.0), p(1.0, y1, 2.8));
        add(p(1.0, y0, 0.0), p(2.0, y1, 0.9));
        add(p(1.0, y0, 2.1), p(2.0, y1, 2.8));
        add(p(2.0, y0, 0.0), p(3.0, y1, 2.8));
        add(p(3.0, y0, 2.2), p(4.0, y1, 2.8));
        add(p(4.0, y0, 0.0), p(5.0, y1, 2.8));
        m
    }

    #[test]
    fn a_cut_wall_brep_is_a_wall_with_its_holes() {
        // the boxes' shared faces cancel in volume but not as faces, so
        // merge them the way the import does first
        let mut m = cut_wall();
        weld(&mut m, 1e-6);
        let w = wall_of_mesh(&m, None).unwrap_or_else(|e| panic!("{e}"));
        assert!((w.thickness - 0.3).abs() < 1e-9);
        assert!((w.z1 - w.z0 - 2.8).abs() < 1e-9);
        assert!((w.start.distance(w.end) - 5.0).abs() < 1e-9);
        assert_eq!(w.voids.len(), 2, "{:?}", w.voids);
        let flip = w.start.x > w.end.x;
        let along = |x: f64| if flip { 5.0 - x } else { x };
        let window = w.voids.iter().find(|v| v.z0 > 0.5).unwrap();
        assert!((window.z0 - 0.9).abs() < 1e-9 && (window.z1 - 2.1).abs() < 1e-9);
        assert!((window.u0.min(window.u1) - along(1.0).min(along(2.0))).abs() < 1e-9);
        let door = w.voids.iter().find(|v| v.z0 < 0.5).unwrap();
        assert!((door.z1 - 2.2).abs() < 1e-9);
    }

    #[test]
    fn shapes_that_are_not_walls_say_why() {
        // a wedge: sloped top
        let mut m = cuboid(DVec3::ZERO, DVec3::new(4.0, 0.2, 3.0));
        for v in &mut m.vertices {
            if v.z > 1.0 && v.x > 1.0 {
                v.z = 2.0;
            }
        }
        let e = wall_of_mesh(&m, None).unwrap_err();
        assert!(e.contains("sloped"), "{e}");
        // an L in plan
        let l = [
            DVec2::new(0.0, 0.0),
            DVec2::new(3.0, 0.0),
            DVec2::new(3.0, 3.0),
            DVec2::new(2.8, 3.0),
            DVec2::new(2.8, 0.2),
            DVec2::new(0.0, 0.2),
        ];
        let mut m = prism(&l, &[], 0.0, 2.5);
        weld(&mut m, 1e-6);
        let e = wall_of_mesh(&m, None).unwrap_err();
        assert!(e.contains("not a straight wall"), "{e}");
        // but it is a slab-like prism
        let s = slab_of_mesh(&m).unwrap();
        assert_eq!(s.outline.len(), 6);
    }

    #[test]
    fn a_slab_brep_keeps_its_hole() {
        let outer = [
            DVec2::new(0.0, 0.0),
            DVec2::new(6.0, 0.0),
            DVec2::new(6.0, 4.0),
            DVec2::new(0.0, 4.0),
        ];
        let hole = [
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 2.0),
            DVec2::new(2.0, 2.0),
            DVec2::new(2.0, 1.0),
        ];
        let m = prism(&outer, &[hole.to_vec()], 2.75, 3.0);
        let s = slab_of_mesh(&m).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(s.holes.len(), 1);
        assert!((s.z1 - 3.0).abs() < 1e-12 && (s.z0 - 2.75).abs() < 1e-12);
        assert!((area2(&s.outline) / 2.0 - 24.0).abs() < 1e-9);
        // two pieces are not one slab
        let mut two = m.clone();
        add_prism(
            &mut two,
            &outer.map(|p| p + DVec2::new(10.0, 0.0)),
            &[],
            2.75,
            3.0,
        );
        let e = slab_of_mesh(&two).unwrap_err();
        assert!(e.contains("2 separate pieces"), "{e}");
    }

    #[test]
    fn column_sections() {
        let sq = creabim_core::Profile::Rectangle {
            width: 0.3,
            depth: 0.5,
        }
        .outline(DVec2::new(1.0, 2.0), 30.0);
        let (c, p, r) = column_section(&sq).unwrap();
        assert!(c.distance(DVec2::new(1.0, 2.0)) < 1e-9);
        assert!((r - 30.0).abs() < 1e-6, "{r}");
        assert!(
            matches!(p, creabim_core::Profile::Rectangle { width, depth } if (width - 0.3).abs() < 1e-9 && (depth - 0.5).abs() < 1e-9)
        );
        let round = creabim_core::Profile::Round { diameter: 0.4 }.outline(DVec2::ZERO, 0.0);
        let (_, p, _) = column_section(&round).unwrap();
        assert!(
            matches!(p, creabim_core::Profile::Round { diameter } if (diameter - 0.4).abs() < 1e-9)
        );
    }
}
