//! How walls keep their place as things around them change, as in Revit:
//!
//! - the location line a wall is drawn along stays put when the wall gets
//!   thicker, changes assembly or is flipped (it flips about that line);
//! - a top tied to a level follows that level, and the base follows its
//!   own, so moving or changing a level resizes the walls tied to it;
//! - a top attached to a roof or slab follows its underside, cut to the
//!   slope ([`top_profile`]).
//!
//! The first two are kept by [`settle`], which the [`crate::Document`] runs
//! after every edit, so each holds whatever made the change.

use crate::{
    assembly, ElementId, ElementKind, LocationLine, Model, Op, Opening, Transaction, Wall, WallTop,
};
use glam::DVec2;

/// The span across a wall of its core, `(exterior side, interior side)`,
/// left of start → end positive. A wall without layers is all core.
fn core_span(model: &Model, wall: &Wall) -> (f64, f64) {
    let t = wall.thickness / 2.0;
    let (lo, hi) = wall
        .assembly
        .and_then(|a| match &model.get(a)?.kind {
            ElementKind::Assembly(asm) if !asm.layers.is_empty() && asm.total() > 1e-9 => {
                Some(assembly::wall_core(asm, wall))
            }
            _ => None,
        })
        .unwrap_or((-t, t));
    if wall.flip {
        (hi, lo)
    } else {
        (lo, hi)
    }
}

/// How far the wall's location line is from its centreline, across the
/// wall (left of start → end positive).
pub fn location_offset(model: &Model, wall: &Wall) -> f64 {
    offset_of(model, wall, wall.location)
}

/// Where `line` would lie across `wall`, from its centreline.
pub fn offset_of(model: &Model, wall: &Wall, line: LocationLine) -> f64 {
    let t = wall.thickness / 2.0;
    // the exterior is on the right unless the wall is flipped
    let ext = if wall.flip { t } else { -t };
    match line {
        LocationLine::Centre => 0.0,
        LocationLine::ExteriorFace => ext,
        LocationLine::InteriorFace => -ext,
        LocationLine::CoreCentre => {
            let (a, b) = core_span(model, wall);
            (a + b) / 2.0
        }
        LocationLine::CoreExterior => core_span(model, wall).0,
        LocationLine::CoreInterior => core_span(model, wall).1,
    }
}

/// The wall's location line, start to end.
pub fn location_line(model: &Model, wall: &Wall) -> (DVec2, DVec2) {
    let n = wall.direction().perp() * location_offset(model, wall);
    (wall.start + n, wall.end + n)
}

/// Put the wall's location line at `a` → `b`: its centreline moves to suit.
pub fn set_location_line(model: &Model, wall: &mut Wall, a: DVec2, b: DVec2) {
    let n = (b - a).normalize_or_zero().perp() * location_offset(model, wall);
    wall.start = a - n;
    wall.end = b - n;
}

/// The absolute height of the wall's base.
pub fn base_z(model: &Model, wall: &Wall) -> f64 {
    model.level(wall.level).map_or(0.0, |l| l.elevation) + wall.base_offset
}

/// The height the wall's top constraint gives it, when it is tied to a
/// level that exists.
pub fn constrained_height(model: &Model, wall: &Wall) -> Option<f64> {
    let WallTop::UpToLevel { level, offset } = wall.top else {
        return None;
    };
    let top = model.level(level)?.elevation + offset;
    Some(top - base_z(model, wall))
}

/// The lowest a wall's top may come down to its base.
pub const MIN_HEIGHT: f64 = 0.05;

/// A wall replaced by an edit, with where its location line was before.
#[derive(Clone, Debug)]
pub struct Kept {
    id: ElementId,
    line: (DVec2, DVec2),
}

/// The walls whose location line `tx` must leave where it is: those it
/// makes thicker or thinner, gives another assembly or flips, without
/// moving their ends or choosing another line. Read before `tx` applies.
pub fn lines_kept(model: &Model, tx: &Transaction) -> Vec<Kept> {
    tx.ops
        .iter()
        .filter_map(|op| {
            let Op::Replace { before, after } = op else {
                return None;
            };
            let (ElementKind::Wall(b), ElementKind::Wall(a)) = (&before.kind, &after.kind) else {
                return None;
            };
            let same_place = a.start == b.start && a.end == b.end && a.location == b.location;
            let reshaped =
                a.thickness != b.thickness || a.flip != b.flip || a.assembly != b.assembly;
            (same_place && reshaped && !b.location.is_centre()).then(|| Kept {
                id: before.id,
                line: location_line(model, b),
            })
        })
        .collect()
}

/// The edits that keep the model's walls where they belong after an edit:
/// the location lines in `kept` back where they were, and walls tied to a
/// level as tall as the levels make them. `levels` says whether the edit
/// touched levels or walls at all (else only `kept` is looked at).
pub fn settle(model: &Model, kept: &[Kept], levels: bool) -> Transaction {
    let mut tx = Transaction::new("Settle walls");
    let mut changed: Vec<crate::Element> = vec![];
    for k in kept {
        let Some(e) = model.get(k.id) else { continue };
        let ElementKind::Wall(w) = &e.kind else {
            continue;
        };
        let now = location_line(model, w);
        if now.0.distance(k.line.0) < 1e-9 && now.1.distance(k.line.1) < 1e-9 {
            continue;
        }
        let mut after = e.clone();
        if let ElementKind::Wall(nw) = &mut after.kind {
            set_location_line(model, nw, k.line.0, k.line.1);
        }
        changed.push(after);
    }
    if levels {
        for e in model.iter() {
            let ElementKind::Wall(w) = &e.kind else {
                continue;
            };
            let WallTop::UpToLevel { level, .. } = w.top else {
                continue;
            };
            let current = changed.iter().position(|c| c.id == e.id);
            let mut after = current.map_or_else(|| e.clone(), |i| changed[i].clone());
            let ElementKind::Wall(nw) = &mut after.kind else {
                continue;
            };
            if model.level(level).is_none() {
                // its top level is gone: it keeps the height it had
                nw.top = WallTop::Unconnected;
            } else if let Some(h) = constrained_height(model, nw) {
                if h >= MIN_HEIGHT && (h - nw.height).abs() > 1e-9 {
                    nw.height = h;
                }
            }
            if after != *e {
                match current {
                    Some(i) => changed[i] = after,
                    None => changed.push(after),
                }
            }
        }
    }
    for after in changed {
        if let Some(before) = model.get(after.id) {
            if after.validate().is_ok() {
                tx = tx.replace(before.clone(), after);
            }
        }
    }
    tx
}

/// Does `tx` touch levels or walls, so wall heights may need settling?
pub fn touches_walls(tx: &Transaction) -> bool {
    tx.ops.iter().any(|op| {
        let e = match op {
            Op::Insert(e) | Op::Remove(e) => e,
            Op::Replace { after, .. } => after,
        };
        matches!(e.kind, ElementKind::Level(_) | ElementKind::Wall(_))
    })
}

// ------------------------------------------------------------ attached tops

/// One flat piece of the underside a wall's top is attached to: its
/// outline in plan and the plane `z = c + g · p` (absolute heights).
#[derive(Clone, Debug, PartialEq)]
pub struct TopFace {
    pub outline: Vec<DVec2>,
    pub g: DVec2,
    pub c: f64,
}

impl TopFace {
    pub fn z(&self, p: DVec2) -> f64 {
        self.c + self.g.dot(p)
    }

    fn contains(&self, p: DVec2) -> bool {
        let poly = &self.outline;
        let n = poly.len();
        let mut inside = false;
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            if (a.y > p.y) != (b.y > p.y) {
                let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
                if p.x < x {
                    inside = !inside;
                }
            }
        }
        inside
    }
}

/// The undersides over a wall that its top follows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TopProfile {
    pub faces: Vec<TopFace>,
}

impl TopProfile {
    /// The height of the top at `p` in plan: the lowest underside over it,
    /// or `None` where nothing it is attached to is overhead.
    pub fn z_at(&self, p: DVec2) -> Option<f64> {
        self.faces
            .iter()
            .filter(|f| f.contains(p))
            .map(|f| f.z(p))
            .fold(None, |m: Option<f64>, z| Some(m.map_or(z, |m| m.min(z))))
    }

    /// Where along `a` → `b` (0..1) it crosses the edge of a face: where
    /// the top bends or steps.
    pub fn breaks(&self, a: DVec2, b: DVec2) -> Vec<f64> {
        let d = b - a;
        let mut out = vec![];
        for f in &self.faces {
            let n = f.outline.len();
            for i in 0..n {
                let (p, q) = (f.outline[i], f.outline[(i + 1) % n]);
                let e = q - p;
                let den = d.perp_dot(e);
                if den.abs() < 1e-12 {
                    continue;
                }
                let t = (p - a).perp_dot(e) / den;
                let s = (p - a).perp_dot(d) / den;
                if t > 1e-6 && t < 1.0 - 1e-6 && (-1e-9..=1.0 + 1e-9).contains(&s) {
                    out.push(t);
                }
            }
        }
        out.sort_by(f64::total_cmp);
        out.dedup_by(|x, y| (*x - *y).abs() < 1e-6);
        out
    }
}

/// The undersides of the roofs and slabs `ids`, as flat pieces.
pub fn undersides(model: &Model, ids: &[ElementId]) -> Vec<TopFace> {
    let mut out = vec![];
    for id in ids {
        match model.get(*id).map(|e| &e.kind) {
            Some(ElementKind::Roof(r)) => {
                let base = r.base(model);
                out.extend(r.faces().into_iter().map(|f| TopFace {
                    outline: f.outline,
                    g: f.g,
                    c: base + f.c,
                }));
            }
            Some(ElementKind::Slab(s)) => {
                let elevation = model.level(s.level).map_or(0.0, |l| l.elevation);
                out.push(TopFace {
                    outline: s.outline(),
                    g: DVec2::ZERO,
                    c: s.top(elevation) - s.thickness,
                });
            }
            // a ceiling stops at the walls' faces: it holds up the
            // partitions under its edges too, so it reaches over them
            Some(ElementKind::Ceiling(c)) => {
                if let Some(outline) = c.outline(model) {
                    out.push(TopFace {
                        outline: grown(&outline, CEILING_REACH),
                        g: DVec2::ZERO,
                        c: c.span(model).0,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

/// How far past its outline a ceiling holds up the walls under it: over
/// half the thickest partition, whose face its edge stops at.
pub const CEILING_REACH: f64 = 0.15;

/// `poly` (counter-clockwise) with every edge pushed out by `d`, corners
/// mitred.
pub fn grown(poly: &[DVec2], d: f64) -> Vec<DVec2> {
    let n = poly.len();
    if n < 3 {
        return poly.to_vec();
    }
    let ccw = crate::polygon::signed_area2(poly) >= 0.0;
    let s = if ccw { d } else { -d };
    (0..n)
        .map(|i| {
            let (a, b, c) = (poly[(i + n - 1) % n], poly[i], poly[(i + 1) % n]);
            // outward normals of the edges either side (right of a ccw edge)
            let n1 = -(b - a).normalize_or_zero().perp();
            let n2 = -(c - b).normalize_or_zero().perp();
            let m = n1 + n2;
            let k = 1.0 + n1.dot(n2);
            if k < 1e-6 {
                b + n1 * s
            } else {
                b + m * (s / k)
            }
        })
        .collect()
}

/// What a wall's top follows, when it is attached to something that is
/// still there.
pub fn top_profile(model: &Model, wall: &Wall) -> Option<TopProfile> {
    if wall.attach_top.is_empty() {
        return None;
    }
    let faces = undersides(model, &wall.attach_top);
    (!faces.is_empty()).then_some(TopProfile { faces })
}

/// The roofs, slabs and ceilings a wall's top would attach to: the lowest ones
/// overhead (their underside above the wall's base) over some of it.
pub fn attach_candidates(model: &Model, wall_id: ElementId) -> Vec<ElementId> {
    let Some(ElementKind::Wall(w)) = model.get(wall_id).map(|e| &e.kind) else {
        return vec![];
    };
    let base = base_z(model, w);
    let samples: Vec<DVec2> = (0..=8)
        .map(|i| w.start.lerp(w.end, i as f64 / 8.0))
        .collect();
    let mut found: Vec<(ElementId, f64)> = vec![];
    for e in model.iter() {
        if !matches!(
            e.kind,
            ElementKind::Roof(_) | ElementKind::Slab(_) | ElementKind::Ceiling(_)
        ) {
            continue;
        }
        let faces = undersides(model, &[e.id]);
        let profile = TopProfile { faces };
        let low = samples
            .iter()
            .filter_map(|p| profile.z_at(*p))
            .filter(|z| *z > base + MIN_HEIGHT)
            .fold(None, |m: Option<f64>, z| Some(m.map_or(z, |m| m.min(z))));
        if let Some(z) = low {
            found.push((e.id, z));
        }
    }
    // the nearest overhead, and anything else about as near (a roof and the
    // slab it sits on)
    let Some(lowest) = found.iter().map(|f| f.1).reduce(f64::min) else {
        return vec![];
    };
    found
        .into_iter()
        .filter(|f| f.1 < lowest + 0.5)
        .map(|f| f.0)
        .collect()
}

// --------------------------------------------------------------- flipping

/// The wall turned round: its exterior on the other side. It flips about
/// its location line ([`settle`] keeps the line where it was), and its
/// doors and windows turn with it.
pub fn flip_wall(wall: &mut Wall) {
    wall.flip = !wall.flip;
}

/// A door hinged on its other jamb.
pub fn flip_hand(o: &mut Opening) {
    o.flip_hand = !o.flip_hand;
}

/// A door opening the other way through the wall; a window facing out of
/// the other face.
pub fn flip_facing(o: &mut Opening) {
    o.flip_facing = !o.flip_facing;
}

/// Split wall `id` in two at the point of it nearest `at`: it keeps the
/// part from its start, a new wall the rest; the doors and windows past
/// the split move to the new one. Refused at an end, or through an
/// opening.
pub fn split_wall(model: &Model, id: ElementId, at: DVec2) -> Result<Transaction, String> {
    let e = model.get(id).ok_or("that wall is gone")?;
    let ElementKind::Wall(w) = &e.kind else {
        return Err("only a wall splits".into());
    };
    let len = w.length();
    let u = w.direction();
    let along = (at - w.start).dot(u);
    if along < 0.05 || along > len - 0.05 {
        return Err("too close to an end of the wall to split it there".into());
    }
    let p = w.start + u * along;
    let mut first = e.clone();
    let mut second = e.clone();
    second.id = ElementId::new();
    second.source = None;
    second.name = format!("{} (2)", e.name);
    if let ElementKind::Wall(a) = &mut first.kind {
        a.end = p;
        a.disallow_join[1] = false;
    }
    if let ElementKind::Wall(b) = &mut second.kind {
        b.start = p;
        b.disallow_join[0] = false;
    }
    let mut tx = Transaction::new(format!("Split {}", e.name)).insert(second.clone());
    for o in model.iter() {
        let ElementKind::Opening(op) = &o.kind else {
            continue;
        };
        if op.host != id {
            continue;
        }
        if op.offset < along && op.offset + op.width > along {
            return Err(format!(
                "{} is in the way: split the wall beside it",
                o.name
            ));
        }
        if op.offset >= along {
            let mut moved = o.clone();
            if let ElementKind::Opening(m) = &mut moved.kind {
                m.host = second.id;
                m.offset -= along;
            }
            tx = tx.replace(o.clone(), moved);
        }
    }
    Ok(tx.replace(e.clone(), first))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Element, Level};

    fn setup() -> (Document, ElementId, ElementId, ElementId) {
        let mut doc = Document::new();
        let l0 = Element::new(
            "L0",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 3.0,
            }),
        );
        let l1 = Element::new(
            "L1",
            ElementKind::Level(Level {
                elevation: 3.0,
                height: 3.0,
            }),
        );
        let (a, b) = (l0.id, l1.id);
        let mut w = Wall::new(a, DVec2::ZERO, DVec2::new(5.0, 0.0), 0.2, 2.5);
        w.top = WallTop::UpToLevel {
            level: b,
            offset: -0.2,
        };
        let we = Element::new("W", ElementKind::Wall(w));
        let wid = we.id;
        doc.commit(Transaction::new("setup").insert(l0).insert(l1).insert(we))
            .unwrap();
        (doc, a, b, wid)
    }

    fn wall(doc: &Document, id: ElementId) -> Wall {
        match &doc.model().get(id).unwrap().kind {
            ElementKind::Wall(w) => w.clone(),
            _ => unreachable!(),
        }
    }

    #[test]
    fn a_wall_tied_to_a_level_follows_it() {
        let (mut doc, ground, first, wid) = setup();
        // inserting it already settled its height: up to 3.0 - 0.2
        assert!((wall(&doc, wid).height - 2.8).abs() < 1e-9);
        doc.commit(crate::levels::set(doc.model(), first, 3.5, 3.0))
            .unwrap();
        assert!((wall(&doc, wid).height - 3.3).abs() < 1e-9);
        // the base level moving up shortens it from below
        doc.commit(crate::levels::set(doc.model(), ground, 0.5, 3.0))
            .unwrap();
        assert!((wall(&doc, wid).height - 2.8).abs() < 1e-9);
        // and one undo puts each step back
        doc.undo();
        assert!((wall(&doc, wid).height - 3.3).abs() < 1e-9);
        // a base offset raises the base; the top stays at the level
        doc.edit(wid, "base", |e| {
            if let ElementKind::Wall(w) = &mut e.kind {
                w.base_offset = 0.3;
            }
        })
        .unwrap();
        assert!((wall(&doc, wid).height - 3.0).abs() < 1e-9);
        // its top level deleted (what is on it moved down): it keeps its height
        doc.commit(crate::levels::delete(
            doc.model(),
            first,
            crate::levels::OnDelete::MoveTo(ground),
        ))
        .unwrap();
        let w = wall(&doc, wid);
        assert_eq!(w.top, WallTop::Unconnected);
        assert!((w.height - 3.0).abs() < 1e-9);
    }

    #[test]
    fn a_thicker_wall_keeps_its_location_line() {
        let (mut doc, _, _, wid) = setup();
        doc.edit(wid, "loc", |e| {
            if let ElementKind::Wall(w) = &mut e.kind {
                w.location = LocationLine::ExteriorFace;
            }
        })
        .unwrap();
        // choosing the line moves nothing
        assert_eq!(wall(&doc, wid).start, DVec2::ZERO);
        let line = location_line(doc.model(), &wall(&doc, wid));
        assert!(line.0.distance(DVec2::new(0.0, -0.1)) < 1e-9);
        doc.edit(wid, "thick", |e| {
            if let ElementKind::Wall(w) = &mut e.kind {
                w.thickness = 0.5;
            }
        })
        .unwrap();
        let w = wall(&doc, wid);
        assert!((location_line(doc.model(), &w).0 - line.0).length() < 1e-9);
        // the wall grew inwards, to its left
        assert!(
            w.start.distance(DVec2::new(0.0, 0.15)) < 1e-9,
            "{:?}",
            w.start
        );
        // flipped, it turns about the exterior face: the body is now right of it
        doc.edit(wid, "flip", |e| {
            if let ElementKind::Wall(w) = &mut e.kind {
                flip_wall(w);
            }
        })
        .unwrap();
        let w = wall(&doc, wid);
        assert!((location_line(doc.model(), &w).0 - line.0).length() < 1e-9);
        assert!(
            w.start.distance(DVec2::new(0.0, -0.35)) < 1e-9,
            "{:?}",
            w.start
        );
        // undoing the flip puts the centreline back too
        doc.undo();
        assert!(wall(&doc, wid).start.distance(DVec2::new(0.0, 0.15)) < 1e-9);
    }

    #[test]
    fn walls_from_older_files_are_unconnected_centre_walls() {
        let level = serde_json::to_string(&ElementId::new()).unwrap();
        let old = format!(
            r#"{{"level":{level},"start":[0.0,0.0],"end":[4.0,0.0],"thickness":0.2,"height":3.0,"base_offset":0.0,"material":null}}"#
        );
        let w: Wall = serde_json::from_str(&old).unwrap();
        assert_eq!(w.location, LocationLine::Centre);
        assert_eq!(w.top, WallTop::Unconnected);
        assert_eq!(w.disallow_join, [false; 2]);
        assert!(w.attach_top.is_empty());
        assert_eq!(w.height, 3.0);
        // the defaults write nothing; set, they round-trip
        let text = serde_json::to_string(&w).unwrap();
        for k in ["location", "top", "disallow_join", "attach_top"] {
            assert!(!text.contains(k), "{text}");
        }
        let mut w = w;
        w.location = LocationLine::CoreInterior;
        w.top = WallTop::UpToLevel {
            level: ElementId::new(),
            offset: 0.25,
        };
        w.disallow_join = [false, true];
        w.attach_top = vec![ElementId::new()];
        let text = serde_json::to_string(&w).unwrap();
        assert_eq!(serde_json::from_str::<Wall>(&text).unwrap(), w);
    }

    #[test]
    fn openings_from_older_files_are_not_flipped() {
        let host = serde_json::to_string(&ElementId::new()).unwrap();
        let old = format!(
            r#"{{"host":{host},"kind":"Door","offset":1.0,"sill":0.0,"width":0.9,"height":2.1}}"#
        );
        let o: Opening = serde_json::from_str(&old).unwrap();
        assert!(!o.flip_hand && !o.flip_facing);
        assert!(!serde_json::to_string(&o).unwrap().contains("flip"));
        // a door opens into the interior: the left of an unflipped wall
        let w = Wall::new(ElementId::new(), DVec2::ZERO, DVec2::X, 0.2, 3.0);
        assert!(o.faces_left(&w));
        let mut o = o;
        flip_facing(&mut o);
        assert!(!o.faces_left(&w));
        let mut fw = w.clone();
        flip_wall(&mut fw);
        assert!(o.faces_left(&fw), "a flipped wall turns its doors round");
    }

    #[test]
    fn a_top_attached_to_a_gable_follows_its_slopes() {
        let (doc, ground, _, wid) = setup();
        let mut m = doc.model().clone();
        let r = crate::Roof::new(
            ground,
            vec![
                DVec2::new(-1.0, -1.0),
                DVec2::new(6.0, -1.0),
                DVec2::new(6.0, 1.0),
                DVec2::new(-1.0, 1.0),
            ],
            crate::RoofShape::Hip,
        );
        let re = Element::new("Roof", ElementKind::Roof(r));
        let rid = re.id;
        let mut d = Document::from_model(m.clone());
        d.commit(Transaction::new("roof").insert(re)).unwrap();
        m = d.model().clone();
        // on a level at 0 its base is at 0: lift it over the wall
        let mut d = Document::from_model(m);
        d.edit(rid, "lift", |e| {
            if let ElementKind::Roof(r) = &mut e.kind {
                r.base_offset = 2.0;
            }
        })
        .unwrap();
        assert_eq!(attach_candidates(d.model(), wid), vec![rid]);
        let mut w = wall(&d, wid);
        w.attach_top = vec![rid];
        let p = top_profile(d.model(), &w).unwrap();
        // the wall runs along the ridge's line (y = 0), 1 m in from each eave
        let slope = 30f64.to_radians().tan();
        let mid = p.z_at(DVec2::new(2.5, 0.0)).unwrap();
        assert!((mid - (2.0 + slope)).abs() < 1e-9, "{mid}");
        // near the hipped end the roof comes down
        let end = p.z_at(DVec2::new(5.5, 0.0)).unwrap();
        assert!(end < mid - 0.1);
        assert!(p.z_at(DVec2::new(20.0, 0.0)).is_none());
        // off the ridge line, a line along the roof crosses its hips
        assert_eq!(
            p.breaks(DVec2::new(-2.0, 0.3), DVec2::new(8.0, 0.3)).len(),
            4
        );
    }

    #[test]
    fn a_wall_splits_in_two_and_its_openings_go_with_their_half() {
        let m = crate::sample::sample_house();
        let (host, door) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Opening(o) => Some((o.host, e.id)),
                _ => None,
            })
            .unwrap();
        let ElementKind::Wall(w) = &m.get(host).unwrap().kind else {
            panic!()
        };
        let ElementKind::Opening(o) = &m.get(door).unwrap().kind else {
            panic!()
        };
        let len = w.length();
        // through the opening: refused
        let mid = w.start + w.direction() * (o.offset + o.width / 2.0);
        assert!(split_wall(&m, host, mid).is_err());
        assert!(split_wall(&m, host, w.start).is_err());
        // before the opening: it moves to the second wall
        let cut = o.offset / 2.0;
        assert!(cut > 0.05, "the sample door is clear of the wall start");
        let mut m2 = m.clone();
        split_wall(&m, host, w.start + w.direction() * cut)
            .unwrap()
            .apply(&mut m2)
            .unwrap();
        let ElementKind::Wall(a) = &m2.get(host).unwrap().kind else {
            panic!()
        };
        assert!((a.length() - cut).abs() < 1e-9);
        let ElementKind::Opening(moved) = &m2.get(door).unwrap().kind else {
            panic!()
        };
        assert_ne!(moved.host, host);
        assert!((moved.offset - (o.offset - cut)).abs() < 1e-9);
        let ElementKind::Wall(b) = &m2.get(moved.host).unwrap().kind else {
            panic!()
        };
        assert!((a.length() + b.length() - len).abs() < 1e-9);
    }
}
