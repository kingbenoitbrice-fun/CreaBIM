//! Crop boxes in the app (Revit's section boxes; see
//! [`creabim_core::cropbox`]): adding them, their frames and picking in the
//! 3D view, their list and fields in Properties, and the menu entries.
//!
//! The 3D window works with one set of crop boxes at a time: those of the
//! selected crop box's view, else of the 3D view open in the drawing pane,
//! else its own. So a 3D view's crop boxes are edited in the 3D window and
//! show on every sheet that view is placed on.

use super::*;
use crate::ui_model::Item;
use creabim_core::{cropbox, CropBox, CropScope};

const FRAME: Color32 = Color32::from_rgb(40, 110, 200);
const FRAME_SELECTED: Color32 = Color32::from_rgb(230, 120, 30);
/// How far a new crop box stands off what it is fitted round, metres.
const MARGIN: f64 = 0.3;

impl CreaBimApp {
    /// Whose crop boxes the 3D window shows and adds to: a 3D view, or
    /// `None` for its own.
    pub(super) fn crop_owner(&self) -> Option<ElementId> {
        let model = self.doc.model();
        let of_box = |id: ElementId| match model.get(id).map(|e| &e.kind) {
            Some(ElementKind::CropBox(c)) => Some(c.view),
            _ => None,
        };
        if let Some(owner) = self.crop_pick.and_then(of_box) {
            return owner;
        }
        if let Some(owner) = self.selection.active.and_then(of_box) {
            return owner;
        }
        self.active_view.filter(|v| {
            matches!(
                self.view(*v).map(|v| &v.kind),
                Some(ViewKind::Axonometric { .. })
            )
        })
    }

    /// Crop the 3D window's scene with the crop boxes it shows; returns the
    /// cut edges to draw.
    pub(super) fn crop_scene_3d(&mut self) -> Vec<(DVec3, DVec3)> {
        let boxes = creabim_geom::cropbox::active(self.doc.model(), self.crop_owner());
        creabim_geom::cropbox::crop_scene(self.doc.model(), &mut self.scene, &boxes)
    }

    fn owner_name(&self, owner: Option<ElementId>) -> String {
        match owner.and_then(|v| self.doc.model().get(v)) {
            Some(e) => format!("3D view “{}”", e.name),
            None => "the 3D window".into(),
        }
    }

    /// The bounds of what the model shows, or of `only` of it.
    fn shown_bounds(&self, only: Option<&[ElementId]>) -> Option<(DVec3, DVec3)> {
        creabim_geom::scene::drawn_scene(self.doc.model())
            .iter()
            .filter(|s| only.is_none_or(|ids| ids.contains(&s.id)))
            .filter(|s| !self.hidden.contains(&s.id))
            .filter_map(|s| s.mesh.bounds())
            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
            .map(|(a, b)| (a.as_dvec3(), b.as_dvec3()))
    }

    /// Selected building elements a crop box can be fitted round.
    fn selected_croppable(&self) -> Vec<ElementId> {
        self.selection
            .all()
            .into_iter()
            .filter(|id| {
                !matches!(
                    self.doc.model().get(*id).map(|e| &e.kind),
                    Some(ElementKind::CropBox(_) | ElementKind::View(_)) | None
                )
            })
            .collect()
    }

    /// Add a crop box to the 3D window's current set. `around` fits it round
    /// the selection; `only` makes it crop just the selection.
    pub(super) fn add_crop_box(&mut self, around: bool, only: bool) {
        let owner = self.crop_owner();
        let picked = self.selected_croppable();
        if (around || only) && picked.is_empty() {
            self.status = "Select the elements to fit the crop box round first.".into();
            return;
        }
        let bounds = if around || only {
            self.shown_bounds(Some(&picked))
        } else {
            self.shown_bounds(None)
        };
        let (lo, hi) = bounds.unwrap_or((DVec3::new(-5.0, -5.0, 0.0), DVec3::new(5.0, 5.0, 3.0)));
        let mut c = CropBox::new(owner, lo - DVec3::splat(MARGIN), hi + DVec3::splat(MARGIN));
        if only {
            c.scope = CropScope::Only(picked);
        }
        let name = cropbox::next_name(self.doc.model(), owner);
        let e = Element::new(name.clone(), ElementKind::CropBox(c));
        let id = e.id;
        let r = self
            .doc
            .commit(Transaction::new(format!("Add {name}")).insert(e));
        if r.is_ok() {
            self.selection.only(id);
            self.face = None;
        }
        let what = if only {
            "it crops only the selected elements"
        } else {
            "it crops everything in the view"
        };
        self.report(
            r,
            format!(
                "Added {name} to {}: {what}. Drag the handles on its faces to crop.",
                self.owner_name(owner)
            ),
        );
    }

    fn crop_box(&self, id: ElementId) -> Option<(&Element, &CropBox)> {
        let e = self.doc.model().get(id)?;
        match &e.kind {
            ElementKind::CropBox(c) => Some((e, c)),
            _ => None,
        }
    }

    fn edit_crop(&mut self, id: ElementId, label: &str, f: impl FnOnce(&mut CropBox)) {
        let r = self.doc.edit(id, label, |e| {
            if let ElementKind::CropBox(c) = &mut e.kind {
                f(c)
            }
        });
        self.report(r, format!("{label}. Ctrl+Z to undo."));
    }

    /// Menu and browser actions for crop boxes; `false` for other actions.
    pub(super) fn crop_action(&mut self, head: &str, arg: &str) -> bool {
        let id = self
            .doc
            .model()
            .iter()
            .find(|e| e.id.0.to_string() == arg)
            .map(|e| e.id);
        match (head, id) {
            ("crop.add", _) => self.add_crop_box(false, false),
            ("crop.around", _) => self.add_crop_box(true, false),
            ("crop.only", _) => self.add_crop_box(true, true),
            ("crop.toggle", Some(id)) => {
                let on = self.crop_box(id).is_some_and(|(_, c)| c.enabled);
                self.edit_crop(id, if on { "Crop box off" } else { "Crop box on" }, |c| {
                    c.enabled = !on
                });
            }
            ("crop.frame", Some(id)) => {
                let hidden = self.crop_box(id).is_some_and(|(_, c)| c.frame_hidden);
                self.edit_crop(
                    id,
                    if hidden {
                        "Show crop box"
                    } else {
                        "Hide crop box"
                    },
                    |c| c.frame_hidden = !hidden,
                );
            }
            ("crop.select", Some(id)) => {
                self.selection.only(id);
                self.face = None;
            }
            _ => return false,
        }
        true
    }

    /// The View menu's crop box entries.
    pub(super) fn crop_menu(&self) -> Vec<Item> {
        let owner = self.crop_owner();
        let some = !self.selected_croppable().is_empty();
        let mut items = vec![
            Item::new("crop.add", "New crop box")
                .icon("crop-box")
                .hint("A box round the whole model that crops everything in the 3D view; drag its faces in"),
            Item::new("crop.around", "New crop box round the selection")
                .enabled(some)
                .hint("Crops everything in the view to the selected elements"),
            Item::new("crop.only", "New crop box for the selection only")
                .enabled(some)
                .hint("Crops only the selected elements; everything else stays whole"),
        ];
        let boxes = cropbox::crop_boxes(self.doc.model(), owner);
        if !boxes.is_empty() {
            items.push(Item::Separator);
            items.push(Item::Header(format!(
                "Crop boxes of {}",
                self.owner_name(owner)
            )));
            for (e, c) in boxes {
                items.push(
                    Item::new(format!("crop.toggle:{}", e.id.0), e.name.clone())
                        .checked(c.enabled)
                        .keep_open()
                        .hint("Crop with this box, or switch it off"),
                );
            }
        }
        items
    }

    /// The crop boxes of `owner`, each with its switches, in Properties.
    pub(super) fn crop_list(&mut self, ui: &mut egui::Ui, owner: Option<ElementId>) {
        let title = format!("Crop boxes of {}", self.owner_name(owner));
        if !self.prop_header(ui, "crops", &title, true, Some("crop-boxes")) {
            return;
        }
        let rows: Vec<(ElementId, String, bool, bool)> =
            cropbox::crop_boxes(self.doc.model(), owner)
                .into_iter()
                .map(|(e, c)| (e.id, e.name.clone(), c.enabled, !c.frame_hidden))
                .collect();
        if rows.is_empty() {
            ui.label(
                egui::RichText::new(
                    "None yet. A crop box cuts the 3D view down to what is inside it.",
                )
                .weak(),
            );
        }
        let mut act: Option<(&str, ElementId)> = None;
        for (id, name, on, shown) in &rows {
            ui.horizontal(|ui| {
                let mut v = *on;
                if ui
                    .checkbox(&mut v, "")
                    .on_hover_text("Crop with it")
                    .changed()
                {
                    act = Some(("crop.toggle", *id));
                }
                let mut s = *shown;
                if ui
                    .checkbox(&mut s, "")
                    .on_hover_text("Show its frame and handles (it crops either way)")
                    .changed()
                {
                    act = Some(("crop.frame", *id));
                }
                let sel = self.selection.contains(*id);
                if ui.selectable_label(sel, name).clicked() {
                    act = Some(("crop.select", *id));
                }
                if ui.small_button("Delete").clicked() {
                    act = Some(("delete", *id));
                }
            });
        }
        if let Some((what, id)) = act {
            if what == "delete" {
                let name = self
                    .crop_box(id)
                    .map_or(String::new(), |(e, _)| e.name.clone());
                let r = self.doc.commit(
                    Transaction::new(format!("Delete {name}"))
                        .remove_all_cascade(self.doc.model(), &[id]),
                );
                self.report(r, format!("Deleted {name}. Ctrl+Z brings it back."));
                self.drop_stale_selection();
            } else {
                self.crop_action(what, &id.0.to_string());
            }
        }
        ui.horizontal(|ui| {
            if ui.button("Add crop box").clicked() {
                self.add_crop_box(false, false);
            }
            let some = !self.selected_croppable().is_empty();
            if ui
                .add_enabled(some, egui::Button::new("Round the selection"))
                .clicked()
            {
                self.add_crop_box(true, false);
            }
        });
    }

    /// Properties of a crop box; returns the undo label when `c` changed.
    pub(super) fn crop_properties(
        &mut self,
        ui: &mut egui::Ui,
        id: ElementId,
        c: &mut CropBox,
    ) -> Option<&'static str> {
        let mut label = None;
        ui.label(
            egui::RichText::new(format!(
                "Crops {}. Drag the round handles on its faces to resize it, the square one to move it.",
                self.owner_name(c.view)
            ))
            .weak(),
        );
        if ui.checkbox(&mut c.enabled, "Crop").changed() {
            label = Some("Crop box on or off");
        }
        let mut shown = !c.frame_hidden;
        if ui
            .checkbox(&mut shown, "Show the box")
            .on_hover_text(
                "Hidden, it still crops; its frame shows again only while it is selected",
            )
            .changed()
        {
            c.frame_hidden = !shown;
            label = Some("Show or hide crop box");
        }
        ui.add_space(4.0);
        ui.label("What it crops");
        let only = matches!(c.scope, CropScope::Only(_));
        if ui.radio(!only, "Everything in the view").clicked() && only {
            c.scope = CropScope::All;
            label = Some("Crop everything");
        }
        let n = match &c.scope {
            CropScope::Only(ids) => ids.len(),
            CropScope::All => 0,
        };
        if ui
            .radio(only, format!("Only chosen elements ({n})"))
            .clicked()
            && !only
        {
            c.scope = CropScope::Only(vec![]);
            label = Some("Crop chosen elements");
            self.start_crop_pick(id);
        }
        if only {
            ui.horizontal(|ui| {
                if self.crop_pick == Some(id) {
                    if ui.button("Done picking").clicked() {
                        self.end_crop_pick();
                    }
                } else if ui.button("Pick elements…").clicked() {
                    self.start_crop_pick(id);
                }
                if n > 0 && ui.button("Clear").clicked() {
                    c.scope = CropScope::Only(vec![]);
                    label = Some("Crop chosen elements");
                }
            });
        }
        ui.add_space(4.0);
        let num = |ui: &mut egui::Ui, text: &str, v: &mut f64| -> bool {
            ui.horizontal(|ui| {
                ui.label(text);
                ui.add(crate::units::drag(v).range(-1e5..=1e5)).changed()
            })
            .inner
        };
        egui::Grid::new(("crop extents", id))
            .num_columns(2)
            .show(ui, |ui| {
                for (axis, name) in ["X", "Y", "Z"].iter().enumerate() {
                    let (mut lo, mut hi) = (c.min[axis], c.max[axis]);
                    if num(ui, &format!("{name} from"), &mut lo) {
                        c.set_face(axis * 2, lo);
                        label = Some("Crop box extents");
                    }
                    if num(ui, "to", &mut hi) {
                        c.set_face(axis * 2 + 1, hi);
                        label = Some("Crop box extents");
                    }
                    ui.end_row();
                }
            });
        let s = c.size();
        ui.label(
            egui::RichText::new(format!(
                "{} × {} × {}",
                crate::units::len(s.x),
                crate::units::len(s.y),
                crate::units::len(s.z)
            ))
            .weak(),
        );
        ui.horizontal(|ui| {
            let fit = |me: &Self, c: &mut CropBox| -> bool {
                let only = match &c.scope {
                    CropScope::Only(ids) if !ids.is_empty() => Some(ids.clone()),
                    _ => None,
                };
                match me.shown_bounds(only.as_deref()) {
                    Some((lo, hi)) => {
                        c.min = lo - DVec3::splat(MARGIN);
                        c.max = hi + DVec3::splat(MARGIN);
                        true
                    }
                    None => false,
                }
            };
            let what = if n > 0 {
                "Fit to its elements"
            } else {
                "Fit to the model"
            };
            if ui.button(what).clicked() && fit(self, c) {
                label = Some("Fit crop box");
            }
        });
        label
    }

    fn start_crop_pick(&mut self, id: ElementId) {
        self.crop_pick = Some(id);
        let picked = match self.crop_box(id).map(|(_, c)| &c.scope) {
            Some(CropScope::Only(ids)) => ids.clone(),
            _ => vec![],
        };
        self.selection.clear();
        self.selection.extend(picked, false);
        self.status = "Click elements in the 3D view to crop them with this box, again to leave them out. Esc or right-click when done.".into();
    }

    fn end_crop_pick(&mut self) {
        if let Some(id) = self.crop_pick.take() {
            self.selection.only(id);
            let n = match self.crop_box(id).map(|(_, c)| &c.scope) {
                Some(CropScope::Only(ids)) => ids.len(),
                _ => 0,
            };
            self.status = format!("The crop box crops {n} chosen elements; the rest stay whole.");
        }
    }

    /// Over Properties while a crop box's elements are being picked.
    pub(super) fn crop_pick_banner(&mut self, ui: &mut egui::Ui) {
        let Some((name, n)) = self.crop_pick.and_then(|id| {
            let (e, c) = self.crop_box(id)?;
            let n = match &c.scope {
                CropScope::Only(ids) => ids.len(),
                CropScope::All => 0,
            };
            Some((e.name.clone(), n))
        }) else {
            return;
        };
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(egui::RichText::new(format!("Picking what {name} crops")).strong());
            ui.label(format!(
                "{n} chosen. Click elements in the 3D view to add them, or click one again to leave it whole."
            ));
            if ui.button("Done picking").clicked() {
                self.end_crop_pick();
            }
        });
        ui.add_space(6.0);
    }

    /// While picking a crop box's elements: a click adds or takes away
    /// what it lands on, Esc or right-click ends. `true` while picking.
    pub(super) fn crop_pick_3d(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        ray: &dyn Fn(egui::Pos2) -> (DVec3, DVec3),
    ) -> bool {
        let Some(id) = self.crop_pick else {
            return false;
        };
        if self.crop_box(id).is_none() {
            self.crop_pick = None;
            return false;
        }
        if response.dragged_by(egui::PointerButton::Primary) {
            self.orbit_3d(response.drag_delta());
        }
        let esc = response.hovered() && ui.input(|i| i.key_pressed(egui::Key::Escape));
        if esc || response.secondary_clicked() {
            self.end_crop_pick();
            return true;
        }
        if response.clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                let (o, d) = ray(p);
                // what shows, as cropped: what is cut away can't be clicked
                if let Some(h) = pick::pick(&self.scene, o.as_vec3(), d.as_vec3()) {
                    // a door or window is cropped with its wall
                    let hit = match self.doc.model().get(h.id).map(|e| &e.kind) {
                        Some(ElementKind::Opening(op)) => op.host,
                        _ => h.id,
                    };
                    let name = self
                        .doc
                        .model()
                        .get(hit)
                        .map_or(String::new(), |e| e.name.clone());
                    let mut added = false;
                    let r = self.doc.edit(id, "Crop box elements", |e| {
                        if let ElementKind::CropBox(c) = &mut e.kind {
                            let mut ids = match std::mem::take(&mut c.scope) {
                                CropScope::Only(ids) => ids,
                                CropScope::All => vec![],
                            };
                            match ids.iter().position(|i| *i == hit) {
                                Some(at) => {
                                    ids.remove(at);
                                }
                                None => {
                                    ids.push(hit);
                                    added = true;
                                }
                            }
                            c.scope = CropScope::Only(ids);
                        }
                    });
                    if added {
                        self.selection.extend([hit], true);
                    } else {
                        self.selection.retain(|s| s != hit);
                    }
                    self.report(
                        r,
                        if added {
                            format!("{name} is cropped by the box. Esc or right-click when done.")
                        } else {
                            format!("{name} left whole. Esc or right-click when done.")
                        },
                    );
                }
            }
        }
        true
    }

    /// Draw the frames of the crop boxes the 3D window shows, with their
    /// names; the selected one stands out (its handles come with it).
    pub(super) fn draw_crop_frames(
        &self,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
    ) {
        let owner = self.crop_owner();
        for (e, c) in cropbox::crop_boxes(self.doc.model(), owner) {
            let selected = self.selection.contains(e.id) || self.crop_pick == Some(e.id);
            if c.frame_hidden && !selected {
                continue;
            }
            let color = match (selected, c.enabled) {
                (true, _) => FRAME_SELECTED,
                (false, true) => FRAME,
                (false, false) => Color32::from_gray(140),
            };
            let stroke = egui::Stroke::new(if selected { 2.0 } else { 1.3 }, color);
            for (a, b) in c.edges() {
                let (Some(pa), Some(pb)) = (project(a), project(b)) else {
                    continue;
                };
                if c.enabled {
                    painter.line_segment([pa, pb], stroke);
                } else {
                    painter.add(egui::Shape::dashed_line(&[pa, pb], stroke, 6.0, 4.0));
                }
            }
            if let Some(at) = project(c.max) {
                let text = if c.enabled {
                    e.name.clone()
                } else {
                    format!("{} (off)", e.name)
                };
                painter.text(
                    at + Vec2::new(4.0, -4.0),
                    egui::Align2::LEFT_BOTTOM,
                    text,
                    egui::FontId::proportional(11.0),
                    color,
                );
            }
        }
    }

    /// The crop box whose frame is near screen point `p`, to click it.
    pub(super) fn crop_frame_near(
        &self,
        p: egui::Pos2,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
    ) -> Option<ElementId> {
        cropbox::crop_boxes(self.doc.model(), self.crop_owner())
            .into_iter()
            .filter(|(_, c)| !c.frame_hidden)
            .filter_map(|(e, c)| {
                let d = c
                    .edges()
                    .iter()
                    .filter_map(|(a, b)| {
                        let (a, b) = (project(*a)?, project(*b)?);
                        let ab = b - a;
                        let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                        Some((a + ab * t).distance(p))
                    })
                    .fold(f32::MAX, f32::min);
                (d < 6.0).then_some((e.id, d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_boxes_are_added_toggled_and_undone_from_the_app() {
        let mut app = CreaBimApp::blank(None);
        app.load_model(sample_house());
        let n = app.doc.model().len();
        app.add_crop_box(false, false);
        assert_eq!(app.doc.model().len(), n + 1);
        let id = app.selection.active.unwrap();
        let (e, c) = app.crop_box(id).unwrap();
        assert_eq!(e.name, "Crop box 1");
        assert_eq!(c.view, None, "the 3D window's own");
        assert!(c.enabled && c.scope == CropScope::All);
        // the 3D window's scene is cropped once the box is pulled in
        app.edit_crop(id, "Crop box extents", |c| c.max.z = c.min.z + 1.5);
        app.scene = build_scene(app.doc.model());
        let edges = app.crop_scene_3d();
        assert!(!edges.is_empty(), "the cut walls show their cut edges");
        let top = app
            .scene
            .iter()
            .filter_map(|s| s.mesh.bounds())
            .map(|b| b.1.z as f64)
            .fold(f64::MIN, f64::max);
        let (_, c) = app.crop_box(id).unwrap();
        assert!(top <= c.max.z + 1e-4, "nothing above the box: {top}");
        // switched off it crops nothing
        assert!(app.crop_action("crop.toggle", &id.0.to_string()));
        app.scene = build_scene(app.doc.model());
        assert!(app.crop_scene_3d().is_empty());
        assert!(app.doc.undo());
        assert!(app.crop_box(id).unwrap().1.enabled);
        // a box for the selection only, and a 3D view's own boxes
        let wall = app
            .doc
            .model()
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Wall(_)))
            .unwrap()
            .id;
        app.selection.only(wall);
        app.add_crop_box(true, true);
        let only = app.selection.active.unwrap();
        assert_eq!(
            app.crop_box(only).unwrap().1.scope,
            CropScope::Only(vec![wall])
        );
        let view = app
            .doc
            .model()
            .iter()
            .find(|e| matches!(&e.kind, ElementKind::View(v) if matches!(v.kind, ViewKind::Axonometric { .. })))
            .unwrap()
            .id;
        app.selection.clear();
        app.active_view = Some(view);
        assert_eq!(app.crop_owner(), Some(view));
        app.add_crop_box(false, false);
        let mine = app.selection.active.unwrap();
        assert_eq!(app.crop_box(mine).unwrap().1.view, Some(view));
        assert_eq!(app.crop_box(mine).unwrap().0.name, "Crop box 1");
    }
}
