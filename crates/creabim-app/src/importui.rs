//! The import report: after an IFC file is opened, what came in as native
//! elements and as meshes, the import's notes grouped by kind, and every
//! element it could not bring across natively, by class, name and
//! GlobalId with the reason. The whole report can be copied, to fix the
//! exporting model or to send with a bug report.

use super::*;
use std::collections::BTreeMap;

/// One import's report.
#[derive(Clone, Debug, Default)]
pub(super) struct ImportReport {
    pub open: bool,
    pub file: String,
    pub schema: String,
    /// Per IFC class: elements, and how many of them are meshes.
    pub counts: Vec<(String, usize, usize)>,
    pub summary: Vec<String>,
    pub details: Vec<String>,
}

impl ImportReport {
    pub fn new(file: &str, imported: &creabim_ifc::ImportResult) -> Self {
        Self {
            open: !imported.warnings.is_empty(),
            file: file.to_string(),
            schema: imported.schema.clone(),
            counts: class_counts(&imported.model),
            summary: imported.warnings.clone(),
            details: imported.details.clone(),
        }
    }

    /// The report as plain text, as copied.
    pub fn text(&self) -> String {
        let mut out = format!("Import of {} ({})\n\nElements:\n", self.file, self.schema);
        for (class, n, meshes) in &self.counts {
            out.push_str(&format!("  {class}: {}\n", count_label(*n, *meshes)));
        }
        if !self.summary.is_empty() {
            out.push_str("\nNotes:\n");
            for s in &self.summary {
                out.push_str(&format!("  {s}\n"));
            }
        }
        if !self.details.is_empty() {
            out.push_str("\nElement by element:\n");
            for d in &self.details {
                out.push_str(&format!("  {d}\n"));
            }
        }
        out
    }
}

/// Elements per IFC class, with how many are meshes; levels, views and
/// the like are left out.
fn class_counts(model: &Model) -> Vec<(String, usize, usize)> {
    let mut c: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for e in model.iter() {
        let class = e.ifc_class();
        if !class.starts_with("Ifc") || class == "IfcBuildingStorey" || class == "IfcGrid" {
            continue;
        }
        let entry = c.entry(class.to_string()).or_default();
        entry.0 += 1;
        entry.1 += usize::from(matches!(e.kind, ElementKind::FreeMesh(_)));
    }
    c.into_iter().map(|(k, (n, m))| (k, n, m)).collect()
}

fn count_label(n: usize, meshes: usize) -> String {
    match (n - meshes, meshes) {
        (_, 0) => format!("{n} native"),
        (0, _) => format!("{n} as meshes"),
        (native, m) => format!("{native} native, {m} as meshes"),
    }
}

impl CreaBimApp {
    /// The report window of the last IFC import.
    pub(super) fn import_report_window(&mut self, ctx: &egui::Context) {
        let area = self.mat.area.unwrap_or_else(|| ctx.content_rect());
        let Some(report) = &mut self.import_report else {
            return;
        };
        if !report.open {
            return;
        }
        let mut open = true;
        let mut copy = false;
        egui::Window::new("Import report")
            .open(&mut open)
            .default_pos(area.min + Vec2::new(80.0, 70.0))
            .default_size(Vec2::new(640.0, 460.0))
            .constrain_to(area)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!("{} ({})", report.file, report.schema)).strong(),
                );
                ui.horizontal_wrapped(|ui| {
                    for (class, n, meshes) in &report.counts {
                        let text = format!("{class}: {}", count_label(*n, *meshes));
                        ui.label(egui::RichText::new(text).small());
                        ui.separator();
                    }
                });
                ui.separator();
                if report.summary.is_empty() {
                    ui.label("Everything came in as it was written.");
                }
                for s in &report.summary {
                    ui.label(format!("• {s}"));
                }
                if !report.details.is_empty() {
                    ui.add_space(6.0);
                    let title = format!("Element by element ({}):", report.details.len());
                    ui.label(egui::RichText::new(title).strong());
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for d in &report.details {
                                let text = egui::RichText::new(d).small();
                                ui.add(egui::Label::new(text).selectable(true));
                            }
                        });
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    copy = ui
                        .button("Copy report")
                        .on_hover_text("Copy the whole report as text")
                        .clicked();
                    ui.label(egui::RichText::new("File › Import report shows it again.").weak());
                });
            });
        if copy {
            ctx.copy_text(report.text());
            self.status = "Import report copied.".into();
        }
        if !open {
            report.open = false;
        }
    }
}
