//! Voids cut the walls and parts they meet, and screen walls show their
//! holes, in 3D and in plan, in the garden sample.

use creabim_core::garden::garden_house;
use creabim_core::{ElementId, ElementKind, Model};
use creabim_geom::{build_scene, plan_view, PlanItem};
use glam::DVec2;

fn id(m: &Model, name: &str) -> ElementId {
    m.iter().find(|e| e.name == name).unwrap().id
}

fn without(m: &Model, gone: ElementId) -> Model {
    Model::from_elements(m.iter().filter(|e| e.id != gone).cloned()).unwrap()
}

fn volume(m: &Model, of: ElementId) -> f64 {
    build_scene(m)
        .iter()
        .filter(|i| i.id == of && !i.ghost)
        .map(|i| i.mesh.volume())
        .sum()
}

fn area(points: &[DVec2]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| points[i].perp_dot(points[(i + 1) % n]))
        .sum::<f64>()
        .abs()
        / 2.0
}

/// The cut area of `of` in the ground floor plan cut at `height`.
fn cut_area(m: &Model, of: ElementId, height: f64) -> f64 {
    let level = id(m, "Ground Floor");
    plan_view(m, level, height)
        .items
        .iter()
        .filter_map(|i| match i {
            PlanItem::Fill { id, points, .. } if *id == of => Some(area(points)),
            _ => None,
        })
        .sum()
}

#[test]
fn the_firebox_cuts_the_wall_and_the_stack_in_3d() {
    let m = garden_house();
    let (firebox, stack, wall) = (
        id(&m, "Firebox"),
        id(&m, "Chimney stack"),
        id(&m, "GF Wall 4"),
    );
    let solid = without(&m, firebox);
    // the box is 1.0 x 0.8 x 0.8: through the 0.3 m wall, 0.6 m into the stack
    let hole = |of| volume(&solid, of) - volume(&m, of);
    assert!(
        (hole(stack) - 0.6 * 0.8 * 0.8).abs() < 1e-3,
        "{}",
        hole(stack)
    );
    assert!(
        (hole(wall) - 0.3 * 0.8 * 0.8).abs() < 1e-3,
        "{}",
        hole(wall)
    );
    // the void itself is a see-through outline, not a solid
    let scene = build_scene(&m);
    let v: Vec<_> = scene.iter().filter(|i| i.id == firebox).collect();
    assert!(!v.is_empty() && v.iter().all(|i| i.ghost));
    // what it does not reach is not cut
    let hearth = id(&m, "Hearth");
    assert!((volume(&solid, hearth) - volume(&m, hearth)).abs() < 1e-9);
}

#[test]
fn the_firebox_shows_as_a_gap_in_plan() {
    let m = garden_house();
    let firebox = id(&m, "Firebox");
    let solid = without(&m, firebox);
    for name in ["Chimney stack", "GF Wall 4"] {
        let of = id(&m, name);
        let (before, after) = (cut_area(&solid, of, 0.5), cut_area(&m, of, 0.5));
        assert!(before > 0.0, "{name} is cut by the plan");
        assert!(after < before - 0.2, "{name}: {before} → {after}");
        // above the firebox the plan is whole again
        assert!((cut_area(&solid, of, 1.2) - cut_area(&m, of, 1.2)).abs() < 1e-6);
    }
    // the void draws nothing of its own
    let level = id(&m, "Ground Floor");
    assert!(plan_view(&m, level, 0.5)
        .items
        .iter()
        .all(|i| i.id() != firebox));
}

#[test]
fn a_screen_wall_is_pierced_in_3d_and_in_plan() {
    let m = garden_house();
    let screen = id(&m, "Garden screen");
    let ElementKind::Wall(w) = &m.get(screen).unwrap().kind else {
        panic!()
    };
    let full = w.length() * w.height * w.thickness;
    let v = volume(&m, screen);
    // two 155 x 140 holes in each 400 x 200 block: 54% open
    let open = 1.0 - 2.0 * 0.155 * 0.14 / 0.08;
    assert!((v - full * open).abs() < full * 0.02, "{v} of {full}");
    // a plan cut through a course of holes draws the blocks between them
    let level = id(&m, "Ground Floor");
    let pieces = plan_view(&m, level, 1.1)
        .items
        .iter()
        .filter(|i| matches!(i, PlanItem::Fill { id, .. } if *id == screen))
        .count();
    assert!(pieces > 40, "{pieces}");
    // a cut through the joints between courses draws it whole
    let solid = plan_view(&m, level, 1.0)
        .items
        .iter()
        .filter(|i| matches!(i, PlanItem::Fill { id, .. } if *id == screen))
        .count();
    assert!(solid < 3, "{solid}");
}

#[test]
fn site_elements_draw_in_plan_and_3d() {
    let m = garden_house();
    let level = id(&m, "Ground Floor");
    let plan = plan_view(&m, level, 1.2);
    let scene = build_scene(&m);
    for name in [
        "Ground",
        "Mound",
        "Patio",
        "Driveway",
        "Oak",
        "Palm",
        "Chimney stack",
    ] {
        let of = id(&m, name);
        assert!(plan.items.iter().any(|i| i.id() == of), "{name} in plan");
        assert!(
            scene.iter().any(|i| i.id == of && i.mesh.volume() > 0.0),
            "{name} in 3D"
        );
    }
}
