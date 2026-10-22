//! Furniture and fixtures in drawings: plans draw an object's symbol, its
//! type's 2D linework, rather than slicing its shape; sections, elevations
//! and the 3D view take its parts from the scene (see
//! [`crate::scene::build_scene`]).

use crate::plan::{Pen, PlanItem};
use creabim_core::objects::{self, SymbolItem};
use creabim_core::{Element, ElementKind, Model};

/// An object's plan symbol, turned and placed. Objects hung above the cut
/// plane (wall cupboards) are drawn dashed, as things overhead are.
pub fn object_plan(model: &Model, e: &Element, cut_height: f64) -> Vec<PlanItem> {
    let ElementKind::Object(o) = &e.kind else {
        return vec![];
    };
    let Some(t) = objects::type_of(model, e) else {
        return vec![];
    };
    let (lo, _) = t.bounds();
    let pen = if o.offset + lo.z > cut_height {
        Pen::Hidden
    } else {
        Pen::Projection
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
        assert!((lo.x - 0.15).abs() < 1e-6 && (hi.x - 1.05).abs() < 1e-6, "{lo} {hi}");
        // a wall cupboard is overhead: dashed
        let cup = m
            .iter()
            .find(|e| e.name.starts_with("Wall cabinet") && matches!(e.kind, ElementKind::Object(_)))
            .unwrap();
        assert!(plan
            .items
            .iter()
            .filter(|i| i.id() == cup.id)
            .all(|i| matches!(i, PlanItem::Polygon { pen: Pen::Hidden, .. })));
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
