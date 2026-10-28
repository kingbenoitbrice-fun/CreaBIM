//! A schedule shown as a live, editable table. Changing a cell edits the
//! model, so plans, sections, sheets and the 3D view update at once.

use crate::selection::Selection;
use creabim_core::{Document, ElementId, ElementKind, ScheduleCategory};
use creabim_geom::schedule::{self, Field};
use eframe::egui::{self, Color32, Rect};

const PAPER: Color32 = Color32::from_rgb(252, 252, 250);
const INK: Color32 = Color32::from_rgb(31, 37, 43);
const MUTED: Color32 = Color32::from_rgb(110, 116, 122);
const HEADER: Color32 = Color32::from_rgb(236, 238, 236);
const STRIPE: Color32 = Color32::from_rgb(246, 247, 245);
const SELECTED: Color32 = Color32::from_rgb(255, 236, 214);
const RULE: Color32 = Color32::from_rgb(224, 226, 222);
const ACCENT: Color32 = Color32::from_rgb(200, 100, 20);
const ROW_H: f32 = 28.0;
const PAD: f32 = 10.0;

/// Light controls for the paper, whatever the interface's theme.
fn paper_visuals() -> egui::Visuals {
    let mut v = egui::Visuals::light();
    v.override_text_color = Some(INK);
    v.extreme_bg_color = Color32::WHITE;

    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
    ] {
        w.corner_radius = egui::CornerRadius::same(4);
    }
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(240, 241, 238);
    v.widgets.inactive.bg_fill = Color32::from_rgb(240, 241, 238);
    v
}

/// A pending change to one element.
type Edit = (ElementId, Box<dyn FnOnce(&mut creabim_core::Element)>);

/// Show the schedule `view` of `category` (grouped by type with
/// `by_type`) in `rect`. Returns a status
/// message when an edit fails or the schedule is exported.
pub fn show(
    ui: &mut egui::Ui,
    rect: Rect,
    doc: &mut Document,
    view: ElementId,
    (category, by_type): (ScheduleCategory, bool),
    selection: &mut Selection,
    csv_path: &str,
) -> Option<String> {
    let table = schedule::schedule_by(doc.model(), category, by_type);
    let listed = table.listed;
    let materials: Vec<(ElementId, String)> = doc
        .model()
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Material(_)))
        .map(|e| (e.id, e.name.clone()))
        .collect();
    let mut message = None;
    let mut edit: Option<Edit> = None;
    ui.painter().rect_filled(rect, 0.0, PAPER);
    let mut child =
        ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(20.0, 14.0))));
    let ui = &mut child;
    // the table sits on paper whatever the interface's theme
    ui.style_mut().visuals = paper_visuals();
    // the fade at a scrolled edge takes the pane's dark colour, not the paper's
    ui.spacing_mut().scroll.fade.strength = 0.0;
    ui.add_space(20.0);
    let noun = category.label().to_lowercase();
    let count = format!(
        "{} {} · read live from the model · edit a cell to change it",
        listed,
        if listed == 1 {
            noun.trim_end_matches('s').to_string()
        } else {
            noun.clone()
        }
    );
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "{} schedule",
                // "Walls" reads "Wall schedule"; "Columns and beams" stays as is
                if category.label().contains(' ') {
                    category.label()
                } else {
                    category.label().trim_end_matches('s')
                }
            ))
            .size(17.0)
            .strong()
            .color(INK),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("Export CSV")
                .on_hover_text(format!("Save the table as {csv_path}"))
                .clicked()
            {
                let r = std::fs::write(csv_path, schedule::to_csv(&table));
                message = Some(match r {
                    Ok(()) => format!("Exported {csv_path}"),
                    Err(e) => format!("Couldn't export {csv_path}: {e}"),
                });
            }
        });
    });
    ui.label(egui::RichText::new(count).color(MUTED));
    ui.add_space(12.0);

    // each column as wide as its widest cell, editors a little wider
    let font = egui::TextStyle::Body.resolve(ui.style());
    let measure = |ui: &egui::Ui, t: &str, strong: bool| {
        let f = if strong {
            egui::FontId::new(font.size, font.family.clone())
        } else {
            font.clone()
        };
        ui.fonts_mut(|f2| f2.layout_no_wrap(t.to_string(), f, INK).size().x)
    };
    let widths: Vec<f32> = table
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let cells = table
                .rows
                .iter()
                .map(|r| measure(ui, &r.cells[i], false))
                .chain(table.totals.get(i).map(|t| measure(ui, t, false)))
                .fold(measure(ui, c.title, true), f32::max);
            let editor: f32 = match c.field {
                Some(f) if f.is_length() => 64.0,
                Some(Field::Name) | Some(Field::Material) => 120.0,
                _ => 0.0,
            };
            cells.max(editor)
                + 2.0 * PAD
                + if c.field == Some(Field::Material) {
                    22.0
                } else {
                    0.0
                }
        })
        .collect();
    let total: f32 = widths.iter().sum();

    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
        let row = |ui: &mut egui::Ui, fill: Option<Color32>| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(total, ROW_H), egui::Sense::hover());
            if let Some(f) = fill {
                ui.painter().rect_filled(r, 0.0, f);
            }
            ui.painter()
                .hline(r.x_range(), r.bottom(), egui::Stroke::new(1.0, RULE));
            r
        };
        let cells = |r: Rect| {
            let mut x = r.left();
            widths
                .iter()
                .map(|w| {
                    let c = Rect::from_min_size(egui::pos2(x, r.top()), egui::vec2(*w, r.height()));
                    x += w;
                    c
                })
                .collect::<Vec<_>>()
        };
        let text_at =
            |ui: &egui::Ui, c: Rect, t: &str, numeric: bool, color: Color32, strong: bool| {
                let (anchor, at) = if numeric {
                    (
                        egui::Align2::RIGHT_CENTER,
                        c.right_center() - egui::vec2(PAD, 0.0),
                    )
                } else {
                    (
                        egui::Align2::LEFT_CENTER,
                        c.left_center() + egui::vec2(PAD, 0.0),
                    )
                };
                let f = if strong {
                    egui::FontId::new(font.size - 0.5, font.family.clone())
                } else {
                    font.clone()
                };
                ui.painter().with_clip_rect(c).text(at, anchor, t, f, color);
            };

        // header
        let h = row(ui, Some(HEADER));
        for (c, col) in cells(h).into_iter().zip(&table.columns) {
            text_at(ui, c, &col.title.to_uppercase(), col.numeric, INK, true);
        }
        for (n, r) in table.rows.iter().enumerate() {
            if r.group {
                // a type's heading: clicking it selects all its instances
                let rr = row(ui, Some(HEADER));
                let resp = ui.interact(
                    rr,
                    egui::Id::new(("schedule_group", view, n)),
                    egui::Sense::click(),
                );
                let color = if resp.hovered() { ACCENT } else { INK };
                text_at(ui, rr, &r.cells[0], false, color, true);
                if r.id != ElementId::NIL
                    && resp
                        .on_hover_text("Click to select every instance of this type")
                        .clicked()
                {
                    selection.extend(creabim_core::types::instances(doc.model(), r.id), false);
                }
                continue;
            }
            let Some(e) = doc.model().get(r.id).cloned() else {
                continue;
            };
            let selected = selection.contains(r.id);
            let fill = if selected {
                Some(SELECTED)
            } else if n % 2 == 1 {
                Some(STRIPE)
            } else {
                None
            };
            let rr = row(ui, fill);
            for (i, (c, col)) in cells(rr).into_iter().zip(&table.columns).enumerate() {
                let text = &r.cells[i];
                let inner = c.shrink2(egui::vec2(PAD - 3.0, 3.0));
                match col.field {
                    Some(f) if f.is_length() && schedule::value(&e, f).is_some() => {
                        // lengths show and read in the project units
                        let mut m = schedule::value(&e, f).unwrap_or_default();
                        let resp = ui.put(inner, crate::units::drag(&mut m).range(0.0..=50.0));
                        if resp.changed() {
                            edit = Some((r.id, Box::new(move |e| schedule::set_value(e, f, m))));
                        }
                    }
                    Some(Field::Name) => {
                        let mut name = e.name.clone();
                        if ui
                            .put(inner, egui::TextEdit::singleline(&mut name))
                            .changed()
                        {
                            edit = Some((r.id, Box::new(move |e| e.name = name)));
                        }
                    }
                    Some(Field::Material) => {
                        let current = creabim_core::material_of(doc.model(), &e).map(|(m, _)| m.id);
                        let mut chosen = current;
                        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                            egui::ComboBox::from_id_salt(("material", r.id))
                                .selected_text(text.as_str())
                                .width(inner.width())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut chosen, None, "-");
                                    for (id, name) in &materials {
                                        ui.selectable_value(&mut chosen, Some(*id), name);
                                    }
                                });
                        });
                        if chosen != current {
                            edit =
                                Some((r.id, Box::new(move |e| schedule::set_material(e, chosen))));
                        }
                    }
                    _ if i == 0 => {
                        // the first column selects the row's element
                        let resp = ui.interact(
                            c,
                            egui::Id::new(("schedule_pick", view, n, r.id)),
                            egui::Sense::click(),
                        );
                        let color = if resp.hovered() { ACCENT } else { INK };
                        text_at(ui, c, text, col.numeric, color, false);
                        // a count of furniture or fixtures selects what it counts
                        let counted = matches!(e.kind, ElementKind::ElementType(_));
                        let hint = if counted {
                            "Click to select every one of this type"
                        } else {
                            "Click to select it"
                        };
                        if resp.on_hover_text(hint).clicked() {
                            if counted {
                                let all = creabim_core::types::instances(doc.model(), r.id);
                                selection.extend(all, false);
                            } else {
                                selection.only(r.id);
                            }
                        }
                    }
                    _ => text_at(ui, c, text, col.numeric, INK, false),
                }
            }
        }
        // totals
        let t = row(ui, Some(HEADER));
        for ((c, col), text) in cells(t).into_iter().zip(&table.columns).zip(&table.totals) {
            text_at(ui, c, text, col.numeric, MUTED, false);
        }
    });
    if let Some((id, f)) = edit {
        selection.only(id);
        if let Err(e) = doc.edit_merged(id, "Edit in schedule", f) {
            message = Some(format!("Couldn't do that: {e}"));
        }
    }
    message
}
