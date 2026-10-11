//! Mirroring the objects of another file. Each object becomes an element
//! that remembers it (`Element::source`), so reading the file again updates
//! that element: a mesh takes the new shape, a wall or slab made from it is
//! refitted, and names, classes and materials given in CreaBIM stay.
//! Objects that carry a CreaBIM id (exported from CreaBIM and edited
//! elsewhere) update the element they came from.

use crate::assign::{fit, shape_of, Assign};
use crate::shape;
use creabim_core::{
    EditMesh, Element, ElementId, ElementKind, FreeMesh, Hatch, Material, Model, Transaction, Wall,
};
use glam::{DVec2, DVec3};
use std::collections::{HashMap, HashSet};

/// One object as read from a file.
#[derive(Clone, Debug, PartialEq)]
pub struct Incoming {
    /// The object's identity in its file (name, or GlobalId).
    pub object: String,
    pub name: String,
    /// Its shape in model coordinates, metres, z up.
    pub mesh: EditMesh,
    /// A kind or class asked for in the file (a Blender custom property, a
    /// FreeCAD IFC type).
    pub hint: Option<Assign>,
    /// The element it was exported from, when it came from CreaBIM.
    pub element: Option<ElementId>,
    /// Material name and sRGB colour.
    pub material: Option<(String, [f32; 3])>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyncReport {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub unchanged: usize,
    /// Objects that could not be read as asked, with why.
    pub notes: Vec<String>,
}

impl Incoming {
    /// A fingerprint of everything read for this object (FNV-1a, stable
    /// across builds since it is saved in projects).
    pub fn stamp(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |bytes: &[u8]| {
            for b in bytes {
                h ^= *b as u64;
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        };
        for v in &self.mesh.vertices {
            for c in v.to_array() {
                eat(&((c * 1e6).round() as i64).to_le_bytes());
            }
        }
        for f in &self.mesh.faces {
            for i in f {
                eat(&i.to_le_bytes());
            }
            eat(b"|");
        }
        eat(format!("{:?}{:?}", self.hint, self.material.as_ref().map(|m| &m.0)).as_bytes());
        h.max(1)
    }
}

impl SyncReport {
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        for (n, what) in [
            (self.added, "added"),
            (self.updated, "updated"),
            (self.removed, "removed"),
        ] {
            if n > 0 {
                parts.push(format!("{n} {what}"));
            }
        }
        if parts.is_empty() {
            parts.push("nothing changed".into());
        }
        let mut s = parts.join(", ");
        if let Some(n) = self.notes.first() {
            s.push_str(&format!(". {n}"));
        }
        s
    }
}

/// The edit that brings the model in line with `incoming`, read from `file`.
pub fn sync(model: &Model, file: &str, incoming: &[Incoming]) -> (Transaction, SyncReport) {
    let mut tx = Transaction::new(format!("Reload {}", file_name(file)));
    let mut report = SyncReport::default();
    let by_object: HashMap<&str, &Element> = model
        .iter()
        .filter_map(|e| {
            let s = e.source.as_ref()?;
            (s.file == file).then_some((s.object.as_str(), e))
        })
        .collect();
    let mut materials: HashMap<String, ElementId> = model
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Material(_)))
        .map(|e| (e.name.clone(), e.id))
        .collect();
    let mut seen: HashSet<ElementId> = HashSet::new();

    for inc in incoming {
        let existing = inc
            .element
            .and_then(|id| model.get(id))
            .or_else(|| by_object.get(inc.object.as_str()).copied());
        let tri = shape::tri(&inc.mesh);
        let stamp = inc.stamp();
        match existing {
            Some(e) => {
                seen.insert(e.id);
                // unchanged in the file: leave it, with any edits made here
                if e.source
                    .as_ref()
                    .is_some_and(|s| s.file == file && s.stamp == stamp)
                {
                    report.unchanged += 1;
                    continue;
                }
                let restamp = |mut el: Element| {
                    if let Some(s) = el.source.as_mut().filter(|s| s.file == file) {
                        s.stamp = stamp;
                    }
                    el
                };
                match update(model, e, inc, &tri) {
                    Ok(Some(after)) => {
                        tx = tx.replace(e.clone(), restamp(after));
                        report.updated += 1;
                    }
                    Ok(None) => {
                        let after = restamp(e.clone());
                        if &after != e {
                            tx = tx.replace(e.clone(), after);
                        }
                        report.unchanged += 1;
                    }
                    Err(why) => {
                        report.unchanged += 1;
                        report.notes.push(format!("{}: {why}", inc.name));
                    }
                }
            }
            None => {
                let material = inc.material.as_ref().map(|(name, color)| {
                    *materials.entry(name.clone()).or_insert_with(|| {
                        let m = Element::new(
                            name.clone(),
                            ElementKind::Material(Material {
                                hatch: Hatch::Empty,
                                color: *color,
                            }),
                        );
                        let id = m.id;
                        tx = std::mem::take(&mut tx).insert(m);
                        id
                    })
                });
                let mut kind = None;
                if let Some(h @ (Assign::Wall | Assign::Slab | Assign::Door | Assign::Window)) =
                    &inc.hint
                {
                    match fit(model, h, &tri, material, None) {
                        Ok(k) => kind = Some(k),
                        Err(why) => report.notes.push(format!("{}: {why}", inc.name)),
                    }
                }
                let kind = match kind {
                    Some(k) => k,
                    None => {
                        let mut f = free_mesh(model, &inc.mesh);
                        f.material = material;
                        if let Some(Assign::Class(c)) = &inc.hint {
                            f.ifc_class = c.clone();
                        }
                        ElementKind::FreeMesh(f)
                    }
                };
                let mut el = Element::new(inc.name.clone(), kind).from_source(file, &inc.object);
                if let Some(s) = el.source.as_mut() {
                    s.stamp = stamp;
                }
                tx = tx.insert(el);
                report.added += 1;
            }
        }
    }

    // objects gone from the file go from the model, unless something else
    // being removed takes them with it
    let gone: Vec<ElementId> = by_object
        .values()
        .filter(|e| !seen.contains(&e.id))
        .map(|e| e.id)
        .collect();
    let gone_set: HashSet<ElementId> = gone.iter().copied().collect();
    for id in &gone {
        let covered = model
            .get(*id)
            .is_some_and(|e| e.references().iter().any(|r| gone_set.contains(r)));
        if !covered {
            tx = tx.remove_cascade(model, *id);
        }
        report.removed += 1;
    }
    (tx, report)
}

fn file_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

/// A free mesh holding `world` (model coordinates), on the level it stands on.
pub fn free_mesh(model: &Model, world: &EditMesh) -> FreeMesh {
    let (lo, hi) = shape::bounds(&world.vertices).unwrap_or_default();
    let (level, elev) = shape::level_at(model, lo.z).map_or((None, 0.0), |(l, z)| (Some(l), z));
    let at = DVec3::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, lo.z);
    let mesh = EditMesh {
        vertices: world.vertices.iter().map(|v| *v - at).collect(),
        faces: world.faces.clone(),
    };
    FreeMesh::new(level, at - DVec3::Z * elev, mesh)
}

/// `e` changed to match the object, or `None` when it already does.
fn update(
    model: &Model,
    e: &Element,
    inc: &Incoming,
    tri: &creabim_geom::mesh::TriMesh,
) -> Result<Option<Element>, String> {
    let current = shape_of(model, e.id);
    let same = current.as_ref().is_some_and(|c| shape::same_shape(c, tri));
    let mut after = e.clone();
    // a kind asked for in the file turns a mesh into it
    if let (ElementKind::FreeMesh(f), Some(h)) = (&e.kind, &inc.hint) {
        match h {
            Assign::Class(c) if &f.ifc_class != c && same => {
                if let ElementKind::FreeMesh(f) = &mut after.kind {
                    f.ifc_class = c.clone();
                }
                return Ok(Some(after));
            }
            Assign::Wall | Assign::Slab | Assign::Door | Assign::Window => {
                after.kind = fit(model, h, tri, f.material, Some(e.id))?;
                return Ok(Some(after));
            }
            _ => {}
        }
    }
    if same {
        return Ok(None);
    }
    after.kind = match &e.kind {
        ElementKind::FreeMesh(f) => {
            let mut g = free_mesh(model, &inc.mesh);
            g.ifc_class = match &inc.hint {
                Some(Assign::Class(c)) => c.clone(),
                _ => f.ifc_class.clone(),
            };
            g.material = f.material;
            ElementKind::FreeMesh(g)
        }
        ElementKind::Wall(w) => match current.as_ref().and_then(|c| refit_wall(w, c, tri)) {
            Some(w) => ElementKind::Wall(w),
            None => fit(model, &Assign::Wall, tri, w.material, Some(e.id))?,
        },
        ElementKind::Slab(s) => fit(model, &Assign::Slab, tri, s.material, Some(e.id))?,
        ElementKind::Opening(o) => {
            let kind = match o.kind {
                creabim_core::OpeningKind::Door => Assign::Door,
                creabim_core::OpeningKind::Window => Assign::Window,
            };
            fit(model, &kind, tri, None, Some(e.id))?
        }
        _ => return Err("this kind of element can't be updated from a mesh yet".into()),
    };
    Ok(Some(after))
}

/// Move a wall's ends, faces, top and base by as much as its shape moved
/// them. Working from the change keeps what the shape doesn't show
/// directly (the overlap at joined corners, the openings) as it was.
/// `None` when the shape turned, which needs a fresh fit.
fn refit_wall(
    w: &Wall,
    before: &creabim_geom::mesh::TriMesh,
    after: &creabim_geom::mesh::TriMesh,
) -> Option<Wall> {
    let d = w.direction();
    let n = d.perp();
    let extents = |m: &creabim_geom::mesh::TriMesh| {
        let pts = shape::points(m);
        let (lo, hi) = shape::bounds(&pts)?;
        let plan: Vec<DVec2> = pts.iter().map(|p| p.truncate()).collect();
        let r = shape::min_rect(&shape::hull(&plan))?;
        let (mut u0, mut u1, mut v0, mut v1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for p in &plan {
            let q = *p - w.start;
            u0 = u0.min(q.dot(d));
            u1 = u1.max(q.dot(d));
            v0 = v0.min(q.dot(n));
            v1 = v1.max(q.dot(n));
        }
        Some((r.along, u0, u1, v0, v1, lo.z, hi.z))
    };
    let (_, bu0, bu1, bv0, bv1, bz0, bz1) = extents(before)?;
    let (along, au0, au1, av0, av1, az0, az1) = extents(after)?;
    if along.dot(d).abs() < (1.0f64).to_radians().cos() {
        return None;
    }
    let shift = ((av0 + av1) - (bv0 + bv1)) / 2.0;
    let mut out = w.clone();
    out.start = w.start + d * (au0 - bu0) + n * shift;
    out.end = w.end + d * (au1 - bu1) + n * shift;
    out.thickness = (w.thickness + (av1 - av0) - (bv1 - bv0)).max(0.01);
    out.base_offset = w.base_offset + (az0 - bz0);
    out.height = (w.height + (az1 - az0) - (bz1 - bz0)).max(0.05);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::{Document, Level};

    fn level_model() -> Model {
        let mut doc = Document::new();
        doc.commit(Transaction::new("t").insert(Element::new(
            "L0",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 3.0,
            }),
        )))
        .unwrap();
        doc.model().clone()
    }

    fn cube(object: &str, at: DVec3, size: DVec3) -> Incoming {
        let mut mesh = EditMesh::cuboid(size);
        for v in &mut mesh.vertices {
            *v += at;
        }
        Incoming {
            object: object.into(),
            name: object.into(),
            mesh,
            hint: None,
            element: None,
            material: Some(("Clay".into(), [0.8, 0.4, 0.3])),
        }
    }

    #[test]
    fn reloading_updates_in_place_keeps_names_and_removes_what_went() {
        let mut doc = Document::from_model(level_model());
        let a = cube("A", DVec3::ZERO, DVec3::ONE);
        let b = cube("B", DVec3::X * 3.0, DVec3::ONE);
        let (tx, r) = sync(doc.model(), "x.blend", &[a.clone(), b.clone()]);
        assert_eq!((r.added, r.updated), (2, 0));
        doc.commit(tx).unwrap();
        // one shared material
        let mats = doc
            .model()
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Material(_)))
            .count();
        assert_eq!(mats, 1);
        let id_a = doc
            .model()
            .iter()
            .find(|e| e.name == "A")
            .map(|e| e.id)
            .unwrap();
        doc.edit_merged(id_a, "Rename", |e| e.name = "Tower".into())
            .unwrap();

        // nothing changed: nothing to do
        let (_, r) = sync(doc.model(), "x.blend", &[a.clone(), b.clone()]);
        assert_eq!((r.added, r.updated, r.unchanged), (0, 0, 2));

        // A grows, B is deleted in Blender
        let a2 = cube("A", DVec3::ZERO, DVec3::new(1.0, 1.0, 4.0));
        let (tx, r) = sync(doc.model(), "x.blend", &[a2]);
        assert_eq!((r.updated, r.removed), (1, 1));
        doc.commit(tx).unwrap();
        let e = doc.model().get(id_a).unwrap();
        assert_eq!(e.name, "Tower");
        let ElementKind::FreeMesh(f) = &e.kind else {
            panic!()
        };
        let (lo, hi) = shape::bounds(&f.mesh.vertices).unwrap();
        assert!((hi.z - lo.z - 4.0).abs() < 1e-9);
        assert_eq!(doc.model().iter().filter(|e| e.source.is_some()).count(), 1);
    }

    #[test]
    fn an_object_made_a_wall_stays_a_wall_and_follows_its_shape() {
        let mut doc = Document::from_model(level_model());
        let a = cube("W", DVec3::new(2.0, 0.0, 0.0), DVec3::new(4.0, 0.2, 3.0));
        let (tx, _) = sync(doc.model(), "x.blend", std::slice::from_ref(&a));
        doc.commit(tx).unwrap();
        let id = doc.model().iter().find(|e| e.name == "W").unwrap().id;
        let wall = crate::assign(doc.model(), id, &Assign::Wall).unwrap();
        let before = doc.model().get(id).unwrap().clone();
        doc.commit(Transaction::new("Assign").replace(before, wall))
            .unwrap();

        // longer in Blender: the wall gets longer, and is still a wall
        let a2 = cube("W", DVec3::new(2.5, 0.0, 0.0), DVec3::new(5.0, 0.2, 3.0));
        let (tx, r) = sync(doc.model(), "x.blend", &[a2]);
        assert_eq!(r.updated, 1, "{r:?}");
        doc.commit(tx).unwrap();
        let ElementKind::Wall(w) = &doc.model().get(id).unwrap().kind else {
            panic!()
        };
        assert!((w.length() - 5.0).abs() < 1e-6, "{}", w.length());
        assert!((w.thickness - 0.2).abs() < 1e-6);
    }

    #[test]
    fn a_kind_asked_for_in_the_file_is_used() {
        let model = level_model();
        let mut a = cube(
            "Floor",
            DVec3::new(0.0, 0.0, -0.3),
            DVec3::new(6.0, 4.0, 0.3),
        );
        a.hint = Some(Assign::Slab);
        let (tx, r) = sync(&model, "x.blend", &[a]);
        assert_eq!(r.added, 1);
        let mut doc = Document::from_model(model);
        doc.commit(tx).unwrap();
        assert!(doc
            .model()
            .iter()
            .any(|e| matches!(e.kind, ElementKind::Slab(_))));
    }
}
