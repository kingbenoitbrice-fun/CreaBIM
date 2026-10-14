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

    let material = |name: &str, hatch: Hatch, color: [f32; 3], category: &str| {
        Element::new(
            name,
            ElementKind::Material(Material {
                category: category.into(),
                ..Material::new(hatch, color)
            }),
        )
    };
    let brick = material("Brick", Hatch::Brick, [0.78, 0.55, 0.45], "brick");
    let concrete = material("Concrete", Hatch::Concrete, [0.70, 0.70, 0.70], "concrete");
    let stud = material("Stud partition", Hatch::Cross, [0.90, 0.89, 0.86], "gypsum");
    let timber = material("Timber", Hatch::Timber, [0.70, 0.56, 0.40], "wood");
    let (brick_id, concrete_id, stud_id, timber_id) = (brick.id, concrete.id, stud.id, timber.id);
    tx = tx
        .insert(brick)
        .insert(concrete)
        .insert(stud)
        .insert(timber);

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
                group: None,
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
    // doors and windows with the plan they are tagged in and the tag's offset
    let mut tagged: Vec<(ElementId, ElementId, DVec2)> = Vec::new();
    for (level, lname) in [(g, "GF"), (f, "FF")] {
        tx = tx.insert(Element::new(
            format!("{lname} Slab"),
            ElementKind::Slab(Slab {
                level,
                boundary: corners.to_vec(),
                thickness: 0.25,
                material: Some(concrete_id),
                top_offset: 0.0,
                bulges: vec![],
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
                    material: Some(brick_id),
                }),
            );
            let id = wall.id;
            // the walls run counter-clockwise, so outside is to their right
            let d = (corners[(i + 1) % 4] - corners[i]).normalize();
            let outside = -d.perp();
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
                let window = Element::new(
                    format!("{lname} Window {}.{}", i + 1, k + 1),
                    ElementKind::Opening(Opening {
                        host: id,
                        kind: OpeningKind::Window,
                        offset: off,
                        sill: 0.9,
                        width: 1.2,
                        height: 1.3,
                        material: None,
                    }),
                );
                tagged.push((window.id, level, outside * 0.9));
                tx = tx.insert(window);
            }
        }
    }
    let front_door = Element::new(
        "Front Door",
        ElementKind::Opening(Opening {
            host: ground_walls[0],
            kind: OpeningKind::Door,
            offset: 1.6,
            sill: 0.0,
            width: 1.0,
            height: 2.2,
            material: None,
        }),
    );
    let front_door_id = front_door.id;
    tagged.push((front_door.id, g, DVec2::new(0.0, 1.0)));
    tx = tx.insert(front_door);
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
            material: Some(stud_id),
        }),
    );
    let pid = part.id;
    let inner_door = Element::new(
        "GF Inner Door",
        ElementKind::Opening(Opening {
            host: pid,
            kind: OpeningKind::Door,
            offset: 3.5,
            sill: 0.0,
            width: 0.9,
            height: 2.1,
            material: None,
        }),
    );
    tagged.push((inner_door.id, g, DVec2::new(0.8, 0.0)));
    tx = tx.insert(part).insert(inner_door);

    // A freeform canopy over the entrance, modelled as a mesh and classified as a roof.
    let mut canopy = EditMesh::cuboid(DVec3::new(3.0, 1.6, 0.15));
    canopy.scale_face(1, 0.85);
    tx = tx.insert(Element::new(
        "Entrance Canopy",
        ElementKind::FreeMesh(FreeMesh {
            ifc_class: "IfcRoof".into(),
            material: Some(timber_id),
            ..FreeMesh::new(Some(g), DVec3::new(2.1, -0.95, 2.6), canopy)
        }),
    ));

    // A garden pergola built with modifiers: one post repeated by two
    // arrays into four, and one rafter repeated along the beam.
    let array = |count, x, y| {
        Modifier::new(ModifierKind::Array {
            count,
            offset: DVec3::new(x, y, 0.0),
        })
    };
    let pergola = DVec3::new(6.5, -5.4, 0.0);
    tx = tx
        .insert(Element::new(
            "Pergola Posts",
            ElementKind::FreeMesh(FreeMesh {
                ifc_class: "IfcColumn".into(),
                material: Some(timber_id),
                modifiers: vec![array(2, 3.0, 0.0), array(2, 0.0, 2.0)],
                ..FreeMesh::new(
                    Some(g),
                    pergola,
                    EditMesh::cuboid(DVec3::new(0.15, 0.15, 2.4)),
                )
            }),
        ))
        .insert(Element::new(
            "Pergola Rafters",
            ElementKind::FreeMesh(FreeMesh {
                ifc_class: "IfcBeam".into(),
                material: Some(timber_id),
                modifiers: vec![array(7, 0.5, 0.0)],
                ..FreeMesh::new(
                    Some(g),
                    pergola + DVec3::new(0.0, 1.0, 2.4),
                    EditMesh::cuboid(DVec3::new(0.08, 2.8, 0.2)),
                )
            }),
        ));

    // Views: a plan per level, two sections, four elevations and a 3D view.
    let plan_g = Element::new("Ground Floor plan", ElementKind::View(View::plan(g)));
    let plan_f = Element::new("First Floor plan", ElementKind::View(View::plan(f)));
    let mut axo = View::axonometric(35.0, 30.0);
    axo.scale = 60.0;
    let axo = Element::new("3D View South-East", ElementKind::View(axo));
    let (plan_g_id, plan_f_id) = (plan_g.id, plan_f.id);
    // a cropped 1:50 plan of the entrance, for the details sheet
    let mut entrance = View::plan(g);
    entrance.scale = 50.0;
    let entrance = Element::new("Entrance plan", ElementKind::View(entrance));
    let entrance_id = entrance.id;
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
    tx = tx
        .insert(plan_g)
        .insert(plan_f)
        .insert(entrance)
        .insert(axo);
    let mut section_a = None;
    for (name, a, b, cut) in [
        (
            "Section A",
            DVec2::new(2.1, -5.0),
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
        let mut view = View::section(a, b, 30.0, cut);
        if name == "South Elevation S" {
            view.style = ViewStyle::Shaded;
        }
        let e = Element::new(name, ElementKind::View(view));
        if name == "Section A" {
            section_a = Some(e.id);
        }
        // where each drawing sits on the sections sheet, and the drawing
        // point pinned there (along the line, height)
        let at = match name {
            "Section A" => Some((DVec2::new(105.0, 215.0), DVec2::new(8.0, 3.0))),
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

    // Schedules, listed on their own sheet next to the entrance detail.
    for (i, category) in [
        ScheduleCategory::Doors,
        ScheduleCategory::Windows,
        ScheduleCategory::Walls,
    ]
    .into_iter()
    .enumerate()
    {
        let e = Element::new(
            format!(
                "{} Schedule",
                &category.label()[..category.label().len() - 1]
            ),
            ElementKind::View(View::schedule(category)),
        );
        // the schedule's top-left corner is its drawing origin
        let top = [280.0, 238.0, 142.0][i];
        placements.push(("A601", e.id, DVec2::new(30.0, top), DVec2::ZERO));
        tx = tx.insert(e);
    }
    placements.push((
        "A601",
        entrance_id,
        DVec2::new(300.0, 190.0),
        DVec2::new(2.1, 0.0),
    ));

    // Dimensions and tags.
    let dim = |view: ElementId, a: HostPoint, b: HostPoint, offset: f64| {
        Element::new(
            "Dimension",
            ElementKind::Dimension(Dimension::linear(view, a, b, offset)),
        )
    };
    let on = |host: ElementId, u: f64, s: f64, from_end: bool| HostPoint {
        host: Some(host),
        local: DVec2::new(u, s),
        from_end,
        body: None,
    };
    let (south, west) = (ground_walls[0], ground_walls[3]);
    let face = -0.15;
    for plan in [plan_g_id, entrance_id] {
        // overall length and depth to the outer faces
        tx = tx
            .insert(dim(
                plan,
                on(south, -0.15, face, false),
                on(south, 0.15, face, true),
                -2.7,
            ))
            .insert(dim(
                plan,
                on(west, -0.15, face, false),
                on(west, 0.15, face, true),
                -1.95,
            ));
        // the front wall's openings as a chain
        let chain = [
            on(south, -0.15, face, false),
            on(front_door_id, 0.0, face, false),
            on(front_door_id, 1.0, face, false),
            on(south, 0.15, face, true),
        ];
        for w in chain.windows(2) {
            tx = tx.insert(dim(plan, w[0], w[1], -2.0));
        }
    }
    for (opening, level, offset) in tagged {
        let plan = if level == g { plan_g_id } else { plan_f_id };
        tx = tx.insert(Element::new(
            "Tag",
            ElementKind::Tag(Tag::new(plan, opening, offset)),
        ));
        if opening == front_door_id {
            tx = tx.insert(Element::new(
                "Tag",
                ElementKind::Tag(Tag::new(entrance_id, opening, offset)),
            ));
        }
    }
    if let Some(section) = section_a {
        // storey heights beside the section
        let level = |id: ElementId, h: f64| HostPoint {
            host: Some(id),
            local: DVec2::new(3.6, h),
            from_end: false,
            body: None,
        };
        tx = tx
            .insert(dim(section, level(g, 0.0), level(f, 0.0), 0.0))
            .insert(dim(section, level(f, 0.0), level(f, 3.0), 0.0));
    }

    // Sheets with the views placed on them.
    let mut sheets = std::collections::HashMap::new();
    for (number, title) in [
        ("A001", "3D View"),
        ("A101", "Floor Plans"),
        ("A201", "Sections and Elevations"),
        ("A601", "Schedules and Details"),
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
                // the entrance detail shows only the front of the house
                crop: (view == entrance_id)
                    .then_some([DVec2::new(-0.8, -2.8), DVec2::new(5.0, 2.0)]),
            }),
        ));
    }

    doc.commit(tx).expect("sample house is valid");
    doc.model().clone()
}
