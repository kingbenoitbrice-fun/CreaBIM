//! Meshes and plan linework for extrusions, sweeps and voids, and for the
//! site: terrain, hard landscape areas and plants (see
//! `creabim_core::parts` and `creabim_core::site`).

use crate::plan::{Pen, PlanItem};
use crate::scene::{slab_mesh_with_holes, SceneItem, PLAIN_SURFACE};
use crate::triangulate::triangulate;
use crate::TriMesh;
use creabim_core::parts::{Form, Part};
use creabim_core::site::{Pattern, PlantForm, Site, SiteForm};
use creabim_core::{members, Element, ElementId, ElementKind, Model};
use glam::{DVec2, DVec3};
use std::f64::consts::{PI, TAU};

/// Default looks of elements without a material.
const PART_COLOR: [f32; 3] = [0.80, 0.76, 0.70];
const VOID_COLOR: [f32; 3] = [0.85, 0.45, 0.25];
const GROUND_COLOR: [f32; 3] = [0.50, 0.62, 0.38];
const AREA_COLOR: [f32; 3] = [0.74, 0.72, 0.68];
const LEAF_COLOR: [f32; 3] = [0.36, 0.53, 0.29];
const NEEDLE_COLOR: [f32; 3] = [0.22, 0.40, 0.26];
const FROND_COLOR: [f32; 3] = [0.42, 0.60, 0.30];
const TRUNK_COLOR: [f32; 3] = [0.45, 0.33, 0.22];

// ---- parts ----

/// A part's solid, as built.
pub fn part_mesh(model: &Model, p: &Part) -> Option<TriMesh> {
    match &p.form {
        Form::Extrusion {
            outline,
            holes,
            base,
            height,
        } => {
            let z = members::elevation(model, p.level) + base;
            Some(slab_mesh_with_holes(outline, holes, z + height, *height))
        }
        Form::Sweep { profile, .. } => {
            let (path, closed) = p.path(model)?;
            let m = sweep_mesh(profile, &path, closed);
            (!m.is_empty()).then_some(m)
        }
    }
}

/// A closed solid: `profile` (counter-clockwise, `x` to the path's left,
/// `y` up) carried along `path`, mitred at its corners and capped at its
/// ends unless it is `closed`.
pub fn sweep_mesh(profile: &[DVec2], path: &[DVec3], closed: bool) -> TriMesh {
    let mut m = TriMesh::default();
    // drop repeated points, which have no direction
    let mut pts: Vec<DVec3> = vec![];
    for p in path {
        if pts.last().is_none_or(|q: &DVec3| q.distance(*p) > 1e-6) {
            pts.push(*p);
        }
    }
    if closed && pts.len() > 2 && pts[0].distance(pts[pts.len() - 1]) < 1e-6 {
        pts.pop();
    }
    let n = pts.len();
    if n < 2 || profile.len() < 3 {
        return m;
    }
    let segs = if closed { n } else { n - 1 };
    // each segment's direction in plan, and its left
    let dir: Vec<DVec2> = (0..segs)
        .map(|i| (pts[(i + 1) % n] - pts[i]).truncate().normalize_or_zero())
        .collect();
    if dir.iter().all(|d| *d == DVec2::ZERO) {
        return m;
    }
    let left = |d: DVec2| DVec2::new(-d.y, d.x);
    // the across-axis at each point, stretched to keep the profile's width
    // square to both segments at a mitre
    let across: Vec<DVec2> = (0..n)
        .map(|i| {
            let before = if i > 0 {
                Some(dir[i - 1])
            } else if closed {
                Some(dir[segs - 1])
            } else {
                None
            };
            let after = (i < segs).then(|| dir[i]);
            match (before, after) {
                (Some(a), Some(b)) => {
                    let (la, lb) = (left(a), left(b));
                    let mitre = (la + lb).normalize_or(lb);
                    let k = mitre.dot(lb).max(0.25);
                    mitre / k
                }
                (Some(a), None) => left(a),
                (None, Some(b)) => left(b),
                (None, None) => DVec2::X,
            }
        })
        .collect();
    let ring = |i: usize| -> Vec<DVec3> {
        profile
            .iter()
            .map(|q| pts[i] + (across[i] * q.x).extend(q.y))
            .collect()
    };
    let rings: Vec<Vec<DVec3>> = (0..n).map(ring).collect();
    let k = profile.len();
    for s in 0..segs {
        let (a, b) = (&rings[s], &rings[(s + 1) % n]);
        let f = (pts[(s + 1) % n] - pts[s]).normalize_or_zero();
        for j in 0..k {
            let j1 = (j + 1) % k;
            let e = profile[j1] - profile[j];
            // outward: the profile edge's right, in the path's frame
            let l = left(dir[s]).extend(0.0);
            let normal = l * e.y - DVec3::Z * e.x;
            let normal = if normal.length_squared() < 1e-18 {
                f
            } else {
                normal
            };
            m.add_quad([a[j], a[j1], b[j1], b[j]], normal);
        }
    }
    if !closed {
        let tris = triangulate(profile);
        let f0 = (pts[1] - pts[0]).normalize_or_zero();
        let f1 = (pts[n - 1] - pts[n - 2]).normalize_or_zero();
        m.add_polygon(&rings[0], &tris, -f0);
        m.add_polygon(&rings[n - 1], &tris, f1);
    }
    m
}

// ---- site ----

/// The triangles of a terrain's top surface, in model coordinates.
pub fn terrain_top(model: &Model, s: &Site) -> Vec<[DVec3; 3]> {
    let SiteForm::Terrain {
        boundary, points, ..
    } = &s.form
    else {
        return vec![];
    };
    let z = members::elevation(model, s.level);
    let inside: Vec<DVec2> = points
        .iter()
        .map(|p| p.truncate())
        .filter(|p| creabim_core::polygon::contains(boundary, *p))
        .collect();
    let (verts, tris) = refine(boundary, &inside);
    tris.iter()
        .map(|t| t.map(|i| verts[i].extend(z + s.ground_at(verts[i]))))
        .collect()
}

/// A triangulation of `boundary` with `inner` points added as corners,
/// its edges flipped towards a Delaunay triangulation.
fn refine(boundary: &[DVec2], inner: &[DVec2]) -> (Vec<DVec2>, Vec<[usize; 3]>) {
    let mut verts = boundary.to_vec();
    let mut tris = triangulate(boundary);
    let n0 = verts.len();
    let ccw = |t: &[usize; 3], v: &[DVec2]| (v[t[1]] - v[t[0]]).perp_dot(v[t[2]] - v[t[0]]) > 0.0;
    for t in tris.iter_mut() {
        if !ccw(t, &verts) {
            t.swap(1, 2);
        }
    }
    for p in inner {
        let Some(i) = tris.iter().position(|t| {
            let [a, b, c] = t.map(|i| verts[i]);
            (b - a).perp_dot(*p - a) >= 0.0
                && (c - b).perp_dot(*p - b) >= 0.0
                && (a - c).perp_dot(*p - c) >= 0.0
        }) else {
            continue;
        };
        if verts.iter().any(|v| v.distance(*p) < 1e-3) {
            continue;
        }
        let k = verts.len();
        verts.push(*p);
        let [a, b, c] = tris[i];
        tris[i] = [a, b, k];
        tris.push([b, c, k]);
        tris.push([c, a, k]);
    }
    // Lawson flips: an interior edge whose opposite corner falls inside
    // the other triangle's circumcircle is swapped
    let in_circle = |a: DVec2, b: DVec2, c: DVec2, d: DVec2| {
        let (ax, ay) = (a.x - d.x, a.y - d.y);
        let (bx, by) = (b.x - d.x, b.y - d.y);
        let (cx, cy) = (c.x - d.x, c.y - d.y);
        (ax * ax + ay * ay) * (bx * cy - cx * by) - (bx * bx + by * by) * (ax * cy - cx * ay)
            + (cx * cx + cy * cy) * (ax * by - bx * ay)
            > 1e-12
    };
    let boundary_edge =
        |a: usize, b: usize| a < n0 && b < n0 && ((a + 1) % n0 == b || (b + 1) % n0 == a);
    for _ in 0..8 * tris.len().max(1) {
        let mut flipped = false;
        'outer: for i in 0..tris.len() {
            for e in 0..3 {
                let (a, b, c) = (tris[i][e], tris[i][(e + 1) % 3], tris[i][(e + 2) % 3]);
                if boundary_edge(a, b) {
                    continue;
                }
                // the triangle across edge a-b runs b-a
                let Some((j, d)) = tris.iter().enumerate().find_map(|(j, t)| {
                    (j != i)
                        .then(|| (0..3).find(|&f| t[f] == b && t[(f + 1) % 3] == a))
                        .flatten()
                        .map(|f| (j, t[(f + 2) % 3]))
                }) else {
                    continue;
                };
                let (pa, pb, pc, pd) = (verts[a], verts[b], verts[c], verts[d]);
                // only a convex quad can flip
                let convex = (pd - pc).perp_dot(pa - pc) * (pd - pc).perp_dot(pb - pc) < 0.0;
                if convex && in_circle(pa, pb, pc, pd) {
                    tris[i] = [c, a, d];
                    tris[j] = [d, b, c];
                    flipped = true;
                    break 'outer;
                }
            }
        }
        if !flipped {
            break;
        }
    }
    (verts, tris)
}

/// A terrain's solid: its top surface, sides down to a flat underside
/// `thickness` below its lowest point. A flat one leaves room for the
/// hard landscape lying in it.
fn terrain_mesh(model: &Model, s: &Site) -> TriMesh {
    let SiteForm::Terrain {
        boundary,
        top,
        thickness,
        points,
        ..
    } = &s.form
    else {
        return TriMesh::default();
    };
    let z = members::elevation(model, s.level);
    if points.is_empty() {
        let holes = creabim_core::site::ground_holes(model, s);
        return slab_mesh_with_holes(boundary, &holes, z + top, *thickness);
    }
    let tops = terrain_top(model, s);
    let mut m = TriMesh::default();
    for t in &tops {
        m.add_polygon(t, &[[0, 1, 2]], (t[1] - t[0]).cross(t[2] - t[0]));
    }
    let low = tops
        .iter()
        .flatten()
        .map(|p| p.z)
        .fold(f64::INFINITY, f64::min)
        - thickness;
    let tris = triangulate(boundary);
    let under: Vec<DVec3> = boundary.iter().map(|p| p.extend(low)).collect();
    m.add_polygon(&under, &tris, -DVec3::Z);
    let n = boundary.len();
    for i in 0..n {
        let (a, b) = (boundary[i], boundary[(i + 1) % n]);
        let (za, zb) = (z + s.ground_at(a), z + s.ground_at(b));
        let e = (b - a).normalize_or_zero();
        let out = DVec3::new(e.y, -e.x, 0.0);
        m.add_polygon(
            &[a.extend(low), b.extend(low), b.extend(zb), a.extend(za)],
            &[[0, 1, 2], [0, 2, 3]],
            out,
        );
    }
    m
}

/// The parts of a plant: (mesh, default colour, is it the crown).
fn plant_meshes(model: &Model, s: &Site) -> Vec<(TriMesh, [f32; 3], bool)> {
    let SiteForm::Plant {
        position,
        base,
        form,
        canopy,
        height,
    } = &s.form
    else {
        return vec![];
    };
    let z0 = members::elevation(model, s.level) + base;
    let (r, h) = (canopy / 2.0, *height);
    let at = |dz: f64| position.extend(z0 + dz);
    let trunk = |top: f64, radius: f64| {
        (
            crate::scene::slab_mesh(&circle(*position, radius, 10), z0 + top, top),
            TRUNK_COLOR,
            false,
        )
    };
    match form {
        PlantForm::Tree => {
            let stem = (h * 0.4).min(h - 0.3).max(h * 0.2);
            vec![
                trunk(stem + 0.2, (r * 0.08).clamp(0.06, 0.35)),
                (
                    ellipsoid(
                        at(stem + (h - stem) / 2.0),
                        DVec3::new(r, r, (h - stem) / 2.0),
                    ),
                    LEAF_COLOR,
                    true,
                ),
            ]
        }
        PlantForm::Conifer => {
            let stem = h * 0.12;
            vec![
                trunk(stem + 0.1, (r * 0.1).clamp(0.05, 0.3)),
                (cone(at(stem), r, h - stem), NEEDLE_COLOR, true),
            ]
        }
        PlantForm::Palm => {
            let crown = (h - r * 0.4).max(h * 0.6);
            let mut fronds = TriMesh::default();
            let count = 9;
            for i in 0..count {
                let a = TAU * i as f64 / count as f64 + 0.3 * (i % 2) as f64;
                fronds.append(&frond(at(crown), DVec2::from_angle(a), r, r * 0.45));
            }
            vec![
                trunk(crown + 0.05, (r * 0.05).clamp(0.1, 0.25)),
                (fronds, FROND_COLOR, true),
            ]
        }
        PlantForm::Shrub => vec![(
            ellipsoid(at(h / 2.0), DVec3::new(r, r, h / 2.0)),
            LEAF_COLOR,
            true,
        )],
    }
}

fn circle(c: DVec2, r: f64, n: usize) -> Vec<DVec2> {
    (0..n)
        .map(|i| c + DVec2::from_angle(TAU * i as f64 / n as f64) * r)
        .collect()
}

/// A faceted ellipsoid about `c` with semi-axes `r`.
fn ellipsoid(c: DVec3, r: DVec3) -> TriMesh {
    let (rings, segs) = (7, 14);
    let p = |i: usize, j: usize| {
        let lat = -PI / 2.0 + PI * i as f64 / rings as f64;
        let lon = TAU * j as f64 / segs as f64;
        c + DVec3::new(lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()) * r
    };
    let mut m = TriMesh::default();
    for i in 0..rings {
        for j in 0..segs {
            let q = [p(i, j), p(i, j + 1), p(i + 1, j + 1), p(i + 1, j)];
            let mid = (q[0] + q[1] + q[2] + q[3]) / 4.0;
            let normal = ((mid - c) / (r * r)).normalize_or_zero();
            if i == 0 {
                m.add_polygon(&[q[0], q[2], q[3]], &[[0, 1, 2]], normal);
            } else if i == rings - 1 {
                m.add_polygon(&[q[0], q[1], q[2]], &[[0, 1, 2]], normal);
            } else {
                m.add_quad(q, normal);
            }
        }
    }
    m
}

/// An upright cone, its base centred on `c`.
fn cone(c: DVec3, r: f64, h: f64) -> TriMesh {
    let n = 14;
    let ring: Vec<DVec3> = circle(c.truncate(), r, n)
        .into_iter()
        .map(|p| p.extend(c.z))
        .collect();
    let apex = c + DVec3::Z * h;
    let mut m = TriMesh::default();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let out = ((a + b) / 2.0 - c).truncate().extend(r / h.max(1e-6) * r);
        m.add_polygon(&[a, b, apex], &[[0, 1, 2]], out);
    }
    let tris: Vec<[usize; 3]> = (1..n - 1).map(|i| [0, i, i + 1]).collect();
    m.add_polygon(&ring, &tris, -DVec3::Z);
    m
}

/// A palm frond from `at` out along `dir`, `length` long and `droop` lower
/// at its tip: a thin leaf, a little thicker along its rib.
fn frond(at: DVec3, dir: DVec2, length: f64, droop: f64) -> TriMesh {
    let side = DVec2::new(-dir.y, dir.x);
    let w = (length * 0.18).max(0.08);
    let p = |t: f64, s: f64, up: f64| {
        let drop = droop * t * t;
        (at.truncate() + dir * length * t + side * s).extend(at.z - drop + up)
    };
    let t = 0.03;
    // a diamond: base, two shoulders, tip; top and bottom faces
    let top = [p(0.0, 0.0, t), p(0.4, w, t), p(1.0, 0.0, t), p(0.4, -w, t)];
    let bottom = [
        p(0.0, 0.0, -t),
        p(0.4, w, -t),
        p(1.0, 0.0, -t),
        p(0.4, -w, -t),
    ];
    let mut m = TriMesh::default();
    for (pts, up) in [(top, 1.0), (bottom, -1.0)] {
        for tri in [[0, 1, 2], [0, 2, 3]] {
            let q = tri.map(|i| pts[i]);
            let n = (q[1] - q[0]).cross(q[2] - q[0]);
            let n = if n.z * up < 0.0 { -n } else { n };
            m.add_polygon(&q, &[[0, 1, 2]], n);
        }
    }
    for i in 0..4 {
        let j = (i + 1) % 4;
        let e = (top[j] - top[i]).truncate().normalize_or_zero();
        m.add_quad(
            [bottom[i], bottom[j], top[j], top[i]],
            DVec3::new(e.y, -e.x, 0.0),
        );
    }
    m
}

/// The solids of a site element, each with its default colour and
/// whether the element's material covers it (a plant's trunk keeps its own).
pub fn site_meshes(model: &Model, s: &Site) -> Vec<(TriMesh, [f32; 3], bool)> {
    match &s.form {
        SiteForm::Terrain { .. } => vec![(terrain_mesh(model, s), GROUND_COLOR, true)],
        SiteForm::Area {
            boundary,
            holes,
            top,
            thickness,
            ..
        } => {
            let z = members::elevation(model, s.level) + top;
            vec![(
                slab_mesh_with_holes(boundary, holes, z, *thickness),
                AREA_COLOR,
                true,
            )]
        }
        SiteForm::Plant { .. } => plant_meshes(model, s),
    }
}

/// The whole mesh and default colour of a part or a site element.
pub fn mesh_of(model: &Model, e: &Element) -> Option<(TriMesh, [f32; 3])> {
    match &e.kind {
        ElementKind::Part(p) => Some((
            part_mesh(model, p)?,
            if p.void { VOID_COLOR } else { PART_COLOR },
        )),
        ElementKind::Site(s) => {
            let parts = site_meshes(model, s);
            let color = parts.iter().find(|p| p.2).map_or(GROUND_COLOR, |p| p.1);
            let mut m = TriMesh::default();
            for (mesh, ..) in parts {
                m.append(&mesh);
            }
            Some((m, color))
        }
        _ => None,
    }
}

/// The scene items of a part or a site element: a void as a see-through
/// ghost, a plant's trunk and crown each in its own look.
pub fn scene_items(model: &Model, e: &Element) -> Vec<SceneItem> {
    let look = |default: [f32; 3], own: bool| {
        creabim_core::material_of(model, e)
            .filter(|_| own)
            .map_or((default, PLAIN_SURFACE), |(_, m)| (m.color, m.surface()))
    };
    let item = |mesh: TriMesh, (color, surface): ([f32; 3], [f32; 3]), ghost: bool| SceneItem {
        id: e.id,
        mesh,
        color,
        surface,
        ghost,
        layer: None,
        finish: None,
    };
    match &e.kind {
        ElementKind::Part(p) => match part_mesh(model, p) {
            Some(m) if p.void => vec![item(m, (VOID_COLOR, PLAIN_SURFACE), true)],
            Some(m) => vec![item(m, look(PART_COLOR, true), false)],
            None => vec![],
        },
        ElementKind::Site(s) => site_meshes(model, s)
            .into_iter()
            .filter(|(m, ..)| !m.is_empty())
            .map(|(m, color, own)| item(m, look(color, own), false))
            .collect(),
        _ => vec![],
    }
}

// ---- plans ----

/// How a part or a site element on its level is drawn in its plan, cut
/// `cut` metres above the level.
pub fn plan_items(model: &Model, e: &Element, cut: f64) -> Vec<PlanItem> {
    let z = |l: ElementId| members::elevation(model, l) + cut;
    match &e.kind {
        // voids show through what they cut
        ElementKind::Part(p) if p.void => vec![],
        ElementKind::Part(p) => {
            let Some(mesh) = part_mesh(model, p) else {
                return vec![];
            };
            let item = SceneItem {
                id: e.id,
                mesh,
                color: PART_COLOR,
                surface: PLAIN_SURFACE,
                ghost: false,
                layer: None,
                finish: None,
            };
            let mut out = vec![];
            crate::plan::sliced_plan(model, &item, z(p.level), &mut out);
            out
        }
        ElementKind::Site(s) => site_plan(model, e.id, s),
        _ => vec![],
    }
}

fn loop_lines(out: &mut Vec<PlanItem>, id: ElementId, pen: Pen, pts: &[DVec2]) {
    let n = pts.len();
    for i in 0..n {
        out.push(PlanItem::Line {
            id,
            pen,
            a: pts[i],
            b: pts[(i + 1) % n],
        });
    }
}

/// A site element in plan: ground with its contours and spot heights, an
/// area with its pattern, a plant's symbol.
fn site_plan(model: &Model, id: ElementId, s: &Site) -> Vec<PlanItem> {
    let mut out = vec![];
    match &s.form {
        SiteForm::Terrain {
            boundary,
            points,
            contour,
            ..
        } => {
            loop_lines(&mut out, id, Pen::Projection, boundary);
            if *contour > 0.0 && !points.is_empty() {
                let base = members::elevation(model, s.level);
                for (a, b) in contours(&terrain_top(model, s), *contour, base) {
                    out.push(PlanItem::Line {
                        id,
                        pen: Pen::Symbol,
                        a,
                        b,
                    });
                }
            }
            // spot heights as small crosses
            for p in points {
                let c = p.truncate();
                for d in [DVec2::new(0.15, 0.15), DVec2::new(0.15, -0.15)] {
                    out.push(PlanItem::Line {
                        id,
                        pen: Pen::Symbol,
                        a: c - d,
                        b: c + d,
                    });
                }
            }
        }
        SiteForm::Area {
            boundary,
            holes,
            pattern,
            ..
        } => {
            loop_lines(&mut out, id, Pen::Projection, boundary);
            for h in holes {
                loop_lines(&mut out, id, Pen::Projection, h);
            }
            let mut loops = vec![boundary.clone()];
            loops.extend(holes.iter().cloned());
            for (a, b) in pattern_lines(pattern, &loops) {
                out.push(PlanItem::Line {
                    id,
                    pen: Pen::Symbol,
                    a,
                    b,
                });
            }
        }
        SiteForm::Plant {
            position,
            form,
            canopy,
            ..
        } => plant_symbol(&mut out, id, *position, *form, canopy / 2.0),
    }
    out
}

/// Contour lines every `interval` of height (heights from `base`), where
/// they cross the triangles.
pub fn contours(tris: &[[DVec3; 3]], interval: f64, base: f64) -> Vec<(DVec2, DVec2)> {
    let mut out = vec![];
    if interval <= 0.0 {
        return out;
    }
    for t in tris {
        let (lo, hi) = t
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
                (a.min(p.z), b.max(p.z))
            });
        let mut k = ((lo - base) / interval).ceil();
        // a cap on how many lines one triangle gives
        let mut guard = 0;
        while base + k * interval < hi && guard < 200 {
            let h = base + k * interval;
            let mut pts = vec![];
            for i in 0..3 {
                let (a, b) = (t[i], t[(i + 1) % 3]);
                if (a.z - h) * (b.z - h) < 0.0 {
                    let s = (h - a.z) / (b.z - a.z);
                    pts.push((a + (b - a) * s).truncate());
                }
            }
            if pts.len() == 2 {
                out.push((pts[0], pts[1]));
            }
            k += 1.0;
            guard += 1;
        }
    }
    out
}

/// The parts of the line `a`–`b` inside the region bounded by `loops`
/// (an outline and its holes, even–odd).
pub fn clip_line(loops: &[Vec<DVec2>], a: DVec2, b: DVec2) -> Vec<(DVec2, DVec2)> {
    let d = b - a;
    let mut ts = vec![];
    for lp in loops {
        let n = lp.len();
        for i in 0..n {
            let (p, q) = (lp[i], lp[(i + 1) % n]);
            let e = q - p;
            let den = d.perp_dot(e);
            if den.abs() < 1e-12 {
                continue;
            }
            let t = (p - a).perp_dot(e) / den;
            let u = (p - a).perp_dot(d) / den;
            if (0.0..1.0).contains(&u) && (0.0..=1.0).contains(&t) {
                ts.push(t);
            }
        }
    }
    ts.push(0.0);
    ts.push(1.0);
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
    let inside = |p: DVec2| {
        loops
            .iter()
            .filter(|l| creabim_core::polygon::contains(l, p))
            .count()
            % 2
            == 1
    };
    ts.windows(2)
        .filter(|w| w[1] - w[0] > 1e-9 && inside(a + d * ((w[0] + w[1]) / 2.0)))
        .map(|w| (a + d * w[0], a + d * w[1]))
        .collect()
}

/// The lines of a surface pattern over the region bounded by `loops`.
pub fn pattern_lines(pattern: &Pattern, loops: &[Vec<DVec2>]) -> Vec<(DVec2, DVec2)> {
    let Some(outline) = loops.first() else {
        return vec![];
    };
    // the pattern's own axes, and the outline's extent in them
    let (angle, step) = match *pattern {
        Pattern::None => return vec![],
        Pattern::Units { angle, size, .. } => (angle, size.min_element()),
        Pattern::Lines { angle, spacing } => (angle, spacing),
        Pattern::Stipple { spacing } => (0.0, spacing),
    };
    let u = DVec2::from_angle(angle.to_radians());
    let v = DVec2::new(-u.y, u.x);
    let (mut lo, mut hi) = (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY));
    for p in outline {
        let q = DVec2::new(p.dot(u), p.dot(v));
        lo = lo.min(q);
        hi = hi.max(q);
    }
    let area = (hi - lo).element_product();
    // too fine a pattern for the area is left out rather than drawn black
    if step.is_nan() || step <= 0.0 || area / (step * step) > 200_000.0 {
        return vec![];
    }
    let world = |x: f64, y: f64| u * x + v * y;
    let mut out = vec![];
    let push = |a: DVec2, b: DVec2, out: &mut Vec<(DVec2, DVec2)>| {
        out.extend(clip_line(loops, a, b));
    };
    match *pattern {
        Pattern::None => {}
        Pattern::Lines { spacing, .. } => {
            let mut y = (lo.y / spacing).ceil() * spacing;
            while y < hi.y {
                push(world(lo.x, y), world(hi.x, y), &mut out);
                y += spacing;
            }
        }
        Pattern::Units { size, stagger, .. } => {
            // courses along u, joints across each course
            let mut row = (lo.y / size.y).floor() as i64;
            loop {
                let y0 = row as f64 * size.y;
                if y0 > hi.y {
                    break;
                }
                if y0 > lo.y {
                    push(world(lo.x, y0), world(hi.x, y0), &mut out);
                }
                let shift = if stagger && row.rem_euclid(2) == 1 {
                    size.x / 2.0
                } else {
                    0.0
                };
                let mut x = ((lo.x - shift) / size.x).floor() * size.x + shift;
                while x < hi.x {
                    if x > lo.x {
                        push(world(x, y0), world(x, y0 + size.y), &mut out);
                    }
                    x += size.x;
                }
                row += 1;
            }
        }
        Pattern::Stipple { spacing } => {
            // a jittered grid of dots, the same every time
            let mut j = 0u64;
            let mut y = (lo.y / spacing).floor() * spacing;
            while y < hi.y {
                let mut x = (lo.x / spacing).floor() * spacing;
                while x < hi.x {
                    j = j
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    let rx = ((j >> 33) % 1000) as f64 / 1000.0;
                    let ry = ((j >> 13) % 1000) as f64 / 1000.0;
                    let p = world(x + rx * spacing, y + ry * spacing);
                    let inside = loops
                        .iter()
                        .filter(|l| creabim_core::polygon::contains(l, p))
                        .count()
                        % 2
                        == 1;
                    if inside {
                        let d = DVec2::splat(spacing * 0.06);
                        out.push((p - d, p + d));
                    }
                    x += spacing;
                }
                y += spacing;
            }
        }
    }
    out
}

/// A plant's plan symbol, `r` its canopy's radius.
pub fn plant_symbol(out: &mut Vec<PlanItem>, id: ElementId, c: DVec2, form: PlantForm, r: f64) {
    let line = |out: &mut Vec<PlanItem>, pen, a, b| out.push(PlanItem::Line { id, pen, a, b });
    match form {
        PlantForm::Tree | PlantForm::Shrub => {
            // a scalloped crown
            let lobes = ((TAU * r / if form == PlantForm::Tree { 0.9 } else { 0.4 }).round()
                as usize)
                .clamp(7, 28);
            let step = TAU / lobes as f64;
            let lobe = r * (step / 2.0).sin() * 1.15;
            for k in 0..lobes {
                let a = step * k as f64;
                let centre = c + DVec2::from_angle(a) * (r - lobe);
                out.push(PlanItem::Arc {
                    id,
                    pen: Pen::Projection,
                    center: centre,
                    radius: lobe,
                    start: a - PI / 2.0 - 0.35,
                    end: a + PI / 2.0 + 0.35,
                });
            }
            if form == PlantForm::Tree {
                // its trunk, and a cross on it
                out.push(PlanItem::Arc {
                    id,
                    pen: Pen::Symbol,
                    center: c,
                    radius: (r * 0.08).clamp(0.06, 0.35),
                    start: 0.0,
                    end: TAU,
                });
                let d = r * 0.25;
                line(out, Pen::Symbol, c - DVec2::X * d, c + DVec2::X * d);
                line(out, Pen::Symbol, c - DVec2::Y * d, c + DVec2::Y * d);
            }
        }
        PlantForm::Conifer => {
            out.push(PlanItem::Arc {
                id,
                pen: Pen::Projection,
                center: c,
                radius: r,
                start: 0.0,
                end: TAU,
            });
            // needles: spokes with a zigzag edge
            let n = 12;
            for k in 0..n {
                let a = TAU * k as f64 / n as f64;
                line(out, Pen::Symbol, c, c + DVec2::from_angle(a) * r * 0.9);
                let tip = c + DVec2::from_angle(a) * r;
                let next = c + DVec2::from_angle(a + TAU / n as f64) * r;
                let mid = c + DVec2::from_angle(a + PI / n as f64) * r * 0.8;
                line(out, Pen::Symbol, tip, mid);
                line(out, Pen::Symbol, mid, next);
            }
        }
        PlantForm::Palm => {
            // fronds radiating from the head, each with a few leaflets
            let n = 9;
            for k in 0..n {
                let a = TAU * k as f64 / n as f64 + 0.3 * (k % 2) as f64;
                let d = DVec2::from_angle(a);
                let s = DVec2::new(-d.y, d.x);
                let tip = c + d * r;
                line(out, Pen::Projection, c, tip);
                for t in [0.35, 0.55, 0.75] {
                    let p = c + d * r * t;
                    let w = r * 0.18 * (1.0 - t * 0.6);
                    line(out, Pen::Symbol, p, p + d * w * 0.8 + s * w);
                    line(out, Pen::Symbol, p, p + d * w * 0.8 - s * w);
                }
            }
            out.push(PlanItem::Arc {
                id,
                pen: Pen::Symbol,
                center: c,
                radius: (r * 0.06).clamp(0.1, 0.25),
                start: 0.0,
                end: TAU,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::parts::{self, Path};
    use creabim_core::{Level, Wall};

    fn level() -> Element {
        Element::new(
            "L0",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 3.0,
            }),
        )
    }

    #[test]
    fn a_swept_coping_is_closed_and_as_big_as_its_profile() {
        let profile = parts::rectangle(0.3, 0.05);
        let path = [DVec3::ZERO, DVec3::new(4.0, 0.0, 0.0)];
        let m = sweep_mesh(&profile, &path, false);
        assert!((m.volume() - 0.3 * 0.05 * 4.0).abs() < 1e-4);
        // round an L, mitred: the centreline length times the section
        let path = [
            DVec3::ZERO,
            DVec3::new(4.0, 0.0, 0.0),
            DVec3::new(4.0, 3.0, 0.0),
        ];
        let m = sweep_mesh(&profile, &path, false);
        assert!(
            (m.volume() - 0.3 * 0.05 * 7.0).abs() < 1e-4,
            "{}",
            m.volume()
        );
        // a closed ring: no caps, still closed
        let ring = [
            DVec3::ZERO,
            DVec3::new(4.0, 0.0, 0.0),
            DVec3::new(4.0, 3.0, 0.0),
            DVec3::new(0.0, 3.0, 0.0),
        ];
        let m = sweep_mesh(&profile, &ring, true);
        assert!(
            (m.volume() - 0.3 * 0.05 * 14.0).abs() < 1e-4,
            "{}",
            m.volume()
        );
    }

    #[test]
    fn steps_as_one_element() {
        let lv = level();
        let p = Part::sweep(
            lv.id,
            parts::steps(4, 0.17, 0.28),
            Path::Points {
                points: vec![DVec3::ZERO, DVec3::new(1.2, 0.0, 0.0)],
                closed: false,
            },
        );
        let m = Model::from_elements([lv]).unwrap();
        let mesh = part_mesh(&m, &p).unwrap();
        let want = 0.17 * 0.28 * (4 + 3 + 2 + 1) as f64 * 1.2;
        assert!((mesh.volume() - want).abs() < 1e-4, "{}", mesh.volume());
        let (_, hi) = mesh.bounds().unwrap();
        assert!((hi.z as f64 - 0.68).abs() < 1e-5);
    }

    #[test]
    fn a_coping_sits_on_its_wall_in_plan_and_3d() {
        let lv = level();
        let w = Element::new(
            "W",
            ElementKind::Wall(Wall::new(
                lv.id,
                DVec2::ZERO,
                DVec2::new(3.0, 0.0),
                0.2,
                1.0,
            )),
        );
        let c = Element::new(
            "Coping",
            ElementKind::Part(Part::sweep(
                lv.id,
                parts::coping(0.2, 0.03, 0.05),
                Path::Wall {
                    wall: w.id,
                    edge: parts::WallEdge::Top,
                    offset: 0.0,
                    extend: [0.0, 0.0],
                },
            )),
        );
        let m = Model::from_elements([lv.clone(), w, c.clone()]).unwrap();
        let (mesh, _) = mesh_of(&m, &c).unwrap();
        let (lo, hi) = mesh.bounds().unwrap();
        assert!((lo.z - 1.0).abs() < 1e-5 && (hi.z - 1.05).abs() < 1e-5);
        assert!((hi.y - 0.13).abs() < 1e-5);
        // a plan cut through the coping's height shows it cut
        let items = plan_items(&m, &c, 1.02);
        assert!(items
            .iter()
            .any(|i| matches!(i, PlanItem::Fill { id, .. } if *id == c.id)));
    }

    #[test]
    fn terrain_follows_its_spot_heights() {
        let lv = level();
        let mut t = Site::terrain(
            lv.id,
            creabim_core::polygon::rectangle(DVec2::ZERO, DVec2::new(10.0, 10.0)),
            0.0,
            0.5,
        );
        if let SiteForm::Terrain {
            points, contour, ..
        } = &mut t.form
        {
            points.push(DVec3::new(5.0, 5.0, 2.0));
            *contour = 0.5;
        }
        let e = Element::new("Ground", ElementKind::Site(t.clone()));
        let m = Model::from_elements([lv, e.clone()]).unwrap();
        let tops = terrain_top(&m, &t);
        // the spot height became a corner of the surface
        assert!(tops.iter().flatten().any(|p| (p.z - 2.0).abs() < 1e-9));
        let (mesh, _) = mesh_of(&m, &e).unwrap();
        assert!(mesh.volume() > 10.0 * 10.0 * 0.5);
        // three contours (0.5, 1.0, 1.5) between the corners and the top
        let lines = contours(&tops, 0.5, 0.0);
        assert!(!lines.is_empty());
        let items = plan_items(&m, &e, 1.2);
        assert!(items.len() > 4);
    }

    #[test]
    fn paving_patterns_stay_inside_the_area() {
        let sq = creabim_core::polygon::rectangle(DVec2::ZERO, DVec2::new(2.0, 1.0));
        let hole = creabim_core::polygon::rectangle(DVec2::new(0.5, 0.25), DVec2::new(1.0, 0.75));
        let loops = vec![sq.clone(), hole];
        let lines = pattern_lines(
            &Pattern::Units {
                size: DVec2::new(0.4, 0.2),
                stagger: true,
                angle: 0.0,
            },
            &loops,
        );
        assert!(!lines.is_empty());
        for (a, b) in &lines {
            let m = (*a + *b) / 2.0;
            assert!(creabim_core::polygon::contains(&sq, m));
            assert!(!(m.x > 0.5 && m.x < 1.0 && m.y > 0.25 && m.y < 0.75));
        }
        assert!(!pattern_lines(&Pattern::Stipple { spacing: 0.1 }, &loops).is_empty());
        assert!(pattern_lines(&Pattern::None, &loops).is_empty());
    }

    #[test]
    fn plants_draw_a_symbol_and_a_form() {
        let lv = level();
        for form in PlantForm::ALL {
            let p = Element::new(
                form.label(),
                ElementKind::Site(Site::plant(lv.id, DVec2::ZERO, form, 4.0, 6.0)),
            );
            let m = Model::from_elements([lv.clone(), p.clone()]).unwrap();
            assert!(!plan_items(&m, &p, 1.2).is_empty());
            let items = scene_items(&m, &p);
            assert!(!items.is_empty());
            let (_, hi) = mesh_of(&m, &p).unwrap().0.bounds().unwrap();
            assert!(hi.z > 1.0 && hi.z < 6.5, "{form:?} {hi}");
        }
    }
}
