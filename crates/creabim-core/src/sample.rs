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

    doc.commit(tx).expect("sample house is valid");
    doc.model().clone()
}
