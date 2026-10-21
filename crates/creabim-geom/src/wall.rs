use crate::TriMesh;
use creabim_core::walls::TopProfile;
use creabim_core::{Opening, Wall, WallEnds};
use glam::{DVec2, DVec3};

/// Rectangles in wall-local coordinates: `u` along the wall, `v` up from its base.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cut {
    pub u0: f64,
    pub u1: f64,
    pub v0: f64,
    pub v1: f64,
}

impl Cut {
    pub fn of(o: &Opening, wall: &Wall) -> Option<Cut> {
        let c = Cut {
            u0: o.offset.max(0.0),
            u1: (o.offset + o.width).min(wall.length()),
            v0: o.sill.max(0.0),
            v1: (o.sill + o.height).min(wall.height),
        };
        (c.u1 > c.u0 + 1e-9 && c.v1 > c.v0 + 1e-9).then_some(c)
    }

    fn contains(&self, u: f64, v: f64) -> bool {
        u > self.u0 && u < self.u1 && v > self.v0 && v < self.v1
    }
}

/// Build a closed solid for a straight wall with rectangular openings.
///
/// The wall face is split into a grid at every opening edge; each grid cell is
/// either solid or void. Solid cells get front and back faces, and every
/// boundary between a solid cell and void (or the wall's outer edge) gets a
/// side face, so ends, top, bottom and opening reveals all come out closed.
/// `ends` mitres or trims the two ends where the wall joins its neighbours
/// and leaves out the gaps where walls cross it; with `top`, the top
/// follows the undersides it is attached to instead of being level.
pub fn wall_mesh(
    wall: &Wall,
    base_z: f64,
    openings: &[&Opening],
    ends: &WallEnds,
    top: Option<&TopProfile>,
) -> TriMesh {
    let t = wall.thickness / 2.0;
    wall_layer_mesh(wall, base_z, openings, ends, top, (-t, t), false)
}

/// One layer of a layered wall as a closed solid: the part of the wall
/// between `band.0` and `band.1` across it (left positive), its ends on
/// the wall's end lines, or past them for a `core` layer that runs on.
pub fn wall_layer_mesh(
    wall: &Wall,
    base_z: f64,
    openings: &[&Opening],
    ends: &WallEnds,
    top: Option<&TopProfile>,
    band: (f64, f64),
    core: bool,
) -> TriMesh {
    let len = wall.length();
    let h = wall.height;
    let cuts: Vec<Cut> = openings.iter().filter_map(|o| Cut::of(o, wall)).collect();

    let mut us = vec![0.0, len];
    let mut vs = vec![0.0, h];
    for c in &cuts {
        us.extend([c.u0, c.u1]);
        vs.extend([c.v0, c.v1]);
    }
    for g in &ends.gaps {
        us.extend([g.lo_mid(), g.hi_mid()]);
    }
    if let Some(top) = top {
        // where the top bends over a ridge or steps at an eave
        us.extend(
            top.breaks(wall.start, wall.end)
                .into_iter()
                .map(|k| k * len),
        );
    }
    let sort = |xs: &mut Vec<f64>| {
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        xs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    };
    sort(&mut us);
    sort(&mut vs);

    let (ni, nj) = (us.len() - 1, vs.len() - 1);
    let solid = |i: isize, j: isize| -> bool {
        if i < 0 || j < 0 || i as usize >= ni || j as usize >= nj {
            return false;
        }
        let (i, j) = (i as usize, j as usize);
        let (u, v) = ((us[i] + us[i + 1]) / 2.0, (vs[j] + vs[j + 1]) / 2.0);
        !ends.in_gap(u) && !cuts.iter().any(|c| c.contains(u, v))
    };

    let d = wall.direction();
    let n2 = DVec2::new(-d.y, d.x);
    let (lo, hi) = band;
    let z0 = base_z + wall.base_offset;
    // an attached top comes down no lower than the heads of openings
    let floor = vs[vs.len().saturating_sub(2)].max(0.0) + 0.01;
    let p = |u: f64, v: f64, w: f64| -> DVec3 {
        // the end grid lines and gap sides follow the joins
        let u = ends.place(wall, u, w, core);
        let xy = wall.start + d * u + n2 * w;
        let z = match top {
            Some(t) if v >= h - 1e-9 => t.z_at(xy).map_or(z0 + h, |z| z.max(z0 + floor.min(h))),
            _ => z0 + v,
        };
        DVec3::new(xy.x, xy.y, z)
    };
    let along = DVec3::new(d.x, d.y, 0.0);
    let across = DVec3::new(n2.x, n2.y, 0.0);

    let mut m = TriMesh::default();
    for i in 0..ni as isize {
        for j in 0..nj as isize {
            if !solid(i, j) {
                continue;
            }
            let (ua, ub) = (us[i as usize], us[i as usize + 1]);
            let (va, vb) = (vs[j as usize], vs[j as usize + 1]);
            m.add_quad(
                [p(ua, va, hi), p(ub, va, hi), p(ub, vb, hi), p(ua, vb, hi)],
                across,
            );
            m.add_quad(
                [p(ua, va, lo), p(ub, va, lo), p(ub, vb, lo), p(ua, vb, lo)],
                -across,
            );
            if !solid(i - 1, j) {
                m.add_quad(
                    [p(ua, va, lo), p(ua, va, hi), p(ua, vb, hi), p(ua, vb, lo)],
                    end_normal(p(ua, va, lo), p(ua, va, hi), -along),
                );
            }
            if !solid(i + 1, j) {
                m.add_quad(
                    [p(ub, va, lo), p(ub, va, hi), p(ub, vb, hi), p(ub, vb, lo)],
                    end_normal(p(ub, va, lo), p(ub, va, hi), along),
                );
            }
            if !solid(i, j - 1) {
                m.add_quad(
                    [p(ua, va, lo), p(ub, va, lo), p(ub, va, hi), p(ua, va, hi)],
                    -DVec3::Z,
                );
            }
            if !solid(i, j + 1) {
                let q = [p(ua, vb, lo), p(ub, vb, lo), p(ub, vb, hi), p(ua, vb, hi)];
                // an attached top slopes: shade it as it lies
                let n = (q[2] - q[0]).cross(q[3] - q[1]).normalize_or(DVec3::Z);
                m.add_quad(q, if n.z < 0.0 { -n } else { n });
            }
        }
    }
    m
}

/// Normal of a vertical end face running from `a` to `b` in plan, facing `towards`.
fn end_normal(a: DVec3, b: DVec3, towards: DVec3) -> DVec3 {
    let e = b - a;
    let n = DVec3::new(-e.y, e.x, 0.0).normalize_or_zero();
    if n == DVec3::ZERO {
        towards
    } else if n.dot(towards) < 0.0 {
        -n
    } else {
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::walls::TopFace;
    use creabim_core::{ElementId, Gap, OpeningKind};

    fn wall() -> Wall {
        Wall::new(
            ElementId::new(),
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 6.0),
            0.2,
            3.0,
        )
    }

    fn opening(offset: f64, sill: f64, w: f64, h: f64) -> Opening {
        Opening {
            host: ElementId::new(),
            kind: OpeningKind::Window,
            offset,
            sill,
            width: w,
            height: h,
            material: None,
            frame: Default::default(),
            flip_hand: false,
            flip_facing: false,
        }
    }

    #[test]
    fn a_crossing_wall_leaves_a_gap() {
        let ends = WallEnds {
            gaps: vec![Gap {
                lo: [2.0, 2.0],
                hi: [2.3, 2.3],
                core: [0.0; 2],
            }],
            ..Default::default()
        };
        let m = wall_mesh(&wall(), 0.0, &[], &ends, None);
        assert!(
            (m.volume() - 4.7 * 3.0 * 0.2).abs() < 1e-5,
            "{}",
            m.volume()
        );
    }

    #[test]
    fn an_attached_top_follows_the_slope() {
        // an underside over the whole wall, rising along it from 2.5 m at
        // its start to 3.5 m at its end
        let top = TopProfile {
            faces: vec![TopFace {
                outline: vec![
                    DVec2::new(0.0, 0.0),
                    DVec2::new(2.0, 0.0),
                    DVec2::new(2.0, 7.0),
                    DVec2::new(0.0, 7.0),
                ],
                g: DVec2::new(0.0, 0.2),
                c: 2.3,
            }],
        };
        let m = wall_mesh(&wall(), 0.0, &[], &WallEnds::default(), Some(&top));
        assert!(
            (m.volume() - 5.0 * 3.0 * 0.2).abs() < 1e-5,
            "{}",
            m.volume()
        );
        let (_, hi) = m.bounds().unwrap();
        assert!((hi.z - 3.5).abs() < 1e-5);
        // where nothing is overhead it keeps its own height
        let mut away = top.clone();
        away.faces[0].outline = vec![
            DVec2::new(10.0, 0.0),
            DVec2::new(12.0, 0.0),
            DVec2::new(12.0, 2.0),
        ];
        let m = wall_mesh(&wall(), 0.0, &[], &WallEnds::default(), Some(&away));
        assert!((m.volume() - 5.0 * 3.0 * 0.2).abs() < 1e-5);
    }

    #[test]
    fn plain_wall_is_a_closed_box() {
        let m = wall_mesh(&wall(), 0.0, &[], &WallEnds::default(), None);
        assert_eq!(m.triangle_count(), 12);
        assert!((m.volume() - 5.0 * 3.0 * 0.2).abs() < 1e-5);
    }

    #[test]
    fn openings_remove_their_volume() {
        let win = opening(1.0, 0.9, 1.2, 1.2);
        let door = opening(3.0, 0.0, 0.9, 2.1);
        let m = wall_mesh(&wall(), 3.0, &[&win, &door], &WallEnds::default(), None);
        let expected = (15.0 - 1.44 - 0.9 * 2.1) * 0.2;
        assert!((m.volume() - expected).abs() < 1e-4, "{}", m.volume());
        let (lo, hi) = m.bounds().unwrap();
        assert!((lo.z - 3.0).abs() < 1e-6 && (hi.z - 6.0).abs() < 1e-6);
    }

    #[test]
    fn opening_past_the_wall_end_is_clipped() {
        let o = opening(4.5, 1.0, 2.0, 1.0);
        let m = wall_mesh(&wall(), 0.0, &[&o], &WallEnds::default(), None);
        assert!((m.volume() - (15.0 - 0.5) * 0.2).abs() < 1e-4);
    }
}
