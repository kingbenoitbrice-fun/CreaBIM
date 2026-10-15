//! The Levels manager (add, duplicate, rename, move and delete levels,
//! and make their floor plans) and the Project units dialog. Every change
//! is one transaction from [`creabim_core::levels`], so it undoes like any
//! edit.

use super::*;
use creabim_core::levels::{self, OnDelete};
use creabim_core::units::{set_units, LengthUnit, Units};
use creabim_core::Underlay;

/// The two windows' own state, for the session.
pub(super) struct LevelsUi {
    pub open: bool,
    /// New levels come with a floor plan view.
    pub with_plan: bool,
    /// The elevation typed for "Add at".
    at: f64,
    /// Names being typed, committed when the field loses focus.
    names: HashMap<ElementId, String>,
    /// A level asked to be deleted: where its elements go (`None`: they
    /// are deleted with it).
    delete: Option<(ElementId, Option<ElementId>)>,
    pub units_open: bool,
}

impl Default for LevelsUi {
    fn default() -> Self {
        Self {
            open: false,
            with_plan: true,
            at: 0.0,
            names: HashMap::new(),
            delete: None,
            units_open: false,
        }
    }
}

/// What a click in the manager asks for, done once the window is drawn.
enum Ask {
    AddAbove,
    AddAt(f64),
    Duplicate(ElementId, bool),
    Rename(ElementId, String),
    Set(ElementId, f64, f64),
    MakePlan(ElementId),
    OpenPlan(ElementId),
    Delete(ElementId),
    ConfirmDelete(ElementId, Option<ElementId>),
    CancelDelete,
}

impl CreaBimApp {
    pub(super) fn open_levels(&mut self) {
        self.levels.open = true;
        let (z, _) = levels::above_top(self.doc.model());
        self.levels.at = z;
    }

    /// Open the manager asking what happens to what is on `level` if it
    /// is deleted.
    pub(super) fn ask_delete_level(&mut self, level: ElementId) {
        self.open_levels();
        let list = levels::sorted(self.doc.model());
        if list.len() < 2 {
            self.status = "A project keeps at least one level.".into();
            return;
        }
        self.levels.delete = Some((level, nearest(&list, level)));
        self.status = "Choose what happens to what is on the level, in the Levels window.".into();
    }

    /// Add a level one storey above the top one (with its plan when that
    /// is on), select it and say so.
    pub(super) fn add_level_above(&mut self) {
        let (z, h) = levels::above_top(self.doc.model());
        self.add_level_at(z, h);
    }

    fn add_level_at(&mut self, elevation: f64, height: f64) {
        let name = levels::next_name(self.doc.model());
        let (tx, id, plan) = levels::add(
            self.doc.model(),
            &name,
            elevation,
            height,
            self.levels.with_plan,
        );
        let r = self.doc.commit(tx);
        self.selection.only(id);
        let msg = match plan {
            Some(_) => format!(
                "Added {name} at {} with its floor plan.",
                crate::units::elevation(elevation)
            ),
            None => format!("Added {name} at {}.", crate::units::elevation(elevation)),
        };
        self.report(r, msg);
    }

    /// The plan view the 2D pane shows, with its level.
    pub(super) fn active_plan(&self) -> Option<(ElementId, ElementId)> {
        let id = self.active_view?;
        match self.view(id)?.kind {
            ViewKind::Plan { level, .. } => Some((id, level)),
            _ => None,
        }
    }

    /// A level's name field in Properties: committed when it loses focus,
    /// so the plans named after the level follow in the same undo step.
    pub(super) fn level_name_field(&mut self, ui: &mut egui::Ui, id: ElementId, name: &str) {
        let draft = self
            .levels
            .names
            .entry(id)
            .or_insert_with(|| name.to_string());
        let r = ui.add(egui::TextEdit::singleline(draft).id_salt(("level-name", id)));
        if r.lost_focus() && draft.trim() != name && !draft.trim().is_empty() {
            let new = draft.trim().to_string();
            self.run_level_ask(Ask::Rename(id, new));
        } else if !r.has_focus() && *draft != *name {
            *draft = name.to_string();
        }
    }

    /// The 2D pane shows a section or elevation, where level lines are drawn.
    pub(super) fn shows_section(&self) -> bool {
        self.active_view
            .and_then(|id| self.view(id))
            .is_some_and(|v| matches!(v.kind, ViewKind::Section { .. }))
    }

    /// The Underlay menu for the plan shown.
    pub(super) fn underlay_items(&self) -> Vec<crate::ui_model::Item> {
        use crate::ui_model::Item;
        let Some((view, level)) = self.active_plan() else {
            return vec![Item::Header("Open a floor plan first".into())];
        };
        let current = self.view(view).map(|v| v.underlay).unwrap_or_default();
        let mut items = vec![];
        for (i, (u, label)) in self.underlay_choices(level).into_iter().enumerate() {
            if i == 3 {
                items.push(Item::Separator);
                items.push(Item::Header("A level".into()));
            }
            let id = match u {
                Underlay::None => "underlay:none".to_string(),
                Underlay::Below => "underlay:below".to_string(),
                Underlay::Above => "underlay:above".to_string(),
                Underlay::Level(l) => format!("underlay:{}", l.0),
            };
            items.push(Item::new(id, label).checked(u == current));
        }
        items
    }

    /// What a plan of `level` can show under it, with how each reads.
    pub(super) fn underlay_choices(&self, level: ElementId) -> Vec<(Underlay, String)> {
        let model = self.doc.model();
        let name = |u: Underlay| {
            u.level(model, level)
                .and_then(|l| model.get(l))
                .map_or("none here".to_string(), |e| e.name.clone())
        };
        let mut out = vec![
            (Underlay::None, "None".to_string()),
            (
                Underlay::Below,
                format!("Level below ({})", name(Underlay::Below)),
            ),
            (
                Underlay::Above,
                format!("Level above ({})", name(Underlay::Above)),
            ),
        ];
        for (id, n, _) in levels::sorted(model).into_iter().rev() {
            if id != level {
                out.push((Underlay::Level(id), n));
            }
        }
        out
    }

    /// Set the underlay of a plan view.
    pub(super) fn set_underlay(&mut self, view: ElementId, u: Underlay) {
        let r = self.doc.edit(view, "View underlay", |e| {
            if let ElementKind::View(v) = &mut e.kind {
                v.underlay = u;
            }
        });
        let what = match u {
            Underlay::None => "Underlay off.".to_string(),
            u => {
                let level = match self.view(view).map(|v| &v.kind) {
                    Some(ViewKind::Plan { level, .. }) => u.level(self.doc.model(), *level),
                    _ => None,
                };
                match level.and_then(|l| self.doc.model().get(l)) {
                    Some(e) => format!(
                        "Underlay: {} shows faintly under this plan. Its lines snap but can't be picked.",
                        e.name
                    ),
                    None => "There is no level there to show.".to_string(),
                }
            }
        };
        self.report(r, what);
    }

    /// The Levels manager window, when open.
    pub(super) fn levels_window(&mut self, ctx: &egui::Context) {
        if !self.levels.open {
            return;
        }
        let mut open = true;
        let mut asks = vec![];
        let area = self.mat.area.unwrap_or_else(|| ctx.content_rect());
        egui::Window::new("Levels")
            .open(&mut open)
            // below the 3D view's navigation buttons, which draw on top
            .default_pos(area.min + Vec2::new(16.0, 56.0))
            .constrain_to(area)
            .min_width(660.0)
            .collapsible(false)
            .show(ctx, |ui| self.levels_contents(ui, &mut asks));
        self.levels.open = open;
        if !open {
            self.levels.delete = None;
        }
        for ask in asks {
            self.run_level_ask(ask);
        }
    }

    fn levels_contents(&mut self, ui: &mut egui::Ui, asks: &mut Vec<Ask>) {
        let model = self.doc.model();
        let list = levels::sorted(model);
        ui.horizontal(|ui| {
            if ui
                .button("Add level above the top")
                .on_hover_text("One storey above the highest level, as tall as it")
                .clicked()
            {
                asks.push(Ask::AddAbove);
            }
            ui.separator();
            ui.label("Add at");
            ui.add(crate::units::drag(&mut self.levels.at).range(-1000.0..=10_000.0));
            if ui
                .button("Add")
                .on_hover_text("A new level at this elevation")
                .clicked()
            {
                asks.push(Ask::AddAt(self.levels.at));
            }
        });
        ui.checkbox(
            &mut self.levels.with_plan,
            "Make a floor plan for each new level",
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(
                "Highest first. Moving a level moves everything on it. Height is the storey's: new walls and columns are this tall. In a section or elevation you can also drag a level line, or draw a new one with the Level tool.",
            )
            .weak(),
        );
        ui.separator();
        egui::Grid::new("levels")
            .striped(true)
            .num_columns(7)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                for h in [
                    "Name",
                    "Elevation",
                    "Height",
                    "To next",
                    "On it",
                    "Floor plan",
                    "",
                ] {
                    ui.label(egui::RichText::new(h).strong());
                }
                ui.end_row();
                for (i, (id, name, l)) in list.iter().enumerate().rev() {
                    let draft = self.levels.names.entry(*id).or_insert_with(|| name.clone());
                    let r = ui.add_sized([150.0, 20.0], egui::TextEdit::singleline(draft));
                    if r.lost_focus() && draft.trim() != name && !draft.trim().is_empty() {
                        asks.push(Ask::Rename(*id, draft.trim().to_string()));
                    } else if !r.has_focus() && *draft != *name {
                        *draft = name.clone();
                    }
                    let (mut z, mut h) = (l.elevation, l.height);
                    let ze = ui.add(crate::units::drag(&mut z).range(-1000.0..=10_000.0));
                    let he = ui.add(crate::units::drag(&mut h).range(0.5..=100.0));
                    if ze.changed() || he.changed() {
                        asks.push(Ask::Set(*id, z, h));
                    }
                    // the gap up to the level above, which the storey can be fitted to
                    match list.get(i + 1) {
                        Some((_, above, a)) => {
                            let gap = a.elevation - l.elevation;
                            ui.horizontal(|ui| {
                                ui.label(crate::units::len(gap))
                                    .on_hover_text(format!("From here up to {above}"));
                                if (gap - l.height).abs() > 1e-6
                                    && gap > 0.5
                                    && ui
                                        .small_button("Fit")
                                        .on_hover_text(format!(
                                            "Make the storey as tall as the gap up to {above}"
                                        ))
                                        .clicked()
                                {
                                    asks.push(Ask::Set(*id, l.elevation, gap));
                                }
                            });
                        }
                        None => {
                            ui.label(egui::RichText::new("top").weak());
                        }
                    }
                    let n = levels::on_level(model, *id).len();
                    ui.label(format!("{n} element{}", if n == 1 { "" } else { "s" }));
                    let plans = levels::plan_views(model, *id);
                    match plans.first() {
                        Some(v) => {
                            if ui
                                .button("Open")
                                .on_hover_text("Show it in the 2D view")
                                .clicked()
                            {
                                asks.push(Ask::OpenPlan(*v));
                            }
                        }
                        None => {
                            if ui
                                .button("Make")
                                .on_hover_text("Make a floor plan of this level")
                                .clicked()
                            {
                                asks.push(Ask::MakePlan(*id));
                            }
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.menu_button("Duplicate", |ui| {
                            if ui.button("The level only").clicked() {
                                asks.push(Ask::Duplicate(*id, false));
                                ui.close();
                            }
                            if ui
                                .button("With everything on it")
                                .on_hover_text(
                                    "Walls, slabs, doors, windows and the rest, one storey up",
                                )
                                .clicked()
                            {
                                asks.push(Ask::Duplicate(*id, true));
                                ui.close();
                            }
                        })
                        .response
                        .on_hover_text("A copy above the top level");
                        let can = list.len() > 1;
                        if ui
                            .add_enabled(can, egui::Button::new("Delete"))
                            .on_disabled_hover_text("A project keeps at least one level")
                            .clicked()
                        {
                            asks.push(Ask::Delete(*id));
                        }
                    });
                    ui.end_row();
                }
            });

        // asked to delete a level: what happens to what is on it
        let Some((del, mut target)) = self.levels.delete else {
            return;
        };
        let Some(e) = model.get(del) else {
            asks.push(Ask::CancelDelete);
            return;
        };
        ui.add_space(8.0);
        ui.separator();
        let on = levels::on_level(model, del);
        let plans = levels::plan_views(model, del);
        ui.label(egui::RichText::new(format!("Delete {}?", e.name)).strong());
        let mut what = vec![];
        if !plans.is_empty() {
            what.push(format!(
                "its floor plan{} (and what is drawn on {})",
                if plans.len() == 1 { "" } else { "s" },
                if plans.len() == 1 { "it" } else { "them" }
            ));
        }
        if on.is_empty() {
            ui.label("Nothing is on it.");
            if !what.is_empty() {
                ui.label(format!("This also deletes {}.", what.join(" and ")));
            }
        } else {
            ui.label(format!(
                "{} element{} on it. This also deletes {}.",
                on.len(),
                if on.len() == 1 { " is" } else { "s are" },
                if what.is_empty() {
                    "dimensions taken from the level".to_string()
                } else {
                    what.join(" and ")
                }
            ));
            let others: Vec<&(ElementId, String, creabim_core::Level)> =
                list.iter().filter(|(id, ..)| *id != del).collect();
            let mut keep = target.is_some();
            ui.radio_value(
                &mut keep,
                true,
                "Move them to another level, keeping them where they are",
            );
            if keep {
                let current = target
                    .and_then(|t| others.iter().find(|(id, ..)| *id == t))
                    .map_or("Pick a level", |(_, n, _)| n.as_str());
                ui.indent("to", |ui| {
                    egui::ComboBox::from_id_salt("move-to")
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            for (id, n, _) in &others {
                                ui.selectable_value(&mut target, Some(*id), n);
                            }
                        });
                });
            }
            ui.radio_value(&mut keep, false, "Delete them with the level");
            if keep && target.is_none() {
                target = nearest(&list, del);
            } else if !keep {
                target = None;
            }
            self.levels.delete = Some((del, target));
        }
        ui.horizontal(|ui| {
            if ui
                .button(egui::RichText::new("Delete level").color(Color32::from_rgb(190, 50, 40)))
                .clicked()
            {
                asks.push(Ask::ConfirmDelete(del, target));
            }
            if ui.button("Cancel").clicked() {
                asks.push(Ask::CancelDelete);
            }
            ui.label(egui::RichText::new("Ctrl+Z brings it all back.").weak());
        });
    }

    fn run_level_ask(&mut self, ask: Ask) {
        match ask {
            Ask::AddAbove => {
                self.add_level_above();
                self.levels.at = levels::above_top(self.doc.model()).0;
            }
            Ask::AddAt(z) => {
                // as tall as the level below it, or the default storey
                let list = levels::sorted(self.doc.model());
                let h = list
                    .iter()
                    .rev()
                    .find(|(_, _, l)| l.elevation < z)
                    .map_or(3.0, |(_, _, l)| l.height);
                self.add_level_at(z, h);
            }
            Ask::Duplicate(id, with) => {
                let Some((tx, new)) =
                    levels::duplicate(self.doc.model(), id, with, self.levels.with_plan)
                else {
                    return;
                };
                let n = tx.ops.len();
                let r = self.doc.commit(tx);
                self.selection.only(new);
                let name = self
                    .doc
                    .model()
                    .get(new)
                    .map(|e| e.name.clone())
                    .unwrap_or_default();
                self.report(
                    r,
                    if with {
                        format!(
                            "Duplicated the level as {name}, with {} elements.",
                            n - 1 - usize::from(self.levels.with_plan)
                        )
                    } else {
                        format!("Duplicated the level as {name}.")
                    },
                );
            }
            Ask::Rename(id, name) => {
                let tx = levels::rename(self.doc.model(), id, &name);
                let plans = tx.ops.len().saturating_sub(1);
                let r = self.doc.commit(tx);
                self.levels.names.remove(&id);
                self.report(
                    r,
                    match plans {
                        0 => format!("Renamed the level {name}."),
                        1 => format!("Renamed the level {name}, and its plan with it."),
                        n => format!("Renamed the level {name}, and its {n} plans with it."),
                    },
                );
            }
            Ask::Set(id, z, h) => {
                let r = self.doc.edit_merged(id, "Change level", |e| {
                    if let ElementKind::Level(l) = &mut e.kind {
                        l.elevation = z;
                        l.height = h;
                    }
                });
                self.report(r, "Moved the level, and everything on it.");
            }
            Ask::MakePlan(id) => {
                let name = self
                    .doc
                    .model()
                    .get(id)
                    .map(|e| e.name.clone())
                    .unwrap_or_default();
                let v = Element::new(format!("{name} plan"), ElementKind::View(View::plan(id)));
                let vid = v.id;
                let r = self
                    .doc
                    .commit(Transaction::new("Make floor plan").insert(v));
                self.active_view = Some(vid);
                self.report(r, format!("Made the floor plan of {name}."));
            }
            Ask::OpenPlan(v) => self.active_view = Some(v),
            Ask::Delete(id) => {
                let list = levels::sorted(self.doc.model());
                self.levels.delete = Some((id, nearest(&list, id)));
            }
            Ask::ConfirmDelete(id, target) => {
                let name = self
                    .doc
                    .model()
                    .get(id)
                    .map(|e| e.name.clone())
                    .unwrap_or_default();
                let n = levels::on_level(self.doc.model(), id).len();
                let what = match target {
                    Some(t) => OnDelete::MoveTo(t),
                    None => OnDelete::Delete,
                };
                let tx = levels::delete(self.doc.model(), id, what);
                let r = self.doc.commit(tx);
                self.levels.delete = None;
                self.drop_stale_selection();
                if self
                    .active_view
                    .is_some_and(|v| self.doc.model().get(v).is_none())
                {
                    self.active_view = None;
                }
                let to = target
                    .and_then(|t| self.doc.model().get(t))
                    .map(|e| e.name.clone());
                self.report(
                    r,
                    match (n, to) {
                        (0, _) => format!("Deleted {name}."),
                        (n, Some(to)) => format!("Deleted {name}; its {n} elements moved to {to}."),
                        (n, None) => format!("Deleted {name} and the {n} elements on it."),
                    },
                );
            }
            Ask::CancelDelete => self.levels.delete = None,
        }
    }

    /// The Project units dialog, when open.
    pub(super) fn units_window(&mut self, ctx: &egui::Context) {
        if !self.levels.units_open {
            return;
        }
        let mut open = true;
        let mut units = self.doc.model().units();
        let before = units;
        let area = self.mat.area.unwrap_or_else(|| ctx.content_rect());
        egui::Window::new("Project units")
            .open(&mut open)
            .default_pos(area.min + Vec2::new(40.0, 64.0))
            .constrain_to(area)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "How lengths are shown and typed everywhere: properties, readouts, dimensions, tags, schedules and sheets. The model keeps metres, so switching is lossless.",
                    )
                    .weak(),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(!units.is_imperial(), "Metric")
                        .clicked()
                        && units.is_imperial()
                    {
                        units = Units::metric();
                    }
                    if ui
                        .selectable_label(units.is_imperial(), "Imperial")
                        .clicked()
                        && !units.is_imperial()
                    {
                        units = Units::imperial();
                    }
                });
                egui::Grid::new("units").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    ui.label("Length");
                    egui::ComboBox::from_id_salt("unit")
                        .selected_text(units.length.label())
                        .width(260.0)
                        .show_ui(ui, |ui| {
                            for u in LengthUnit::ALL {
                                if ui.selectable_label(units.length == u, u.label()).clicked() {
                                    units = Units::of(u);
                                }
                            }
                        });
                    ui.end_row();
                    ui.label(if units.length.is_fractional() {
                        "Rounding"
                    } else {
                        "Decimals"
                    });
                    let current = units
                        .precisions()
                        .into_iter()
                        .find(|(p, _)| *p == units.precision)
                        .map(|(_, l)| l)
                        .unwrap_or_default();
                    egui::ComboBox::from_id_salt("precision")
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            for (p, l) in units.precisions() {
                                ui.selectable_value(&mut units.precision, p, l);
                            }
                        });
                    ui.end_row();
                    ui.label("Looks like");
                    ui.label(
                        egui::RichText::new(format!(
                            "{}   ·   {}   ·   {}",
                            units.length(1.0668),
                            units.length(0.2032),
                            units.area(12.5)
                        ))
                        .monospace(),
                    );
                    ui.end_row();
                });
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(
                        "Any unit can be typed in a field: 3'-6 1/2\", 3 6, 1067 mm, 1.2 m. Sheet sizes stay in millimetres.",
                    )
                    .weak(),
                );
            });
        self.levels.units_open = open;
        if units != before {
            let tx = set_units(self.doc.model(), units);
            let r = self.doc.commit(tx);
            crate::units::set(units);
            self.report(r, format!("Units: {}.", units.length.label()));
        }
    }
}

/// The level nearest `id` other than itself: where its elements go by default.
fn nearest(list: &[(ElementId, String, creabim_core::Level)], id: ElementId) -> Option<ElementId> {
    let z = list.iter().find(|(i, ..)| *i == id)?.2.elevation;
    list.iter()
        .filter(|(i, ..)| *i != id)
        .min_by(|a, b| {
            (a.2.elevation - z)
                .abs()
                .total_cmp(&(b.2.elevation - z).abs())
                // the one below wins a tie
                .then(a.2.elevation.total_cmp(&b.2.elevation))
        })
        .map(|(i, ..)| *i)
}
