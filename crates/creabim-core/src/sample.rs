//! A small sample house used by the demo command, tests and the app's "New sample".

use crate::*;
use glam::{DVec2, DVec3};

pub fn sample_house() -> Model {
    let mut doc = Document::new();
    let l0 = Element::new(
        "Ground Floor",
        ElementKind::Level(Level {
            elevation: 0.0,
            height: 3.0,
        }),
    );
    let l1 = Element::new(
        "First Floor",
        ElementKind::Level(Level {
            elevation: 3.0,
            height: 3.0,
        }),
    );
    let (g, f) = (l0.id, l1.id);
    let mut tx = Transaction::new("Sample house").insert(l0).insert(l1);

    for (label, a, b) in [
        ("A", DVec2::new(0.0, -1.0), DVec2::new(0.0, 7.0)),
        ("B", DVec2::new(10.0, -1.0), DVec2::new(10.0, 7.0)),
        ("1", DVec2::new(-1.0, 0.0), DVec2::new(11.0, 0.0)),
        ("2", DVec2::new(-1.0, 6.0), DVec2::new(11.0, 6.0)),
    ] {
        tx = tx.insert(Element::new(
            format!("Grid {label}"),
            ElementKind::Grid(Grid {
                label: label.into(),
                start: a,
                end: b,
            }),
        ));
    }

    let corners = [
        DVec2::new(0.0, 0.0),
        DVec2::new(10.0, 0.0),
        DVec2::new(10.0, 6.0),
        DVec2::new(0.0, 6.0),
    ];
    let mut ground_walls = Vec::new();
    for (level, lname) in [(g, "GF"), (f, "FF")] {
        tx = tx.insert(Element::new(
            format!("{lname} Slab"),
            ElementKind::Slab(Slab {
                level,
                boundary: corners.to_vec(),
                thickness: 0.25,
            }),
        ));
        for i in 0..4 {
            let wall = Element::new(
                format!("{lname} Wall {}", i + 1),
                ElementKind::Wall(Wall {
                    level,
                    start: corners[i],
                    end: corners[(i + 1) % 4],
                    thickness: 0.3,
                    height: 3.0,
                    base_offset: 0.0,
                }),
            );
            let id = wall.id;
            tx = tx.insert(wall);
            if level == g {
                ground_walls.push(id);
            }
            let windows: &[f64] = match i {
                0 | 2 => &[1.5, 6.5],
                _ => &[2.4],
            };
            for (k, &off) in windows.iter().enumerate() {
                if level == g && i == 0 && k == 0 {
                    continue; // the front door goes here instead
                }
                tx = tx.insert(Element::new(
                    format!("{lname} Window {}.{}", i + 1, k + 1),
                    ElementKind::Opening(Opening {
                        host: id,
                        kind: OpeningKind::Window,
                        offset: off,
                        sill: 0.9,
                        width: 1.2,
                        height: 1.3,
                    }),
                ));
            }
        }
    }
    tx = tx.insert(Element::new(
        "Front Door",
        ElementKind::Opening(Opening {
            host: ground_walls[0],
            kind: OpeningKind::Door,
            offset: 1.6,
            sill: 0.0,
            width: 1.0,
            height: 2.2,
        }),
    ));
    // An inner partition with a door.
    let part = Element::new(
        "GF Partition",
        ElementKind::Wall(Wall {
            level: g,
            start: DVec2::new(6.0, 0.15),
            end: DVec2::new(6.0, 5.85),
            thickness: 0.12,
            height: 3.0,
            base_offset: 0.0,
        }),
    );
    let pid = part.id;
    tx = tx.insert(part).insert(Element::new(
        "GF Inner Door",
        ElementKind::Opening(Opening {
            host: pid,
            kind: OpeningKind::Door,
            offset: 3.5,
            sill: 0.0,
            width: 0.9,
            height: 2.1,
        }),
    ));

    // A freeform canopy over the entrance, modelled as a mesh and classified as a roof.
    let mut canopy = EditMesh::cuboid(DVec3::new(3.0, 1.6, 0.15));
    canopy.scale_face(1, 0.85);
    tx = tx.insert(Element::new(
        "Entrance Canopy",
        ElementKind::FreeMesh(FreeMesh {
            level: Some(g),
            origin: DVec3::new(2.1, -0.95, 2.6),
            mesh: canopy,
            ifc_class: "IfcRoof".into(),
        }),
    ));

    // Views: a plan per level, two sections, four elevations and a 3D view.
    let plan_g = Element::new("Ground Floor plan", ElementKind::View(View::plan(g)));
    let plan_f = Element::new("First Floor plan", ElementKind::View(View::plan(f)));
    let mut axo = View::axonometric(35.0, 30.0);
    axo.scale = 50.0;
    let axo = Element::new("3D View South-East", ElementKind::View(axo));
    let mut placements = vec![
        (
            "A101",
            plan_g.id,
            DVec2::new(160.0, 240.0),
            DVec2::new(5.0, 3.0),
        ),
        (
            "A101",
            plan_f.id,
            DVec2::new(400.0, 240.0),
            DVec2::new(5.0, 3.0),
        ),
        ("A001", axo.id, DVec2::new(200.0, 165.0), DVec2::ZERO),
    ];
    tx = tx.insert(plan_g).insert(plan_f).insert(axo);
    for (name, a, b, cut) in [
        (
            "Section A",
            DVec2::new(2.1, -3.0),
            DVec2::new(2.1, 9.0),
            true,
        ),
        (
            "Section B",
            DVec2::new(-2.0, 3.0),
            DVec2::new(12.0, 3.0),
            true,
        ),
        (
            "South Elevation S",
            DVec2::new(-2.0, -4.0),
            DVec2::new(12.0, -4.0),
            false,
        ),
        (
            "East Elevation E",
            DVec2::new(14.0, -2.0),
            DVec2::new(14.0, 8.0),
            false,
        ),
        (
            "North Elevation N",
            DVec2::new(12.0, 10.0),
            DVec2::new(-2.0, 10.0),
            false,
        ),
        (
            "West Elevation W",
            DVec2::new(-4.0, 8.0),
            DVec2::new(-4.0, -2.0),
            false,
        ),
    ] {
        let e = Element::new(name, ElementKind::View(View::section(a, b, 30.0, cut)));
        // where each drawing sits on the sections sheet, and the drawing
        // point pinned there (along the line, height)
        let at = match name {
            "Section A" => Some((DVec2::new(105.0, 215.0), DVec2::new(6.0, 3.0))),
            "Section B" => Some((DVec2::new(290.0, 215.0), DVec2::new(7.0, 3.0))),
            "South Elevation S" => Some((DVec2::new(105.0, 110.0), DVec2::new(7.0, 3.0))),
            "East Elevation E" => Some((DVec2::new(290.0, 110.0), DVec2::new(5.0, 3.0))),
            _ => None,
        };
        if let Some((center, target)) = at {
            placements.push(("A201", e.id, center, target));
        }
        tx = tx.insert(e);
    }

    // Sheets with the views placed on them.
    let mut sheets = std::collections::HashMap::new();
    for (number, title) in [
        ("A001", "3D View"),
        ("A101", "Floor Plans"),
        ("A201", "Sections and Elevations"),
    ] {
        let size = if number == "A101" { "A2" } else { "A3" };
        let sheet = Element::new(title, ElementKind::Sheet(Sheet::a(number, size)));
        sheets.insert(number, sheet.id);
        tx = tx.insert(sheet);
    }
    for (number, view, center, target) in placements {
        tx = tx.insert(Element::new(
            "Viewport",
            ElementKind::Viewport(Viewport {
                sheet: sheets[number],
                view,
                center,
                target,
            }),
        ));
    }

    doc.commit(tx).expect("sample house is valid");
    doc.model().clone()
}
