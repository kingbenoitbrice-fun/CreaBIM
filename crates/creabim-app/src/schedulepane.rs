//! A schedule shown as a live, editable table. Changing a cell edits the
//! model, so plans, sections, sheets and the 3D view update at once.

use crate::selection::Selection;
use creabim_core::{Document, ElementId, ElementKind, ScheduleCategory};
use creabim_geom::schedule::{self, Field};
use eframe::egui::{self, Color32, Rect};

/// A pending change to one element.
type Edit = (ElementId, Box<dyn FnOnce(&mut creabim_core::Element)>);

/// Show the schedule `view` of `category` in `rect`. Returns a status
/// message when an edit fails or the schedule is exported.
pub fn show(
    ui: &mut egui::Ui,
    rect: Rect,
    doc: &mut Document,
    view: ElementId,
    category: ScheduleCategory,
    selection: &mut Selection,
    csv_path: &str,
) -> Option<String> {
    let table = schedule::schedule(doc.model(), category);
    let materials: Vec<(ElementId, String)> = doc
        .model()
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Material(_)))
        .map(|e| (e.id, e.name.clone()))
        .collect();
    let mut message = None;
    let mut edit: Option<Edit> = None;
    ui.painter()
        .rect_filled(rect, 0.0, Color32::from_rgb(252, 252, 250));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(12.0)));
    let ui = &mut child;
    ui.add_space(22.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "Every {} in the model, read live. Edit a cell to change the model.",
                category.label().to_lowercase().trim_end_matches('s')
            ))
            .weak(),
        );
        if ui.button("Export CSV").clicked() {
            let r = std::fs::write(csv_path, schedule::to_csv(&table));
            message = Some(match r {
                Ok(()) => format!("Exported {csv_path}"),
                Err(e) => format!("Couldn't export {csv_path}: {e}"),
            });
        }
    });
    ui.add_space(6.0);
    egui::ScrollArea::both().show(ui, |ui| {
        egui::Grid::new(("schedule", view))
            .striped(true)
            .spacing([14.0, 4.0])
            .min_col_width(40.0)
            .show(ui, |ui| {
                for c in &table.columns {
                    ui.label(egui::RichText::new(c.title).strong());
                }
                ui.end_row();
                for row in &table.rows {
                    let Some(e) = doc.model().get(row.id).cloned() else {
                        continue;
                    };
                    for (i, c) in table.columns.iter().enumerate() {
                        let text = &row.cells[i];
                        match c.field {
                            Some(f) if f.is_length() => {
                                let Some(v) = schedule::value(&e, f) else {
                                    ui.label(text);
                                    continue;
                                };
                                let mut mm = v * 1000.0;
                                let r = ui.add(
                                    egui::DragValue::new(&mut mm)
                                        .speed(5.0)
                                        .range(0.0..=50_000.0)
                                        .max_decimals(0),
                                );
                                if r.changed() {
                                    edit = Some((
                                        row.id,
                                        Box::new(move |e| schedule::set_value(e, f, mm / 1000.0)),
                                    ));
                                }
                            }
                            Some(Field::Name) => {
                                let mut name = e.name.clone();
                                if ui
                                    .add(egui::TextEdit::singleline(&mut name).desired_width(110.0))
                                    .changed()
                                {
                                    edit = Some((row.id, Box::new(move |e| e.name = name)));
                                }
                            }
                            Some(Field::Material) => {
                                let current =
                                    creabim_core::material_of(doc.model(), &e).map(|(m, _)| m.id);
                                let mut chosen = current;
                                egui::ComboBox::from_id_salt(("material", row.id))
                                    .selected_text(text.as_str())
                                    .width(110.0)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut chosen, None, "-");
                                        for (id, name) in &materials {
                                            ui.selectable_value(&mut chosen, Some(*id), name);
                                        }
                                    });
                                if chosen != current {
                                    edit = Some((
                                        row.id,
                                        Box::new(move |e| schedule::set_material(e, chosen)),
                                    ));
                                }
                            }
                            _ if i == 0 => {
                                // the first column selects the row's element
                                if ui
                                    .selectable_label(selection.contains(row.id), text)
                                    .clicked()
                                {
                                    selection.only(row.id);
                                }
                            }
                            _ => {
                                ui.label(text);
                            }
                        }
                    }
                    ui.end_row();
                }
                for t in &table.totals {
                    ui.label(egui::RichText::new(t).italics());
                }
                ui.end_row();
            });
    });
    if let Some((id, f)) = edit {
        selection.only(id);
        if let Err(e) = doc.edit_merged(id, "Edit in schedule", f) {
            message = Some(format!("Couldn't do that: {e}"));
        }
    }
    message
}
