//! Entourage: cars and people, to give drawings and views their scale.
//! Simple massing shapes, free-standing, with a plan outline each.

use super::{cylinder, look, rrect, soft_box, squircle, stack, v2, v3, Build, Look};
use crate::objects::{CatalogueObject, Mount, ObjectCategory};
use crate::{EditMesh, Hatch};
use glam::DVec3;
use std::f64::consts::TAU;

fn paint() -> Look {
    let mut l = look(
        "Car paint, silver",
        "metal",
        Hatch::Empty,
        [170, 176, 184],
        0.3,
    );
    l.1.metallic = 0.6;
    l
}
fn tinted() -> Look {
    let mut l = look("Glass, tinted", "glass", Hatch::Empty, [60, 72, 84], 0.05);
    l.1.opacity = 0.6;
    l
}
fn rubber() -> Look {
    look("Rubber, black", "plastic", Hatch::Empty, [30, 30, 32], 0.9)
}
fn clothes() -> Look {
    look(
        "Figure, grey",
        "textile",
        Hatch::Empty,
        [150, 154, 160],
        0.95,
    )
}

/// A wheel: a short cylinder on an axle along x, centred on `c`.
fn wheel(c: DVec3, r: f64, width: f64) -> EditMesh {
    let n = 14;
    let mut m = EditMesh::default();
    for side in [-1.0, 1.0] {
        for i in 0..n {
            let a = TAU * i as f64 / n as f64;
            m.vertices
                .push(c + DVec3::new(side * width / 2.0, r * a.cos(), r * a.sin()));
        }
    }
    let n32 = n as u32;
    // the -x face looks along -x, the +x face along +x
    m.faces.push((0..n32).rev().collect());
    m.faces.push((n32..2 * n32).collect());
    for i in 0..n32 {
        let j = (i + 1) % n32;
        m.faces.push(vec![i, j, n32 + j, n32 + i]);
    }
    m
}

fn car() -> CatalogueObject {
    let mut b = Build::new();
    let (w, l) = (1.8, 4.5);
    let c = v2(0.0, -l / 2.0);
    b.add(
        "Body",
        paint(),
        soft_box(v3(-w / 2.0, -l, 0.28), v3(w / 2.0, 0.0, 0.95), 0.2, 0.06),
        true,
    );
    let k = 4;
    b.add(
        "Cabin",
        tinted(),
        stack(&[
            (rrect(c + v2(0.0, 0.1), w - 0.12, 2.5, 0.2, k), 0.95),
            (rrect(c + v2(0.0, 0.2), w - 0.36, 1.7, 0.15, k), 1.45),
        ]),
        false,
    );
    b.outline(rrect(c + v2(0.0, 0.1), w - 0.12, 2.5, 0.2, k));
    for (x, y) in [(-0.78, -0.85), (0.78, -0.85), (-0.78, -3.65), (0.78, -3.65)] {
        b.add("Wheel", rubber(), wheel(v3(x, y, 0.33), 0.33, 0.22), false);
    }
    b.finish(
        "car",
        "Car",
        "A family car, 4.5 m long and 1.8 m wide, for scale",
        ObjectCategory::Entourage,
        ("IfcFurniture", "NOTDEFINED"),
        Mount::Floor,
    )
}

fn person() -> CatalogueObject {
    let mut b = Build::new();
    let n = 16;
    for x in [-0.09, 0.09] {
        b.add(
            "Leg",
            clothes(),
            cylinder(v2(x, -0.13), 0.07, 0.0, 0.85),
            false,
        );
    }
    b.add(
        "Body",
        clothes(),
        stack(&[
            (squircle(v2(0.0, -0.13), 0.19, 0.11, 2.4, n), 0.82),
            (squircle(v2(0.0, -0.13), 0.23, 0.12, 2.4, n), 1.4),
            (squircle(v2(0.0, -0.13), 0.12, 0.08, 2.4, n), 1.5),
        ]),
        true,
    );
    b.add(
        "Head",
        clothes(),
        stack(&[
            (squircle(v2(0.0, -0.13), 0.05, 0.05, 2.0, n), 1.5),
            (squircle(v2(0.0, -0.13), 0.1, 0.11, 2.0, n), 1.58),
            (squircle(v2(0.0, -0.13), 0.1, 0.11, 2.0, n), 1.7),
            (squircle(v2(0.0, -0.13), 0.06, 0.06, 2.0, n), 1.76),
        ]),
        true,
    );
    b.finish(
        "person",
        "Person",
        "A standing figure 1.76 m tall, for scale",
        ObjectCategory::Entourage,
        ("IfcFurniture", "NOTDEFINED"),
        Mount::Floor,
    )
}

fn child() -> CatalogueObject {
    let mut b = Build::new();
    let n = 16;
    for x in [-0.06, 0.06] {
        b.add(
            "Leg",
            clothes(),
            cylinder(v2(x, -0.1), 0.05, 0.0, 0.55),
            false,
        );
    }
    b.add(
        "Body",
        clothes(),
        stack(&[
            (squircle(v2(0.0, -0.1), 0.13, 0.08, 2.4, n), 0.53),
            (squircle(v2(0.0, -0.1), 0.15, 0.09, 2.4, n), 0.92),
            (squircle(v2(0.0, -0.1), 0.08, 0.06, 2.4, n), 1.0),
        ]),
        true,
    );
    b.add(
        "Head",
        clothes(),
        stack(&[
            (squircle(v2(0.0, -0.1), 0.04, 0.04, 2.0, n), 1.0),
            (squircle(v2(0.0, -0.1), 0.09, 0.09, 2.0, n), 1.06),
            (squircle(v2(0.0, -0.1), 0.09, 0.09, 2.0, n), 1.17),
            (squircle(v2(0.0, -0.1), 0.05, 0.05, 2.0, n), 1.22),
        ]),
        true,
    );
    b.finish(
        "child",
        "Child",
        "A child 1.2 m tall, for scale",
        ObjectCategory::Entourage,
        ("IfcFurniture", "NOTDEFINED"),
        Mount::Floor,
    )
}

pub(super) fn starters() -> Vec<CatalogueObject> {
    vec![car(), person(), child()]
}
