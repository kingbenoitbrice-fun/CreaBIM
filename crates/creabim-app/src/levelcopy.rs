//! Copying and moving elements between levels (Revit's "Paste aligned to
//! selected levels"), and putting footings under walls and columns, from
//! the Edit and right-click menus. Each is one transaction from
//! [`creabim_core::levels`] or [`creabim_core::footing`], so it undoes as
//! one step.

use super::*;
use crate::ui_model::Item;
use creabim_core::levels;
use std::collections::HashSet;

/// What the window is asking for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LevelAsk {
    /// Copies of the selection on each level ticked.
    Copy,
    /// The selection moved to one level.
    Move,
}

/// The Copy to levels / Move to level window's own state.
#[derive(Default)]
pub(super) struct LevelCopyUi {
    pub open: Option<LevelAsk>,
    /// The levels ticked (copy) or picked (move).
    pub targets: HashSet<ElementId>,
}

impl CreaBimApp {
    /// What the selection holds that can go to another level.
    fn level_set(&self) -> Vec<ElementId> {
        levels::movable(self.doc.model(), &self.selection.all())
    }

    /// The level most of the selection stands on.
    fn selection_level(&self) -> Option<ElementId> {
        let model = self.doc.model();
        let mut counts: Vec<(ElementId, usize)> = vec![];
        for id in self.selection.all() {
            let Some(l) = model.get(id).and_then(|e| match &e.kind {
                ElementKind::Shaft(s) => Some(s.level),
                ElementKind::View(_) => None,
                k => levels::level_of(k),
            }) else {
                continue;
            };
            match counts.iter_mut().find(|c| c.0 == l) {
                Some(c) => c.1 += 1,
                None => counts.push((l, 1)),
            }
        }
        counts.into_iter().max_by_key(|c| c.1).map(|c| c.0)
    }

    /// Open the window for `ask`, or say why it can't be used.
    pub(super) fn open_level_copy(&mut self, ask: LevelAsk) {
        if self.level_set().is_empty() {
            self.status = "Select walls, slabs, columns or other elements on a level first.".into();
            return;
        }
        if levels::sorted(self.doc.model()).len() < 2 {
            self.status = "There is only one level. Add another first (Levels…).".into();
            return;
        }
        self.level_copy.open = Some(ask);
        self.level_copy.targets.clear();
        self.status = match ask {
            LevelAsk::Copy => "Tick the levels to copy the selection to.".into(),
            LevelAsk::Move => "Pick the level to move the selection to.".into(),
        };
    }

    /// Copies of the selection on each of `targets`, as one undo step;
    /// the copies are selected.
    pub(super) fn copy_to_levels(&mut self, targets: &[ElementId]) {
        let ids = self.selection.all();
        let (tx, copies) = levels::copy_to_levels(self.doc.model(), &ids, targets);
        if copies.is_empty() {
            self.status = "Nothing to copy: select elements on a level first.".into();
            return;
        }
        let names = self.level_names(targets);
        let r = self.doc.commit(tx);
        if r.is_ok() {
            self.selection.clear();
            self.selection.extend(copies.clone(), false);
            self.face = None;
        }
        self.report(
            r,
            format!(
                "Copied to {names}: {} elements, at the same place in plan. Ctrl+Z undoes it in one step.",
                copies.len()
            ),
        );
    }

    /// Move the selection to `to`, as one undo step.
    pub(super) fn move_to_level(&mut self, to: ElementId) {
        let ids = self.selection.all();
        let tx = levels::move_to_level(self.doc.model(), &ids, to);
        let n = tx.ops.len();
        if n == 0 {
            self.status = "Nothing moved: it is on that level already.".into();
            return;
        }
        let names = self.level_names(&[to]);
        let r = self.doc.commit(tx);
        self.report(
            r,
            format!("Moved {n} elements to {names}, at the same place in plan. Ctrl+Z to undo."),
        );
    }

    /// Paste what was copied onto level `to`, at the same place in plan.
    pub(super) fn paste_onto_level(&mut self, to: ElementId) {
        if self.clipboard.is_empty() {
            self.status = "Nothing copied yet (Ctrl+C copies the selection).".into();
            return;
        }
        let model = self.doc.model();
        let (copies, _) = creabim_core::transform::fresh_copies(model, &self.clipboard);
        let copies = levels::paste_onto(model, copies, to);
        let ids: Vec<ElementId> = copies.iter().map(|c| c.id).collect();
        let n = copies.len();
        let mut tx = Transaction::new(format!("Paste {n} elements onto a level"));
        for c in copies {
            tx = tx.insert(c);
        }
        let names = self.level_names(&[to]);
        let r = self.doc.commit(tx);
        if r.is_ok() {
            self.selection.clear();
            self.selection.extend(ids, false);
            self.face = None;
        }
        self.report(r, format!("Pasted {n} elements onto {names}."));
    }

    /// Footings under the selected walls and columns that have none: a
    /// strip under each wall, a pad under each column, of the Footing
    /// tool's types when it has them.
    pub(super) fn footings_under_selection(&mut self) {
        let ids = self.selection.all();
        let (tx, new) = footings_under(self.doc.model(), &self.plan, &ids);
        if new.is_empty() {
            let model = self.doc.model();
            let hosts = ids
                .iter()
                .filter(|id| !creabim_core::footing::footings_under(model, **id).is_empty())
                .count();
            self.status = if hosts > 0 {
                format!(
                    "{hosts} {} picked, but each already has a footing.",
                    if hosts == 1 {
                        "wall or column"
                    } else {
                        "walls and columns"
                    }
                )
            } else {
                "Select walls or columns without footings to put footings under them.".into()
            };
            return;
        }
        let r = self.doc.commit(tx);
        if r.is_ok() {
            self.selection.clear();
            self.selection.extend(new.clone(), false);
        }
        self.report(r, footings_placed(new.len()));
    }

    /// The selected walls and columns a footing can go under that have
    /// none yet.
    pub(super) fn footing_hosts(&self) -> Vec<ElementId> {
        let model = self.doc.model();
        self.selection
            .all()
            .into_iter()
            .filter(|id| footing_host(model, *id))
            .collect()
    }

    fn level_names(&self, ids: &[ElementId]) -> String {
        let model = self.doc.model();
        let names: Vec<String> = ids
            .iter()
            .filter_map(|i| model.get(*i).map(|e| e.name.clone()))
            .collect();
        names.join(", ")
    }

    /// The menu items for the selection: footings under it, copy to
    /// levels, move to a level.
    pub(super) fn level_menu_items(&self) -> Vec<Item> {
        let model = self.doc.model();
        let mut items = vec![];
        let hosts = self.selection.all().into_iter().any(|id| {
            matches!(model.get(id).map(|e| &e.kind),
                Some(ElementKind::Wall(w)) if w.curtain.is_none())
                || matches!(model.get(id).map(|e| &e.kind), Some(ElementKind::Column(_)))
        });
        if hosts {
            items.push(
                Item::new("footing.under", "Footings under selection")
                    .icon("footing")
                    .hint("A strip footing under each wall, a pad footing under each column"),
            );
        }
        let some = !self.level_set().is_empty();
        items.push(
            Item::new("levels.copy", "Copy to levels…")
                .icon("copy-levels")
                .enabled(some)
                .hint("Copies on the levels you tick, at the same place in plan"),
        );
        items.push(
            Item::new("levels.move", "Move to level…")
                .icon("move-level")
                .enabled(some)
                .hint("Onto another level, at the same place in plan"),
        );
        items
    }

    /// The Paste aligned to level submenu: one item a level.
    pub(super) fn paste_level_items(&self) -> Vec<Item> {
        levels::sorted(self.doc.model())
            .into_iter()
            .rev()
            .map(|(id, name, _)| {
                Item::new(format!("paste.level:{}", id.0), name).enabled(!self.clipboard.is_empty())
            })
            .collect()
    }

    /// Run an item of these menus; false when `head` is not one.
    pub(super) fn level_action(&mut self, head: &str, arg: &str) -> bool {
        match head {
            "footing.under" => self.footings_under_selection(),
            "levels.copy" => self.open_level_copy(LevelAsk::Copy),
            "levels.move" => self.open_level_copy(LevelAsk::Move),
            "paste.level" => {
                let to = self
                    .doc
                    .model()
                    .levels()
                    .find(|(e, _)| e.id.0.to_string() == arg)
                    .map(|(e, _)| e.id);
                if let Some(to) = to {
                    self.paste_onto_level(to);
                }
            }
            _ => return false,
        }
        true
    }

    /// The Copy to levels / Move to level window, when open.
    pub(super) fn level_copy_window(&mut self, ctx: &egui::Context) {
        let Some(ask) = self.level_copy.open else {
            return;
        };
        let mut open = true;
        let mut done: Option<Vec<ElementId>> = None;
        let area = self.mat.area.unwrap_or_else(|| ctx.content_rect());
        let title = match ask {
            LevelAsk::Copy => "Copy to levels",
            LevelAsk::Move => "Move to level",
        };
        let list = levels::sorted(self.doc.model());
        let from = self.selection_level();
        let n = self.level_set().len();
        egui::Window::new(title)
            .open(&mut open)
            .default_pos(area.min + Vec2::new(60.0, 80.0))
            .constrain_to(area)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(match ask {
                        LevelAsk::Copy => format!(
                            "{n} elements, with the doors and windows in their walls. Each copy sits on its new level as the original sits on its own, at the same place in plan; tops tied to a level move up or down as many storeys."
                        ),
                        LevelAsk::Move => format!(
                            "{n} elements, with their doors, windows and footings, onto another level at the same place in plan."
                        ),
                    })
                    .weak(),
                );
                ui.separator();
                for (id, name, l) in list.iter().rev() {
                    let here = from == Some(*id);
                    let text = format!(
                        "{name}  ({}){}",
                        crate::units::elevation(l.elevation),
                        if here { ", where it is now" } else { "" }
                    );
                    match ask {
                        LevelAsk::Copy => {
                            let mut on = self.level_copy.targets.contains(id);
                            if ui.add_enabled(!here, egui::Checkbox::new(&mut on, text)).changed() {
                                if on {
                                    self.level_copy.targets.insert(*id);
                                } else {
                                    self.level_copy.targets.remove(id);
                                }
                            }
                        }
                        LevelAsk::Move => {
                            if ui
                                .add_enabled(!here, egui::Button::new(text))
                                .clicked()
                            {
                                done = Some(vec![*id]);
                            }
                        }
                    }
                }
                if ask == LevelAsk::Copy {
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.small_button("All above").clicked() {
                            let z = from.and_then(|f| list.iter().find(|l| l.0 == f)).map_or(f64::MIN, |l| l.2.elevation);
                            self.level_copy.targets =
                                list.iter().filter(|l| l.2.elevation > z + 1e-6).map(|l| l.0).collect();
                        }
                        if ui.small_button("All below").clicked() {
                            let z = from.and_then(|f| list.iter().find(|l| l.0 == f)).map_or(f64::MAX, |l| l.2.elevation);
                            self.level_copy.targets =
                                list.iter().filter(|l| l.2.elevation < z - 1e-6).map(|l| l.0).collect();
                        }
                        if ui.small_button("None").clicked() {
                            self.level_copy.targets.clear();
                        }
                    });
                    let k = self.level_copy.targets.len();
                    let go = ui.add_enabled(
                        k > 0,
                        egui::Button::new(match k {
                            0 => "Copy".to_string(),
                            1 => "Copy to 1 level".to_string(),
                            k => format!("Copy to {k} levels"),
                        }),
                    );
                    if go.clicked() {
                        // lowest first, as the levels are listed
                        let mut t: Vec<ElementId> = list
                            .iter()
                            .map(|l| l.0)
                            .filter(|i| self.level_copy.targets.contains(i))
                            .collect();
                        t.dedup();
                        done = Some(t);
                    }
                }
            });
        if let Some(targets) = done {
            match ask {
                LevelAsk::Copy => self.copy_to_levels(&targets),
                LevelAsk::Move => {
                    if let Some(to) = targets.first() {
                        self.move_to_level(*to);
                    }
                }
            }
            open = false;
        }
        if !open {
            self.level_copy.open = None;
        }
    }
}

/// Can a footing go under `id`: a wall that isn't a curtain wall, or a
/// column, with no footing under it yet?
pub(crate) fn footing_host(model: &creabim_core::Model, id: ElementId) -> bool {
    let fits = match model.get(id).map(|e| &e.kind) {
        Some(ElementKind::Wall(w)) => w.curtain.is_none(),
        Some(ElementKind::Column(_)) => true,
        _ => false,
    };
    fits && creabim_core::footing::footings_under(model, id).is_empty()
}

/// Footings under those of `ids` that can take one, of the Footing tool's
/// type when it has one, else as wide as its bar says: one undo step.
/// Returns the edit and the new footings.
pub(crate) fn footings_under(
    model: &creabim_core::Model,
    state: &crate::planview::PlanState,
    ids: &[ElementId],
) -> (creabim_core::Transaction, Vec<ElementId>) {
    let (mut tx, new) = creabim_core::footing::under(model, ids, &|e| {
        if let ElementKind::Footing(f) = &e.kind {
            // the tool's type for this kind, if there is one
            let t = creabim_core::types::types_for(model, &e.kind)
                .into_iter()
                .map(|(id, _, _)| id)
                .find(|id| state.types.get(&creabim_core::TypeCategory::Footing) == Some(id));
            let kind = f.kind;
            if let Some(t) = t {
                creabim_core::types::stamp(model, e, t);
            } else if kind == creabim_core::FootingKind::Strip {
                // as wide as the bar says, but never narrower than the wall
                if let ElementKind::Footing(f) = &mut e.kind {
                    f.width = f.width.max(state.members.footing_width);
                }
            }
        }
    });
    tx.label = "Footings under selection".into();
    (tx, new)
}

/// What the status bar says once `n` footings are placed in one go.
pub(crate) fn footings_placed(n: usize) -> String {
    format!(
        "{n} footings placed: they follow their walls and columns, and strips meet where the walls join. Ctrl+Z takes them all back. Set their depth in Properties."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> CreaBimApp {
        let mut app = CreaBimApp::blank(None);
        app.load_model(creabim_core::sample::sample_house());
        app
    }

    fn level(app: &CreaBimApp, name: &str) -> ElementId {
        app.doc
            .model()
            .levels()
            .find(|(e, _)| e.name == name)
            .unwrap()
            .0
            .id
    }

    fn walls_on(app: &CreaBimApp, l: ElementId) -> Vec<ElementId> {
        app.doc
            .model()
            .iter()
            .filter(|e| matches!(&e.kind, ElementKind::Wall(w) if w.level == l))
            .map(|e| e.id)
            .collect()
    }

    #[test]
    fn copy_to_levels_from_the_menu_copies_and_undoes_in_one_step() {
        let mut app = app();
        let ctx = egui::Context::default();
        app.run_action(&ctx, "level.add");
        let (g, f) = (level(&app, "Ground Floor"), level(&app, "First Floor"));
        let l2 = level(&app, "Level 2");
        let walls = walls_on(&app, f);
        app.selection.clear();
        app.selection.extend(walls.clone(), false);
        let items = app.menu_items("context");
        assert!(items
            .iter()
            .any(|i| matches!(i, Item::Action { id, .. } if id == "levels.copy")));
        assert!(items
            .iter()
            .any(|i| matches!(i, Item::Action { id, .. } if id == "footing.under")));
        app.run_action(&ctx, "levels.copy");
        assert_eq!(app.level_copy.open, Some(LevelAsk::Copy));
        let before = app.doc.model().len();
        let on_ground = walls_on(&app, g).len();
        app.copy_to_levels(&[g, l2]);
        assert_eq!(walls_on(&app, l2).len(), walls.len());
        assert_eq!(walls_on(&app, g).len(), on_ground + walls.len());
        assert!(app.status.contains("Copied"), "{}", app.status);
        app.run_action(&ctx, "cmd:Undo");
        assert_eq!(app.doc.model().len(), before);
        // paste aligned to a level puts what was copied there
        app.selection.clear();
        app.selection.extend(walls.clone(), false);
        app.run_action(&ctx, "cmd:Copy");
        app.run_action(&ctx, &format!("paste.level:{}", l2.0));
        assert_eq!(walls_on(&app, l2).len(), walls.len());
        // and move to level takes them off their own
        app.selection.clear();
        app.selection.extend(walls.clone(), false);
        app.move_to_level(g);
        assert!(walls_on(&app, f).is_empty());
    }

    #[test]
    fn a_whole_typed_floor_copies_to_new_levels() {
        let mut app = CreaBimApp::blank(None);
        let ctx = egui::Context::default();
        app.run_action(&ctx, "new.typed");
        for _ in 0..3 {
            app.run_action(&ctx, "level.add");
        }
        let f = level(&app, "First Floor");
        let model = app.doc.model();
        // all a box round the first floor plan can take
        let mut pick: Vec<ElementId> = vec![];
        for it in creabim_geom::plan_view(model, f, 1.2).items {
            if crate::planview::box_selectable(&app.doc, it.id()) && !pick.contains(&it.id()) {
                pick.push(it.id());
            }
        }
        for e in model.iter() {
            if matches!(e.kind, ElementKind::Tag(_)) {
                pick.push(e.id);
            }
        }
        let targets: Vec<ElementId> = ["Level 2", "Level 3", "Level 4"]
            .iter()
            .map(|n| level(&app, n))
            .collect();
        app.selection.clear();
        app.selection.extend(pick, false);
        app.copy_to_levels(&targets);
        assert!(app.status.starts_with("Copied"), "{}", app.status);
        // walls, windows, slab and shaft on each level
        for l in &targets {
            assert_eq!(walls_on(&app, *l).len(), 4);
        }
    }

    #[test]
    fn footings_go_under_the_selected_walls_and_columns() {
        let mut app = app();
        let ctx = egui::Context::default();
        let g = level(&app, "Ground Floor");
        let walls = walls_on(&app, g);
        app.selection.clear();
        app.selection.extend(walls.clone(), false);
        app.run_action(&ctx, "footing.under");
        let footings = app
            .doc
            .model()
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Footing(_)))
            .count();
        assert_eq!(footings, walls.len());
        // they show dashed in the ground floor plan, below the cut
        let plan = creabim_geom::plan_view(app.doc.model(), g, 1.2);
        assert!(plan.items.iter().any(|it| matches!(it,
            PlanItem::Line { pen: Pen::Hidden, id, .. }
                if matches!(app.doc.model().get(*id).map(|e| &e.kind), Some(ElementKind::Footing(_))))));
        // a second time adds none
        app.selection.clear();
        app.selection.extend(walls, false);
        app.run_action(&ctx, "footing.under");
        let again = app
            .doc
            .model()
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Footing(_)))
            .count();
        assert_eq!(again, footings);
    }
}
