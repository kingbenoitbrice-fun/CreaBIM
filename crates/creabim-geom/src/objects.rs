//! Furniture and fixtures in drawings: plans draw an object's symbol, its
//! type's 2D linework, rather than slicing its shape; sections, elevations
//! and the 3D view take its parts from the scene (see
//! [`crate::scene::build_scene`]).

use crate::plan::{Pen, PlanItem};
use creabim_core::objects::{self, SymbolItem};
use creabim_core::{Element, ElementKind, Model};

/// An object's plan symbol, turned and placed, in the fine pen of small
/// things. Objects hung above the cut plane (wall cupboards) are drawn
/// dashed, as things overhead are; the symbols of building services (a
/// light, a socket, a diffuser) are drawn in full wherever they are, as
/// services drawings show them.
pub fn object_plan(model: &Model, e: &Element, cut_height: f64) -> Vec<PlanItem> {
    let ElementKind::Object(o) = &e.kind else {
        return vec![];
    };
    let Some(t) = objects::type_of(model, e) else {
        return vec![];
    };
    let (lo, _) = t.bounds();
    let floor = model.level(o.level).map_or(0.0, |l| l.elevation);
    let above = objects::base_z_of(model, o, t) - floor + lo.z > cut_height;
    let pen = if above && !t.category.is_services() {
        Pen::Hidden
    } else {
        Pen::Fine
    };
    t.plan_symbol()
        .into_iter()
        .map(|s| match s {
            SymbolItem::Loop(pts) => PlanItem::Polygon {
                id: e.id,
                pen,
                points: pts.into_iter().map(|p| o.to_plan(p)).collect(),
            },
            SymbolItem::Line(a, b) => PlanItem::Line {
                id: e.id,
                pen,
                a: o.to_plan(a),
                b: o.to_plan(b),
            },
        })
        .collect()
}

/// The footprint of an object wholly below the cut (furniture, a bath, a
/// kitchen run) filled in a light tint of each part's material, under
/// its symbol. Building services and what the cut passes through or
/// hangs above it are left plain.
pub fn object_shade(
    model: &Model,
    e: &Element,
    cut_height: f64,
    graphics: &creabim_core::PlanGraphics,
) -> Vec<PlanItem> {
    let ElementKind::Object(o) = &e.kind else {
        return vec![];
    };
    let Some(t) = objects::type_of(model, e) else {
        return vec![];
    };
    let floor = model.level(o.level).map_or(0.0, |l| l.elevation);
    let (_, hi) = t.bounds();
    if t.category.is_services() || objects::base_z_of(model, o, t) - floor + hi.z > cut_height {
        return vec![];
    }
    let mut out = Vec::new();
    for (mesh, material) in objects::model_parts(model, e) {
        let rgb = match material.and_then(|m| model.get(m)).map(|m| &m.kind) {
            Some(ElementKind::Material(m)) => m.color,
            _ => [0.8, 0.78, 0.74],
        };
        let Some(rgb) = graphics.below_tint(rgb) else {
            return vec![];
        };
        let pts: Vec<glam::DVec2> = mesh.vertices.iter().map(|v| v.truncate()).collect();
        let points = objects::hull(&pts);
        if points.len() >= 3 {
            out.push(PlanItem::Shade {
                id: e.id,
                points,
                rgb,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec2;

    #[test]
    fn plans_draw_symbols_and_sections_cut_the_parts() {
        let m = creabim_core::sample::furnished_house();
        let g = m.iter().find(|e| e.name == "Ground Floor").unwrap().id;
        let plan = crate::plan::plan_view(&m, g, 1.2);
        let sofa = m
            .iter()
            .find(|e| e.name.starts_with("Sofa") && matches!(e.kind, ElementKind::Object(_)))
            .expect("a sofa");
        let lines: Vec<&PlanItem> = plan.items.iter().filter(|i| i.id() == sofa.id).collect();
        assert!(lines.len() >= 6, "{}", lines.len());
        // against the west wall, its front to the east
        let (lo, hi) = lines
            .iter()
            .filter_map(|i| i.bounds())
            .fold((DVec2::splat(1e9), DVec2::splat(-1e9)), |(a, b), (l, h)| {
                (a.min(l), b.max(h))
            });
        assert!(
            (lo.x - 0.15).abs() < 1e-6 && (hi.x - 1.05).abs() < 1e-6,
            "{lo} {hi}"
        );
        // a wall cupboard is overhead: dashed
        let cup = m
            .iter()
            .find(|e| {
                e.name.starts_with("Wall cabinet") && matches!(e.kind, ElementKind::Object(_))
            })
            .unwrap();
        assert!(plan
            .items
            .iter()
            .filter(|i| i.id() == cup.id)
            .all(|i| matches!(
                i,
                PlanItem::Polygon {
                    pen: Pen::Hidden,
                    ..
                }
            )));
        // a light hangs far above the cut, but its symbol is drawn in full,
        // as are the sockets' down by the floor
        for name in ["Ceiling light", "Double socket", "Radiator"] {
            let e = m
                .iter()
                .find(|e| e.name.starts_with(name) && matches!(e.kind, ElementKind::Object(_)))
                .unwrap_or_else(|| panic!("a {name}"));
            let items: Vec<&PlanItem> = plan.items.iter().filter(|i| i.id() == e.id).collect();
            assert!(items.len() >= 3, "{name}");
            assert!(items.iter().all(|i| !matches!(
                i,
                PlanItem::Polygon {
                    pen: Pen::Hidden,
                    ..
                } | PlanItem::Line {
                    pen: Pen::Hidden,
                    ..
                }
            )));
        }
        // every part of every object is in the 3D scene, in its material
        let scene = crate::scene::build_scene(&m);
        let parts = scene.iter().filter(|i| i.id == sofa.id).count();
        let t = objects::type_of(&m, sofa).unwrap();
        assert_eq!(parts, t.parts.len());
        assert!(scene
            .iter()
            .filter(|i| i.id == sofa.id)
            .all(|i| i.color != [0.8, 0.78, 0.74]));
    }
}
