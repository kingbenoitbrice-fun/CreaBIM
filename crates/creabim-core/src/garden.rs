//! The sample house in its garden: the demo of extrusions, sweeps, voids,
//! screen walls and the site tools. A chimney stack on the west gable with
//! its firebox cut through the wall, a breeze-block screen with a coping
//! behind the patio, terrain with a mound, paving, a gravel path, a
//! driveway, trees and shrubs, a car and a person.

use crate::parts::{self, Path, WallEdge};
use crate::site::{Pattern, PlantForm};
use crate::{library, Document, Element, ElementId, ElementKind, Model, Part, Site, Transaction};
use glam::{DVec2, DVec3};

fn rect(a: (f64, f64), b: (f64, f64)) -> Vec<DVec2> {
    crate::polygon::rectangle(DVec2::new(a.0, a.1), DVec2::new(b.0, b.1))
}

/// A material by name: the model's own, or added from the library.
fn material(doc: &mut Document, name: &str) -> Option<ElementId> {
    let (id, new) = library::find_or_new(doc.model(), name)?;
    if let Some(e) = new {
        doc.commit(Transaction::new("Material").insert(e)).ok()?;
    }
    Some(id)
}

pub fn garden_house() -> Model {
    let mut doc = Document::from_model(crate::sample::sample_house());
    let find = |m: &Model, name: &str| m.iter().find(|e| e.name == name).map(|e| e.id);
    let g = find(doc.model(), "Ground Floor").expect("the sample has its levels");
    let brick = find(doc.model(), "Brick");
    let concrete = find(doc.model(), "Concrete");
    let (tx, types) = crate::types::add_starters(doc.model());
    doc.commit(tx).expect("starter types are valid");
    let screen_type = types
        .iter()
        .copied()
        .find(|id| crate::screen::is_screen_type(doc.model(), *id))
        .expect("a screen wall type");
    let grass = material(&mut doc, "Grass");
    let flags = material(&mut doc, "Paving, concrete flags");
    let blocks = material(&mut doc, "Block paving");
    let gravel = material(&mut doc, "Gravel");

    let mut tx = Transaction::new("Garden");
    // a chimney stack on the west gable, its firebox cut through the wall
    let mut stack = Part::extrusion(g, rect((-1.05, 1.0), (-0.15, 2.4)), 0.0, 7.2);
    stack.material = brick;
    stack.ifc_class = "IfcChimney".into();
    tx = tx.insert(Element::new("Chimney stack", ElementKind::Part(stack)));
    let firebox = Part::extrusion(g, rect((-0.75, 1.3), (0.25, 2.1)), 0.12, 0.8).as_void();
    tx = tx.insert(Element::new("Firebox", ElementKind::Part(firebox)));
    let hearth = {
        let mut p = Part::extrusion(g, rect((0.15, 1.1), (0.75, 2.3)), 0.0, 0.1);
        p.material = concrete;
        p
    };
    tx = tx.insert(Element::new("Hearth", ElementKind::Part(hearth)));

    // a breeze-block screen behind the patio, with a coping on its top
    let mut screen = crate::Wall::new(g, DVec2::new(-1.0, 10.0), DVec2::new(9.0, 10.0), 0.1, 1.8);
    screen.material = concrete;
    let mut screen = Element::new("Garden screen", ElementKind::Wall(screen));
    screen.of_type = Some(crate::types::TypeRef::new(screen_type));
    let screen_id = screen.id;
    tx = tx.insert(screen);
    let mut coping = Part::sweep(
        g,
        parts::coping(0.1, 0.03, 0.05),
        Path::Wall {
            wall: screen_id,
            edge: WallEdge::Top,
            offset: 0.0,
            extend: [0.03, 0.03],
        },
    );
    coping.material = concrete;
    coping.ifc_class = "IfcCovering".into();
    tx = tx.insert(Element::new("Coping", ElementKind::Part(coping)));

    // the ground, flat round the house, and a mound in the north-east
    let mut ground = Site::terrain(g, rect((-9.0, -14.0), (22.0, 15.0)), -0.15, 0.6);
    ground.material = grass;
    tx = tx.insert(Element::new("Ground", ElementKind::Site(ground)));
    let mut mound = Site::terrain(
        g,
        vec![
            DVec2::new(13.0, 8.0),
            DVec2::new(18.0, 7.0),
            DVec2::new(21.5, 10.0),
            DVec2::new(21.5, 14.5),
            DVec2::new(15.0, 14.5),
            DVec2::new(12.0, 11.5),
        ],
        -0.15,
        0.3,
    );
    if let crate::SiteForm::Terrain {
        points, contour, ..
    } = &mut mound.form
    {
        points.push(DVec3::new(17.5, 11.5, 1.2));
        points.push(DVec3::new(15.0, 10.5, 0.6));
        *contour = 0.25;
    }
    mound.material = grass;
    tx = tx.insert(Element::new("Mound", ElementKind::Site(mound)));

    // hard landscape, flush with the ground
    let area = |outline: Vec<DVec2>, depth: f64, pattern: Pattern, material| {
        let mut a = Site::area(g, outline, depth, pattern);
        if let crate::SiteForm::Area { top, .. } = &mut a.form {
            *top = -0.15;
        }
        a.material = material;
        ElementKind::Site(a)
    };
    let flag = Pattern::Units {
        size: DVec2::new(0.6, 0.4),
        stagger: true,
        angle: 0.0,
    };
    let block = Pattern::Units {
        size: DVec2::new(0.2, 0.1),
        stagger: true,
        angle: 90.0,
    };
    tx = tx
        .insert(Element::new(
            "Patio",
            area(rect((0.0, 6.15), (9.0, 9.0)), 0.1, flag, flags),
        ))
        .insert(Element::new(
            "Front path",
            area(
                rect((1.0, -9.0), (2.0, -0.15)),
                0.1,
                Pattern::Stipple { spacing: 0.12 },
                gravel,
            ),
        ))
        .insert(Element::new(
            "Driveway",
            area(rect((11.5, -14.0), (15.0, 4.0)), 0.15, block, blocks),
        ));

    // planting
    for (name, at, form, canopy, height) in [
        ("Oak", (-5.0, -6.0), PlantForm::Tree, 6.0, 9.0),
        ("Birch", (5.5, -8.0), PlantForm::Tree, 4.0, 7.0),
        ("Palm", (-5.0, 9.0), PlantForm::Palm, 4.0, 6.5),
        ("Pine", (18.5, -9.0), PlantForm::Conifer, 3.5, 9.0),
        ("Pine", (19.0, -4.0), PlantForm::Conifer, 3.0, 7.5),
        ("Shrub", (1.0, 11.2), PlantForm::Shrub, 1.4, 1.2),
        ("Shrub", (3.5, 11.4), PlantForm::Shrub, 1.6, 1.3),
        ("Shrub", (6.5, 11.2), PlantForm::Shrub, 1.4, 1.1),
        ("Shrub", (10.8, -6.0), PlantForm::Shrub, 1.2, 1.0),
    ] {
        let mut p = Site::plant(g, DVec2::new(at.0, at.1), form, canopy, height);
        if let crate::SiteForm::Plant { base, .. } = &mut p.form {
            *base = -0.15;
        }
        tx = tx.insert(Element::new(name, ElementKind::Site(p)));
    }
    doc.commit(tx).expect("the garden is valid");

    // a car on the drive and a person by the front path
    for (key, at, turn) in [
        ("car", DVec2::new(13.25, 0.5), 0.0),
        ("person", DVec2::new(3.0, -4.0), 200.0),
    ] {
        let c = crate::catalogue::starter(&format!("starter/{key}")).expect("an entourage starter");
        let (tx, _) = crate::objects::place(doc.model(), &c, g, at, turn);
        doc.commit(tx).expect("entourage is valid");
    }
    doc.model().clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_garden_has_every_part_and_the_firebox_cuts_wall_and_stack() {
        let m = garden_house();
        let count = |f: &dyn Fn(&ElementKind) -> bool| m.iter().filter(|e| f(&e.kind)).count();
        assert_eq!(count(&|k| matches!(k, ElementKind::Part(p) if p.void)), 1);
        assert_eq!(count(&|k| matches!(k, ElementKind::Part(p) if !p.void)), 3);
        assert!(count(&|k| matches!(k, ElementKind::Site(s) if s.is_plant())) >= 5);
        let cuts = parts::void_cuts(&m);
        let name = |id: ElementId| m.get(id).unwrap().name.clone();
        let mut targets: Vec<String> = cuts.iter().map(|c| name(c.0)).collect();
        targets.sort();
        assert_eq!(targets, ["Chimney stack", "GF Wall 4"], "{targets:?}");
        let screen = m.iter().find(|e| e.name == "Garden screen").unwrap();
        assert!(crate::screen::is_screen(&m, screen.id));
        assert!(crate::screen::screen_openings(&m, screen.id).len() > 50);
    }
}
