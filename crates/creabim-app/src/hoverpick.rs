//! Pre-selection: the element under the pointer is highlighted before it
//! is clicked, and when several are under it (a beam inside a wall, a
//! column behind a slab edge) Tab cycles which one a click takes. A small
//! badge by the pointer counts them; clicking it lists them to pick from.

use creabim_core::{ElementId, ElementKind, Model};
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};

/// The pre-selection colour: a quiet blue, apart from the orange of the
/// selection.
pub const HOVER: Color32 = Color32::from_rgb(40, 130, 230);

/// Where the pointer is reading elements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    /// A drawing view (plan, section, elevation, 3D view), on its own or
    /// worked on through a sheet.
    View(ElementId),
    /// The 3D window.
    Window3d,
    /// A sheet's viewports.
    Sheet(ElementId),
}

/// What is under the pointer in one pane.
#[derive(Clone, Debug, PartialEq)]
pub struct HoverPick {
    pub pane: Pane,
    /// The pointer, where `under` was read.
    pub at: Pos2,
    /// Everything under it, the one a plain click takes first.
    pub under: Vec<ElementId>,
    /// The one Tab has moved to: highlighted, and what a click takes.
    pub index: usize,
    /// The model revision `under` was read at.
    pub revision: u64,
    /// The list of `under` is open.
    pub list: bool,
    /// The badge (and the list when open) as drawn last frame.
    pub badge: Option<Rect>,
}

impl HoverPick {
    /// The element a click here takes.
    pub fn current(&self) -> Option<ElementId> {
        self.under.get(self.index).copied()
    }

    /// The element a click at `p` in `pane` takes, when it is where the
    /// pointer was read.
    pub fn at(&self, pane: Pane, p: Pos2) -> Option<ElementId> {
        (self.pane == pane && self.at.distance(p) < 4.0)
            .then(|| self.current())
            .flatten()
    }

    /// Is `p` over the badge, its list, or on the way from the pointer
    /// to them? The pointer moving there keeps what was read.
    fn keeps(&self, p: Pos2) -> bool {
        // only moving down and right, toward the badge, keeps it
        self.badge.is_some_and(|b| {
            b.expand(4.0).contains(p)
                || (p.x > self.at.x + 1.0
                    && p.y > self.at.y + 1.0
                    && Rect::from_two_pos(self.at, b.left_top())
                        .expand(4.0)
                        .contains(p))
        })
    }
}

/// Keep the pre-selection of `pane` up to date. `pointer` is where the
/// pointer is over the pane (none when it is elsewhere, or busy with a
/// drag or a tool); `read` lists what is under a point. Tab and Shift+Tab
/// cycle what is under it. Returns a status line when Tab moved.
pub fn track(
    ctx: &egui::Context,
    slot: &mut Option<HoverPick>,
    pane: Pane,
    pointer: Option<Pos2>,
    revision: u64,
    model: &Model,
    read: impl FnOnce(Pos2) -> Vec<ElementId>,
) -> Option<String> {
    let mine = slot.as_ref().is_some_and(|h| h.pane == pane);
    let Some(p) = pointer else {
        // gone from the pane: keep it only for the badge and its list
        let over_badge = ctx.pointer_hover_pos().is_some_and(|p| {
            slot.as_ref()
                .is_some_and(|h| h.badge.is_some_and(|b| b.contains(p)))
        });
        if mine && !slot.as_ref().is_some_and(|h| h.list) && !over_badge {
            *slot = None;
        }
        return None;
    };
    let fresh = slot.as_ref().is_some_and(|h| {
        h.pane == pane && h.revision == revision && (h.at.distance(p) < 0.5 || h.keeps(p))
    });
    if !fresh {
        let under = read(p);
        match slot {
            // the same stack under a pointer that moved keeps Tab's choice
            Some(h) if h.pane == pane && h.under == under => {
                h.at = p;
                h.revision = revision;
                h.list = false;
            }
            _ => {
                *slot = (!under.is_empty()).then_some(HoverPick {
                    pane,
                    at: p,
                    under,
                    index: 0,
                    revision,
                    list: false,
                    badge: None,
                });
            }
        }
    }
    let h = slot.as_mut()?;
    // (the pane itself may hold egui's focus; only typing keeps Tab)
    if h.under.len() < 2 || ctx.text_edit_focused() {
        return None;
    }
    let (next, back) = ctx.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::NONE, egui::Key::Tab),
            i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab),
        )
    });
    if !next && !back {
        return None;
    }
    // Tab is ours here, not egui's to move focus between widgets with
    ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
        m.move_focus(egui::FocusDirection::None);
    });
    let n = h.under.len();
    h.index = if next {
        (h.index + 1) % n
    } else {
        (h.index + n - 1) % n
    };
    let id = h.current()?;
    Some(format!(
        "{} ({} of {n} under the pointer). Click to select it, Tab for the next.",
        describe(model, id),
        h.index + 1
    ))
}

/// What to call an element in the list: its kind and name.
pub fn describe(model: &Model, id: ElementId) -> String {
    let Some(e) = model.get(id) else {
        return "Gone".into();
    };
    let kind = match &e.kind {
        ElementKind::Opening(o) => match o.kind {
            creabim_core::OpeningKind::Door => "Door",
            creabim_core::OpeningKind::Window => "Window",
        },
        ElementKind::FreeMesh(m) => match m.ifc_class.as_str() {
            "IfcBeam" => "Beam",
            "IfcColumn" => "Column",
            "IfcMember" => "Member",
            _ => "Mass",
        },
        ElementKind::Grid(_) => "Grid line",
        ElementKind::Viewport(_) => "View on sheet",
        k => k.name(),
    };
    if e.name.is_empty() || e.name == kind {
        kind.to_string()
    } else {
        format!("{kind} · {}", e.name)
    }
}

/// The badge by the pointer counting what is under it, for `pane`, and
/// its list once clicked. Returns the element picked from the list.
pub fn badge(
    ctx: &egui::Context,
    slot: &mut Option<HoverPick>,
    pane: Pane,
    model: &Model,
) -> Option<ElementId> {
    let h = slot.as_mut().filter(|h| h.pane == pane)?;
    if h.under.len() < 2 {
        h.badge = None;
        return None;
    }
    let n = h.under.len();
    let pos = h.at + Vec2::new(14.0, 14.0);
    let mut picked = None;
    let mut toggle = false;
    let mut hovered_row = None;
    let area = egui::Area::new(egui::Id::new("hoverpick-badge"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .interactable(true)
        .show(ctx, |ui| {
            let fill = Color32::from_rgba_unmultiplied(255, 255, 255, 235);
            egui::Frame::new()
                .fill(fill)
                .stroke(egui::Stroke::new(1.0, HOVER.gamma_multiply(0.7)))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    let text = egui::RichText::new(format!("{}/{n} · Tab", h.index + 1))
                        .size(11.0)
                        .color(HOVER);
                    let r = ui
                        .add(egui::Label::new(text).sense(egui::Sense::click()))
                        .on_hover_text(format!(
                            "{n} elements under the pointer. Tab cycles them; click to list them."
                        ));
                    if r.clicked() {
                        toggle = true;
                    }
                    if h.list {
                        ui.add_space(2.0);
                        for (i, id) in h.under.iter().enumerate() {
                            let label = describe(model, *id);
                            let row = ui.selectable_label(
                                i == h.index,
                                egui::RichText::new(label)
                                    .size(12.0)
                                    .color(Color32::from_gray(35)),
                            );
                            if row.hovered() {
                                hovered_row = Some(i);
                            }
                            if row.clicked() {
                                picked = Some(*id);
                            }
                        }
                    }
                });
        });
    h.badge = Some(area.response.rect);
    if let Some(i) = hovered_row {
        h.index = i;
    }
    if toggle {
        h.list = !h.list;
    }
    if picked.is_some() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        h.list = false;
    }
    picked
}

/// The element highlighted as about to be picked, wherever the pointer is
/// reading it: every view shows it, not only the one under the pointer.
pub fn highlighted(slot: &Option<HoverPick>) -> Option<ElementId> {
    slot.as_ref().and_then(|h| h.current())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_cycles_what_is_under_the_pointer_and_a_move_keeps_the_choice() {
        let ctx = egui::Context::default();
        let model = Model::default();
        let (a, b, c) = (ElementId::new(), ElementId::new(), ElementId::new());
        let read_two = |_: Pos2| vec![a, b];
        let mut slot = None;
        let pane = Pane::View(ElementId::new());
        let tab = |shift: bool| egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: if shift {
                    egui::Modifiers::SHIFT
                } else {
                    egui::Modifiers::NONE
                },
            }],
            ..Default::default()
        };
        let p = Pos2::new(10.0, 10.0);
        let mut said = None;
        ctx.run_ui(tab(false), |ui| {
            said = track(ui.ctx(), &mut slot, pane, Some(p), 1, &model, read_two);
        })
        .textures_delta
        .clear();
        let h = slot.clone().unwrap();
        assert_eq!(h.current(), Some(b), "Tab moved to the second");
        assert!(said.unwrap().contains("2 of 2"));
        // the pointer moves over the same two: Tab's choice stays
        ctx.run_ui(Default::default(), |ui| {
            track(
                ui.ctx(),
                &mut slot,
                pane,
                Some(p + Vec2::new(30.0, 0.0)),
                1,
                &model,
                read_two,
            );
        })
        .textures_delta
        .clear();
        assert_eq!(slot.as_ref().unwrap().current(), Some(b));
        assert_eq!(
            slot.as_ref().unwrap().at(pane, p + Vec2::new(30.0, 1.0)),
            Some(b)
        );
        // Shift+Tab goes back
        ctx.run_ui(tab(true), |ui| {
            track(
                ui.ctx(),
                &mut slot,
                pane,
                Some(p + Vec2::new(30.0, 0.0)),
                1,
                &model,
                read_two,
            );
        })
        .textures_delta
        .clear();
        assert_eq!(slot.as_ref().unwrap().current(), Some(a));
        // over something else, it starts again from the top
        ctx.run_ui(Default::default(), |ui| {
            track(
                ui.ctx(),
                &mut slot,
                pane,
                Some(Pos2::new(300.0, 300.0)),
                1,
                &model,
                |_| vec![c],
            );
        })
        .textures_delta
        .clear();
        assert_eq!(slot.as_ref().unwrap().current(), Some(c));
        // leaving the pane drops it
        ctx.run_ui(Default::default(), |ui| {
            track(ui.ctx(), &mut slot, pane, None, 1, &model, read_two);
        })
        .textures_delta
        .clear();
        assert!(slot.is_none());
    }
}
