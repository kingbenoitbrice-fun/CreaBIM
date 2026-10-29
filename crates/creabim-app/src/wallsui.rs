//! How walls and their doors and windows behave, in the app: a wall's
//! location line, base and top constraints, end joins and attached top in
//! Properties, and the flip, join and attach edits picked from a button,
//! a key or a right-click menu. Every change is one transaction, so it
//! undoes like any edit.

use super::*;
use creabim_core::{walls, LocationLine, Opening, OpeningKind, Wall, WallTop};

/// A length field in a row, as the rest of Properties draws them.
fn num(ui: &mut egui::Ui, text: &str, v: &mut f64, lo: f64, hi: f64) -> bool {
    ui.horizontal(|ui| {
        ui.label(text);
        ui.add(crate::units::drag(v).range(lo..=hi)).changed()
    })
    .inner
}

/// The model's levels, lowest first.
fn levels(model: &Model) -> Vec<(ElementId, String, f64)> {
    let mut v: Vec<_> = model
        .levels()
        .map(|(e, l)| (e.id, e.name.clone(), l.elevation))
        .collect();
    v.sort_by(|a, b| a.2.total_cmp(&b.2));
    v
}

fn name_of(model: &Model, id: ElementId) -> String {
    model
        .get(id)
        .map_or("(missing)".to_string(), |e| e.name.clone())
}

/// Which side a wall's exterior is on, in words.
pub(super) fn exterior_side(w: &Wall) -> &'static str {
    if w.flip {
        "Exterior on the left (looking from start to end)"
    } else {
        "Exterior on the right (looking from start to end)"
    }
}

/// A wall's location line, flip, constraints, joins and attached top.
/// Returns the edit's name when something changed.
pub(super) fn wall_fields(
    ui: &mut egui::Ui,
    model: &Model,
    id: ElementId,
    w: &mut Wall,
) -> &'static str {
    let mut label = "";
    ui.horizontal(|ui| {
        egui::ComboBox::from_label("Location line")
            .selected_text(w.location.label())
            .width(130.0)
            .show_ui(ui, |ui| {
                for l in LocationLine::ALL {
                    if ui.selectable_value(&mut w.location, l, l.label()).changed() {
                        label = "Wall location line"
                    }
                }
            })
            .response
            .on_hover_text(
                "The line the wall is drawn and dimensioned on. It stays put when the wall gets thicker, changes its layers or is flipped.",
            );
    });
    ui.horizontal(|ui| {
        if ui
            .button("Flip")
            .on_hover_text("Swap the wall's exterior and interior faces, about its location line (Space). Its doors and windows turn with it.")
            .clicked()
        {
            walls::flip_wall(w);
            label = "Flip wall"
        }
        ui.label(egui::RichText::new(exterior_side(w)).weak());
    });

    ui.add_space(4.0);
    let lv = levels(model);
    ui.horizontal(|ui| {
        egui::ComboBox::from_label("Base level")
            .selected_text(name_of(model, w.level))
            .width(130.0)
            .show_ui(ui, |ui| {
                for (l, n, _) in &lv {
                    if ui.selectable_value(&mut w.level, *l, n).changed() {
                        label = "Wall base level"
                    }
                }
            });
    });
    if num(ui, "Base offset", &mut w.base_offset, -10.0, 50.0) {
        label = "Wall base offset"
    }
    let shown = match w.top {
        WallTop::Unconnected => "Unconnected".to_string(),
        WallTop::UpToLevel { level, .. } => format!("Up to {}", name_of(model, level)),
    };
    ui.horizontal(|ui| {
        egui::ComboBox::from_label("Top")
            .selected_text(shown)
            .width(130.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(w.top.is_unconnected(), "Unconnected")
                    .clicked()
                    && !w.top.is_unconnected()
                {
                    w.top = WallTop::Unconnected;
                    label = "Wall top constraint"
                }
                for (l, n, _) in &lv {
                    let on = matches!(w.top, WallTop::UpToLevel { level, .. } if level == *l);
                    if ui.selectable_label(on, format!("Up to {n}")).clicked() && !on {
                        w.top = WallTop::UpToLevel {
                            level: *l,
                            offset: 0.0,
                        };
                        label = "Wall top constraint"
                    }
                }
            })
            .response
            .on_hover_text(
                "Unconnected: the wall is as tall as its height. Up to a level: its top follows that level when it moves.",
            );
    });
    match &mut w.top {
        WallTop::UpToLevel { offset, .. } => {
            if num(ui, "Top offset", offset, -50.0, 50.0) {
                label = "Wall top offset"
            }
            let h = walls::constrained_height(model, w).unwrap_or(w.height);
            ui.label(
                egui::RichText::new(format!(
                    "Height {}, from its top constraint",
                    crate::units::len(h.max(walls::MIN_HEIGHT))
                ))
                .weak(),
            );
        }
        WallTop::Unconnected => {
            if num(ui, "Height", &mut w.height, 0.1, 50.0) {
                label = "Wall height"
            }
        }
    }

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("Joins at");
        for (i, end) in ["start", "end"].into_iter().enumerate() {
            let mut on = !w.disallow_join[i];
            if ui
                .checkbox(&mut on, end)
                .on_hover_text("Off: this end stays square and free, and is not pulled along when the wall it touches moves. Right-click the end in plan to switch it too.")
                .changed()
            {
                w.disallow_join[i] = !on;
                label = if on {
                    "Allow wall join"
                } else {
                    "Disallow wall join"
                }
            }
        }
    });

    if w.attach_top.is_empty() {
        let found = walls::attach_candidates(model, id);
        if ui
            .add_enabled(
                !found.is_empty(),
                egui::Button::new("Attach top to roof/slab"),
            )
            .on_hover_text(
                "The top follows the underside of the roof or slab above, cut to its slope.",
            )
            .on_disabled_hover_text("There is no roof or slab above this wall.")
            .clicked()
        {
            w.attach_top = found;
            label = "Attach wall top"
        }
    } else {
        let names: Vec<String> = w.attach_top.iter().map(|a| name_of(model, *a)).collect();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("Top attached to {}", names.join(", "))).weak());
            if ui
                .button("Detach")
                .on_hover_text("The top goes back to the wall's own height.")
                .clicked()
            {
                w.attach_top.clear();
                label = "Detach wall top"
            }
        });
    }

    ui.add_space(4.0);
    ui.label(format!("Length {}", crate::units::len(w.length())));
    let (mut a, mut b) = walls::location_line(model, w);
    let moved_a = num(ui, "Start X", &mut a.x, -1e4, 1e4) | num(ui, "Start Y", &mut a.y, -1e4, 1e4);
    let moved_b = num(ui, "End X", &mut b.x, -1e4, 1e4) | num(ui, "End Y", &mut b.y, -1e4, 1e4);
    if moved_a || moved_b {
        walls::set_location_line(model, w, a, b);
        label = if moved_a {
            "Move wall start"
        } else {
            "Move wall end"
        }
    }
    label
}

/// A door's hand and swing, or a window's facing. Returns the edit's name.
pub(super) fn opening_fields(ui: &mut egui::Ui, model: &Model, o: &mut Opening) -> &'static str {
    let mut label = "";
    let host = match model.get(o.host).map(|e| &e.kind) {
        Some(ElementKind::Wall(w)) => w.clone(),
        _ => return label,
    };
    match o.kind {
        OpeningKind::Door => {
            ui.label(format!(
                "Hung {}",
                creabim_geom::opening::door_hand(o, &host).to_lowercase()
            ))
            .on_hover_text("The hinge side seen from the side it opens to, and whether it opens into the building or out of it.");
            ui.horizontal(|ui| {
                if ui
                    .button("Flip hand")
                    .on_hover_text("Hinge it on the other jamb (Shift+Space).")
                    .clicked()
                {
                    walls::flip_hand(o);
                    label = "Flip door hand"
                }
                if ui
                    .button("Flip swing")
                    .on_hover_text("Open it to the other side of the wall (Space).")
                    .clicked()
                {
                    walls::flip_facing(o);
                    label = "Flip door swing"
                }
            });
        }
        OpeningKind::Window => {
            // the exterior is on the wall's left when it is flipped
            let out = o.faces_left(&host) == host.flip;
            ui.label(if out {
                "Its outside faces the wall's exterior"
            } else {
                "Turned round: its outside faces the wall's interior"
            });
            if ui
                .button("Flip inside/outside")
                .on_hover_text("Turn it round in its wall: its sill board and frame move to the other face (Space).")
                .clicked()
            {
                walls::flip_facing(o);
                label = "Flip window"
            }
        }
        OpeningKind::Hole => {
            ui.label("A bare opening: a hole through the wall with no door or window in it");
        }
    }
    label
}

impl CreaBimApp {
    /// Flip the selected walls, doors and windows: a wall's faces, a door's
    /// swing (or, with `hand`, its hinge side), a window's facing.
    pub(super) fn flip_selected(&mut self, hand: bool) {
        let model = self.doc.model();
        let mut tx = Transaction::new(if hand { "Flip door hand" } else { "Flip" });
        let mut n = 0;
        for id in self.selection.all() {
            let Some(before) = model.get(id) else {
                continue;
            };
            // a section or elevation looks the other way, its notes mirrored
            if let (false, Some(flip)) = (hand, creabim_core::transform::flip_view(model, id)) {
                tx.ops.extend(flip.ops);
                n += 1;
                continue;
            }
            let mut after = before.clone();
            let changed = match &mut after.kind {
                ElementKind::Wall(w) if !hand => {
                    walls::flip_wall(w);
                    true
                }
                ElementKind::Opening(o) if hand => {
                    let door = o.kind == OpeningKind::Door;
                    if door {
                        walls::flip_hand(o);
                    }
                    door
                }
                ElementKind::Opening(o) => {
                    walls::flip_facing(o);
                    true
                }
                _ => false,
            };
            if changed {
                tx = tx.replace(before.clone(), after);
                n += 1;
            }
        }
        if n == 0 {
            self.status = if hand {
                "Select a door to flip its hand.".into()
            } else {
                "Select a wall, door, window, section or elevation to flip it.".into()
            };
            return;
        }
        let r = self.doc.commit(tx);
        self.report(
            r,
            format!("Flipped {n} element{}.", if n == 1 { "" } else { "s" }),
        );
    }

    /// Allow or disallow the join at a wall's end (0 start, 1 end).
    pub(super) fn toggle_join(&mut self, id: ElementId, end: usize) {
        let Some(ElementKind::Wall(w)) = self.doc.model().get(id).map(|e| &e.kind) else {
            return;
        };
        let allow = w.disallow_join[end];
        let label = if allow {
            "Allow wall join"
        } else {
            "Disallow wall join"
        };
        let r = self.doc.edit(id, label, |e| {
            if let ElementKind::Wall(w) = &mut e.kind {
                w.disallow_join[end] = !allow;
            }
        });
        let which = if end == 0 { "start" } else { "end" };
        self.report(
            r,
            if allow {
                format!("The wall's {which} joins again.")
            } else {
                format!("The wall's {which} is free: it stays square and is not pulled along.")
            },
        );
    }

    /// Attach the selected walls' tops to the roof or slab above them, or
    /// detach them.
    pub(super) fn attach_selected(&mut self, attach: bool) {
        let model = self.doc.model();
        let mut tx = Transaction::new(if attach {
            "Attach wall top"
        } else {
            "Detach wall top"
        });
        let mut n = 0;
        for id in self.selection.all() {
            let Some(before) = model.get(id) else {
                continue;
            };
            let ElementKind::Wall(w) = &before.kind else {
                continue;
            };
            let to = if attach {
                walls::attach_candidates(model, id)
            } else {
                vec![]
            };
            if to == w.attach_top || (attach && to.is_empty()) {
                continue;
            }
            let mut after = before.clone();
            if let ElementKind::Wall(w) = &mut after.kind {
                w.attach_top = to;
            }
            tx = tx.replace(before.clone(), after);
            n += 1;
        }
        if n == 0 {
            self.status = if attach {
                "Select walls with a roof or slab above them.".into()
            } else {
                "Select walls whose tops are attached.".into()
            };
            return;
        }
        let r = self.doc.commit(tx);
        let what = if attach {
            "attached to the roof or slab above"
        } else {
            "detached"
        };
        self.report(
            r,
            format!("{n} wall top{} {what}.", if n == 1 { "" } else { "s" }),
        );
    }
}

/// The wall end nearer `p`: 0 its start, 1 its end.
pub(super) fn nearest_end(w: &Wall, p: DVec2) -> usize {
    usize::from(p.distance(w.end) < p.distance(w.start))
}

impl CreaBimApp {
    /// The right-click menu's wall, door and window items, for the
    /// selection and where the menu was opened.
    pub(super) fn wall_menu_items(&self) -> Vec<crate::ui_model::Item> {
        use crate::ui_model::Item;
        let model = self.doc.model();
        let mut items = vec![];
        let sel: Vec<&Element> = self
            .selection
            .all()
            .into_iter()
            .filter_map(|id| model.get(id))
            .collect();
        let walls: Vec<(ElementId, &Wall)> = sel
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::Wall(w) => Some((e.id, w)),
                _ => None,
            })
            .collect();
        let doors = sel
            .iter()
            .any(|e| matches!(&e.kind, ElementKind::Opening(o) if o.kind == OpeningKind::Door));
        let openings = sel
            .iter()
            .any(|e| matches!(&e.kind, ElementKind::Opening(_)));
        if !walls.is_empty() || openings {
            let what = match (walls.is_empty(), doors) {
                (false, _) => "Flip wall",
                (true, true) => "Flip door swing",
                (true, false) => "Flip window inside/outside",
            };
            items.push(Item::new("wall.flip", what).key("Space").icon("brick-wall"));
        }
        let views = sel.iter().any(|e| {
            matches!(&e.kind, ElementKind::View(v) if matches!(v.kind, ViewKind::Section { .. }))
        });
        if views && walls.is_empty() && !openings {
            items.push(
                Item::new("wall.flip", "Flip view direction")
                    .key("Space")
                    .icon("flip-horizontal-2")
                    .hint("Look the other way along the same line"),
            );
        }
        if doors {
            items.push(
                Item::new("wall.hand", "Flip door hand")
                    .key("Shift+Space")
                    .icon("door-open"),
            );
        }
        // the join at the end nearest the click, for one wall
        let at = if self.shell.context_3d {
            self.menu_at_3d
        } else {
            self.plan.menu_at
        };
        if let ([(id, w)], Some(at)) = (walls.as_slice(), at) {
            let end = nearest_end(w, at.truncate());
            let which = if end == 0 { "start" } else { "end" };
            let text = if w.disallow_join[end] {
                format!("Allow join at its {which}")
            } else {
                format!("Disallow join at its {which}")
            };
            items.push(
                Item::new(format!("wall.join:{}:{end}", id.0), text)
                    .icon("link")
                    .hint("A disallowed end stays square and free: it doesn't join, and isn't pulled along"),
            );
            items.push(
                Item::new(format!("wall.split:{}", id.0), "Split wall here")
                    .icon("scissors")
                    .hint("Two walls, meeting where you right-clicked; doors and windows stay where they are"),
            );
        }
        if !walls.is_empty() {
            items.push(
                Item::new("wall.attach", "Attach top to roof/slab")
                    .hint("The top follows the underside of the roof or slab above"),
            );
            if walls.iter().any(|(_, w)| !w.attach_top.is_empty()) {
                items.push(Item::new("wall.detach", "Detach top"));
            }
        }
        if !items.is_empty() {
            items.push(Item::Separator);
        }
        items
    }

    /// Run a wall item picked from a menu or the tool strip; false when
    /// `head` is not one.
    pub(super) fn wall_action(&mut self, head: &str, arg: &str) -> bool {
        match head {
            "wall.flip" => self.flip_selected(false),
            "wall.hand" => self.flip_selected(true),
            "wall.attach" => self.attach_selected(true),
            "wall.split" => {
                let at = if self.shell.context_3d {
                    self.menu_at_3d
                } else {
                    self.plan.menu_at
                };
                let id = self
                    .doc
                    .model()
                    .iter()
                    .find(|e| e.id.0.to_string() == arg)
                    .map(|e| e.id);
                let r = match (id, at) {
                    (Some(id), Some(at)) => {
                        creabim_core::walls::split_wall(self.doc.model(), id, at.truncate())
                            .and_then(|tx| self.doc.commit(tx).map_err(|e| e.to_string()))
                    }
                    _ => Err("right-click the wall where it should split".into()),
                };
                self.report(r, "Split the wall in two. Ctrl+Z to undo.");
            }
            "wall.detach" => self.attach_selected(false),
            "wall.join" => {
                let (id, end) = arg.rsplit_once(':').unwrap_or((arg, "0"));
                let id = self
                    .doc
                    .model()
                    .iter()
                    .find(|e| e.id.0.to_string() == id)
                    .map(|e| e.id);
                if let Some(id) = id {
                    self.toggle_join(id, usize::from(end == "1"));
                }
            }
            "wall.loc" => {
                if let Some(l) = arg
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| LocationLine::ALL.get(i))
                {
                    if self.plan.wall_location != *l {
                        // a location line of its own: new walls have no type
                        self.plan.types.remove(&creabim_core::TypeCategory::Wall);
                    }
                    self.plan.wall_location = *l;
                    self.status =
                        format!("New walls are drawn on their {}.", l.label().to_lowercase());
                }
            }
            "wall.flipnew" => {
                self.plan.wall_flip = !self.plan.wall_flip;
                self.status = format!(
                    "New walls have their exterior on the {} (Space flips it).",
                    if self.plan.wall_flip { "left" } else { "right" }
                );
            }
            _ => return false,
        }
        true
    }

    /// The Wall tool's location line choices, for its option strip.
    pub(super) fn wall_location_items(&self) -> Vec<crate::ui_model::Item> {
        LocationLine::ALL
            .iter()
            .enumerate()
            .map(|(i, l)| {
                crate::ui_model::Item::new(format!("wall.loc:{i}"), l.label())
                    .checked(self.plan.wall_location == *l)
            })
            .collect()
    }
}
