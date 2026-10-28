//! Meshes and plan linework for columns, beams, roofs, stairs and railings
//! (see `creabim_core::members`).

use crate::plan::{Pen, PlanItem};
use crate::scene::slab_mesh;
use crate::triangulate::triangulate;
use crate::TriMesh;
use creabim_core::members::{self, Flight, RAIL_SIZE};
use creabim_core::{Element, ElementKind, Model, Roof, Stair};
use glam::{DVec2, DVec3};

/// The mesh and default colour of a member; `None` for other elements.
pub fn member_mesh(model: &Model, e: &Element) -> Option<(TriMesh, [f32; 3])> {
    Some(match &e.kind {
        ElementKind::Column(c) => {
            let (z0, z1) = c.span(model);
            (slab_mesh(&c.outline(), z1, z1 - z0), [0.78, 0.76, 0.72])
        }
        ElementKind::Beam(b) => (
            slab_mesh(&b.outline(), b.top(model), b.depth),
            [0.72, 0.64, 0.52],
        ),
        ElementKind::Roof(r) => {
            let holes = creabim_core::slab::holes_of(model, e);
            (
                cut_holes(roof_mesh(r, r.base(model)), &holes),
                [0.66, 0.34, 0.26],
            )
        }
        ElementKind::Stair(s) => (
            stair_mesh(s, s.base(model), s.rise(model)),
            [0.82, 0.80, 0.76],
        ),
        ElementKind::Railing(r) => (
            railing_mesh(&r.path_in(model), r.height, r.post_spacing),
            [0.32, 0.34, 0.38],
        ),
        ElementKind::Footing(f) => {
            let top = f.top(model);
            let mut m = TriMesh::default();
            for piece in creabim_core::footing::outlines(model, e, None) {
                m.append(&slab_mesh(&piece, top, f.thickness));
            }
            (m, [0.66, 0.66, 0.64])
        }
        _ => return None,
    })
}

/// A mesh with each hole (a polygon in plan) cut upright through it.
pub fn cut_holes(mesh: TriMesh, holes: &[Vec<DVec2>]) -> TriMesh {
    let Some((lo, hi)) = mesh.bounds() else {
        return mesh;
    };
    let (z0, z1) = (lo.z as f64 - 1.0, hi.z as f64 + 1.0);
    holes.iter().filter(|h| h.len() >= 3).fold(mesh, |m, h| {
        let tool = slab_mesh(h, z1, z1 - z0);
        crate::scene::boolean_mesh(&m, &tool, creabim_core::BoolOp::Difference)
    })
}

/// A roof as a closed solid: each flat piece of its underside, raised by the
/// roof's upright thickness for the top, with upright edges round the eaves.
pub fn roof_mesh(r: &Roof, base: f64) -> TriMesh {
    roof_layer_mesh(r, base, 0.0, r.vertical_thickness())
}

/// One layer of a layered roof: the part of the roof from `from` to `to`
/// above its underside, measured upright.
pub fn roof_layer_mesh(r: &Roof, base: f64, from: f64, to: f64) -> TriMesh {
    let mut m = TriMesh::default();
    let tv = to - from;
    let eaves = r.eave_outline();
    let on_eaves = |a: DVec2, b: DVec2| {
        let mid = (a + b) / 2.0;
        let n = eaves.len();
        (0..n).any(|i| {
            let (p, q) = (eaves[i], eaves[(i + 1) % n]);
            let d = q - p;
            let t = ((mid - p).dot(d) / d.length_squared().max(1e-12)).clamp(0.0, 1.0);
            mid.distance(p + d * t) < 1e-6
                && (b - a)
                    .normalize_or_zero()
                    .perp_dot(d.normalize_or_zero())
                    .abs()
                    < 1e-6
        })
    };
    for f in r.faces() {
        let pts = &f.outline;
        let tris = triangulate(pts);
        let lo: Vec<DVec3> = pts
            .iter()
            .map(|p| p.extend(base + from + f.z(*p)))
            .collect();
        let hi: Vec<DVec3> = lo.iter().map(|p| *p + DVec3::Z * tv).collect();
        let up = DVec3::new(-f.g.x, -f.g.y, 1.0);
        m.add_polygon(&hi, &tris, up);
        m.add_polygon(&lo, &tris, -up);
        let ccw = crate::triangulate::signed_area2(pts) > 0.0;
        let n = pts.len();
        for i in 0..n {
            let j = (i + 1) % n;
            if !on_eaves(pts[i], pts[j]) {
                continue;
            }
            let e = (pts[j] - pts[i]).normalize_or_zero();
            let out = if ccw { -e.perp() } else { e.perp() };
            m.add_quad([lo[i], lo[j], hi[j], hi[i]], out.extend(0.0));
        }
    }
    m
}

/// A side profile (along, up) extruded `width` across, centred on `origin`.
fn extrude_profile(m: &mut TriMesh, profile: &[DVec2], origin: DVec3, dir: DVec2, width: f64) {
    let side = dir.perp();
    let at = |p: DVec2, v: f64| origin + (dir * p.x + side * v).extend(p.y);
    let tris = triangulate(profile);
    let left: Vec<DVec3> = profile.iter().map(|p| at(*p, width / 2.0)).collect();
    let right: Vec<DVec3> = profile.iter().map(|p| at(*p, -width / 2.0)).collect();
    m.add_polygon(&left, &tris, side.extend(0.0));
    m.add_polygon(&right, &tris, -side.extend(0.0));
    let ccw = crate::triangulate::signed_area2(profile) > 0.0;
    let n = profile.len();
    for i in 0..n {
        let j = (i + 1) % n;
        let e = profile[j] - profile[i];
        if e.length() < 1e-9 {
            continue;
        }
        let out = if ccw {
            DVec2::new(e.y, -e.x)
        } else {
            DVec2::new(-e.y, e.x)
        };
        let normal = (dir * out.x).extend(out.y);
        m.add_quad([right[i], right[j], left[j], left[i]], normal);
    }
}

/// The side profile of one flight, counter-clockwise in (along, up): the
/// steps on top, a sloping waist underneath. Its top is the last tread; the
/// last riser climbs onto the floor or landing beyond.
pub fn flight_profile(risers: u32, riser: f64, tread: f64, waist: f64) -> Vec<DVec2> {
    let n = risers.max(1);
    if n == 1 {
        return vec![
            DVec2::ZERO,
            DVec2::new(tread, 0.0),
            DVec2::new(tread, riser),
            DVec2::new(0.0, riser),
        ];
    }
    let run = (n - 1) as f64 * tread;
    let slope = riser / tread;
    // the underside runs parallel to the line through the inner corners
    let wv = waist * (1.0 + slope * slope).sqrt();
    let u0 = wv / slope;
    let mut p = vec![DVec2::new(0.0, riser), DVec2::ZERO];
    if u0 < run - 1e-9 {
        p.push(DVec2::new(u0, 0.0));
        p.push(DVec2::new(run, (n - 1) as f64 * riser - wv));
    } else {
        p.push(DVec2::new(run, 0.0));
    }
    // back down the steps: the last riser is the edge of the floor (or
    // landing) it climbs to, so the flight stops at its top tread
    let mut steps = Vec::new();
    for i in 1..n {
        let u = i as f64 * tread;
        steps.push(DVec2::new(u, i as f64 * riser));
        if i + 1 < n {
            steps.push(DVec2::new(u, (i + 1) as f64 * riser));
        }
    }
    p.extend(steps.into_iter().rev());
    p.dedup_by(|a, b| a.distance(*b) < 1e-9);
    p
}

/// Solid steps for every flight, and the landing between them.
pub fn stair_mesh(s: &Stair, base: f64, rise: f64) -> TriMesh {
    let lay = s.layout(rise);
    let mut m = TriMesh::default();
    for f in &lay.flights {
        let prof = flight_profile(f.risers, lay.riser, s.tread, s.waist);
        extrude_profile(&mut m, &prof, f.start.extend(base + f.z0), f.dir, s.width);
    }
    if let Some((outline, z)) = &lay.landing {
        let t = s.waist.max(lay.riser);
        m.append(&slab_mesh(outline, base + z, t.min(*z).max(0.02)));
    }
    m
}

/// A square bar of side `size` from `a` to `b`.
fn bar(m: &mut TriMesh, a: DVec3, b: DVec3, size: f64) {
    let along = (b - a).normalize_or_zero();
    if along == DVec3::ZERO {
        return;
    }
    let s = DVec3::Z.cross(along).try_normalize().unwrap_or(DVec3::X);
    let u = along.cross(s).normalize();
    let h = size / 2.0;
    let c = |p: DVec3, i: f64, j: f64| p + s * (i * h) + u * (j * h);
    let corners = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];
    for k in 0..4 {
        let (i0, j0) = corners[k];
        let (i1, j1) = corners[(k + 1) % 4];
        let normal = s * (i0 + i1) + u * (j0 + j1);
        m.add_quad(
            [c(a, i0, j0), c(a, i1, j1), c(b, i1, j1), c(b, i0, j0)],
            normal,
        );
    }
    let ring = |p: DVec3| corners.map(|(i, j)| c(p, i, j));
    m.add_quad(ring(a), -along);
    m.add_quad(ring(b), along);
}

/// Posts, a top rail and a bottom rail along `path` (model coordinates).
pub fn railing_mesh(path: &[DVec3], height: f64, spacing: f64) -> TriMesh {
    let mut m = TriMesh::default();
    let top = height - RAIL_SIZE / 2.0;
    for w in path.windows(2) {
        bar(
            &mut m,
            w[0] + DVec3::Z * top,
            w[1] + DVec3::Z * top,
            RAIL_SIZE,
        );
        bar(
            &mut m,
            w[0] + DVec3::Z * 0.1,
            w[1] + DVec3::Z * 0.1,
            members::POST_SIZE,
        );
    }
    for p in members::posts_along(path, spacing) {
        bar(
            &mut m,
            p,
            p + DVec3::Z * (height - RAIL_SIZE),
            members::POST_SIZE,
        );
    }
    m
}

/// Closed outline lines.
fn ring(items: &mut Vec<PlanItem>, e: &Element, pen: Pen, pts: &[DVec2]) {
    for i in 0..pts.len() {
        items.push(PlanItem::Line {
            id: e.id,
            pen,
            a: pts[i],
            b: pts[(i + 1) % pts.len()],
        });
    }
}

/// How a member on `level` is drawn in its plan, cut `cut` metres above
/// the level.
pub fn member_plan(model: &Model, e: &Element, cut: f64) -> Vec<PlanItem> {
    let mut items = Vec::new();
    let Some(level) = members::level_of(&e.kind) else {
        return items;
    };
    let z = members::elevation(model, level) + cut;
    match &e.kind {
        ElementKind::Column(c) => {
            let (z0, z1) = c.span(model);
            let pts = c.outline();
            if z0 < z && z < z1 {
                items.push(PlanItem::Polygon {
                    id: e.id,
                    pen: Pen::Cut,
                    points: pts,
                });
            } else if z1 <= z {
                ring(&mut items, e, Pen::Projection, &pts);
            } else {
                ring(&mut items, e, Pen::Hidden, &pts);
            }
        }
        ElementKind::Beam(b) => {
            let top = b.top(model);
            let pts = b.outline();
            if top - b.depth < z && z < top {
                items.push(PlanItem::Polygon {
                    id: e.id,
                    pen: Pen::Cut,
                    points: pts,
                });
            } else {
                // above the cut, as usual: dashed
                let pen = if top <= z {
                    Pen::Projection
                } else {
                    Pen::Hidden
                };
                ring(&mut items, e, pen, &pts);
            }
        }
        ElementKind::Roof(r) => {
            // seen from above: eaves, and the ridges and hips between faces
            let mut seen: Vec<(DVec2, DVec2)> = Vec::new();
            for f in r.faces() {
                let n = f.outline.len();
                for i in 0..n {
                    let (a, b) = (f.outline[i], f.outline[(i + 1) % n]);
                    let dup = seen.iter().any(|(p, q)| {
                        (p.distance(a) < 1e-6 && q.distance(b) < 1e-6)
                            || (p.distance(b) < 1e-6 && q.distance(a) < 1e-6)
                    });
                    if !dup && a.distance(b) > 1e-6 {
                        seen.push((a, b));
                        items.push(PlanItem::Line {
                            id: e.id,
                            pen: Pen::Projection,
                            a,
                            b,
                        });
                    }
                }
            }
            // the wall plate, where the roof meets the walls, dashed
            if r.overhang > 1e-6 {
                ring(&mut items, e, Pen::Hidden, &r.boundary);
            }
            // holes through it, crossed
            for h in creabim_core::slab::holes_of(model, e) {
                items.extend(crate::plan::hole_marks(e.id, &h, Pen::Projection));
            }
            // seen from above the roof plan shows which way each face falls
            let top = r
                .faces()
                .iter()
                .flat_map(|f| f.outline.iter().map(|p| f.z(*p)).collect::<Vec<_>>())
                .fold(0.0, f64::max);
            if z > r.base(model) + top + r.vertical_thickness() {
                slope_arrows(&mut items, e, r);
            }
        }
        ElementKind::Stair(s) => stair_plan(&mut items, e, s, s.rise(model), cut - s.base_offset),
        ElementKind::Footing(f) => {
            // below the floor, as usual: dashed; cut when the plan plane
            // passes through it
            let top = f.top(model);
            let z = members::elevation(model, f.plan_level(model)) + cut;
            for piece in creabim_core::footing::outlines(model, e, None) {
                if top - f.thickness < z && z < top {
                    items.push(PlanItem::Polygon {
                        id: e.id,
                        pen: Pen::Cut,
                        points: piece,
                    });
                } else {
                    let pen = if top <= z {
                        Pen::Hidden
                    } else {
                        Pen::Projection
                    };
                    ring(&mut items, e, pen, &piece);
                }
            }
        }
        ElementKind::Railing(r) => {
            let path = r.path_in(model);
            for w in path.windows(2) {
                items.push(PlanItem::Line {
                    id: e.id,
                    pen: Pen::Projection,
                    a: w[0].truncate(),
                    b: w[1].truncate(),
                });
            }
            let h = members::POST_SIZE / 2.0;
            for p in r.posts(model) {
                let c = p.truncate();
                ring(
                    &mut items,
                    e,
                    Pen::Projection,
                    &[
                        c + DVec2::new(-h, -h),
                        c + DVec2::new(h, -h),
                        c + DVec2::new(h, h),
                        c + DVec2::new(-h, h),
                    ],
                );
            }
        }
        _ => {}
    }
    items
}

/// The pitch of a slope as roof plans write it: degrees, or for a slope
/// too shallow to read in degrees a fall of one in so many.
pub fn pitch_text(degrees: f64) -> String {
    if degrees >= 5.0 {
        format!("{}°", (degrees * 10.0).round() / 10.0)
    } else {
        format!("1:{}", (1.0 / degrees.to_radians().tan()).round())
    }
}

/// An arrow down each sloping face of a roof, from near its top toward
/// its eave, with the pitch written beside it.
fn slope_arrows(items: &mut Vec<PlanItem>, e: &Element, r: &Roof) {
    for f in r.faces() {
        let n = f.outline.len();
        if n < 3 || f.g.length() < 1e-6 {
            continue;
        }
        // the face's centre, by area
        let (mut c, mut area) = (DVec2::ZERO, 0.0);
        for i in 0..n {
            let (a, b) = (f.outline[i], f.outline[(i + 1) % n]);
            let k = a.perp_dot(b);
            c += (a + b) * k;
            area += k;
        }
        if area.abs() < 1e-9 {
            continue;
        }
        let c = c / (3.0 * area);
        let down = -f.g.normalize();
        // as long as the face is deep down its slope, up to 1.6 m
        let depth = {
            let ts = f.outline.iter().map(|p| (*p - c).dot(down));
            let (lo, hi) = ts.fold((f64::MAX, f64::MIN), |(lo, hi), t| (lo.min(t), hi.max(t)));
            hi - lo
        };
        let len = (depth * 0.5).min(1.6);
        if len < 0.2 {
            continue;
        }
        let (tail, head) = (c - down * len / 2.0, c + down * len / 2.0);
        let line = |items: &mut Vec<PlanItem>, a, b| {
            items.push(PlanItem::Line {
                id: e.id,
                pen: Pen::Symbol,
                a,
                b,
            })
        };
        line(items, tail, head);
        let (back, wing) = (-down * 0.25, down.perp() * 0.1);
        line(items, head, head + back + wing);
        line(items, head, head + back - wing);
        // the pitch at the arrow's tail, clear of the shaft
        let slope = f.g.length().atan().to_degrees();
        items.push(PlanItem::Label {
            id: e.id,
            at: tail - down * 0.3,
            text: pitch_text(slope),
        });
    }
}

/// A stair in plan: its sides and risers up to the cut, a break line where
/// the plan cuts it, the rest dashed, and an arrow up the walking line.
fn stair_plan(items: &mut Vec<PlanItem>, e: &Element, s: &Stair, rise: f64, cut: f64) {
    let lay = s.layout(rise);
    let r = lay.riser;
    let line = |items: &mut Vec<PlanItem>, pen, a, b| {
        items.push(PlanItem::Line {
            id: e.id,
            pen,
            a,
            b,
        });
    };
    let mut broken = false;
    for f in &lay.flights {
        let side = f.dir.perp() * s.width / 2.0;
        let at = |u: f64| f.start + f.dir * u;
        // the first riser the cut plane meets
        let cut_at: Option<u32> = (0..f.risers).find(|i| f.z0 + (*i + 1) as f64 * r > cut + 1e-9);
        let pen_for = |i: u32| match cut_at {
            Some(k) if i > k || broken => Pen::Hidden,
            _ => Pen::Projection,
        };
        for i in 0..f.risers {
            let u = i as f64 * s.tread;
            line(items, pen_for(i), at(u) - side, at(u) + side);
        }
        // the sides, split at the break
        let run = f.run(s.tread);
        let split = match (cut_at, broken) {
            (_, true) => 0.0,
            (Some(k), false) => (k as f64 + 0.5) * s.tread,
            (None, false) => run,
        };
        let split = split.min(run);
        for sd in [-side, side] {
            if split > 1e-9 {
                line(items, Pen::Projection, at(0.0) + sd, at(split) + sd);
            }
            if run - split > 1e-9 {
                line(items, Pen::Hidden, at(split) + sd, at(run) + sd);
            }
        }
        if let (Some(_), false) = (cut_at, broken) {
            break_line(items, e, f, s, split);
            broken = true;
        }
    }
    if let Some((outline, z)) = &lay.landing {
        let pen = if broken || *z > cut {
            Pen::Hidden
        } else {
            Pen::Projection
        };
        ring(items, e, pen, outline);
    }
    // walking line with an arrow at its top end
    let mut walk: Vec<DVec2> = Vec::new();
    for (k, f) in lay.flights.iter().enumerate() {
        if k == 1 {
            let f1 = lay.flights[0];
            walk.push(f1.end(s.tread) + f1.dir * s.width / 2.0);
        }
        walk.push(f.start);
        walk.push(f.end(s.tread));
    }
    walk.dedup_by(|a, b| a.distance(*b) < 1e-9);
    for w in walk.windows(2) {
        line(items, Pen::Symbol, w[0], w[1]);
    }
    if let (Some(&end), Some(f)) = (walk.last(), lay.flights.last()) {
        let back = -f.dir * 0.25;
        let wing = f.dir.perp() * 0.12;
        line(items, Pen::Symbol, end, end + back + wing);
        line(items, Pen::Symbol, end, end + back - wing);
    }
    if let Some(f) = lay.flights.first() {
        items.push(PlanItem::Label {
            id: e.id,
            at: f.start - f.dir * 0.35,
            text: "UP".into(),
        });
    }
}

/// A whole stair seen from above, as the floor over it sees it through
/// its well: `line(pen, a, b, z)` takes each tread edge, side and landing
/// edge in projection and the walking line with its arrow, `z` being how
/// high above the stair's base the line is.
pub fn stair_lines(s: &Stair, rise: f64, line: &mut dyn FnMut(Pen, DVec2, DVec2, f64)) {
    let lay = s.layout(rise);
    let r = lay.riser;
    for f in &lay.flights {
        let side = f.dir.perp() * s.width / 2.0;
        let at = |u: f64| f.start + f.dir * u;
        for i in 0..f.risers {
            let u = i as f64 * s.tread;
            line(
                Pen::Projection,
                at(u) - side,
                at(u) + side,
                f.z0 + (i + 1) as f64 * r,
            );
        }
        let run = f.run(s.tread);
        let top = f.z0 + f.risers as f64 * r;
        for sd in [-side, side] {
            line(Pen::Projection, at(0.0) + sd, at(run) + sd, top);
        }
    }
    if let Some((outline, z)) = &lay.landing {
        let n = outline.len();
        for i in 0..n {
            line(Pen::Projection, outline[i], outline[(i + 1) % n], *z);
        }
    }
    let mut walk: Vec<(DVec2, f64)> = Vec::new();
    for (k, f) in lay.flights.iter().enumerate() {
        if k == 1 {
            let f1 = lay.flights[0];
            walk.push((f1.end(s.tread) + f1.dir * s.width / 2.0, f.z0));
        }
        walk.push((f.start, f.z0));
        walk.push((f.end(s.tread), f.z0 + f.risers as f64 * r));
    }
    walk.dedup_by(|a, b| a.0.distance(b.0) < 1e-9);
    for w in walk.windows(2) {
        line(Pen::Symbol, w[0].0, w[1].0, w[1].1);
    }
    if let (Some(&(end, z)), Some(f)) = (walk.last(), lay.flights.last()) {
        let back = -f.dir * 0.25;
        let wing = f.dir.perp() * 0.12;
        line(Pen::Symbol, end, end + back + wing, z);
        line(Pen::Symbol, end, end + back - wing, z);
    }
}

/// The zigzag drawn across a flight where the plan cuts it.
fn break_line(items: &mut Vec<PlanItem>, e: &Element, f: &Flight, s: &Stair, u: f64) {
    let side = f.dir.perp();
    let w = s.width / 2.0 + 0.1;
    let slant = 0.3 * s.width;
    let p = |v: f64, du: f64| f.start + f.dir * (u + du + slant * v / w) + side * v;
    let pts = [
        p(-w, 0.0),
        p(-0.15, 0.0),
        p(-0.05, 0.15),
        p(0.05, -0.15),
        p(0.15, 0.0),
        p(w, 0.0),
    ];
    for k in 0..pts.len() - 1 {
        items.push(PlanItem::Line {
            id: e.id,
            pen: Pen::Symbol,
            a: pts[k],
            b: pts[k + 1],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::{
        Beam, Column, ElementId, Landing, Level, Profile, Railing, RoofShape, Transaction,
    };

    fn model() -> (Model, ElementId) {
        let mut m = Model::new();
        let l = Element::new(
            "Ground",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 3.0,
            }),
        );
        let id = l.id;
        Transaction::new("l").insert(l).apply(&mut m).unwrap();
        (m, id)
    }

    fn rect(w: f64, d: f64) -> Vec<DVec2> {
        vec![
            DVec2::ZERO,
            DVec2::new(w, 0.0),
            DVec2::new(w, d),
            DVec2::new(0.0, d),
        ]
    }

    #[test]
    fn column_and_beam_meshes_have_the_right_volume() {
        let (m, l) = model();
        let c = Element::new(
            "C",
            ElementKind::Column(Column::new(
                l,
                DVec2::new(2.0, 2.0),
                Profile::Rectangle {
                    width: 0.3,
                    depth: 0.4,
                },
            )),
        );
        let (mesh, _) = member_mesh(&m, &c).unwrap();
        assert!((mesh.volume() - 0.3 * 0.4 * 3.0).abs() < 1e-5);
        let round = Element::new(
            "C",
            ElementKind::Column(Column::new(
                l,
                DVec2::ZERO,
                Profile::Round { diameter: 0.4 },
            )),
        );
        let (mesh, _) = member_mesh(&m, &round).unwrap();
        let exact = std::f64::consts::PI * 0.04 * 3.0;
        assert!((mesh.volume() - exact).abs() / exact < 0.02);
        let b = Element::new(
            "B",
            ElementKind::Beam(Beam::new(l, DVec2::ZERO, DVec2::new(5.0, 0.0))),
        );
        let (mesh, _) = member_mesh(&m, &b).unwrap();
        assert!((mesh.volume() - 5.0 * 0.2 * 0.4).abs() < 1e-5);
        let (lo, hi) = mesh.bounds().unwrap();
        assert!((hi.z - 3.0).abs() < 1e-5 && (lo.z - 2.6).abs() < 1e-5);
    }

    #[test]
    fn roof_volume_is_plan_area_times_upright_thickness() {
        let (_, l) = model();
        for shape in RoofShape::ALL {
            let mut r = Roof::new(l, rect(10.0, 6.0), shape);
            r.pitch = 30.0;
            let m = roof_mesh(&r, 3.0);
            let want = r.plan_area() * r.vertical_thickness();
            assert!(
                (m.volume() - want).abs() / want < 1e-4,
                "{shape:?}: {} vs {want}",
                m.volume()
            );
        }
    }

    #[test]
    fn a_straight_stair_climbs_its_full_rise() {
        let (m, l) = model();
        let s = Stair::new(l, DVec2::ZERO, 0.0);
        let mesh = stair_mesh(&s, 0.0, s.rise(&m));
        let (lo, hi) = mesh.bounds().unwrap();
        assert!(lo.z.abs() < 1e-5);
        // up to the last tread, one riser below the floor above
        assert!((hi.z as f64 - 16.0 * 3.0 / 17.0).abs() < 1e-5);
        // 17 risers of 176 mm, 16 treads
        assert!((hi.x - 16.0 * 0.28).abs() < 1e-4);
        assert!(mesh.volume() > 0.0);
        let prof = flight_profile(17, 3.0 / 17.0, 0.28, 0.15);
        assert!(crate::triangulate::signed_area2(&prof) > 0.0);
    }

    #[test]
    fn an_l_stair_and_its_railings_mesh() {
        let (mut m, l) = model();
        let mut s = Stair::new(l, DVec2::ZERO, 0.0);
        s.landing = Some(Landing {
            after: 8,
            left: true,
        });
        let e = Element::new("S", ElementKind::Stair(s.clone()));
        let sid = e.id;
        Transaction::new("s").insert(e).apply(&mut m).unwrap();
        let mesh = stair_mesh(&s, 0.0, 3.0);
        assert!(mesh.volume() > 0.0);
        let r = Railing::along_stair(&m, sid, false).unwrap();
        let re = Element::new("R", ElementKind::Railing(r));
        let (rm, _) = member_mesh(&m, &re).unwrap();
        let (_, hi) = rm.bounds().unwrap();
        assert!(
            (hi.z - 4.0).abs() < 0.01,
            "a metre over the top nosing: {hi:?}"
        );
    }

    #[test]
    fn columns_and_beams_have_a_schedule() {
        let (mut m, l) = model();
        Transaction::new("add")
            .insert(Element::new(
                "C1",
                ElementKind::Column(Column::new(
                    l,
                    DVec2::ZERO,
                    Profile::Round { diameter: 0.3 },
                )),
            ))
            .insert(Element::new(
                "B1",
                ElementKind::Beam(Beam::new(l, DVec2::ZERO, DVec2::new(4.0, 0.0))),
            ))
            .apply(&mut m)
            .unwrap();
        let s = crate::schedule::schedule(&m, creabim_core::ScheduleCategory::Structure);
        assert_eq!(s.rows.len(), 2);
        assert_eq!(s.rows[0].cells[1], "Beam");
        assert_eq!(s.rows[1].cells[4], "Ø 300");
        assert_eq!(s.totals[5], "7000");
    }

    #[test]
    fn plans_show_beams_dashed_and_stairs_broken() {
        let (m, l) = model();
        let b = Element::new(
            "B",
            ElementKind::Beam(Beam::new(l, DVec2::ZERO, DVec2::new(5.0, 0.0))),
        );
        let items = member_plan(&m, &b, 1.2);
        assert!(items.iter().all(|i| matches!(
            i,
            PlanItem::Line {
                pen: Pen::Hidden,
                ..
            }
        )));
        let s = Element::new("S", ElementKind::Stair(Stair::new(l, DVec2::ZERO, 0.0)));
        let items = member_plan(&m, &s, 1.2);
        let count = |pen: Pen| {
            items
                .iter()
                .filter(|i| matches!(i, PlanItem::Line { pen: p, .. } if *p == pen))
                .count()
        };
        assert!(count(Pen::Hidden) > 0 && count(Pen::Projection) > 0);
        assert!(items
            .iter()
            .any(|i| matches!(i, PlanItem::Label { text, .. } if text == "UP")));
        let c = Element::new(
            "C",
            ElementKind::Column(Column::new(
                l,
                DVec2::ZERO,
                Profile::Round { diameter: 0.3 },
            )),
        );
        assert!(matches!(
            member_plan(&m, &c, 1.2)[0],
            PlanItem::Polygon { pen: Pen::Cut, .. }
        ));
    }
}
