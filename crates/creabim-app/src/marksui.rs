//! Marks: the Mark field in Properties, with its duplicate warning, and
//! the Mark numbering dialog, where the project's numbering rules are
//! edited, saved to a file, loaded from one and applied. The rules are
//! project data ([`creabim_core::MarkRules`]); every change is one
//! transaction, so it undoes like any edit.

use super::*;
use creabim_core::marks::{self, MarkClass, MarkOrder, MarkRules};

/// The dialog's own state, for the session.
#[derive(Default)]
pub(super) struct MarksUi {
    pub open: bool,
    /// Renumber typed marks too.
    replace: bool,
    /// Renumber only this kind; none is every kind.
    only: Option<MarkClass>,
}

/// Can this element carry a mark?
pub(super) fn markable(kind: &ElementKind) -> bool {
    MarkClass::of(kind).is_some()
        || matches!(
            kind,
            ElementKind::Object(_) | ElementKind::FreeMesh(_) | ElementKind::Opening(_)
        )
}

impl CreaBimApp {
    /// The Mark field of a selected element: what it is typed as, or
    /// (greyed in the empty field) the number the rules give it, and a
    /// warning when another element has the same mark.
    pub(super) fn mark_field(&mut self, ui: &mut egui::Ui, id: ElementId) {
        let model = self.doc.model();
        let Some(e) = model.get(id) else { return };
        if !markable(&e.kind) {
            return;
        }
        let shown = marks::mark_of(model, id);
        let typed = marks::manual(e).is_some();
        let mut text = e.mark.clone().unwrap_or_default();
        let hint = match (&shown, typed) {
            (Some(m), false) => format!("{m} (numbered)"),
            _ => "None".into(),
        };
        let changed = ui
            .horizontal(|ui| {
                ui.label("Mark").on_hover_text(
                    "What its tags and schedules show. Leave it empty for the number the project's rules give it (File, Mark numbering).",
                );
                ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .hint_text(hint)
                        .desired_width(120.0),
                )
                .changed()
            })
            .inner;
        if changed {
            let value = (!text.trim().is_empty()).then(|| text.clone());
            let r = self.doc.edit_merged(id, "Mark", |e| e.mark = value.clone());
            self.report(r, "Mark changed.");
        }
        let model = self.doc.model();
        if let Some((m, others)) = marks::duplicates(model)
            .into_iter()
            .find(|(_, ids)| ids.contains(&id))
        {
            let n = others.len() - 1;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "⚠ {m} is also the mark of {n} other element{}.",
                        if n == 1 { "" } else { "s" }
                    ))
                    .color(ui.visuals().warn_fg_color),
                );
                if ui.small_button("Select them").clicked() {
                    self.selection.extend(others.clone(), false);
                }
            });
        }
    }

    /// The Mark numbering dialog, when open.
    pub(super) fn marks_window(&mut self, ctx: &egui::Context) {
        if !self.marks.open {
            return;
        }
        let mut open = true;
        let before = marks::rules(self.doc.model());
        let mut rules = before.clone();
        let mut action: Option<&'static str> = None;
        let area = self.mat.area.unwrap_or_else(|| ctx.content_rect());
        egui::Window::new("Mark numbering")
            .open(&mut open)
            .default_pos(area.min + Vec2::new(40.0, 64.0))
            .constrain_to(area)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "How doors, windows and other elements are numbered when they have no mark of their own. Tags and schedules show these numbers. The rules belong to the project and can be saved for another one.",
                    )
                    .weak(),
                );
                ui.add_space(6.0);
                rules_grid(ui, &mut rules);
                ui.horizontal(|ui| {
                    ui.label("Ground level number").on_hover_text(
                        "The number of the lowest level at or above ±0, for level-based marks; levels above count up, those below count down",
                    );
                    ui.add(egui::DragValue::new(&mut rules.ground_number).range(-9..=99));
                });
                ui.horizontal(|ui| {
                    if ui
                        .button("D01, W01")
                        .on_hover_text("Doors and windows counted through the building, lowest level first")
                        .clicked()
                    {
                        rules = MarkRules::default();
                    }
                    if ui
                        .button("D-101, W-101")
                        .on_hover_text("Level-based: the level's number, then the count on that level")
                        .clicked()
                    {
                        rules = MarkRules::level_based();
                    }
                    if ui.button("Save rules…").clicked() {
                        action = Some("save");
                    }
                    if ui.button("Load rules…").clicked() {
                        action = Some("load");
                    }
                });
                ui.separator();
                ui.label(egui::RichText::new("Apply").strong());
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("renumber_only")
                        .selected_text(self.marks.only.map_or("Every kind", |c| c.label()))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.marks.only, None, "Every kind");
                            for c in MarkClass::ALL {
                                ui.selectable_value(&mut self.marks.only, Some(c), c.label());
                            }
                        });
                    ui.checkbox(&mut self.marks.replace, "Replace typed marks too");
                });
                ui.horizontal(|ui| {
                    if ui
                        .button("Renumber")
                        .on_hover_text(
                            "Write the rules' numbers onto the elements without a mark, so later changes to the model leave them as they are. With Replace, every element is numbered afresh.",
                        )
                        .clicked()
                    {
                        action = Some("renumber");
                    }
                    if ui
                        .button("Marks from names")
                        .on_hover_text(
                            "Elements named like \"D-101 sliding doors\" take D-101 as their mark",
                        )
                        .clicked()
                    {
                        action = Some("names");
                    }
                });
                ui.separator();
                self.duplicates_list(ui);
            });
        self.marks.open = open;
        if rules != before {
            let tx = marks::set_rules(self.doc.model(), rules.clone());
            let r = self.doc.commit_merged(tx);
            self.report(r, "Mark numbering changed.");
        }
        match action {
            Some("save") => self.save_mark_rules(&rules),
            Some("load") => self.load_mark_rules(),
            Some("renumber") => self.renumber_marks(),
            Some("names") => {
                let tx = marks::marks_from_names(self.doc.model());
                let n = tx.ops.len();
                let r = self.doc.commit(tx);
                self.report(r, format!("{n} marks taken from names."));
            }
            _ => {}
        }
    }

    /// Renumber as the dialog is set (see [`marks::renumber`]).
    pub(super) fn renumber_marks(&mut self) {
        let classes: Vec<MarkClass> = self.marks.only.into_iter().collect();
        let tx = marks::renumber(self.doc.model(), self.marks.replace, &classes);
        let n = tx.ops.len();
        let r = self.doc.commit(tx);
        self.report(r, format!("{n} marks renumbered."));
    }

    /// The marks held by more than one element: the model check.
    fn duplicates_list(&mut self, ui: &mut egui::Ui) {
        let dups = marks::duplicates(self.doc.model());
        if dups.is_empty() {
            ui.label(egui::RichText::new("✔ No two elements share a mark.").weak());
            return;
        }
        ui.label(
            egui::RichText::new(format!(
                "⚠ {} mark{} used more than once:",
                dups.len(),
                if dups.len() == 1 { " is" } else { "s are" }
            ))
            .color(ui.visuals().warn_fg_color),
        );
        egui::ScrollArea::vertical()
            .max_height(160.0)
            .show(ui, |ui| {
                for (m, ids) in dups {
                    ui.horizontal(|ui| {
                        let names: Vec<String> = ids
                            .iter()
                            .filter_map(|i| self.doc.model().get(*i).map(|e| e.name.clone()))
                            .collect();
                        ui.label(egui::RichText::new(&m).strong());
                        ui.label(names.join(", "));
                        if ui.small_button("Select").clicked() {
                            self.selection.extend(ids.clone(), false);
                        }
                    });
                }
            });
    }

    fn save_mark_rules(&mut self, rules: &MarkRules) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Save mark numbering")
            .set_file_name("mark-numbering.json")
            .add_filter("Mark numbering", &["json"])
            .save_file()
        else {
            return;
        };
        let r = std::fs::write(&path, rules.to_json());
        self.report(r, format!("Saved {}.", path.display()));
    }

    fn load_mark_rules(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Load mark numbering")
            .add_filter("Mark numbering", &["json"])
            .pick_file()
        else {
            return;
        };
        let rules = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| MarkRules::from_json(&t));
        match rules {
            Ok(rules) => {
                let tx = marks::set_rules(self.doc.model(), rules);
                let r = self.doc.commit(tx);
                self.report(r, format!("Loaded {}.", path.display()));
            }
            Err(e) => self.status = format!("Couldn't load {}: {e}", path.display()),
        }
    }
}

/// One row a kind: numbered or not, prefix, separator, digits, by level,
/// order, first number, and what the first mark looks like.
fn rules_grid(ui: &mut egui::Ui, rules: &mut MarkRules) {
    egui::Grid::new("mark_rules")
        .num_columns(8)
        .spacing([10.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            for h in [
                "Number", "Prefix", "Sep.", "Digits", "By level", "Order", "From", "First",
            ] {
                ui.label(egui::RichText::new(h).weak());
            }
            ui.end_row();
            for class in MarkClass::ALL {
                let r = rules.rule_mut(class);
                ui.checkbox(&mut r.auto, class.label());
                ui.add(egui::TextEdit::singleline(&mut r.prefix).desired_width(44.0));
                ui.add(egui::TextEdit::singleline(&mut r.separator).desired_width(24.0));
                ui.add(egui::DragValue::new(&mut r.padding).range(1..=6));
                ui.checkbox(&mut r.by_level, "");
                egui::ComboBox::from_id_salt(("mark_order", class))
                    .selected_text(r.order.label())
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for o in MarkOrder::ALL {
                            ui.selectable_value(&mut r.order, o, o.label());
                        }
                    });
                ui.add(egui::DragValue::new(&mut r.start).range(0..=9999));
                let sample = r.format(1, r.start);
                ui.label(if r.auto {
                    egui::RichText::new(sample).monospace()
                } else {
                    egui::RichText::new("typed only").weak()
                });
                ui.end_row();
            }
        });
}

impl CreaBimApp {
    /// For a level selected while a section or elevation is the active
    /// view: show its head there or not, and put a dragged head back.
    pub(super) fn level_head_controls(&mut self, ui: &mut egui::Ui, level: ElementId) {
        let Some(view) = self.active_view else { return };
        let Some(e) = self.doc.model().get(view) else {
            return;
        };
        let ElementKind::View(v) = &e.kind else {
            return;
        };
        if !matches!(v.kind, ViewKind::Section { .. }) {
            return;
        }
        let name = e.name.clone();
        let head = v.level_heads.get(&level).copied().unwrap_or_default();
        let mut shown = !head.hidden;
        let mut edited = head;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut shown, format!("Head in {name}"))
                .on_hover_text(
                    "Show this level's name and elevation in this view; its line stays either way",
                )
                .changed()
            {
                edited.hidden = !shown;
            }
            if head.offset.is_some()
                && ui
                    .small_button("Put it back")
                    .on_hover_text("Undo the drag: the automatic layout places it again")
                    .clicked()
            {
                edited.offset = None;
            }
        });
        if edited != head {
            let r = self.doc.edit(view, "Level head", |e| {
                if let ElementKind::View(v) = &mut e.kind {
                    if edited.is_default() {
                        v.level_heads.remove(&level);
                    } else {
                        v.level_heads.insert(level, edited);
                    }
                }
            });
            self.report(r, "Level head changed.");
        }
    }
}
