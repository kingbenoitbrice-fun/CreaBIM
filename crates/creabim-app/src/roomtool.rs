//! The Room, Room separator and Ceiling tools. A room is placed by
//! clicking inside walls (it then follows them as they move) or drawn as
//! an outline where nothing closes it off; each new room gets a tag in
//! the level's plans. A ceiling goes over the room clicked, following it,
//! or is drawn. A room separator is a line of two clicks.

use crate::planview::{PlanState, Tool};
use crate::selection::Selection;
use creabim_core::room::{self, CEILING_HEIGHT};
use creabim_core::{
    Ceiling, Document, Element, ElementId, ElementKind, Model, ModelError, Room, RoomField,
    Separator, Tag, Transaction, ViewKind,
};
use eframe::egui;
use glam::DVec2;

/// What an outline drawn with the slab modes becomes, instead of a slab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    Room,
    Ceiling,
}

impl Space {
    pub fn of(tool: Tool) -> Option<Space> {
        match tool {
            Tool::Room => Some(Space::Room),
            Tool::Ceiling => Some(Space::Ceiling),
            _ => None,
        }
    }
}

/// A point well inside `poly`: its middle when that is inside, else the
/// middle of its first triangle.
fn inside_point(poly: &[DVec2]) -> DVec2 {
    let middle = poly.iter().copied().sum::<DVec2>() / poly.len().max(1) as f64;
    if creabim_core::polygon::contains(poly, middle) {
        return middle;
    }
    creabim_core::polygon::triangulate(poly)
        .first()
        .map_or(middle, |t| (poly[t[0]] + poly[t[1]] + poly[t[2]]) / 3.0)
}

/// The height a new ceiling hangs at on `level`: the usual one, or lower
/// in a storey too low for it.
fn ceiling_height(model: &Model, level: ElementId) -> f64 {
    let storey = model.level(level).map_or(3.0, |l| l.height);
    CEILING_HEIGHT.min(storey - 0.1).max(0.5)
}

/// A room on `level` whose outline holds `p`.
pub fn room_at(model: &Model, level: ElementId, p: DVec2) -> Option<ElementId> {
    model
        .iter()
        .filter(|e| matches!(&e.kind, ElementKind::Room(r) if r.level == level))
        .find(|e| {
            room::outline(model, e.id).is_some_and(|o| creabim_core::polygon::contains(&o, p))
        })
        .map(|e| e.id)
}

/// A new room with its next number, and a tag for it at its point in each
/// of its level's plans (not its ceiling plans).
fn room_with_tags(model: &Model, r: Room) -> (Element, Vec<Element>) {
    let level = r.level;
    let mut r = r;
    r.number = room::next_number(model, level);
    let room = Element::new("Room", ElementKind::Room(r));
    let style = model
        .iter()
        .find(|e| e.name == "Room name" && matches!(e.kind, ElementKind::TextStyle(_)))
        .map(|e| e.id);
    let tags = creabim_core::levels::plan_views(model, level)
        .into_iter()
        .filter(|v| {
            matches!(model.get(*v).map(|e| &e.kind),
                Some(ElementKind::View(v)) if matches!(v.kind, ViewKind::Plan { .. }) && !v.ceiling_plan)
        })
        .map(|v| {
            let mut t = Tag::new(v, room.id, DVec2::ZERO);
            t.style = style;
            Element::new("Room tag", ElementKind::Tag(t))
        })
        .collect();
    (room, tags)
}

fn commit_room(
    doc: &mut Document,
    r: Room,
    label: &str,
    selection: &mut Selection,
) -> Result<ElementId, ModelError> {
    let (room, tags) = room_with_tags(doc.model(), r);
    let id = room.id;
    let mut tx = Transaction::new(label).insert(room);
    for t in tags {
        tx = tx.insert(t);
    }
    doc.commit(tx)?;
    selection.only(id);
    Ok(id)
}

/// What a closed outline drawn with the Room or Ceiling tool becomes.
pub fn drawn(
    doc: &mut Document,
    space: Space,
    level: ElementId,
    outline: Vec<DVec2>,
    selection: &mut Selection,
) -> Result<String, ModelError> {
    match space {
        Space::Room => {
            let mut r = Room::new(level, inside_point(&outline));
            r.boundary = outline;
            if creabim_core::polygon::signed_area2(&r.boundary) < 0.0 {
                r.boundary.reverse();
            }
            let id = commit_room(doc, r, "Draw room", selection)?;
            let units = doc.model().units();
            Ok(format!(
                "Room {} added with the outline you drew ({}). Name it in Properties. Ctrl+Z to undo.",
                number_of(doc.model(), id),
                units.area(room::area(doc.model(), id))
            ))
        }
        Space::Ceiling => {
            let c = Ceiling::drawn(level, outline, ceiling_height(doc.model(), level));
            let e = Element::new("Ceiling", ElementKind::Ceiling(c));
            let id = e.id;
            doc.commit(Transaction::new("Draw ceiling").insert(e))?;
            selection.only(id);
            Ok(format!(
                "Ceiling added {} above the floor. Set its height, thickness and grid in Properties. Ctrl+Z to undo.",
                crate::units::len(ceiling_height(doc.model(), level))
            ))
        }
    }
}

fn number_of(model: &Model, id: ElementId) -> String {
    match model.get(id).map(|e| &e.kind) {
        Some(ElementKind::Room(r)) => r.number.clone(),
        _ => String::new(),
    }
}

/// A click inside walls with the Room or Ceiling tool (Fill room).
pub fn fill(
    doc: &mut Document,
    space: Space,
    level: ElementId,
    p: DVec2,
    selection: &mut Selection,
) -> Result<String, ModelError> {
    let model = doc.model();
    match space {
        Space::Room => {
            if let Some(there) = room_at(model, level, p) {
                selection.only(there);
                let name = model.get(there).map_or("", |e| e.name.as_str());
                return Ok(format!(
                    "There's a room here already ({name}). Add a room separator to split it."
                ));
            }
            let Some(shape) = room::enclosed(model, level, p) else {
                return Ok("That spot isn't closed in by walls or room separators on this level. Click inside a room, or draw its outline (1 Polygon, 2 Rectangle).".into());
            };
            let id = commit_room(doc, Room::new(level, p), "Place room", selection)?;
            Ok(format!(
                "Room {} added: {}, to the walls' inner faces. It follows them when they move. Name it in Properties. Ctrl+Z to undo.",
                number_of(doc.model(), id),
                doc.model().units().area(shape.area())
            ))
        }
        Space::Ceiling => {
            let height = ceiling_height(model, level);
            let c = match room_at(model, level, p) {
                Some(r) => {
                    let covered = model.iter().any(
                        |e| matches!(&e.kind, ElementKind::Ceiling(c) if c.room == Some(r)),
                    );
                    if covered {
                        return Ok("That room has a ceiling already; select it to change its height.".into());
                    }
                    Ceiling::in_room(level, r, height)
                }
                None => match room::enclosed(model, level, p) {
                    Some(shape) => Ceiling::drawn(level, shape.outline, height),
                    None => {
                        return Ok("That spot isn't closed in by walls on this level. Click inside a room, or draw the ceiling's outline (1 Polygon, 2 Rectangle).".into())
                    }
                },
            };
            let follows = c.room.is_some();
            let e = Element::new("Ceiling", ElementKind::Ceiling(c));
            let id = e.id;
            doc.commit(Transaction::new("Place ceiling").insert(e))?;
            selection.only(id);
            Ok(if follows {
                format!("Ceiling added over the room, {} above the floor; it follows the room's walls. Ctrl+Z to undo.", crate::units::len(height))
            } else {
                format!("Ceiling added inside the walls, {} above the floor. Place a room first for one that follows them. Ctrl+Z to undo.", crate::units::len(height))
            })
        }
    }
}

/// A click with the Room separator tool: its start, then its end.
pub fn separator_click(
    doc: &mut Document,
    level: ElementId,
    p: DVec2,
    state: &mut PlanState,
    selection: &mut Selection,
) -> Result<(), ModelError> {
    let Some(a) = state.pending.take() else {
        state.pending = Some(p);
        state.status = Some("Click where the separator ends.".into());
        return Ok(());
    };
    if a.distance(p) < 0.05 {
        state.pending = Some(a);
        return Ok(());
    }
    let e = Element::new(
        "Room separator",
        ElementKind::RoomSeparator(Separator { level, a, b: p }),
    );
    selection.only(e.id);
    state.status = Some("Room separator added: rooms on either side of it are now apart.".into());
    doc.commit(Transaction::new("Draw room separator").insert(e))
}

/// Tag every room on plan `view`'s level that has no tag in it yet, at the
/// room's point. Returns how many were tagged.
pub fn tag_rooms(doc: &mut Document, view: ElementId) -> Result<usize, ModelError> {
    let model = doc.model();
    let Some(ElementKind::View(v)) = model.get(view).map(|e| &e.kind) else {
        return Ok(0);
    };
    let ViewKind::Plan { level, .. } = v.kind else {
        return Ok(0);
    };
    let style = model
        .iter()
        .find(|e| e.name == "Room name" && matches!(e.kind, ElementKind::TextStyle(_)))
        .map(|e| e.id);
    let tagged: std::collections::HashSet<ElementId> = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Tag(t) if t.view == view => Some(t.host),
            _ => None,
        })
        .collect();
    let mut tx = Transaction::new("Tag rooms");
    let mut n = 0;
    for e in model.iter() {
        if matches!(&e.kind, ElementKind::Room(r) if r.level == level) && !tagged.contains(&e.id) {
            let mut t = Tag::new(view, e.id, DVec2::ZERO);
            t.style = style;
            tx = tx.insert(Element::new("Room tag", ElementKind::Tag(t)));
            n += 1;
        }
    }
    if n > 0 {
        doc.commit(tx)?;
    }
    Ok(n)
}

/// Turn room labels written as notes into rooms; the status message.
pub fn rooms_from_notes(doc: &mut Document) -> String {
    let (tx, skipped) = room::from_notes(doc.model());
    let made = tx
        .ops
        .iter()
        .filter(
            |o| matches!(o, creabim_core::Op::Insert(e) if matches!(e.kind, ElementKind::Room(_))),
        )
        .count();
    if made > 0 {
        if let Err(e) = doc.commit(tx) {
            return format!("Couldn't do that: {e}");
        }
    }
    let left = if skipped.is_empty() {
        String::new()
    } else {
        format!(
            " Left as notes, as no walls close them off or a room is there already: {}. Draw those rooms, or add room separators.",
            skipped.join(", ")
        )
    };
    match made {
        0 => format!("No room labels (a name over an area, such as LIVING / 37.2 m²) became rooms.{left}"),
        n => format!("{n} room labels are now rooms with tags; their areas follow the walls. Ctrl+Z to undo.{left}"),
    }
}

/// The objects standing in a doorway or a door's swing, and a message
/// listing them.
pub fn check_doorways(model: &Model) -> (Vec<ElementId>, String) {
    let clashes = creabim_core::checks::objects_in_openings(model);
    if clashes.is_empty() {
        return (
            vec![],
            "No objects stand in a doorway, a window or a door's swing.".into(),
        );
    }
    let mut ids: Vec<ElementId> = clashes.iter().map(|c| c.object).collect();
    ids.dedup();
    let lines = creabim_core::checks::report(model, &clashes);
    (
        ids,
        format!(
            "{} objects are in the way (selected): {}.",
            clashes.len(),
            lines.join("; ")
        ),
    )
}

/// A text field with its label; true when it changed.
fn text(ui: &mut egui::Ui, label: &str, v: &mut String) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(v).changed()
    })
    .inner
}

/// A length in the project's units, with its label; true when it changed.
fn length(ui: &mut egui::Ui, label: &str, v: &mut f64, lo: f64, hi: f64) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(crate::units::drag(v).range(lo..=hi)).changed()
    })
    .inner
}

/// A room's properties: what it measures, then its number, finishes,
/// occupancy, department and height. Returns the edit's label, or "".
pub fn room_fields(ui: &mut egui::Ui, model: &Model, r: &mut Room) -> &'static str {
    let mut label = "";
    match room::shape_of(model, r) {
        Some(s) => {
            ui.label(format!(
                "Area {}   Perimeter {}",
                crate::units::area(room::net_area(model, r, &s)),
                crate::units::len(s.perimeter())
            ));
            ui.label(
                egui::RichText::new(if r.is_drawn() {
                    "Its outline is drawn; it doesn't follow the walls."
                } else {
                    "Measured to the walls' inner faces; it follows them as they move."
                })
                .weak(),
            );
        }
        None => {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Not enclosed: the walls and separators round its point don't close it off.",
            );
        }
    }
    if text(ui, "Number", &mut r.number) {
        label = "Room number";
    }
    for (name, v) in [
        ("Floor finish", &mut r.floor_finish),
        ("Wall finish", &mut r.wall_finish),
        ("Ceiling finish", &mut r.ceiling_finish),
        ("Occupancy", &mut r.occupancy),
        ("Department", &mut r.department),
    ] {
        if text(ui, name, v) {
            label = "Room data";
        }
    }
    let mut own = r.height.is_some();
    let found = room::height(
        model,
        &Room {
            height: None,
            ..r.clone()
        },
    );
    if ui
        .checkbox(&mut own, "Own height")
        .on_hover_text("Otherwise it is as high as its ceiling, or its storey")
        .changed()
    {
        r.height = own.then_some(found);
        label = "Room height";
    }
    match r.height.as_mut() {
        Some(h) => {
            if length(ui, "Height", h, 0.1, 100.0) {
                label = "Room height";
            }
        }
        None => {
            ui.label(format!(
                "Height {} (to its ceiling or the storey above)",
                crate::units::len(found)
            ));
        }
    }
    if r.is_drawn()
        && ui
            .button("Follow the walls instead")
            .on_hover_text("Forget the drawn outline: measure it to the walls round its point")
            .clicked()
    {
        r.boundary.clear();
        label = "Room follows walls";
    }
    label
}

/// A ceiling's properties: its height, thickness and grid. Returns the
/// edit's label, or "".
pub fn ceiling_fields(ui: &mut egui::Ui, model: &Model, c: &mut Ceiling) -> &'static str {
    let mut label = "";
    match c.room.and_then(|r| model.get(r)) {
        Some(r) if c.boundary.is_empty() => {
            ui.label(format!(
                "Over {} {}: it follows the room's walls.",
                number_of(model, r.id),
                r.name
            ));
        }
        _ => {
            ui.label(egui::RichText::new("Its outline is drawn.").weak());
        }
    }
    if length(ui, "Height", &mut c.height, 0.1, 50.0) {
        label = "Ceiling height";
    }
    if length(ui, "Thickness", &mut c.thickness, 0.001, 1.0) {
        label = "Ceiling thickness";
    }
    let mut grid = c.grid > 0.0;
    if ui
        .checkbox(&mut grid, "Suspended grid")
        .on_hover_text("Tiles drawn in ceiling plans")
        .changed()
    {
        c.grid = if grid { 0.6 } else { 0.0 };
        label = "Ceiling grid";
    }
    if c.grid > 0.0 && length(ui, "Tile", &mut c.grid, 0.1, 5.0) {
        label = "Ceiling grid";
    }
    label
}

/// What a room's tag shows, as tick boxes in order. Returns the edit's
/// label, or "".
pub fn tag_fields(ui: &mut egui::Ui, fields: &mut Vec<RoomField>) -> &'static str {
    let mut label = "";
    let mut shown: Vec<RoomField> = if fields.is_empty() {
        RoomField::DEFAULT.to_vec()
    } else {
        fields.clone()
    };
    ui.label("Shows");
    for f in RoomField::ALL {
        let mut on = shown.contains(&f);
        if ui.checkbox(&mut on, f.label()).changed() {
            if on {
                // keep the order fields are listed in
                shown.push(f);
                shown.sort_by_key(|x| RoomField::ALL.iter().position(|y| y == x));
            } else {
                shown.retain(|x| *x != f);
            }
            label = "Room tag fields";
        }
    }
    if !label.is_empty() {
        *fields = if shown == RoomField::DEFAULT {
            vec![]
        } else {
            shown
        };
    }
    label
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::starter::starter_house;

    fn named(m: &Model, name: &str) -> ElementId {
        m.iter().find(|e| e.name == name).unwrap().id
    }

    #[test]
    fn a_room_placed_inside_walls_gets_a_number_and_a_tag() {
        let m = starter_house();
        let study = named(&m, "Study");
        let level = match &m.get(study).unwrap().kind {
            ElementKind::Room(r) => r.level,
            _ => unreachable!(),
        };
        // take the study away, then place it again
        let tx = Transaction::new("No study").remove_cascade(&m, study);
        let mut doc = Document::from_model(m.clone());
        doc.commit(tx).unwrap();
        let mut sel = Selection::default();
        let msg = fill(&mut doc, Space::Room, level, DVec2::new(2.0, 3.5), &mut sel).unwrap();
        assert!(msg.starts_with("Room 0.10 added"), "{msg}");
        let id = sel.active.unwrap();
        assert!(room::area(doc.model(), id) > 15.0);
        let tags = doc
            .model()
            .iter()
            .filter(|e| matches!(&e.kind, ElementKind::Tag(t) if t.host == id))
            .count();
        assert!(tags >= 1);
        // clicking there again finds it
        let again = fill(&mut doc, Space::Room, level, DVec2::new(2.0, 3.5), &mut sel).unwrap();
        assert!(again.contains("already"), "{again}");
        // and a ceiling over it follows it
        fill(
            &mut doc,
            Space::Ceiling,
            level,
            DVec2::new(2.0, 3.5),
            &mut sel,
        )
        .unwrap();
        let c = sel.active.unwrap();
        assert!(
            matches!(&doc.model().get(c).unwrap().kind, ElementKind::Ceiling(c) if c.room == Some(id))
        );
    }

    #[test]
    fn a_separator_takes_two_clicks() {
        let m = starter_house();
        let level = named(&m, "Ground Floor");
        let mut doc = Document::from_model(m);
        let mut state = PlanState::default();
        let mut sel = Selection::default();
        separator_click(&mut doc, level, DVec2::new(8.0, 5.6), &mut state, &mut sel).unwrap();
        assert!(state.pending.is_some());
        separator_click(&mut doc, level, DVec2::new(8.0, 9.6), &mut state, &mut sel).unwrap();
        assert!(matches!(
            doc.model().get(sel.active.unwrap()).unwrap().kind,
            ElementKind::RoomSeparator(_)
        ));
    }
}
