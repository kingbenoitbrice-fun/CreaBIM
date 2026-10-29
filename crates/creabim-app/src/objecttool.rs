//! The Object tool: places the object picked in the Catalogue window where
//! the pointer is. An object that stands against a wall (a WC, a kitchen
//! unit, a bed) turns its back to the nearest wall face within reach and
//! sits on it; anything else, or with no wall near, stands where clicked,
//! turned as set in the tool bar, and what hangs from the ceiling (a
//! light, a diffuser) is centred where clicked. Each goes in at its type's
//! height: a socket 300 mm up, a switch 1100 mm. The plan and the 3D view
//! show its symbol under the pointer before the click.

use crate::planview::PlanState;
use crate::selection::Selection;
use creabim_core::objects::{self, CatalogueObject, Mount, Object};
use creabim_core::{Document, ElementId, ElementKind, Model, ModelError};
use eframe::egui::{self, Color32, Pos2, Stroke};
use glam::{DVec2, DVec3};

/// How far from a wall face an object that stands against walls finds it.
pub const WALL_REACH: f64 = 0.6;

/// What the Object tool places, and how it is turned.
#[derive(Clone, Debug, Default)]
pub struct ObjectSettings {
    /// The catalogue object placed; none until one is picked.
    pub current: Option<CatalogueObject>,
    /// When it is a type of the project's own: new objects are of it.
    pub project_type: Option<ElementId>,
    /// The turn of objects placed away from walls, degrees anticlockwise.
    pub rotation: f64,
    /// Snap to walls (for those that stand against them).
    pub free: bool,
}

/// How an object went against a wall.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Walled {
    /// It went against one.
    pub on: bool,
    /// How far it slid along it to keep clear of a door or window.
    pub slid: f64,
    /// It fits nowhere on that wall clear of its doors and windows.
    pub blocked: bool,
}

/// Where and how turned `c` goes for a pointer at `p` on `level`, and
/// how it went against a wall: clear of the wall's doors and windows.
pub fn placement(
    model: &Model,
    level: ElementId,
    p: DVec2,
    c: &CatalogueObject,
    set: &ObjectSettings,
) -> (DVec2, f64, Walled) {
    if c.object.mount == Mount::Wall && !set.free {
        if let Some(s) =
            objects::against_wall_clear(model, level, p, WALL_REACH, &c.object, c.object.offset)
        {
            let walled = Walled {
                on: true,
                slid: s.slid,
                blocked: s.blocked,
            };
            return (s.at, s.rotation, walled);
        }
    }
    let rotation = set.rotation.rem_euclid(360.0);
    if c.object.mount == Mount::Ceiling {
        return (
            objects::centred(&c.object, p, rotation),
            rotation,
            Walled::default(),
        );
    }
    (p, rotation, Walled::default())
}

/// A click with the Object tool.
pub fn click(
    doc: &mut Document,
    level: ElementId,
    p: DVec2,
    state: &mut PlanState,
    selection: &mut Selection,
) -> Result<(), ModelError> {
    let Some(c) = state.objects.current.clone() else {
        state.status = Some("Pick an object in the Catalogue window first.".into());
        state.want_catalogue = true;
        return Ok(());
    };
    let (at, rotation, walled) = placement(doc.model(), level, p, &c, &state.objects);
    let own = state
        .objects
        .project_type
        .filter(|t| objects::type_of_id(doc.model(), *t).is_some());
    let (tx, id) = match own {
        Some(t) => {
            let mut e = objects::instance(t, &c.name, level, at, rotation);
            if let (ElementKind::Object(o), Some(ty)) =
                (&mut e.kind, objects::type_of_id(doc.model(), t))
            {
                o.offset = ty.offset;
            }
            let id = e.id;
            (
                creabim_core::Transaction::new(format!("Place {}", c.name)).insert(e),
                id,
            )
        }
        None => objects::place(doc.model(), &c, level, at, rotation),
    };
    doc.commit(tx)?;
    selection.only(id);
    state.status = Some(if walled.blocked {
        format!(
            "Placed {} against the wall, but it is in the way of a door or window there and no stretch of that wall is clear. Click for another; Esc ends.",
            c.name
        )
    } else if walled.slid > 1e-3 {
        format!(
            "Placed {} against the wall, slid {} along it to keep clear of a door or window. Click for another; Esc ends.",
            c.name,
            crate::units::len(walled.slid)
        )
    } else if walled.on {
        format!(
            "Placed {} against the wall. Click for another; Esc ends.",
            c.name
        )
    } else {
        format!(
            "Placed {}. Click for another; the tool bar turns the next one, Esc ends.",
            c.name
        )
    });
    Ok(())
}

/// The object about to be placed, drawn under the pointer: its plan
/// symbol on the floor, and in 3D its box too.
pub fn preview(
    painter: &egui::Painter,
    project: &dyn Fn(DVec3) -> Option<Pos2>,
    model: &Model,
    level: ElementId,
    p: DVec2,
    state: &PlanState,
    boxed: bool,
) {
    let Some(c) = &state.objects.current else {
        return;
    };
    let (at, rotation, walled) = placement(model, level, p, c, &state.objects);
    let z = model.level(level).map_or(0.0, |l| l.elevation);
    let mut o = Object::new(level, at, rotation);
    o.offset = c.object.offset;
    // its box where it will stand: up a wall, or under the ceiling
    let base = objects::base_z_of(model, &o, &c.object) - z;
    let stroke = Stroke::new(1.5, Color32::from_rgb(230, 120, 30));
    let thin = Stroke::new(1.0, Color32::from_rgb(230, 120, 30));
    let line = |pts: &[DVec3], s: Stroke| {
        let scr: Option<Vec<Pos2>> = pts.iter().map(|q| project(*q)).collect();
        if let Some(scr) = scr {
            painter.add(egui::Shape::line(scr, s));
        }
    };
    let on = |q: DVec2, h: f64| o.to_plan(q).extend(z + h);
    for item in c.object.plan_symbol() {
        match item {
            objects::SymbolItem::Loop(pts) => {
                let mut v: Vec<DVec3> = pts.iter().map(|q| on(*q, 0.0)).collect();
                v.extend(v.first().copied());
                line(&v, thin);
            }
            objects::SymbolItem::Line(a, b) => line(&[on(a, 0.0), on(b, 0.0)], thin),
        }
    }
    let (lo, hi) = c.object.bounds();
    let corners = [
        DVec2::new(lo.x, lo.y),
        DVec2::new(hi.x, lo.y),
        DVec2::new(hi.x, hi.y),
        DVec2::new(lo.x, hi.y),
        DVec2::new(lo.x, lo.y),
    ];
    let bottom: Vec<DVec3> = corners.iter().map(|q| on(*q, base + lo.z)).collect();
    line(&bottom, stroke);
    if boxed {
        let top: Vec<DVec3> = corners.iter().map(|q| on(*q, base + hi.z)).collect();
        line(&top, thin);
        for (a, b) in bottom.iter().zip(&top).take(4) {
            line(&[*a, *b], thin);
        }
    }
    // which way it faces
    let front = on(DVec2::new(0.0, lo.y), 0.0);
    let ahead = front + o.facing().extend(0.0) * 0.25;
    line(&[front, ahead], stroke);
    if let Some(s) = project(on(DVec2::new(hi.x, lo.y), (base + hi.z).max(0.0))) {
        painter.text(
            s + egui::Vec2::new(10.0, -10.0),
            egui::Align2::LEFT_BOTTOM,
            if walled.blocked {
                format!("{} · in the way of a door or window", c.name)
            } else if walled.slid > 1e-3 {
                format!("{} · against the wall, clear of the opening", c.name)
            } else if walled.on {
                format!("{} · against the wall", c.name)
            } else {
                format!("{} · {:.0}°", c.name, rotation)
            },
            egui::FontId::proportional(12.0),
            stroke.color,
        );
    }
}

/// The tool bar's fields for the Object tool.
pub fn tool_options(state: &PlanState) -> Vec<serde_json::Value> {
    use serde_json::json;
    if state.tool != crate::planview::Tool::Object {
        return vec![];
    }
    let s = &state.objects;
    vec![
        json!({
            "kind": "toggle", "id": "object:catalogue", "label": s.current.as_ref().map_or("Pick an object…".to_string(), |c| c.name.clone()),
            "on": false, "icon": "sofa",
            "hint": "The object placed: pick another in the Catalogue window",
        }),
        json!({
            "kind": "number", "key": "object.rotation", "label": "Turn", "value": s.rotation,
            "unit": "°", "step": 15.0, "min": -360.0, "max": 360.0,
            "hint": "The turn of objects placed away from walls",
        }),
        json!({
            "kind": "toggle", "id": "object:turn", "label": "Turn 90°", "on": false,
            "icon": "rotate-cw", "hint": "Turn the next object a quarter (also the Space key)",
        }),
        json!({
            "kind": "toggle", "id": "object:free", "label": "Free of walls", "on": s.free,
            "icon": "move", "hint": "Place where clicked even near a wall, turned as set",
        }),
        json!({"kind": "sep"}),
    ]
}

/// A toggle clicked in the tool bar. True when it asks for the catalogue.
pub fn toggle_option(state: &mut PlanState, what: &str) -> bool {
    let s = &mut state.objects;
    match what {
        "turn" => s.rotation = (s.rotation + 90.0).rem_euclid(360.0),
        "free" => s.free = !s.free,
        "catalogue" => return true,
        _ => {}
    }
    false
}

/// A number typed in the tool bar.
pub fn set_option(state: &mut PlanState, key: &str, v: f64) {
    if key == "object.rotation" {
        state.objects.rotation = v.rem_euclid(360.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::sample::sample_house;

    fn ground(doc: &Document) -> ElementId {
        doc.model()
            .levels()
            .min_by(|a, b| a.1.elevation.total_cmp(&b.1.elevation))
            .unwrap()
            .0
            .id
    }

    #[test]
    fn an_object_put_in_a_doorway_slides_clear_of_it() {
        let mut doc = Document::from_model(sample_house());
        let model = doc.model();
        let (door, wall) = model
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Opening(o) if o.kind == creabim_core::OpeningKind::Door => {
                    match &model.get(o.host)?.kind {
                        ElementKind::Wall(w) => Some((o.clone(), w.clone())),
                        _ => None,
                    }
                }
                _ => None,
            })
            .unwrap();
        let level = wall.level;
        let u = door.offset + door.width / 2.0;
        let mut st = PlanState::default();
        let mut sel = Selection::default();
        st.objects.current = creabim_core::catalogue::starter("starter/wc");
        // just inside the doorway, on either side of the wall
        for side in [1.0, -1.0] {
            let p = wall.start
                + wall.direction() * u
                + wall.direction().perp() * side * (wall.thickness / 2.0 + 0.2);
            click(&mut doc, level, p, &mut st, &mut sel).unwrap();
            let msg = st.status.clone().unwrap_or_default();
            assert!(msg.contains("slid") || msg.contains("in the way"), "{msg}");
        }
        let clashes = creabim_core::checks::objects_in_openings(doc.model());
        assert!(
            clashes
                .iter()
                .all(|c| c.opening != door_id(doc.model(), &door)),
            "{clashes:?}"
        );
    }

    fn door_id(m: &Model, o: &creabim_core::Opening) -> ElementId {
        m.iter()
            .find(|e| matches!(&e.kind, ElementKind::Opening(x) if x == o))
            .unwrap()
            .id
    }

    #[test]
    fn objects_go_against_walls_or_where_clicked() {
        let mut doc = Document::from_model(sample_house());
        let level = ground(&doc);
        let mut st = PlanState::default();
        let mut sel = Selection::default();
        // nothing picked: asks for the catalogue
        click(&mut doc, level, DVec2::new(2.0, 2.0), &mut st, &mut sel).unwrap();
        assert!(st.want_catalogue);
        let n0 = doc.model().iter().count();
        // a WC near the south wall's inside face backs onto it
        st.objects.current = creabim_core::catalogue::starter("starter/wc");
        let wall = doc
            .model()
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Wall(w) if w.level == level => Some(w.clone()),
                _ => None,
            })
            .unwrap();
        let mid = (wall.start + wall.end) / 2.0;
        let inside = mid + wall.direction().perp() * (wall.thickness / 2.0 + 0.3);
        click(&mut doc, level, inside, &mut st, &mut sel).unwrap();
        let id = sel.active.unwrap();
        let ElementKind::Object(o) = &doc.model().get(id).unwrap().kind else {
            panic!("an object")
        };
        let face = mid + wall.direction().perp() * wall.thickness / 2.0;
        assert!(
            o.position.distance(face) < 1e-6,
            "{:?} {face:?}",
            o.position
        );
        // it faces away from the wall
        assert!(o.facing().dot(inside - face) > 0.0);
        assert!(doc.model().iter().count() > n0 + 1);
        // free of walls: where clicked, turned as set
        st.objects.free = true;
        st.objects.rotation = 90.0;
        click(&mut doc, level, inside, &mut st, &mut sel).unwrap();
        let ElementKind::Object(o) = &doc.model().get(sel.active.unwrap()).unwrap().kind else {
            panic!()
        };
        assert!(o.position.distance(inside) < 1e-9);
        assert_eq!(o.rotation, 90.0);
        // its type came in once for both
        let wc_types = doc
            .model()
            .iter()
            .filter(|e| match &e.kind {
                ElementKind::ElementType(t) => matches!(
                    &t.params,
                    creabim_core::TypeParams::Object(o) if o.catalogue.as_deref() == Some("starter/wc")
                ),
                _ => false,
            })
            .count();
        assert_eq!(wc_types, 1);
        // toggles
        assert!(toggle_option(&mut st, "catalogue"));
        toggle_option(&mut st, "turn");
        assert_eq!(st.objects.rotation, 180.0);
        set_option(&mut st, "object.rotation", -90.0);
        assert_eq!(st.objects.rotation, 270.0);
    }

    #[test]
    fn fixtures_go_in_at_their_height_and_lights_where_clicked() {
        let mut doc = Document::from_model(sample_house());
        let level = ground(&doc);
        let mut st = PlanState::default();
        let mut sel = Selection::default();
        // a switch backs onto the south wall, 1100 mm up
        st.objects.current = creabim_core::catalogue::starter("starter/switch-1-gang");
        click(&mut doc, level, DVec2::new(3.0, 0.4), &mut st, &mut sel).unwrap();
        let e = doc.model().get(sel.active.unwrap()).unwrap();
        let ElementKind::Object(o) = &e.kind else {
            panic!("an object")
        };
        assert!((o.offset - 1.1).abs() < 1e-9 && (o.position.y - 0.15).abs() < 1e-9);
        // a second one of the project's own type takes the type's height too
        st.objects.project_type = e.of_type.as_ref().map(|t| t.id);
        click(&mut doc, level, DVec2::new(4.0, 0.4), &mut st, &mut sel).unwrap();
        let ElementKind::Object(o) = &doc.model().get(sel.active.unwrap()).unwrap().kind else {
            panic!()
        };
        assert!((o.offset - 1.1).abs() < 1e-9);
        // a downlight hangs under the ceiling, its middle where clicked,
        // even next to a wall
        st.objects.project_type = None;
        st.objects.current = creabim_core::catalogue::starter("starter/downlight");
        let p = DVec2::new(3.0, 0.4);
        click(&mut doc, level, p, &mut st, &mut sel).unwrap();
        let m = doc.model();
        let e = m.get(sel.active.unwrap()).unwrap();
        let c = creabim_geom::annotate::centre(m, e.id).unwrap();
        assert!((c.truncate() - p).length() < 1e-9, "{c}");
        assert!(c.z > 2.7 && c.z < 2.75, "{c}");
    }
}
