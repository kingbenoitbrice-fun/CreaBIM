use crate::{triangulate::triangulate, wall::wall_mesh, TriMesh};
use creabim_core::{assembly, csg, BoolOp, Element, ElementId, ElementKind, Model, Opening};
use glam::{DVec2, DVec3};

/// Roughness, metallic and opacity of an element without a material.
pub const PLAIN_SURFACE: [f32; 3] = [creabim_core::element::DEFAULT_ROUGHNESS, 0.0, 1.0];

/// One element's renderable geometry.
#[derive(Clone, Debug)]
pub struct SceneItem {
    pub id: ElementId,
    pub mesh: TriMesh,
    pub color: [f32; 3],
    /// Roughness, metallic and opacity, from the element's material.
    pub surface: [f32; 3],
    /// A cutter: drawn as a see-through outline in 3D, left out of
    /// drawings and exports, but still clickable.
    pub ghost: bool,
    /// For one layer of a layered wall, slab or roof: which layer. A
    /// layered element is one item per layer, all with its id.
    pub layer: Option<LayerPart>,
    /// A look of its own over the element's material, by name: a
    /// window's glass, a door's handle.
    pub finish: Option<&'static str>,
}

/// Which layer of its element's assembly a scene item is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerPart {
    pub index: usize,
    pub material: Option<ElementId>,
    pub function: creabim_core::LayerFunction,
    pub core: bool,
}

impl LayerPart {
    fn of(b: &assembly::Band) -> Self {
        Self {
            index: b.index,
            material: b.material,
            function: b.function,
            core: b.core,
        }
    }
}

/// Default colours for layers with no material, by function.
fn layer_color(f: creabim_core::LayerFunction, plain: [f32; 3]) -> [f32; 3] {
    use creabim_core::LayerFunction as F;
    match f {
        F::Insulation => [0.93, 0.86, 0.45],
        F::Membrane => [0.25, 0.27, 0.30],
        F::Finish => [0.92, 0.91, 0.88],
        _ => plain,
    }
}

/// The solid of each layer of a layered element, with its look; air gaps
/// are left empty. `None` for an element that isn't layered.
fn layer_items(
    model: &Model,
    e: &Element,
    joins: &std::collections::HashMap<ElementId, creabim_core::WallEnds>,
    plain: [f32; 3],
) -> Option<Vec<SceneItem>> {
    let (_, a) = assembly::assembly_of(model, e)?;
    let mut linings: Vec<SceneItem> = Vec::new();
    let parts: Vec<(assembly::Band, TriMesh)> = match &e.kind {
        ElementKind::Wall(w) => {
            let level = model.level(w.level)?;
            // its doors and windows, and the curtain walls set in it
            let owned = creabim_core::curtain::wall_openings(model, e.id);
            let openings: Vec<&Opening> = owned.iter().collect();
            let ends = joins.get(&e.id).cloned().unwrap_or_default();
            let top = creabim_core::walls::top_profile(model, w);
            linings = reveal_linings(
                model,
                e.id,
                w,
                &openings,
                &assembly::wall_bands(a, w),
                level.elevation,
                plain,
            );
            assembly::wall_bands(a, w)
                .into_iter()
                .map(|b| {
                    let m = crate::wall::wall_layer_mesh(
                        w,
                        level.elevation,
                        &openings,
                        &ends,
                        top.as_ref(),
                        (b.s0, b.s1),
                        b.core,
                    );
                    (b, m)
                })
                .collect()
        }
        ElementKind::Slab(s) => {
            let level = model.level(s.level)?;
            let top = s.top(level.elevation);
            let outline = s.outline();
            let holes = creabim_core::slab::holes_of(model, e);
            assembly::depth_bands(a, s.thickness)
                .into_iter()
                .map(|b| {
                    let m = slab_mesh_with_holes(&outline, &holes, top - b.s0, b.s1 - b.s0);
                    (b, m)
                })
                .collect()
        }
        ElementKind::Roof(r) => {
            let base = r.base(model);
            // layers are square to the slope; upright they are thicker
            let k = r.vertical_thickness() / r.thickness.max(1e-9);
            let tv = r.vertical_thickness();
            let holes = creabim_core::slab::holes_of(model, e);
            assembly::depth_bands(a, r.thickness)
                .into_iter()
                .map(|b| {
                    let m = crate::members::roof_layer_mesh(r, base, tv - b.s1 * k, tv - b.s0 * k);
                    (b, crate::members::cut_holes(m, &holes))
                })
                .collect()
        }
        _ => return None,
    };
    Some(
        parts
            .into_iter()
            .filter(|(b, _)| b.function != creabim_core::LayerFunction::AirGap)
            .map(|(b, mesh)| {
                let look = b.material.and_then(|m| match &model.get(m)?.kind {
                    ElementKind::Material(m) => Some((m.color, m.surface())),
                    _ => None,
                });
                let (color, surface) =
                    look.unwrap_or((layer_color(b.function, plain), PLAIN_SURFACE));
                SceneItem {
                    id: e.id,
                    mesh,
                    color,
                    surface,
                    ghost: false,
                    layer: Some(LayerPart::of(&b)),
                    finish: None,
                }
            })
            .chain(linings)
            .collect(),
    )
}

/// The [`SceneItem::finish`] of a reveal lining.
pub const REVEAL: &str = "Reveal";

/// How thick a reveal lining is: thin enough to leave the opening its size.
const LINING: f64 = 0.004;

impl SceneItem {
    /// A reveal lining, seen in 3D only: drawings and exports leave it out.
    pub fn is_lining(&self) -> bool {
        self.finish == Some(REVEAL)
    }
}

/// The reveals of a layered wall's doors and windows lined, for the 3D
/// view: each layer's part of the jambs, head and sill in the finish on
/// its side of the wall (render outside, plaster inside), so an opening
/// doesn't show the insulation or the membrane cut open.
fn reveal_linings(
    model: &Model,
    id: ElementId,
    w: &creabim_core::Wall,
    openings: &[&Opening],
    bands: &[assembly::Band],
    level_z: f64,
    plain: [f32; 3],
) -> Vec<SceneItem> {
    use creabim_core::LayerFunction as F;
    let look = |b: &assembly::Band| {
        b.material
            .and_then(|m| match &model.get(m)?.kind {
                ElementKind::Material(m) => Some((m.color, m.surface())),
                _ => None,
            })
            .unwrap_or((layer_color(b.function, plain), PLAIN_SURFACE))
    };
    let finishes: Vec<&assembly::Band> = bands.iter().filter(|b| b.function == F::Finish).collect();
    let d = w.direction();
    let n = DVec2::new(-d.y, d.x);
    let z0 = level_z + w.base_offset;
    let at = |u: f64, v: f64, s: f64| (w.start + d * u + n * s).extend(z0 + v);
    let mut out = Vec::new();
    for o in openings {
        if o.kind == creabim_core::OpeningKind::Hole {
            continue;
        }
        let Some(c) = crate::wall::Cut::of(o, w) else {
            continue;
        };
        for b in bands.iter().filter(|b| b.function != F::AirGap) {
            // a finish shows itself; the rest take the finish nearest them
            let mid = (b.s0 + b.s1) / 2.0;
            let from = if b.function == F::Finish {
                Some(b)
            } else {
                finishes.iter().copied().min_by(|x, y| {
                    ((x.s0 + x.s1) / 2.0 - mid)
                        .abs()
                        .total_cmp(&((y.s0 + y.s1) / 2.0 - mid).abs())
                })
            };
            let (color, surface) = from.map_or((plain, PLAIN_SURFACE), look);
            let mut mesh = TriMesh::default();
            let mut slab = |u0: f64, u1: f64, v0: f64, v1: f64| {
                let p = |i: usize| {
                    at(
                        if i & 1 == 0 { u0 } else { u1 },
                        if i & 2 == 0 { v0 } else { v1 },
                        if i & 4 == 0 { b.s0 } else { b.s1 },
                    )
                };
                cuboid(&mut mesh, [p(0), p(1), p(2), p(3), p(4), p(5), p(6), p(7)]);
            };
            slab(c.u0, c.u0 + LINING, c.v0, c.v1);
            slab(c.u1 - LINING, c.u1, c.v0, c.v1);
            slab(c.u0, c.u1, c.v1 - LINING, c.v1);
            if c.v0 > 1e-6 {
                slab(c.u0, c.u1, c.v0, c.v0 + LINING);
            }
            // one item per finish, for all the wall's openings
            match out.iter_mut().find(|i: &&mut SceneItem| i.color == color) {
                Some(i) => i.mesh.append(&mesh),
                None => out.push(SceneItem {
                    id,
                    mesh,
                    color,
                    surface,
                    ghost: false,
                    layer: None,
                    finish: Some(REVEAL),
                }),
            }
        }
    }
    out
}

/// A six-sided box from its corners, corner `i` having bit 0 for the far
/// end along the first axis, bit 1 along the second and bit 2 along the
/// third.
fn cuboid(m: &mut TriMesh, c: [DVec3; 8]) {
    let centre = c.iter().copied().sum::<DVec3>() / 8.0;
    for face in [
        [0, 1, 3, 2],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 3, 7, 5],
    ] {
        let q = face.map(|i| c[i]);
        let mid = q.iter().copied().sum::<DVec3>() / 4.0;
        let normal = (q[1] - q[0]).cross(q[3] - q[0]);
        let normal = if normal.dot(mid - centre) < 0.0 {
            -normal
        } else {
            normal
        };
        m.add_quad(q, normal);
    }
}

/// The lines of every sketch, in model coordinates: (sketch, from, to).
pub fn sketch_lines(model: &Model) -> Vec<(ElementId, DVec3, DVec3)> {
    let mut out = Vec::new();
    for e in model.iter() {
        if let ElementKind::Sketch(s) = &e.kind {
            let z = s
                .level
                .and_then(|l| model.level(l))
                .map_or(0.0, |l| l.elevation);
            for (a, b) in s.segments() {
                out.push((e.id, a + DVec3::Z * z, b + DVec3::Z * z));
            }
        }
    }
    out
}

/// Build viewport geometry for every element with a body.
pub fn build_scene(model: &Model) -> Vec<SceneItem> {
    let mut out = Vec::new();
    let joins = creabim_core::wall_joins(model);
    for e in model.iter() {
        let plain = match &e.kind {
            ElementKind::Wall(_) => [0.86, 0.84, 0.80],
            ElementKind::Slab(_) => [0.70, 0.70, 0.72],
            _ => [0.66, 0.34, 0.26],
        };
        if let ElementKind::Wall(
            w @ creabim_core::Wall {
                curtain: Some(_), ..
            },
        ) = &e.kind
        {
            out.extend(curtain_items(model, e, w));
            continue;
        }
        if let Some(layers) = layer_items(model, e, &joins, plain) {
            out.extend(layers);
            continue;
        }
        let (mesh, color) = match &e.kind {
            ElementKind::Wall(w) => {
                let Some(level) = model.level(w.level) else {
                    continue;
                };
                // its doors and windows, and the curtain walls set in it
                let owned = creabim_core::curtain::wall_openings(model, e.id);
                let openings: Vec<&Opening> = owned.iter().collect();
                let ends = joins.get(&e.id).cloned().unwrap_or_default();
                let top = creabim_core::walls::top_profile(model, w);
                (
                    wall_mesh(w, level.elevation, &openings, &ends, top.as_ref()),
                    [0.86, 0.84, 0.80],
                )
            }
            ElementKind::Slab(s) => {
                let Some(level) = model.level(s.level) else {
                    continue;
                };
                let holes = creabim_core::slab::holes_of(model, e);
                (
                    slab_mesh_with_holes(&s.outline(), &holes, s.top(level.elevation), s.thickness),
                    [0.70, 0.70, 0.72],
                )
            }
            ElementKind::Shaft(s) => {
                // a see-through stand-in, to see and pick the shaft by
                let (z0, z1) = s.span(model);
                out.push(SceneItem {
                    id: e.id,
                    mesh: slab_mesh(&s.boundary, z1, z1 - z0),
                    color: [0.85, 0.45, 0.25],
                    surface: PLAIN_SURFACE,
                    ghost: true,
                    layer: None,
                    finish: None,
                });
                continue;
            }
            ElementKind::Ceiling(c) => {
                let Some(outline) = c.outline(model) else {
                    continue;
                };
                let (_, top) = c.span(model);
                (slab_mesh(&outline, top, c.thickness), [0.95, 0.95, 0.93])
            }
            ElementKind::Opening(o) => {
                let Some(ElementKind::Wall(w)) = model.get(o.host).map(|h| &h.kind) else {
                    continue;
                };
                let Some(level) = model.level(w.level) else {
                    continue;
                };
                out.extend(opening_items(model, e, o, w, level.elevation));
                continue;
            }
            ElementKind::Object(_) => {
                // one item per part, each in its own material
                for (mesh, material) in creabim_core::objects::model_parts(model, e) {
                    let look = material.and_then(|m| match &model.get(m)?.kind {
                        ElementKind::Material(m) => Some((m.color, m.surface())),
                        _ => None,
                    });
                    let (color, surface) = look.unwrap_or(([0.8, 0.78, 0.74], PLAIN_SURFACE));
                    out.push(SceneItem {
                        id: e.id,
                        mesh: edit_mesh_to_tri(&mesh, DVec3::ZERO),
                        color,
                        surface,
                        ghost: false,
                        layer: None,
                        finish: None,
                    });
                }
                continue;
            }
            ElementKind::FreeMesh(f) => {
                let z = f
                    .level
                    .and_then(|l| model.level(l))
                    .map_or(0.0, |l| l.elevation);
                (
                    edit_mesh_to_tri(&f.evaluated().mesh, f.origin + DVec3::Z * z),
                    [0.62, 0.75, 0.90],
                )
            }
            ElementKind::Level(_)
            | ElementKind::Grid(_)
            | ElementKind::View(_)
            | ElementKind::Sheet(_)
            | ElementKind::Viewport(_)
            | ElementKind::Material(_)
            | ElementKind::Assembly(_)
            | ElementKind::Dimension(_)
            | ElementKind::Tag(_)
            | ElementKind::Note(_)
            | ElementKind::TextStyle(_)
            | ElementKind::SheetTemplate(_)
            | ElementKind::Units(_)
            | ElementKind::MarkRules(_)
            | ElementKind::ElementType(_)
            | ElementKind::Sketch(_)
            | ElementKind::Cut(_)
            | ElementKind::Guide(_)
            | ElementKind::CropBox(_)
            | ElementKind::GridGroup(_)
            | ElementKind::Room(_)
            | ElementKind::RoomSeparator(_) => continue,
            ElementKind::Column(_)
            | ElementKind::Beam(_)
            | ElementKind::Roof(_)
            | ElementKind::Stair(_)
            | ElementKind::Railing(_)
            | ElementKind::Footing(_) => match crate::members::member_mesh(model, e) {
                Some(m) => m,
                None => continue,
            },
        };
        // a material's look wins over the element type's default
        let (color, surface) = creabim_core::material_of(model, e)
            .map_or((color, PLAIN_SURFACE), |(_, m)| (m.color, m.surface()));
        out.push(SceneItem {
            id: e.id,
            mesh,
            color,
            surface,
            ghost: false,
            layer: None,
            finish: None,
        });
    }
    apply_cuts(model, &mut out);
    out
}

/// The scene as drawings show it: cutters left out.
pub fn drawn_scene(model: &Model) -> Vec<SceneItem> {
    let mut s = build_scene(model);
    s.retain(|i| !i.ghost && !i.is_lining());
    s
}

/// One item per element, a layered element's layers in one mesh with its
/// core's look: for exports that take one shape per element.
pub fn merged(items: Vec<SceneItem>) -> Vec<SceneItem> {
    let mut out: Vec<SceneItem> = Vec::with_capacity(items.len());
    let mut at: std::collections::HashMap<ElementId, usize> = std::collections::HashMap::new();
    for item in items.into_iter().filter(|i| !i.is_lining()) {
        match at.get(&item.id) {
            Some(&i) => {
                let o = &mut out[i];
                o.mesh.append(&item.mesh);
                // the first core layer gives the look
                if item.layer.is_some_and(|l| l.core) && !o.layer.is_some_and(|l| l.core) {
                    (o.color, o.surface, o.layer) = (item.color, item.surface, item.layer);
                }
            }
            None => {
                at.insert(item.id, out.len());
                out.push(item);
            }
        }
    }
    for o in &mut out {
        o.layer = None;
    }
    out
}

/// [`build_scene`] with one item per element.
pub fn element_scene(model: &Model) -> Vec<SceneItem> {
    merged(build_scene(model))
}

/// [`drawn_scene`] with one item per element.
pub fn drawn_elements(model: &Model) -> Vec<SceneItem> {
    merged(drawn_scene(model))
}

/// Every cut in the model, in order: (target, cutter, operation).
pub fn cuts(model: &Model) -> Vec<(ElementId, ElementId, BoolOp)> {
    model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Cut(c) => Some((c.target, c.cutter, c.op)),
            _ => None,
        })
        .collect()
}

/// Is `id` used to cut something? Cutters are hidden from drawings.
pub fn is_cutter(model: &Model, id: ElementId) -> bool {
    model
        .iter()
        .any(|e| matches!(&e.kind, ElementKind::Cut(c) if c.cutter == id))
}

/// Apply every cut to the meshes of its target, using the cutters' own
/// shapes (a cutter that is itself cut cuts with its uncut shape, so cuts
/// can never loop), and turn the cutters into ghosts.
fn apply_cuts(model: &Model, items: &mut [SceneItem]) {
    let cuts = cuts(model);
    if cuts.is_empty() {
        return;
    }
    // a layered cutter cuts with all its layers
    let mut original: std::collections::HashMap<ElementId, TriMesh> =
        std::collections::HashMap::new();
    for (_, c, _) in &cuts {
        if original.contains_key(c) {
            continue;
        }
        let mut tool = TriMesh::default();
        for i in items.iter().filter(|i| i.id == *c) {
            tool.append(&i.mesh);
        }
        if !tool.is_empty() {
            original.insert(*c, tool);
        }
    }
    for (target, cutter, op) in &cuts {
        let Some(tool) = original.get(cutter) else {
            continue;
        };
        for item in items.iter_mut().filter(|i| i.id == *target) {
            item.mesh = boolean_mesh(&item.mesh, tool, *op);
        }
    }
    for item in items.iter_mut() {
        if original.contains_key(&item.id) {
            item.ghost = true;
        }
    }
}

/// `a op b` for two closed triangle meshes.
pub fn boolean_mesh(a: &TriMesh, b: &TriMesh, op: BoolOp) -> TriMesh {
    let polys = |m: &TriMesh| -> Vec<csg::Polygon> {
        m.indices
            .chunks_exact(3)
            .filter_map(|t| {
                let p = |i: u32| {
                    let v = m.positions[i as usize];
                    DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64)
                };
                csg::Polygon::new(vec![p(t[0]), p(t[1]), p(t[2])])
            })
            .collect()
    };
    let result = csg::boolean(polys(a), polys(b), op);
    let mut out = TriMesh::default();
    for p in &result {
        let n = (p.points[1] - p.points[0])
            .cross(p.points[2] - p.points[0])
            .normalize_or_zero();
        let fan: Vec<[usize; 3]> = (1..p.points.len() - 1).map(|i| [0, i, i + 1]).collect();
        // the fan's winding already follows the polygon's
        out.add_polygon(&p.points, &fan, newell(&p.points).normalize_or(n));
    }
    out
}

fn newell(p: &[DVec3]) -> DVec3 {
    let mut n = DVec3::ZERO;
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        n += DVec3::new(
            (a.y - b.y) * (a.z + b.z),
            (a.z - b.z) * (a.x + b.x),
            (a.x - b.x) * (a.y + b.y),
        );
    }
    n
}

/// A door's or window's parts, each in its own colour: the frame and leaf
/// in the opening's material (or a painted default), glass see-through.
/// An element can so have several items, all with its id.
fn opening_items(
    model: &Model,
    e: &creabim_core::Element,
    o: &Opening,
    w: &creabim_core::Wall,
    level_z: f64,
) -> Vec<SceneItem> {
    use crate::opening::Part;
    use creabim_core::OpeningKind;
    let material = creabim_core::material_of(model, e).map(|(_, m)| (m.color, m.surface()));
    crate::opening::opening_meshes(o, w, level_z)
        .into_iter()
        .map(|(part, mesh)| {
            let (color, surface) = match (part, o.kind, material) {
                (Part::Glass, ..) => (GLASS_COLOR, GLASS_SURFACE),
                (Part::Handle, ..) => ([0.72, 0.73, 0.75], [0.3, 1.0, 1.0]),
                (_, _, Some(m)) => m,
                (Part::Frame, OpeningKind::Window | OpeningKind::Hole, None) => {
                    ([0.93, 0.93, 0.91], PLAIN_SURFACE)
                }
                (Part::Frame, OpeningKind::Door, None) => ([0.50, 0.35, 0.22], PLAIN_SURFACE),
                (Part::Leaf, _, None) => ([0.62, 0.45, 0.30], PLAIN_SURFACE),
            };
            SceneItem {
                id: e.id,
                mesh,
                color,
                surface,
                ghost: false,
                layer: None,
                finish: match part {
                    Part::Glass => Some("Glass"),
                    Part::Handle => Some("Metal"),
                    Part::Frame | Part::Leaf => None,
                },
            }
        })
        .collect()
}

/// Glass in doors and windows: pale blue, smooth and mostly see-through.
pub const GLASS_COLOR: [f32; 3] = [0.62, 0.78, 0.88];
pub const GLASS_SURFACE: [f32; 3] = [0.05, 0.0, 0.3];

/// Each element's whole mesh: the items of one element (a window's frame
/// and glass) merged.
pub fn element_meshes(scene: &[SceneItem]) -> std::collections::HashMap<ElementId, TriMesh> {
    let mut out: std::collections::HashMap<ElementId, TriMesh> = Default::default();
    for i in scene.iter().filter(|i| !i.is_lining()) {
        out.entry(i.id).or_default().append(&i.mesh);
    }
    out
}

/// A curtain wall's mullions and panels, each kind in its own look: the
/// mullions and solid panels in the wall's material (or anodised
/// aluminium and a pale grey), glass see-through.
fn curtain_items(model: &Model, e: &Element, w: &creabim_core::Wall) -> Vec<SceneItem> {
    use crate::curtain::CurtainPart;
    let (Some(level), Some(grid)) = (model.level(w.level), &w.curtain) else {
        return vec![];
    };
    let openings: Vec<&Opening> = model
        .dependents(e.id)
        .into_iter()
        .filter_map(|d| match &model.get(d)?.kind {
            ElementKind::Opening(o) => Some(o),
            _ => None,
        })
        .collect();
    let material = creabim_core::material_of(model, e).map(|(_, m)| (m.color, m.surface()));
    crate::curtain::curtain_meshes(w, grid, &openings, level.elevation)
        .into_iter()
        .map(|(part, mesh)| {
            let (color, surface) = match (part, material) {
                (CurtainPart::Glass, _) => (GLASS_COLOR, GLASS_SURFACE),
                (_, Some(m)) => m,
                (CurtainPart::Mullion, None) => ([0.62, 0.64, 0.67], [0.35, 0.9, 1.0]),
                (CurtainPart::Panel, None) => ([0.80, 0.80, 0.78], PLAIN_SURFACE),
            };
            SceneItem {
                id: e.id,
                mesh,
                color,
                surface,
                ghost: false,
                layer: None,
                finish: match part {
                    CurtainPart::Glass => Some("Glass"),
                    CurtainPart::Mullion => Some("Metal"),
                    CurtainPart::Panel => None,
                },
            }
        })
        .collect()
}

/// An extruded slab with holes through it: top face at `top_z`,
/// `thickness` downwards, each hole (a polygon inside the outline) cut
/// straight through.
pub fn slab_mesh_with_holes(
    boundary: &[DVec2],
    holes: &[Vec<DVec2>],
    top_z: f64,
    thickness: f64,
) -> TriMesh {
    if holes.is_empty() {
        return slab_mesh(boundary, top_z, thickness);
    }
    let face = creabim_core::polygon::bridged(boundary, holes);
    let mut m = TriMesh::default();
    let tris = triangulate(&face);
    let top: Vec<DVec3> = face.iter().map(|p| p.extend(top_z)).collect();
    let bottom: Vec<DVec3> = face.iter().map(|p| p.extend(top_z - thickness)).collect();
    m.add_polygon(&top, &tris, DVec3::Z);
    m.add_polygon(&bottom, &tris, -DVec3::Z);
    // the outline counter-clockwise and the holes clockwise: material is
    // on the left of every edge, so the sides face right
    let mut loops = vec![boundary.to_vec()];
    loops.extend(holes.iter().cloned());
    for (k, lp) in loops.iter_mut().enumerate() {
        let ccw = crate::triangulate::signed_area2(lp) > 0.0;
        if ccw != (k == 0) {
            lp.reverse();
        }
        let n = lp.len();
        for i in 0..n {
            let (a, b) = (lp[i], lp[(i + 1) % n]);
            let e = (b - a).normalize_or_zero();
            m.add_quad(
                [
                    a.extend(top_z - thickness),
                    b.extend(top_z - thickness),
                    b.extend(top_z),
                    a.extend(top_z),
                ],
                DVec3::new(e.y, -e.x, 0.0),
            );
        }
    }
    m
}

/// Extruded slab: top face at `top_z`, `thickness` downwards.
pub fn slab_mesh(boundary: &[DVec2], top_z: f64, thickness: f64) -> TriMesh {
    let mut m = TriMesh::default();
    let tris = triangulate(boundary);
    let top: Vec<DVec3> = boundary.iter().map(|p| p.extend(top_z)).collect();
    let bottom: Vec<DVec3> = boundary
        .iter()
        .map(|p| p.extend(top_z - thickness))
        .collect();
    m.add_polygon(&top, &tris, DVec3::Z);
    m.add_polygon(&bottom, &tris, -DVec3::Z);
    let ccw = crate::triangulate::signed_area2(boundary) > 0.0;
    for i in 0..boundary.len() {
        let (a, b) = (boundary[i], boundary[(i + 1) % boundary.len()]);
        let e = (b - a).normalize_or_zero();
        let out = if ccw {
            DVec2::new(e.y, -e.x)
        } else {
            DVec2::new(-e.y, e.x)
        };
        m.add_quad(
            [
                bottom[i],
                bottom[(i + 1) % boundary.len()],
                top[(i + 1) % boundary.len()],
                top[i],
            ],
            out.extend(0.0),
        );
    }
    m
}

/// Flat-shaded triangles from a polygon mesh, offset to `origin`, face by
/// face (so triangle k still maps back to its face).
///
/// Coincident corners are welded: corners closer than a micron become one
/// position, and faces lying in one plane share the vertex itself. A
/// corner on a crease is still one vertex per face direction, so every
/// face keeps its own flat normal, but they all sit at exactly the same
/// point, so the mesh is closed wherever the edit mesh is (see
/// [`TriMesh::is_closed`]).
pub fn edit_mesh_to_tri(mesh: &creabim_core::EditMesh, origin: DVec3) -> TriMesh {
    use std::collections::HashMap;
    const TOL: f64 = 1e-6;
    let key = |p: DVec3| {
        (
            (p.x / TOL).round() as i64,
            (p.y / TOL).round() as i64,
            (p.z / TOL).round() as i64,
        )
    };
    let mut first: HashMap<(i64, i64, i64), usize> = HashMap::new();
    let corner: Vec<usize> = (0..mesh.vertices.len())
        .map(|v| *first.entry(key(mesh.vertices[v])).or_insert(v))
        .collect();
    let mut m = TriMesh::default();
    let mut shared: HashMap<(usize, [i64; 3]), u32> = HashMap::new();
    for f in 0..mesh.faces.len() {
        let face = &mesh.faces[f];
        let n = mesh.face_normal(f);
        let nk = (n * 1e5).round().as_i64vec3().to_array();
        let mut index = |k: usize| -> (u32, DVec3) {
            let c = corner[face[k] as usize];
            let p = mesh.vertices[c] + origin;
            let i = *shared.entry((c, nk)).or_insert_with(|| {
                m.positions.push(p.as_vec3().to_array());
                m.normals.push(n.as_vec3().to_array());
                (m.positions.len() - 1) as u32
            });
            (i, p)
        };
        let tris: Vec<[(u32, DVec3); 3]> = mesh
            .face_corners(f)
            .into_iter()
            .map(|t| t.map(&mut index))
            .collect();
        for [(a, pa), (b, pb), (c, pc)] in tris {
            // keep the winding consistent with the normal
            if (pb - pa).cross(pc - pa).dot(n) >= 0.0 {
                m.indices.extend([a, b, c]);
            } else {
                m.indices.extend([a, c, b]);
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slab_volume_matches_area_times_thickness() {
        let l = [
            DVec2::new(0.0, 0.0),
            DVec2::new(4.0, 0.0),
            DVec2::new(4.0, 2.0),
            DVec2::new(2.0, 2.0),
            DVec2::new(2.0, 4.0),
            DVec2::new(0.0, 4.0),
        ];
        let m = slab_mesh(&l, 0.0, 0.25);
        assert!((m.volume() - 12.0 * 0.25).abs() < 1e-5);
        let mut cw = l.to_vec();
        cw.reverse();
        assert!((slab_mesh(&cw, 0.0, 0.25).volume() - 3.0).abs() < 1e-5);
    }

    #[test]
    fn a_slab_top_offset_lifts_its_mesh_and_arcs_round_it() {
        let mut model = creabim_core::sample::sample_house();
        let (id, elevation, t) = model
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Slab(s) => Some((e.id, model.level(s.level)?.elevation, s.thickness)),
                _ => None,
            })
            .unwrap();
        let mut e = model.get(id).unwrap().clone();
        let ElementKind::Slab(s) = &mut e.kind else {
            panic!()
        };
        // a 4 x 2 m slab whose far side is a half circle, its top 0.6 m up
        s.boundary = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(4.0, 0.0),
            DVec2::new(4.0, 2.0),
            DVec2::new(0.0, 2.0),
        ];
        s.bulges = vec![0.0, 1.0, 0.0, 0.0];
        s.top_offset = 0.6;
        creabim_core::Transaction::new("edit")
            .replace(model.get(id).unwrap().clone(), e)
            .apply(&mut model)
            .unwrap();
        let scene = build_scene(&model);
        let item = scene.iter().find(|i| i.id == id).unwrap();
        let (lo, hi) = item.mesh.bounds().unwrap();
        assert!((hi.z as f64 - (elevation + 0.6)).abs() < 1e-5);
        assert!((lo.z as f64 - (elevation + 0.6 - t)).abs() < 1e-5);
        // the arc bulges a metre past the straight side
        assert!((hi.x - 5.0).abs() < 1e-4);
        let area = 8.0 + std::f64::consts::PI / 2.0;
        assert!((item.mesh.volume() - area * t).abs() < 0.01);
    }

    #[test]
    fn a_slab_with_a_hole_loses_its_volume() {
        let outer = creabim_core::polygon::rectangle(DVec2::ZERO, DVec2::new(4.0, 4.0));
        let hole = creabim_core::polygon::rectangle(DVec2::new(1.0, 1.0), DVec2::new(2.0, 3.0));
        let m = slab_mesh_with_holes(&outer, std::slice::from_ref(&hole), 0.0, 0.25);
        assert!((m.volume() - 14.0 * 0.25).abs() < 1e-5, "{}", m.volume());
        // the plan cut at mid-depth has the hole in it
        let cut = crate::slice::slice(&m, -0.1);
        assert_eq!(cut.len(), 1);
        let area = crate::triangulate::signed_area2(&cut[0]) / 2.0;
        assert!((area - 14.0).abs() < 1e-5, "{area}");
    }

    #[test]
    fn edit_mesh_corners_are_welded_and_faces_stay_flat() {
        // a box whose faces each carry their own copies of the corners, as
        // an imported mesh does
        let cube = creabim_core::EditMesh::cuboid(DVec3::new(2.0, 2.0, 2.0));
        let mut split = creabim_core::EditMesh::default();
        for f in &cube.faces {
            let base = split.vertices.len() as u32;
            split
                .vertices
                .extend(f.iter().map(|&v| cube.vertices[v as usize]));
            split
                .faces
                .push((0..f.len() as u32).map(|i| base + i).collect());
        }
        let m = edit_mesh_to_tri(&split, DVec3::ZERO);
        assert!(m.is_closed());
        // eight corners, three face directions each
        assert_eq!(m.positions.len(), 24);
        let mut corners: Vec<[u32; 3]> = m.positions.iter().map(|p| p.map(f32::to_bits)).collect();
        corners.sort_unstable();
        corners.dedup();
        assert_eq!(corners.len(), 8);
        for t in m.indices.chunks(3) {
            let n = |i: u32| m.normals[i as usize];
            assert!(n(t[0]) == n(t[1]) && n(t[1]) == n(t[2]), "flat faces");
        }
        // a face split in two shares the corners along the split
        let mut halves = cube.clone();
        let top = halves
            .faces
            .iter()
            .position(|f| f.iter().all(|&v| cube.vertices[v as usize].z > 1.0))
            .unwrap();
        let q = halves.faces.remove(top);
        halves.faces.push(vec![q[0], q[1], q[2]]);
        halves.faces.push(vec![q[0], q[2], q[3]]);
        let m = edit_mesh_to_tri(&halves, DVec3::ZERO);
        assert_eq!(m.positions.len(), 24);
        assert!(m.is_closed());
        // and the plan cut of the closed box is one loop
        let cut = crate::slice::slice(&m, 1.0);
        assert_eq!(cut.len(), 1);
    }

    #[test]
    fn edit_mesh_volume_is_preserved() {
        let mut e = creabim_core::EditMesh::cuboid(DVec3::new(2.0, 2.0, 2.0));
        e.extrude_face(1, 1.0);
        let m = edit_mesh_to_tri(&e, DVec3::new(10.0, 0.0, 0.0));
        assert!((m.volume() - 12.0).abs() < 1e-4);
    }

    #[test]
    fn window_reveals_show_the_finish_not_the_insulation() {
        let m = creabim_core::starter::starter_house();
        let scene = build_scene(&m);
        let linings: Vec<&SceneItem> = scene.iter().filter(|i| i.is_lining()).collect();
        assert!(!linings.is_empty());
        let insulation = layer_color(creabim_core::LayerFunction::Insulation, [0.0; 3]);
        let insulation_materials: Vec<[f32; 3]> = scene
            .iter()
            .filter(|i| {
                i.layer
                    .is_some_and(|l| l.function == creabim_core::LayerFunction::Insulation)
            })
            .map(|i| i.color)
            .collect();
        for l in &linings {
            assert_ne!(l.color, insulation);
            assert!(!insulation_materials.contains(&l.color), "{:?}", l.color);
            // thin, closed boxes
            assert!(l.mesh.volume() > 0.0);
        }
        // drawings and exports leave them out
        assert!(!drawn_scene(&m).iter().any(|i| i.is_lining()));
        let walls: f64 = element_meshes(&scene).values().map(|m| m.volume()).sum();
        let without: Vec<SceneItem> = scene.iter().filter(|i| !i.is_lining()).cloned().collect();
        let plain: f64 = element_meshes(&without).values().map(|m| m.volume()).sum();
        assert!((walls - plain).abs() < 1e-9);
    }
}
