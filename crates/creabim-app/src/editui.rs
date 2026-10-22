//! Edit mode in the app: opening the selected element for editing and
//! closing it again (see [`crate::editmode`] for its grips).

use super::*;

impl CreaBimApp {
    /// Open the selected element in Edit mode, or close it.
    pub(super) fn toggle_edit_mode(&mut self) {
        if self.plan.edit.take().is_some() {
            self.status = "Finished editing.".into();
            return;
        }
        let Some(id) = self.selection.active.filter(|_| self.selection.len() == 1) else {
            self.status = "Select one element to edit it.".into();
            return;
        };
        if self.plan.tool != Tool::Select {
            self.plan.tool = Tool::Select;
        }
        self.status = editmode::enter(self.doc.model(), id, &mut self.plan.edit);
    }

    /// Edit mode ends when its element is no longer the one selected, is
    /// gone, or a drawing tool is picked.
    pub(super) fn keep_edit_mode(&mut self) {
        let Some(id) = self.plan.edit else { return };
        if self.selection.active != Some(id)
            || self.selection.len() != 1
            || self.plan.tool != Tool::Select
            || self.doc.model().get(id).is_none()
        {
            self.plan.edit = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> CreaBimApp {
        let mut app = CreaBimApp::blank(None);
        app.load_model(creabim_core::sample::typed_house());
        app
    }

    fn shaft(app: &CreaBimApp) -> ElementId {
        app.doc
            .model()
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Shaft(_)))
            .unwrap()
            .id
    }

    #[test]
    fn edit_mode_opens_on_the_selection_and_ends_when_it_changes() {
        let mut app = app();
        // nothing selected: nothing to edit
        app.toggle_edit_mode();
        assert_eq!(app.plan.edit, None);
        let id = shaft(&app);
        app.selection.only(id);
        app.toggle_edit_mode();
        assert_eq!(app.plan.edit, Some(id));
        assert!(app.status.contains("Editing"), "{}", app.status);
        // Esc steps out of it, keeping the selection
        assert!(app.plan.step_back());
        assert_eq!(app.plan.edit, None);
        assert!(app.selection.contains(id));
        // and selecting something else ends it
        app.toggle_edit_mode();
        let slab = app
            .doc
            .model()
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Slab(_)))
            .unwrap()
            .id;
        app.selection.only(slab);
        app.keep_edit_mode();
        assert_eq!(app.plan.edit, None);
    }

    #[test]
    fn a_shaft_shows_and_is_picked_on_every_plan_it_reaches() {
        let app = app();
        let m = app.doc.model();
        let id = shaft(&app);
        let ElementKind::Shaft(s) = &m.get(id).unwrap().kind else {
            unreachable!()
        };
        let (z0, z1) = s.span(m);
        let mut seen = 0;
        for (l, lv) in m.levels() {
            let plan = creabim_geom::plan_view(m, l.id, 1.2);
            let drawn = plan.items.iter().any(|it| it.id() == id);
            let reaches = lv.elevation >= z0 - 1e-6 && lv.elevation <= z1 + 1e-6;
            assert_eq!(drawn, reaches, "{}", l.name);
            if drawn {
                seen += 1;
                // a click inside it takes the shaft, not the floor round it
                let c = s.boundary.iter().copied().sum::<glam::DVec2>() / s.boundary.len() as f64;
                assert_eq!(planview::hit_test(&plan.items, c, 0.05, 0.01), Some(id));
            }
        }
        // its own level and the one its top reaches
        assert!(seen >= 2, "{seen}");
    }

    #[test]
    fn the_3d_view_and_views_change_how_they_draw() {
        let mut app = app();
        let ctx = egui::Context::default();
        app.run_action(&ctx, "render3d:3");
        assert_eq!(app.shell.settings.render_3d, RenderMode::Wireframe);
        let items = app.menu_items("render3d");
        assert!(items.iter().any(|i| matches!(i,
            crate::ui_model::Item::Action { id, checked: Some(true), .. } if id == "render3d:3")));
        // a section draws in wireframe, on its sheets too
        let section = app
            .doc
            .model()
            .iter()
            .find(|e| matches!(&e.kind, ElementKind::View(v) if matches!(v.kind, ViewKind::Section { .. })))
            .unwrap()
            .id;
        app.active_view = Some(section);
        app.run_action(&ctx, "viewstyle:2");
        assert_eq!(app.view(section).unwrap().style, ViewStyle::Wireframe);
        // and shows the shaft passing through it, dashed, to click
        let d = creabim_geom::section::view_drawing(app.doc.model(), section).unwrap();
        let sh = shaft(&app);
        assert!(d.items.iter().any(|it| it.id() == sh));
    }
}
