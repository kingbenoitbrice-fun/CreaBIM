//! Building parts modelled from a profile: extrusions, sweeps and voids.
//!
//! An extrusion is a plan outline (with holes) pulled straight up between
//! two heights above its level: a chimney breast, a hearth, a plinth
//! block, a pier. A sweep carries a section profile along a path: a
//! coping along a wall's top, a plinth along its base, a kerb, or a flight
//! of steps (a stepped profile along a straight line). A sweep can follow
//! a wall, so it stays on the wall when the wall moves or changes height.
//!
//! Either can be a void instead of a solid: then it cuts every wall, slab,
//! roof, site surface and solid part it meets (a firebox in a chimney, a
//! hole through a slab), and is itself only shown as a see-through
//! outline in 3D. Which elements a void cuts is worked out from where
//! things are, so a void moved away stops cutting.
//!
//! CreaBIM gives these the geometry and nothing more: what a part *is* (a
//! chimney, a coping, a step) is its name, its material and the IFC class
//! the user picks for it.

use crate::{transform::Xform, Element, ElementId, ElementKind, Model};
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

/// The IFC class a part exports as unless the user picks another.
pub const PROXY: &str = "IfcBuildingElementProxy";

/// An extrusion or a sweep, solid or void.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub level: ElementId,
    pub form: Form,
    /// Cuts what it meets instead of being solid.
    #[serde(default, skip_serializing_if = "is_false")]
    pub void: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<ElementId>,
    /// The IFC class it exports as (a void is always an IfcOpeningElement).
    #[serde(default = "proxy", skip_serializing_if = "is_proxy")]
    pub ifc_class: String,
}

fn is_false(v: &bool) -> bool {
    !*v
}
fn proxy() -> String {
    PROXY.into()
}
fn is_proxy(s: &String) -> bool {
    s == PROXY
}

/// How a part's shape is made.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Form {
    /// A plan outline, counter-clockwise, pulled up from `base` above the
    /// level by `height`, less its holes.
    Extrusion {
        outline: Vec<DVec2>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        holes: Vec<Vec<DVec2>>,
        base: f64,
        height: f64,
    },
    /// A section profile carried along a path. The profile is in the
    /// path's own axes: `x` across it (positive to the left, looking
    /// along the path), `y` up from it.
    Sweep { profile: Vec<DVec2>, path: Path },
}

/// What a sweep runs along.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Path {
    /// Through points in plan, `z` above the level. A closed path runs on
    /// from the last point back to the first.
    Points {
        points: Vec<DVec3>,
        #[serde(default, skip_serializing_if = "is_false")]
        closed: bool,
    },
    /// Along a wall's centreline, from its start to its end, at its top or
    /// its base: it follows the wall when the wall moves or grows.
    /// `offset` lifts the path off that edge; `extend` runs it on past the
    /// wall's start and end (negative stops it short).
    Wall {
        wall: ElementId,
        edge: WallEdge,
        #[serde(default)]
        offset: f64,
        #[serde(default)]
        extend: [f64; 2],
    },
}

/// Which edge of a wall a sweep follows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WallEdge {
    #[default]
    Top,
    Base,
}

impl WallEdge {
    pub fn label(self) -> &'static str {
        match self {
            WallEdge::Top => "Top",
            WallEdge::Base => "Base",
        }
    }
}

impl Part {
    /// A solid extrusion of `outline` between `base` and `base + height`
    /// above `level`.
    pub fn extrusion(level: ElementId, mut outline: Vec<DVec2>, base: f64, height: f64) -> Self {
        if crate::polygon::signed_area2(&outline) < 0.0 {
            outline.reverse();
        }
        Self {
            level,
            form: Form::Extrusion {
                outline,
                holes: vec![],
                base,
                height,
            },
            void: false,
            material: None,
            ifc_class: proxy(),
        }
    }

    /// A solid sweep of `profile` along `path`.
    pub fn sweep(level: ElementId, profile: Vec<DVec2>, path: Path) -> Self {
        Self {
            level,
            form: Form::Sweep { profile, path },
            void: false,
            material: None,
            ifc_class: proxy(),
        }
    }

    /// The same part as a void.
    pub fn as_void(mut self) -> Self {
        self.void = true;
        self
    }

    /// "Extrusion", "Sweep", "Void"...: what it is called by default.
    pub fn label(&self) -> &'static str {
        match (&self.form, self.void) {
            (Form::Extrusion { .. }, false) => "Extrusion",
            (Form::Sweep { .. }, false) => "Sweep",
            (Form::Extrusion { .. }, true) => "Void",
            (Form::Sweep { .. }, true) => "Swept void",
        }
    }

    /// The wall a sweep follows.
    pub fn host(&self) -> Option<ElementId> {
        match &self.form {
            Form::Sweep {
                path: Path::Wall { wall, .. },
                ..
            } => Some(*wall),
            _ => None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let finite = |p: &[DVec2]| p.iter().all(|q| q.is_finite());
        match &self.form {
            Form::Extrusion {
                outline,
                holes,
                base,
                height,
            } => {
                if outline.len() < 3 || !finite(outline) {
                    return Err("an extrusion's outline needs three finite corners".into());
                }
                if !(height.is_finite() && *height > 0.0 && base.is_finite()) {
                    return Err(format!("extrusion height must be positive, got {height}"));
                }
                if holes.iter().any(|h| h.len() < 3 || !finite(h)) {
                    return Err("an extrusion's holes need three finite corners".into());
                }
            }
            Form::Sweep { profile, path } => {
                if profile.len() < 3 || !finite(profile) {
                    return Err("a sweep's profile needs three finite corners".into());
                }
                if crate::polygon::signed_area2(profile).abs() < 1e-12 {
                    return Err("a sweep's profile has no area".into());
                }
                match path {
                    Path::Points { points, closed } => {
                        if points.len() < 2 + usize::from(*closed)
                            || points.iter().any(|p| !p.is_finite())
                        {
                            return Err("a sweep's path needs two finite points".into());
                        }
                    }
                    Path::Wall { offset, extend, .. } => {
                        if !offset.is_finite() || !extend.iter().all(|e| e.is_finite()) {
                            return Err("a sweep's wall offsets must be finite".into());
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// The path as built, in model coordinates, and whether it closes.
    /// None for an extrusion, or a sweep whose wall is gone or too short.
    pub fn path(&self, model: &Model) -> Option<(Vec<DVec3>, bool)> {
        let Form::Sweep { path, .. } = &self.form else {
            return None;
        };
        match path {
            Path::Points { points, closed } => {
                let z = crate::members::elevation(model, self.level);
                Some((points.iter().map(|p| *p + DVec3::Z * z).collect(), *closed))
            }
            Path::Wall {
                wall,
                edge,
                offset,
                extend,
            } => {
                let ElementKind::Wall(w) = &model.get(*wall)?.kind else {
                    return None;
                };
                let d = w.direction();
                let (a, b) = (w.start - d * extend[0], w.end + d * extend[1]);
                if (b - a).dot(d) < 1e-3 {
                    return None;
                }
                let base = crate::members::elevation(model, w.level) + w.base_offset;
                let z = match edge {
                    WallEdge::Top => base + w.height,
                    WallEdge::Base => base,
                } + offset;
                Some((vec![a.extend(z), b.extend(z)], false))
            }
        }
    }

    /// The corners of its shape's bounding box, in model coordinates.
    pub fn bounds(&self, model: &Model) -> Option<(DVec3, DVec3)> {
        match &self.form {
            Form::Extrusion {
                outline,
                base,
                height,
                ..
            } => {
                let z = crate::members::elevation(model, self.level) + base;
                let (lo, hi) = flat_bounds(outline)?;
                Some((lo.extend(z), hi.extend(z + height)))
            }
            Form::Sweep { profile, .. } => {
                let (pts, _) = self.path(model)?;
                let (plo, phi) = flat_bounds(profile)?;
                let reach = plo.x.abs().max(phi.x.abs());
                let mut lo = DVec3::splat(f64::INFINITY);
                let mut hi = DVec3::splat(f64::NEG_INFINITY);
                for p in pts {
                    lo = lo.min(p - DVec3::new(reach, reach, -plo.y));
                    hi = hi.max(p + DVec3::new(reach, reach, phi.y));
                }
                // a mitre at a sharp corner reaches further than the profile
                let pad = DVec3::new(reach, reach, 0.0);
                Some((lo - pad, hi + pad))
            }
        }
    }

    /// Where it is, for turning and scaling it about itself.
    pub fn pivot(&self, model: &Model) -> Option<DVec3> {
        let (lo, hi) = self.bounds(model)?;
        Some(((lo + hi) / 2.0).truncate().extend(lo.z))
    }
}

fn flat_bounds(pts: &[DVec2]) -> Option<(DVec2, DVec2)> {
    let first = *pts.first()?;
    Some(
        pts.iter()
            .fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))),
    )
}

// ---- profiles ----

/// A rectangle `width` across and `height` up, centred across the path.
pub fn rectangle(width: f64, height: f64) -> Vec<DVec2> {
    let w = width / 2.0;
    vec![
        DVec2::new(-w, 0.0),
        DVec2::new(w, 0.0),
        DVec2::new(w, height),
        DVec2::new(-w, height),
    ]
}

/// A coping over a wall `wall` thick: `depth` deep, standing `overhang`
/// proud of both faces, with a drip groove's worth of step under each
/// overhang when there is room.
pub fn coping(wall: f64, overhang: f64, depth: f64) -> Vec<DVec2> {
    let (w, o) = (wall / 2.0, overhang.max(0.0));
    if o < 0.01 {
        return rectangle(wall, depth);
    }
    // the overhang's underside is a little up from the wall top
    let lip = (depth / 3.0).min(0.015);
    vec![
        DVec2::new(-w, 0.0),
        DVec2::new(w, 0.0),
        DVec2::new(w, lip),
        DVec2::new(w + o, lip),
        DVec2::new(w + o, depth),
        DVec2::new(-w - o, depth),
        DVec2::new(-w - o, lip),
        DVec2::new(-w, lip),
    ]
}

/// A plinth along a wall `wall` thick: `height` high, standing `proud` out
/// of the wall's right face (its exterior, unless the wall is flipped),
/// or out of both faces with `both`.
pub fn plinth(wall: f64, proud: f64, height: f64, both: bool) -> Vec<DVec2> {
    let w = wall / 2.0;
    let left = if both { -w - proud } else { -w };
    vec![
        DVec2::new(left, 0.0),
        DVec2::new(w + proud, 0.0),
        DVec2::new(w + proud, height),
        DVec2::new(left, height),
    ]
    .into_iter()
    .map(|p| DVec2::new(-p.x, p.y))
    .rev()
    .collect()
}

/// A flight of `count` steps, `rise` high and `going` deep each, `width`
/// wide: its profile across a path that runs along the nosings' line
/// (from side to side), so the steps climb to the path's left. Swept
/// along a straight path `width` long, it is a flight of steps as one
/// element.
pub fn steps(count: u32, rise: f64, going: f64) -> Vec<DVec2> {
    let n = count.max(1);
    // the profile climbs towards -x: the path's right side is the bottom
    let mut pts = vec![DVec2::new(0.0, 0.0)];
    for i in 0..n {
        let x = -(i as f64) * going;
        let z = (i + 1) as f64 * rise;
        pts.push(DVec2::new(x, z));
        pts.push(DVec2::new(x - going, z));
    }
    pts.push(DVec2::new(-(n as f64) * going, 0.0));
    // counter-clockwise, as profiles are
    if crate::polygon::signed_area2(&pts) < 0.0 {
        pts.reverse();
    }
    pts
}

// ---- voids ----

/// Can a void cut elements of this kind?
fn cuttable(kind: &ElementKind) -> bool {
    match kind {
        ElementKind::Wall(w) => w.curtain.is_none(),
        ElementKind::Slab(_) | ElementKind::Roof(_) => true,
        ElementKind::Part(p) => !p.void,
        ElementKind::Site(s) => s.cuttable(),
        _ => false,
    }
}

/// The corners of the box an element fills, roughly (a little generous),
/// for the elements voids can cut.
pub fn element_bounds(model: &Model, e: &Element) -> Option<(DVec3, DVec3)> {
    let z0 = |l: ElementId| crate::members::elevation(model, l);
    match &e.kind {
        ElementKind::Wall(w) => {
            let t = DVec2::splat(w.thickness);
            let base = z0(w.level) + w.base_offset;
            let top = if w.attach_top.is_empty() {
                w.height
            } else {
                // attached tops follow roofs over them
                w.height * 2.0 + 1.0
            };
            Some((
                (w.start.min(w.end) - t).extend(base),
                (w.start.max(w.end) + t).extend(base + top),
            ))
        }
        ElementKind::Slab(s) => {
            let (lo, hi) = flat_bounds(&s.outline())?;
            let top = s.top(z0(s.level));
            Some((lo.extend(top - s.thickness), hi.extend(top)))
        }
        ElementKind::Roof(r) => {
            let (lo, hi) = flat_bounds(&r.boundary)?;
            let o = DVec2::splat(r.overhang + 0.1);
            let base = r.base(model);
            Some((
                (lo - o).extend(base - r.vertical_thickness() - 0.1),
                (hi + o).extend(base + r.peak() + r.vertical_thickness() + 0.1),
            ))
        }
        ElementKind::Part(p) => p.bounds(model),
        ElementKind::Site(s) => s.bounds(model),
        _ => None,
    }
}

/// Every void and what it cuts: (target, void), in model order.
pub fn void_cuts(model: &Model) -> Vec<(ElementId, ElementId)> {
    let voids: Vec<(ElementId, (DVec3, DVec3))> = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Part(p) if p.void => Some((e.id, p.bounds(model)?)),
            _ => None,
        })
        .collect();
    if voids.is_empty() {
        return vec![];
    }
    let mut out = vec![];
    for e in model.iter().filter(|e| cuttable(&e.kind)) {
        let Some((lo, hi)) = element_bounds(model, e) else {
            continue;
        };
        for (v, (vlo, vhi)) in &voids {
            let eps = 1e-6;
            if vlo.cmplt(hi - eps).all() && lo.cmplt(*vhi - eps).all() {
                out.push((e.id, *v));
            }
        }
    }
    out
}

/// Is `id` a void?
pub fn is_void(model: &Model, id: ElementId) -> bool {
    matches!(model.get(id).map(|e| &e.kind), Some(ElementKind::Part(p)) if p.void)
}

// ---- editing ----

/// Move, turn or scale a part. `None` when it can't take `t` (it stays
/// upright, and a sweep along a wall moves only with its wall).
pub fn transform(part: &mut Part, t: &Xform) -> Option<()> {
    let dz = match t {
        Xform::Move(d) => d.z,
        _ => 0.0,
    };
    let factor = match t {
        Xform::Scale { factor, .. } => Some(*factor),
        Xform::Stretch { .. } => return None,
        _ => None,
    };
    let mirror = |pts: &mut Vec<DVec2>| {
        if crate::polygon::signed_area2(pts) < 0.0 {
            pts.reverse();
        }
    };
    match &mut part.form {
        Form::Extrusion {
            outline,
            holes,
            base,
            height,
        } => {
            for p in outline.iter_mut().chain(holes.iter_mut().flatten()) {
                *p = t.point2(*p);
            }
            mirror(outline);
            *base += dz;
            if let Some(f) = factor {
                *height *= f;
            }
        }
        Form::Sweep { profile, path } => {
            match path {
                Path::Points { points, .. } => {
                    for p in points.iter_mut() {
                        *p = t.point2(p.truncate()).extend(p.z + dz);
                    }
                }
                // it goes where its wall goes
                Path::Wall { .. } => return Some(()),
            }
            if let Some(f) = factor {
                for p in profile.iter_mut() {
                    *p *= f;
                }
            }
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Level, Slab, Wall};

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
    fn sweep_follows_its_wall() {
        let lv = level();
        let l = lv.id;
        let mut w = Element::new(
            "W",
            ElementKind::Wall(Wall::new(l, DVec2::ZERO, DVec2::new(4.0, 0.0), 0.2, 2.0)),
        );
        let wid = w.id;
        let p = Part::sweep(
            l,
            coping(0.2, 0.03, 0.06),
            Path::Wall {
                wall: wid,
                edge: WallEdge::Top,
                offset: 0.0,
                extend: [0.0, 0.0],
            },
        );
        let m = Model::from_elements([lv.clone(), w.clone()]).unwrap();
        let (pts, closed) = p.path(&m).unwrap();
        assert!(!closed);
        assert_eq!(
            pts,
            vec![DVec3::new(0.0, 0.0, 2.0), DVec3::new(4.0, 0.0, 2.0)]
        );
        // the wall grows and moves; the coping goes with it
        if let ElementKind::Wall(w) = &mut w.kind {
            w.height = 2.5;
            w.end = DVec2::new(6.0, 0.0);
        }
        let m = Model::from_elements([lv, w]).unwrap();
        let (pts, _) = p.path(&m).unwrap();
        assert_eq!(pts[1], DVec3::new(6.0, 0.0, 2.5));
        assert_eq!(p.host(), Some(wid));
    }

    #[test]
    fn a_void_cuts_only_what_it_meets() {
        let lv = level();
        let l = lv.id;
        let near = Element::new(
            "Chimney",
            ElementKind::Part(Part::extrusion(
                l,
                crate::polygon::rectangle(DVec2::ZERO, DVec2::new(1.2, 0.6)),
                0.0,
                3.0,
            )),
        );
        let far = Element::new(
            "Slab",
            ElementKind::Slab(Slab::new(
                l,
                crate::polygon::rectangle(DVec2::new(10.0, 10.0), DVec2::new(12.0, 12.0)),
                0.2,
            )),
        );
        let firebox = Element::new(
            "Firebox",
            ElementKind::Part(
                Part::extrusion(
                    l,
                    crate::polygon::rectangle(DVec2::new(0.3, -0.1), DVec2::new(0.9, 0.4)),
                    0.3,
                    0.7,
                )
                .as_void(),
            ),
        );
        let (cid, vid) = (near.id, firebox.id);
        let m = Model::from_elements([lv, near, far, firebox]).unwrap();
        assert_eq!(void_cuts(&m), vec![(cid, vid)]);
        assert!(is_void(&m, vid) && !is_void(&m, cid));
    }

    #[test]
    fn steps_climb_in_equal_risers() {
        let p = steps(3, 0.15, 0.3);
        assert!(crate::polygon::signed_area2(&p) > 0.0);
        let top = p.iter().map(|q| q.y).fold(0.0, f64::max);
        assert!((top - 0.45).abs() < 1e-9);
        // the area is the sum of the stacked treads: 0.3 * 0.15 * (3 + 2 + 1)
        let area = crate::polygon::signed_area2(&p) / 2.0;
        assert!((area - 0.3 * 0.15 * 6.0).abs() < 1e-9);
    }

    #[test]
    fn parts_move_and_turn() {
        let l = ElementId::new();
        let mut p = Part::extrusion(
            l,
            crate::polygon::rectangle(DVec2::ZERO, DVec2::ONE),
            0.0,
            1.0,
        );
        transform(&mut p, &Xform::Move(DVec3::new(1.0, 0.0, 0.5))).unwrap();
        let Form::Extrusion { outline, base, .. } = &p.form else {
            unreachable!()
        };
        assert_eq!(outline[0], DVec2::new(1.0, 0.0));
        assert_eq!(*base, 0.5);
        assert!(transform(
            &mut p,
            &Xform::Stretch {
                center: DVec3::ZERO,
                factor: DVec3::ONE
            }
        )
        .is_none());
    }
}
