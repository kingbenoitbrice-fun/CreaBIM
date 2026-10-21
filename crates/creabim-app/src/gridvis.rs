//! Grid systems and per-view visibility in the app: the grid groups panel
//! and menus, grid lines with their bubbles in the 3D view, and what each
//! view (and the 3D view) leaves out.
//!
//! What a view leaves out is stored on the view ([`ViewHidden`]) and edited
//! through ordinary transactions, so it undoes, saves, and carries over to
//! the sheets the view is placed on. The 3D view is not a view element: the
//! categories it leaves out are kept in the user's settings, the single
//! elements it hides for the session.
//!
//! Each Visibility menu and panel starts with quick switches for the 2D
//! objects (grids, annotation, guides, sketches), the ones most often in
//! the way while modelling.

use super::*;
use crate::ui_model::Item;
use creabim_core::grids::{self, GridGroup, LabelStyle};
use creabim_core::{Category, Grid, ViewHidden};
use serde_json::{json, Value};
use std::collections::HashSet;

/// A change to what a view leaves out, picked in a panel.
type HiddenEdit = Box<dyn FnOnce(&mut ViewHidden)>;

/// Where visibility changes go: a view element, or the 3D view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum VisTarget {
    View(ElementId),
    Window3d,
}

impl VisTarget {
    /// The prefix of its menu actions.
    fn prefix(self) -> &'static str {
        match self {
            VisTarget::View(_) => "vis",
            VisTarget::Window3d => "vis3",
        }
    }
}

/// The quick switches of a Visibility menu: groups of categories, by key.
pub(super) const QUICK: [(&str, &str, &str, &[Category]); 4] = [
    ("grids", "Grids", "grid-3x3", &[Category::Grids]),
    (
        "annotation",
        "Annotation",
        "tag",
        &[
            Category::Dimensions,
            Category::Tags,
            Category::Notes,
            Category::Levels,
            Category::SectionMarks,
        ],
    ),
    ("guides", "Guides", "slash", &[Category::Guides]),
    ("sketches", "Sketches", "pen-tool", &[Category::Sketches]),
];

/// What a quick switch covers, with "all" for every 2D object.
fn quick_group(key: &str, in_3d: bool) -> Vec<Category> {
    if key == "all" {
        let mut all: Vec<Category> = QUICK.iter().flat_map(|q| q.3.iter().copied()).collect();
        if in_3d {
            all.push(Category::Openings);
        }
        return all;
    }
    if key == "cutters" {
        return vec![Category::Openings];
    }
    QUICK
        .iter()
        .find(|q| q.0 == key)
        .map(|q| q.3.to_vec())
        .unwrap_or_default()
}

/// The hint of a quick switch.
fn quick_hint(key: &str) -> &'static str {
    match key {
        "annotation" => "Dimensions, tags, notes, level lines and section marks",
        "guides" => "Construction guides; Shift+G hides or shows them in every view",
        "grids" => "Grid lines and their bubbles",
        _ => "",
    }
}

/// How far a grid bubble sits past the end of its line in 3D, metres.
const BUBBLE_OUT_3D: f64 = 1.0;
/// A grid bubble's radius in the 3D view, points, for one or two characters.
const BUBBLE_R_3D: f32 = 11.0;

pub(super) fn color32(c: [f32; 3]) -> Color32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(b(c[0]), b(c[1]), b(c[2]))
}

impl CreaBimApp {
    // ---- what a view leaves out -----------------------------------------

    /// What `t` leaves out now.
    pub(super) fn view_hidden(&self, t: VisTarget) -> Option<ViewHidden> {
        match t {
            VisTarget::View(id) => self.view(id).map(|v| v.hidden.clone()),
            VisTarget::Window3d => Some(self.vis3d.clone()),
        }
    }

    /// Change what `t` leaves out. For a view it is one undo step.
    pub(super) fn set_view_hidden(
        &mut self,
        t: VisTarget,
        label: &str,
        f: impl FnOnce(&mut ViewHidden),
    ) {
        match t {
            VisTarget::View(id) => {
                let r = self.doc.edit(id, label, |e| {
                    if let ElementKind::View(v) = &mut e.kind {
                        f(&mut v.hidden);
                    }
                });
                let name = self.doc.model().get(id).map_or("", |e| e.name.as_str());
                let msg = format!("{label} in {name}.");
                self.report(r, msg);
            }
            VisTarget::Window3d => {
                f(&mut self.vis3d);
                self.hidden_rev += 1;
                self.snaps_3d = None;
                self.status = format!("{label} in the 3D view.");
                // the categories are kept for next time; single elements
                // are for the session
                let s = &mut self.shell.settings;
                if s.hidden_3d != self.vis3d.categories {
                    s.hidden_3d = self.vis3d.categories.clone();
                    s.store();
                }
            }
        }
    }

    /// The view a right-click "hide in view" or the Visibility menu acts on.
    pub(super) fn vis_target(&self, in_3d: bool) -> Option<VisTarget> {
        if in_3d {
            return Some(VisTarget::Window3d);
        }
        let id = self.active_view?;
        match &self.view(id)?.kind {
            ViewKind::Schedule { .. } => None,
            _ => Some(VisTarget::View(id)),
        }
    }

    /// Elements the 3D view leaves out by its settings, and the guides
    /// when they are hidden everywhere.
    pub(super) fn hidden_in_3d(&self) -> HashSet<ElementId> {
        let mut out = self.vis3d.hidden_set(self.doc.model());
        if self.shell.settings.guides_hidden {
            out.extend(guide_ids(self.doc.model()));
        }
        out
    }

    /// Hide or show the guides in every view, keeping them.
    pub(super) fn set_guides_hidden(&mut self, hidden: bool) {
        let s = &mut self.shell.settings;
        s.guides_hidden = hidden;
        s.store();
        self.hidden_rev += 1;
        self.snaps_3d = None;
        let n = guide_ids(self.doc.model()).len();
        self.status = if hidden {
            format!("Guides hidden in every view ({n} kept). Shift+G or View > Guides brings them back.")
        } else {
            format!("Guides shown ({n}). Shift+G hides them again.")
        };
    }

    /// Turn the 3D view's X-ray on or off.
    pub(super) fn set_xray(&mut self, on: bool) {
        let s = &mut self.shell.settings;
        s.xray = on;
        s.store();
        self.geometry_key = None;
        self.status = if on {
            "X-ray: walls, slabs, roofs and masses are see-through, so beams, columns and what is inside them show. Alt+Z turns it off.".into()
        } else {
            "X-ray off.".into()
        };
    }

    /// Does the view show elements hidden inside others, dashed?
    pub(super) fn shows_hidden_elements(&self, id: ElementId) -> bool {
        self.view(id).is_some_and(|v| v.show_hidden)
    }

    /// Show or stop showing, dashed, what is hidden inside other elements in
    /// view `id`. One undo step.
    pub(super) fn toggle_hidden_elements(&mut self, id: ElementId) {
        let on = !self.shows_hidden_elements(id);
        let label = if on {
            "Show hidden elements"
        } else {
            "Stop showing hidden elements"
        };
        let r = self.doc.edit(id, label, |e| {
            if let ElementKind::View(v) = &mut e.kind {
                v.show_hidden = on;
            }
        });
        self.report(
            r,
            if on {
                "Elements inside others (a beam in a wall) show dashed in this view and on its sheets."
            } else {
                "Hidden elements are left out again."
            },
        );
    }

    /// Hide the selection in `t`: the elements themselves, their
    /// categories, or the grid groups of the selected grid lines.
    pub(super) fn hide_selected_in(&mut self, t: VisTarget, how: &str) {
        let ids = self.selection.all();
        if ids.is_empty() {
            self.status = "Select something to hide first.".into();
            return;
        }
        let model = self.doc.model();
        let cutters = creabim_core::visibility::cutters(model);
        let cats: Vec<Category> = ids
            .iter()
            .filter_map(|id| Category::of(model.get(*id)?, &cutters))
            .fold(vec![], |mut v, c| {
                if !v.contains(&c) {
                    v.push(c);
                }
                v
            });
        let groups: Vec<ElementId> = ids
            .iter()
            .filter_map(|id| match &model.get(*id)?.kind {
                ElementKind::Grid(g) => g.group,
                ElementKind::GridGroup(_) => Some(*id),
                _ => None,
            })
            .collect();
        match how {
            "category" if !cats.is_empty() => {
                let names: Vec<&str> = cats.iter().map(|c| c.label()).collect();
                let label = format!("Hide {}", names.join(", "));
                self.set_view_hidden(t, &label, |h| {
                    for c in cats {
                        h.set_category(c, true);
                    }
                });
            }
            "group" if !groups.is_empty() => {
                self.set_view_hidden(t, "Hide grid system", |h| {
                    for g in groups {
                        h.set_grid_group(g, true);
                    }
                });
            }
            "element" => {
                let label = match ids.len() {
                    1 => "Hide element".to_string(),
                    n => format!("Hide {n} elements"),
                };
                self.set_view_hidden(t, &label, |h| {
                    for id in ids {
                        h.set_element(id, true);
                    }
                });
            }
            _ => {
                self.status = "Nothing of that kind is selected.".into();
                return;
            }
        }
        self.selection.clear();
        self.face = None;
    }

    // ---- menus ---------------------------------------------------------

    /// The items of the Visibility menu of `t`.
    pub(super) fn visibility_items(&self, t: VisTarget) -> Vec<Item> {
        let Some(h) = self.view_hidden(t) else {
            return vec![];
        };
        let p = t.prefix();
        let model = self.doc.model();
        let in_3d = t == VisTarget::Window3d;
        let shown = |key: &str| {
            quick_group(key, in_3d)
                .iter()
                .all(|c| !h.hides_category(*c))
        };
        let mut items = vec![
            Item::Header(match t {
                VisTarget::View(_) => "This view shows".into(),
                VisTarget::Window3d => "The 3D view shows".into(),
            }),
            Item::new(format!("{p}.quick:all"), "All 2D objects")
                .checked(shown("all"))
                .icon("eye")
                .keep_open()
                .hint("Grids, annotation, guides and sketches together"),
        ];
        for (key, label, icon, _) in QUICK {
            items.push(
                Item::new(format!("{p}.quick:{key}"), label)
                    .checked(shown(key))
                    .icon(icon)
                    .keep_open()
                    .hint(quick_hint(key)),
            );
        }
        if in_3d {
            let s = &self.shell.settings;
            items.extend([
                Item::new(format!("{p}.quick:cutters"), "Void cutters")
                    .checked(shown("cutters"))
                    .icon("square-dashed")
                    .keep_open()
                    .hint("The outlines of elements used to cut voids into others"),
                Item::new("ground.grid", "Ground grid")
                    .checked(s.ground_grid)
                    .icon("grid-3x3")
                    .keep_open(),
                Item::new("plan3d", "The 2D view in 3D space")
                    .checked(self.plan_in_3d)
                    .icon("map")
                    .keep_open(),
                Item::Separator,
                Item::new("xray", "X-ray")
                    .key("Alt+Z")
                    .checked(s.xray)
                    .icon("xray")
                    .keep_open()
                    .hint(
                        "Walls, slabs, roofs and masses see-through, so what is inside them shows",
                    ),
            ]);
        } else if let VisTarget::View(id) = t {
            items.extend([
                Item::Separator,
                Item::new(
                    format!("vis.reveal:{}", id.0),
                    "Elements hidden inside others",
                )
                .checked(self.shows_hidden_elements(id))
                .icon("xray")
                .keep_open()
                .hint(
                    "Draw what is inside another element, like a beam in a wall, with dashed lines",
                ),
            ]);
        }
        if self.shell.settings.guides_hidden {
            items.push(
                Item::new("guides.toggle", "Guides are hidden everywhere: show them")
                    .key("Shift+G")
                    .icon("slash"),
            );
        }
        items.push(Item::Separator);
        items.push(Item::Header("Categories".into()));
        for (i, c) in Category::ALL.iter().enumerate() {
            items.push(
                Item::new(format!("{p}.cat:{i}"), c.label())
                    .checked(!h.hides_category(*c))
                    .keep_open(),
            );
        }
        let groups = grids::groups(model);
        if !groups.is_empty() {
            items.push(Item::Separator);
            items.push(Item::Header("Grid systems".into()));
            for (e, g) in groups {
                items.push(
                    Item::new(format!("{p}.grp:{}", e.id.0), &e.name)
                        .checked(!h.grid_groups.contains(&e.id))
                        .enabled(!h.hides_category(Category::Grids))
                        .hint(format!("Labels {}1, {}A…", g.prefix, g.prefix))
                        .keep_open(),
                );
            }
        }
        let hidden: Vec<&Element> = h.elements.iter().filter_map(|id| model.get(*id)).collect();
        if !hidden.is_empty() {
            items.push(Item::Separator);
            let mut list: Vec<Item> = hidden
                .iter()
                .map(|e| Item::new(format!("{p}.unhide:{}", e.id.0), format!("Show {}", e.name)))
                .collect();
            list.push(Item::Separator);
            list.push(Item::new(format!("{p}.unhideall"), "Show them all"));
            items.push(Item::sub(
                format!("Hidden elements ({})", hidden.len()),
                list,
            ));
        }
        items.push(Item::Separator);
        items.push(
            Item::new(format!("{p}.reset"), "Show everything")
                .icon("eye")
                .enabled(!h.is_empty()),
        );
        items
    }

    /// Right-click items that hide the selection in the view clicked.
    pub(super) fn hide_in_view_items(&self, in_3d: bool) -> Vec<Item> {
        if self.selection.is_empty() {
            return vec![];
        }
        let enabled = self.vis_target(in_3d).is_some();
        let where_ = if in_3d { "3D view" } else { "view" };
        let p = if in_3d { "vis3" } else { "vis" };
        let mut items = vec![
            Item::new(format!("{p}.hide:element"), format!("Hide in {where_}"))
                .icon("eye-off")
                .enabled(enabled)
                .hint("Leave the selection out of this view only; its Visibility settings bring it back"),
            Item::new(format!("{p}.hide:category"), format!("Hide category in {where_}"))
                .enabled(enabled)
                .hint("Leave out every element of the selection's kind (walls, windows…) in this view"),
        ];
        let model = self.doc.model();
        let grouped = self.selection.all().iter().any(|id| {
            matches!(
                model.get(*id).map(|e| &e.kind),
                Some(ElementKind::Grid(Grid { group: Some(_), .. }) | ElementKind::GridGroup(_))
            )
        });
        if grouped {
            items.push(
                Item::new(
                    format!("{p}.hide:group"),
                    format!("Hide grid system in {where_}"),
                )
                .enabled(enabled),
            );
        }
        items.push(Item::Separator);
        items
    }

    /// The grid systems dropdown of the Grid tool.
    pub(super) fn grid_group_items(&self) -> Vec<Item> {
        let model = self.doc.model();
        let active = self.active_grid_group();
        let mut items = vec![
            Item::Header("New grid lines go in".into()),
            Item::new("gg.active:", "No grid system").checked(active.is_none()),
        ];
        for (e, g) in grids::groups(model) {
            let n = grid_count(model, Some(e.id));
            items.push(
                Item::new(format!("gg.active:{}", e.id.0), &e.name)
                    .checked(active == Some(e.id))
                    .hint(format!(
                        "{n} lines, labelled {}{} and {}{}",
                        g.prefix,
                        g.vertical.format(1),
                        g.prefix,
                        g.horizontal.format(1)
                    )),
            );
        }
        items.push(Item::Separator);
        items.push(
            Item::new("gg.new", "New grid system")
                .icon("plus")
                .hint("A group of grid lines with its own labels (A-1, B-1…) and colour, e.g. for a second building"),
        );
        items.push(
            Item::new("gg.renumber", "Renumber the lines in order")
                .hint("West to east and south to north"),
        );
        let selected_lines = self
            .selection
            .all()
            .iter()
            .filter(|id| matches!(model.get(**id).map(|e| &e.kind), Some(ElementKind::Grid(_))))
            .count();
        if selected_lines > 0 {
            let mut to = vec![Item::new("gg.adopt:", "No grid system")];
            to.extend(
                grids::groups(model)
                    .into_iter()
                    .map(|(e, _)| Item::new(format!("gg.adopt:{}", e.id.0), &e.name)),
            );
            items.push(Item::sub(
                format!("Move the {selected_lines} selected lines to"),
                to,
            ));
        }
        let arg = active.map_or(String::new(), |g| g.0.to_string());
        items.push(Item::new(format!("gg.select:{arg}"), "Select its lines"));
        if let Some(g) = active {
            items.push(Item::new(
                format!("gg.edit:{}", g.0),
                "Edit the grid system…",
            ));
        }
        items
    }

    /// The active grid group, if it still exists.
    pub(super) fn active_grid_group(&self) -> Option<ElementId> {
        self.plan.grid_group.filter(|g| {
            matches!(
                self.doc.model().get(*g).map(|e| &e.kind),
                Some(ElementKind::GridGroup(_))
            )
        })
    }

    pub(super) fn new_grid_group(&mut self) {
        let e = grids::new_group(self.doc.model());
        let (id, name) = (e.id, e.name.clone());
        let r = self
            .doc
            .commit(Transaction::new(format!("New grid system {name}")).insert(e));
        if r.is_ok() {
            self.plan.grid_group = Some(id);
            self.plan.tool = Tool::Grid;
            self.selection.only(id);
            self.face = None;
        }
        self.report(
            r,
            format!("New grid system {name}: grid lines you draw now go in it."),
        );
    }

    /// Put the selected grid lines in `group` (`None`: out of any). They
    /// keep their labels unless one is taken there already.
    pub(super) fn move_selected_grids(&mut self, group: Option<ElementId>) {
        let model = self.doc.model();
        let mut tx = Transaction::new("Move grid lines to another system");
        let mut probe = model.clone();
        for id in self.selection.all() {
            let Some(e) = probe.get(id).cloned() else {
                continue;
            };
            let ElementKind::Grid(g) = &e.kind else {
                continue;
            };
            if g.group == group {
                continue;
            }
            let taken = probe.iter().any(|o| {
                o.id != id
                    && matches!(&o.kind, ElementKind::Grid(og) if og.group == group && og.label == g.label)
            });
            let mut after = e.clone();
            if let ElementKind::Grid(ag) = &mut after.kind {
                if taken {
                    ag.label = grids::next_label(&probe, group, ag.start, ag.end);
                }
                ag.group = group;
            }
            let step = Transaction::new("").replace(e.clone(), after.clone());
            if step.apply(&mut probe).is_ok() {
                tx = tx.replace(e, after);
            }
        }
        if tx.ops.is_empty() {
            self.status = "Select grid lines to move first.".into();
            return;
        }
        let n = tx.ops.len();
        let r = self.doc.commit(tx);
        self.report(r, format!("Moved {n} grid lines."));
    }

    /// Menu choices from this module's menus. False when `head` is not one.
    pub(super) fn grid_vis_action(&mut self, head: &str, arg: &str) -> bool {
        let model = self.doc.model();
        let by_uuid = || {
            model
                .iter()
                .find(|e| e.id.0.to_string() == arg)
                .map(|e| e.id)
        };
        let index = arg.parse::<usize>().ok();
        let (prefix, what) = head.split_once('.').unwrap_or((head, ""));
        match prefix {
            "vis" | "vis3" if what == "hide" => {
                if let Some(t) = self.vis_target(prefix == "vis3") {
                    self.hide_selected_in(t, arg);
                }
                true
            }
            "vis" | "vis3" => {
                let Some(t) = self.vis_target(prefix == "vis3") else {
                    self.status = "Open a plan, section or 3D view first.".into();
                    return true;
                };
                let Some(h) = self.view_hidden(t) else {
                    return true;
                };
                let id = by_uuid();
                match what {
                    "quick" => {
                        let group = quick_group(arg, t == VisTarget::Window3d);
                        if group.is_empty() {
                            return true;
                        }
                        let hide = group.iter().all(|c| !h.hides_category(*c));
                        let name = match QUICK.iter().find(|q| q.0 == arg) {
                            Some(q) => q.1.to_lowercase(),
                            None if arg == "cutters" => "void cutters".into(),
                            None => "2D objects".into(),
                        };
                        let label = format!("{} {name}", if hide { "Hide" } else { "Show" });
                        self.set_view_hidden(t, &label, |h| {
                            for c in group {
                                h.set_category(c, hide);
                            }
                        });
                        if arg == "all" && t == VisTarget::Window3d {
                            let s = &mut self.shell.settings;
                            s.ground_grid = !hide;
                            s.store();
                            self.geometry_key = None;
                        }
                    }
                    "reveal" => {
                        if let Some(v) = id {
                            self.toggle_hidden_elements(v);
                        }
                    }
                    "cat" => {
                        if let Some(c) = index.and_then(|i| Category::ALL.get(i)).copied() {
                            let hide = !h.hides_category(c);
                            let label =
                                format!("{} {}", if hide { "Hide" } else { "Show" }, c.label());
                            self.set_view_hidden(t, &label, |h| h.set_category(c, hide));
                        }
                    }
                    "grp" => {
                        if let Some(g) = id {
                            let hide = !h.grid_groups.contains(&g);
                            let label = if hide {
                                "Hide grid system"
                            } else {
                                "Show grid system"
                            };
                            self.set_view_hidden(t, label, |h| h.set_grid_group(g, hide));
                        }
                    }
                    "unhide" => {
                        if let Some(e) = id {
                            self.set_view_hidden(t, "Show hidden element", |h| {
                                h.set_element(e, false)
                            });
                        }
                    }
                    "unhideall" => {
                        self.set_view_hidden(t, "Show hidden elements", |h| h.elements.clear())
                    }
                    "reset" => {
                        self.set_view_hidden(t, "Show everything", |h| *h = ViewHidden::default())
                    }
                    _ => return false,
                }
                true
            }
            "gg" => {
                match what {
                    "active" => {
                        self.plan.grid_group = by_uuid();
                        self.plan.tool = Tool::Grid;
                        self.plan.cancel();
                        let name = self
                            .plan
                            .grid_group
                            .and_then(|g| self.doc.model().get(g))
                            .map_or("no grid system".to_string(), |e| e.name.clone());
                        self.status = format!("New grid lines go in {name}.");
                    }
                    "new" => self.new_grid_group(),
                    "adopt" => self.move_selected_grids(by_uuid()),
                    "renumber" => {
                        let g = self.active_grid_group();
                        let tx = grids::renumber(self.doc.model(), g);
                        if tx.ops.is_empty() {
                            self.status = "The lines are numbered in order already.".into();
                        } else {
                            let n = tx.ops.len();
                            let r = self.doc.commit(tx);
                            self.report(r, format!("Relabelled {n} grid lines in order."));
                        }
                    }
                    "select" => {
                        let ids = grid_ids(self.doc.model(), by_uuid());
                        self.selection.clear();
                        self.selection.extend(ids, false);
                        self.face = None;
                    }
                    "edit" => {
                        if let Some(g) = by_uuid() {
                            self.selection.only(g);
                            self.face = None;
                        }
                    }
                    _ => return false,
                }
                true
            }
            _ => false,
        }
    }

    /// Items for the top bar when they apply: the Grid tool's grid system,
    /// and the active view's Visibility menu.
    pub(super) fn grid_vis_options(&self) -> Vec<Value> {
        let mut out = vec![];
        if self.plan.tool == Tool::Grid {
            let name = self
                .active_grid_group()
                .and_then(|g| self.doc.model().get(g))
                .map_or("None".to_string(), |e| e.name.clone());
            out.push(json!({
                "kind": "menu", "key": "gridgroups", "label": "Grid system", "value": name,
                "icon": "grid-3x3",
                "hint": "Which grid system new grid lines go in, and its labels",
            }));
            out.push(json!({"kind": "sep"}));
        }
        if self.vis_target(false).is_some() {
            out.push(json!({
                "kind": "menu", "key": "visibility", "label": "", "value": "Visibility",
                "icon": "eye",
                "hint": "What this view shows: grids, annotation, guides, categories, grid systems and hidden elements. Sheets show their views the same way.",
            }));
        }
        out
    }

    /// Items of this module's menus, by key.
    pub(super) fn grid_vis_menu(&self, key: &str) -> Option<Vec<Item>> {
        Some(match key {
            "gridgroups" => self.grid_group_items(),
            "visibility" => self.visibility_items(self.vis_target(false)?),
            "vis3d" => self.visibility_items(VisTarget::Window3d),
            _ => return None,
        })
    }

    /// Set the 3D ground grid's spacing to a typed length, in any unit.
    /// Returns what went wrong when it can't be read.
    pub(super) fn set_ground_spacing_text(&mut self, text: &str) -> Result<f64, String> {
        let (lo, hi) = GROUND_SPACING_RANGE;
        let v = crate::units::parse(text)
            .filter(|v| v.is_finite() && *v > 0.0)
            .ok_or_else(|| format!("Couldn't read \"{text}\" as a length."))?;
        if !(lo..=hi).contains(&v) {
            return Err(format!(
                "Pick a spacing between {} and {}.",
                crate::units::len(lo),
                crate::units::len(hi)
            ));
        }
        let s = &mut self.shell.settings;
        s.ground_grid_spacing = v;
        s.ground_grid = true;
        s.store();
        self.geometry_key = None;
        self.status = format!("Ground grid every {}.", crate::units::len(v));
        Ok(v)
    }

    /// The little window the ground grid's "Custom…" spacing is typed in.
    pub(super) fn ground_spacing_window(&mut self, ctx: &egui::Context) {
        let Some(mut text) = self.shell.ground_custom.take() else {
            return;
        };
        let mut open = true;
        let mut done = false;
        let mut error = None;
        let area = self.mat.area.unwrap_or_else(|| ctx.content_rect());
        egui::Window::new("Ground grid spacing")
            .open(&mut open)
            .default_pos(area.min + Vec2::new(60.0, 80.0))
            .constrain_to(area)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Any spacing, in the project's units or another (1250, 1.25 m, 4'-0\").",
                    )
                    .weak(),
                );
                ui.horizontal(|ui| {
                    ui.label("Every");
                    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(110.0));
                    if !r.has_focus() && !r.lost_focus() && ui.memory(|m| m.focused().is_none()) {
                        r.request_focus();
                    }
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("Apply").clicked() || enter {
                        done = true;
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for s in GROUND_SPACINGS {
                        if ui.small_button(crate::units::len(s)).clicked() {
                            text = crate::units::len(s);
                            done = true;
                        }
                    }
                });
            });
        if done {
            match self.set_ground_spacing_text(&text) {
                Ok(_) => return,
                Err(e) => error = Some(e),
            }
        }
        if let Some(e) = error {
            self.status = e;
        }
        if open {
            self.shell.ground_custom = Some(text);
        }
    }

    // ---- properties panel ---------------------------------------------

    /// The Visibility section of a view's properties.
    pub(super) fn visibility_panel(&mut self, ui: &mut egui::Ui, t: VisTarget) {
        let Some(h) = self.view_hidden(t) else {
            return;
        };
        let mut change: Option<(String, HiddenEdit)> = None;
        // switches that are not the view's own: run as menu actions
        let mut extra: Option<&str> = None;
        let mut reveal = None;
        let title = if h.is_empty() {
            "Visibility".to_string()
        } else {
            let n = h.categories.len() + h.elements.len() + h.grid_groups.len();
            format!("Visibility ({n} hidden)")
        };
        egui::CollapsingHeader::new(title)
            .id_salt(("visibility", format!("{t:?}")))
            .default_open(t != VisTarget::Window3d)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(match t {
                        VisTarget::View(_) => "What this view shows. Sheets show it the same way.",
                        VisTarget::Window3d => "What the 3D view shows.",
                    })
                    .weak(),
                );
                // the 2D objects first, in groups
                let in_3d = t == VisTarget::Window3d;
                ui.horizontal_wrapped(|ui| {
                    for (key, label, _, _) in QUICK {
                        let group = quick_group(key, in_3d);
                        let mut shown = group.iter().all(|c| !h.hides_category(*c));
                        if ui
                            .checkbox(&mut shown, label)
                            .on_hover_text(quick_hint(key))
                            .changed()
                        {
                            let label = format!(
                                "{} {}",
                                if shown { "Show" } else { "Hide" },
                                label.to_lowercase()
                            );
                            change = Some((
                                label,
                                Box::new(move |h| {
                                    for c in group {
                                        h.set_category(c, !shown);
                                    }
                                }),
                            ));
                        }
                    }
                });
                if in_3d {
                    ui.horizontal_wrapped(|ui| {
                        let mut ground = self.shell.settings.ground_grid;
                        if ui.checkbox(&mut ground, "Ground grid").changed() {
                            extra = Some("ground.grid");
                        }
                        let mut xray = self.shell.settings.xray;
                        if ui
                            .checkbox(&mut xray, "X-ray")
                            .on_hover_text("Walls, slabs, roofs and masses see-through (Alt+Z)")
                            .changed()
                        {
                            extra = Some("xray");
                        }
                    });
                } else if let VisTarget::View(id) = t {
                    let mut on = self.shows_hidden_elements(id);
                    if ui
                        .checkbox(&mut on, "Elements hidden inside others, dashed")
                        .on_hover_text("A beam in a wall, a stud wall in a cavity: drawn with dashed hidden lines")
                        .changed()
                    {
                        reveal = Some(id);
                    }
                }
                let mut guides = !self.shell.settings.guides_hidden;
                if ui
                    .checkbox(&mut guides, "Guides in every view")
                    .on_hover_text("Hide all guides without deleting them, and bring them back (Shift+G)")
                    .changed()
                {
                    extra = Some("guides.toggle");
                }
                ui.add_space(2.0);
                ui.label(egui::RichText::new("Categories").strong());
                egui::Grid::new(("vis-cats", format!("{t:?}")))
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (i, c) in Category::ALL.iter().enumerate() {
                            let mut shown = !h.hides_category(*c);
                            if ui.checkbox(&mut shown, c.label()).changed() {
                                let c = *c;
                                let label = format!(
                                    "{} {}",
                                    if shown { "Show" } else { "Hide" },
                                    c.label()
                                );
                                change =
                                    Some((label, Box::new(move |h| h.set_category(c, !shown))));
                            }
                            if i % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
                let groups: Vec<(ElementId, String, [f32; 3])> = grids::groups(self.doc.model())
                    .into_iter()
                    .map(|(e, g)| (e.id, e.name.clone(), g.color))
                    .collect();
                if !groups.is_empty() {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Grid systems").strong());
                    ui.add_enabled_ui(!h.hides_category(Category::Grids), |ui| {
                        for (id, name, color) in groups {
                            ui.horizontal(|ui| {
                                swatch(ui, color);
                                let mut shown = !h.grid_groups.contains(&id);
                                if ui.checkbox(&mut shown, name).changed() {
                                    let label = if shown {
                                        "Show grid system"
                                    } else {
                                        "Hide grid system"
                                    };
                                    change = Some((
                                        label.into(),
                                        Box::new(move |h| h.set_grid_group(id, !shown)),
                                    ));
                                }
                            });
                        }
                    });
                }
                let hidden: Vec<(ElementId, String)> = h
                    .elements
                    .iter()
                    .filter_map(|id| self.doc.model().get(*id))
                    .map(|e| (e.id, e.name.clone()))
                    .collect();
                if !hidden.is_empty() {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Hidden elements").strong());
                    for (id, name) in hidden {
                        ui.horizontal(|ui| {
                            ui.label(name);
                            if ui.small_button("Unhide").clicked() {
                                change = Some((
                                    "Show hidden element".into(),
                                    Box::new(move |h| h.set_element(id, false)),
                                ));
                            }
                        });
                    }
                }
                ui.add_space(4.0);
                if ui
                    .add_enabled(!h.is_empty(), egui::Button::new("Show everything"))
                    .clicked()
                {
                    change = Some((
                        "Show everything".into(),
                        Box::new(|h| *h = ViewHidden::default()),
                    ));
                }
                ui.label(
                    egui::RichText::new(
                        "Right-click an element in a view to hide it or its category there.",
                    )
                    .weak(),
                );
            });
        if let Some((label, f)) = change {
            self.set_view_hidden(t, &label, f);
        }
        if let Some(id) = reveal {
            self.toggle_hidden_elements(id);
        }
        match extra {
            Some("guides.toggle") => self.set_guides_hidden(!self.shell.settings.guides_hidden),
            Some("xray") => self.set_xray(!self.shell.settings.xray),
            Some("ground.grid") => {
                let s = &mut self.shell.settings;
                s.ground_grid = !s.ground_grid;
                s.store();
                self.geometry_key = None;
            }
            _ => {}
        }
    }

    /// Every grid system, with the one new lines go in, and a button for a
    /// new one.
    pub(super) fn grid_systems_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Grid systems").strong());
        let model = self.doc.model();
        let active = self.active_grid_group();
        let rows: Vec<(Option<ElementId>, String, [f32; 3], usize)> =
            std::iter::once((None, "No grid system".to_string(), grids::UNGROUPED_COLOR))
                .chain(
                    grids::groups(model)
                        .into_iter()
                        .map(|(e, g)| (Some(e.id), e.name.clone(), g.color)),
                )
                .map(|(id, name, c)| (id, name, c, grid_count(model, id)))
                .collect();
        let mut pick = None;
        let mut edit = None;
        for (id, name, color, n) in rows {
            ui.horizontal(|ui| {
                if ui
                    .radio(active == id, "")
                    .on_hover_text("New grid lines go here")
                    .clicked()
                {
                    pick = Some(id);
                }
                swatch(ui, color);
                ui.label(format!("{name} ({n})"));
                if let Some(g) = id {
                    if ui.small_button("Edit").clicked() {
                        edit = Some(g);
                    }
                }
            });
        }
        if ui.button("New grid system").clicked() {
            self.new_grid_group();
        }
        if let Some(g) = pick {
            self.plan.grid_group = g;
        }
        if let Some(g) = edit {
            self.selection.only(g);
            self.face = None;
        }
    }

    /// Properties of a grid group. Returns the undo label when it changed.
    pub(super) fn grid_group_properties(
        &mut self,
        ui: &mut egui::Ui,
        id: ElementId,
        g: &mut GridGroup,
    ) -> &'static str {
        let mut label = "";
        ui.label(
            egui::RichText::new(
                "A grid system: its lines are labelled on their own, with the prefix in front.",
            )
            .weak(),
        );
        ui.horizontal(|ui| {
            ui.label("Prefix");
            if ui
                .add(egui::TextEdit::singleline(&mut g.prefix).desired_width(60.0))
                .changed()
            {
                label = "Grid prefix";
            }
        });
        let style = |ui: &mut egui::Ui, text: &str, salt: &str, s: &mut LabelStyle| {
            let mut changed = false;
            egui::ComboBox::new(("gg-style", salt, id), text)
                .selected_text(s.label())
                .show_ui(ui, |ui| {
                    for o in LabelStyle::ALL {
                        changed |= ui.selectable_value(s, o, o.label()).changed();
                    }
                });
            changed
        };
        if style(ui, "North–south lines", "v", &mut g.vertical)
            | style(ui, "East–west lines", "h", &mut g.horizontal)
        {
            label = "Grid label style";
        }
        ui.horizontal(|ui| {
            ui.label("Colour");
            // the picker works in 8-bit sRGB, as the colour is stored
            let mut c = g.color.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
            if ui.color_edit_button_srgb(&mut c).changed() {
                g.color = c.map(|v| v as f32 / 255.0);
                label = "Grid colour";
            }
        });
        let n = grid_count(self.doc.model(), Some(id));
        ui.label(format!(
            "{n} lines, e.g. {}{} and {}{}.",
            g.prefix,
            g.vertical.format(1),
            g.prefix,
            g.horizontal.format(1)
        ));
        ui.horizontal_wrapped(|ui| {
            let active = self.active_grid_group() == Some(id);
            if ui
                .add_enabled(!active, egui::Button::new("Draw lines in it"))
                .clicked()
            {
                self.plan.grid_group = Some(id);
                self.plan.tool = Tool::Grid;
                self.plan.cancel();
                self.status = Tool::Grid.hint().into();
            }
            if ui.button("Select its lines").clicked() {
                let ids = grid_ids(self.doc.model(), Some(id));
                self.selection.clear();
                self.selection.extend(ids, false);
            }
            if ui.button("Renumber in order").clicked() {
                let tx = grids::renumber(self.doc.model(), Some(id));
                if !tx.ops.is_empty() {
                    let r = self.doc.commit(tx);
                    self.report(r, "Relabelled the lines west to east and south to north.");
                }
            }
        });
        ui.label(
            egui::RichText::new(
                "Deleting the system keeps its lines, ungrouped and with their labels.",
            )
            .weak(),
        );
        label
    }

    /// The grid system part of a grid line's properties.
    pub(super) fn grid_properties(&mut self, ui: &mut egui::Ui, g: &mut Grid) -> &'static str {
        let mut label = "";
        let model = self.doc.model();
        let groups: Vec<(ElementId, String)> = grids::groups(model)
            .into_iter()
            .map(|(e, _)| (e.id, e.name.clone()))
            .collect();
        let current = grids::group_of(model, g).map(|(e, _)| e.name.clone());
        let mut pick = None;
        egui::ComboBox::from_label("Grid system")
            .selected_text(current.unwrap_or_else(|| "None".into()))
            .show_ui(ui, |ui| {
                if ui.selectable_label(g.group.is_none(), "None").clicked() {
                    pick = Some(None);
                }
                for (id, name) in &groups {
                    if ui.selectable_label(g.group == Some(*id), name).clicked() {
                        pick = Some(Some(*id));
                    }
                }
            });
        if let Some(to) = pick.filter(|to| *to != g.group) {
            // it takes the next label of its new group
            g.label = grids::next_label(model, to, g.start, g.end);
            g.group = to;
            label = "Move grid line to another system";
        }
        ui.label(format!("Shows as {}", grids::display_label(model, g)));
        label
    }

    // ---- 3D ------------------------------------------------------------

    /// Grid lines the 3D view shows, with their colour.
    fn grids_3d(&self) -> Vec<(ElementId, Grid, [f32; 3])> {
        let model = self.doc.model();
        if self.vis3d.hides_category(Category::Grids) {
            return vec![];
        }
        let hidden = self.hidden_in_3d();
        model
            .iter()
            .filter(|e| !hidden.contains(&e.id) && !self.hidden.contains(&e.id))
            .filter_map(|e| match &e.kind {
                ElementKind::Grid(g) => Some((e.id, g.clone(), grids::color_of(model, g))),
                _ => None,
            })
            .collect()
    }

    /// The heights grid lines span in 3D: the lowest level to the highest
    /// (at least one storey), and every level between.
    fn grid_heights_3d(&self) -> Vec<f64> {
        let mut zs: Vec<f64> = self
            .doc
            .model()
            .levels()
            .map(|(_, l)| l.elevation)
            .collect();
        zs.sort_by(f64::total_cmp);
        zs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        let lo = zs.first().copied().unwrap_or(0.0);
        let hi = zs.last().copied().unwrap_or(0.0);
        if hi - lo < 0.5 {
            let storey = self
                .doc
                .model()
                .levels()
                .map(|(_, l)| l.height)
                .fold(0.0, f64::max);
            zs = vec![lo, lo + if storey > 0.0 { storey } else { 3.0 }];
        }
        zs
    }

    /// Depth-tested lines for the grid lines in 3D: along the line at each
    /// level, up both ends, and out to the bubbles at the top.
    pub(super) fn grid_lines_3d(&self) -> Vec<Vertex> {
        let zs = self.grid_heights_3d();
        let (lo, hi) = (zs[0], zs[zs.len() - 1]);
        let mut out = Vec::new();
        for (id, g, color) in self.grids_3d() {
            let c = if self.selection.contains(id) {
                [0.95, 0.55, 0.20]
            } else {
                color
            };
            let mut seg = |a: DVec3, b: DVec3| {
                out.push(line_vertex(a.as_vec3().to_array(), c));
                out.push(line_vertex(b.as_vec3().to_array(), c));
            };
            for z in &zs {
                seg(g.start.extend(*z), g.end.extend(*z));
            }
            let dir = (g.end - g.start).normalize_or_zero();
            for (p, out_dir) in [(g.start, -dir), (g.end, dir)] {
                seg(p.extend(lo), p.extend(hi));
                seg(p.extend(hi), (p + out_dir * BUBBLE_OUT_3D).extend(hi));
            }
        }
        out
    }

    /// Where a grid line's bubbles are in 3D.
    fn bubbles_3d(&self) -> Vec<(ElementId, String, [f32; 3], DVec3)> {
        let zs = self.grid_heights_3d();
        let hi = zs[zs.len() - 1];
        let model = self.doc.model();
        let mut out = vec![];
        for (id, g, color) in self.grids_3d() {
            let dir = (g.end - g.start).normalize_or_zero();
            let text = grids::display_label(model, &g);
            for (p, d) in [(g.start, -dir), (g.end, dir)] {
                out.push((id, text.clone(), color, (p + d * BUBBLE_OUT_3D).extend(hi)));
            }
        }
        out
    }

    /// The grid bubbles over the 3D view: a circle with the label, facing
    /// the viewer, at both ends of each line.
    pub(super) fn draw_grid_bubbles(
        &self,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
    ) {
        for (id, text, color, at) in self.bubbles_3d() {
            let Some(c) = project(at) else { continue };
            if !painter.clip_rect().expand(20.0).contains(c) {
                continue;
            }
            let selected = self.selection.contains(id);
            let ring = if selected {
                Color32::from_rgb(242, 140, 51)
            } else {
                color32(color)
            };
            let r = BUBBLE_R_3D * creabim_geom::plan::bubble_scale(&text) as f32;
            painter.circle(
                c,
                r,
                Color32::from_rgba_unmultiplied(255, 255, 255, 235),
                egui::Stroke::new(if selected { 2.5 } else { 1.5 }, ring),
            );
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                &text,
                egui::FontId::proportional(12.0),
                ring,
            );
        }
    }

    /// The grid line under `p` in the 3D view (a bubble, or its line at a
    /// level), with the point on it under the pointer, for picking and
    /// dragging it.
    pub(super) fn grid_line_near(
        &self,
        p: egui::Pos2,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
    ) -> Option<(ElementId, DVec3)> {
        self.grid_hit_3d(p, project, None)
    }

    /// Where the pointer holds grid line `id` in the 3D view, if it is on
    /// that line or its bubble (other grids crossing there don't count).
    pub(super) fn grid_grab(
        &self,
        p: egui::Pos2,
        id: ElementId,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
    ) -> Option<DVec3> {
        self.grid_hit_3d(p, project, Some(id)).map(|h| h.1)
    }

    fn grid_hit_3d(
        &self,
        p: egui::Pos2,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
        only: Option<ElementId>,
    ) -> Option<(ElementId, DVec3)> {
        let zs = self.grid_heights_3d();
        let hi = zs[zs.len() - 1];
        // a bubble is the deliberate target: it wins over lines passing by
        let bubble = self
            .bubbles_3d()
            .into_iter()
            .filter(|(id, ..)| only.is_none_or(|o| o == *id))
            .filter_map(|(id, text, _, at)| {
                let c = project(at)?;
                let r = BUBBLE_R_3D * creabim_geom::plan::bubble_scale(&text) as f32;
                let d = c.distance(p);
                (d <= r + 2.0).then_some((id, d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id);
        if let Some(id) = bubble {
            let g = match self.doc.model().get(id).map(|e| &e.kind) {
                Some(ElementKind::Grid(g)) => g.clone(),
                _ => return None,
            };
            // the bubble end nearest the pointer
            let near = [g.start, g.end]
                .into_iter()
                .filter_map(|e| Some((e, project(e.extend(hi))?.distance(p))))
                .min_by(|a, b| a.1.total_cmp(&b.1))?;
            return Some((id, near.0.extend(hi)));
        }
        let mut best: Option<(f32, ElementId, DVec3)> = None;
        for (id, g, _) in self.grids_3d() {
            if only.is_some_and(|o| o != id) {
                continue;
            }
            for z in &zs {
                let (a, b) = (g.start.extend(*z), g.end.extend(*z));
                let (Some(sa), Some(sb)) = (project(a), project(b)) else {
                    continue;
                };
                let ab = sb - sa;
                let t = ((p - sa).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                let d = (sa + ab * t).distance(p);
                if d < 6.0 && best.is_none_or(|(bd, _, _)| d < bd) {
                    best = Some((d, id, a + (b - a) * t as f64));
                }
            }
        }
        best.map(|(_, id, at)| (id, at))
    }

    /// The grid line whose bubble is under `p` in the 3D view.
    pub(super) fn grid_bubble_near(
        &self,
        p: egui::Pos2,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
    ) -> Option<ElementId> {
        self.bubbles_3d()
            .into_iter()
            .filter_map(|(id, text, _, at)| {
                let c = project(at)?;
                let r = BUBBLE_R_3D * creabim_geom::plan::bubble_scale(&text) as f32;
                let d = c.distance(p);
                (d <= r + 2.0).then_some((id, d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }
}

/// How many grid lines are in `group` (`None`: ungrouped).
fn grid_count(model: &Model, group: Option<ElementId>) -> usize {
    grid_ids(model, group).len()
}

/// Every guide in the model.
pub(super) fn guide_ids(model: &Model) -> Vec<ElementId> {
    model
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Guide(_)))
        .map(|e| e.id)
        .collect()
}

fn grid_ids(model: &Model, group: Option<ElementId>) -> Vec<ElementId> {
    model
        .iter()
        .filter(|e| matches!(&e.kind, ElementKind::Grid(g) if g.group == group))
        .map(|e| e.id)
        .collect()
}

/// A small square of colour.
fn swatch(ui: &mut egui::Ui, c: [f32; 3]) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
    ui.painter().rect_filled(r, 2.0, color32(c));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The app round the sample house, with a guide added.
    fn app() -> CreaBimApp {
        let mut app = CreaBimApp::blank(None);
        app.load_model(sample_house());
        let guide: creabim_core::Guide = serde_json::from_value(json!({
            "level": null, "a": [0.0, -2.0, 0.0], "b": [4.0, -2.0, 0.0],
        }))
        .unwrap();
        let e = Element::new("Guide", ElementKind::Guide(guide));
        app.doc.commit(Transaction::new("Guide").insert(e)).unwrap();
        app
    }

    fn checked(items: &[Item], id: &str) -> Option<bool> {
        items.iter().find_map(|i| match i {
            Item::Action { id: i, checked, .. } if i == id => *checked,
            _ => None,
        })
    }

    #[test]
    fn the_3d_views_quick_switches_hide_grids_annotation_and_guides() {
        let mut app = app();
        let ctx = egui::Context::default();
        let model = app.doc.model().clone();
        let grids: Vec<ElementId> = model
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Grid(_)))
            .map(|e| e.id)
            .collect();
        assert!(!grids.is_empty());
        let items = app.visibility_items(VisTarget::Window3d);
        assert_eq!(checked(&items, "vis3.quick:grids"), Some(true));
        app.run_action(&ctx, "vis3.quick:grids");
        assert!(grids.iter().all(|g| app.hidden_in_3d().contains(g)));
        // kept for the next session
        assert!(app.shell.settings.hidden_3d.contains(&Category::Grids));
        let items = app.visibility_items(VisTarget::Window3d);
        assert_eq!(checked(&items, "vis3.quick:grids"), Some(false));
        assert_eq!(checked(&items, "vis3.quick:all"), Some(false));
        // annotation goes as a group
        app.run_action(&ctx, "vis3.quick:annotation");
        for c in [Category::Dimensions, Category::Tags, Category::Notes] {
            assert!(app.vis3d.hides_category(c));
        }
        // everything back, ground grid included
        app.run_action(&ctx, "vis3.quick:all");
        assert!(app.vis3d.categories.is_empty());
        app.run_action(&ctx, "vis3.quick:all");
        assert!(app.vis3d.hides_category(Category::Guides));
        assert!(!app.shell.settings.ground_grid);
        // the 3D view's Visibility is on its navigation bar
        let nav = app.nav_state();
        assert_eq!(nav["menus"][0]["key"], "vis3d");
        assert!(nav["buttons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["id"] == "xray"));
    }

    #[test]
    fn guides_hide_everywhere_and_come_back() {
        let mut app = app();
        let ctx = egui::Context::default();
        let guides = guide_ids(app.doc.model());
        assert_eq!(guides.len(), 1);
        app.run_action(&ctx, "guides.toggle");
        assert!(app.shell.settings.guides_hidden);
        assert!(app.hidden_in_3d().contains(&guides[0]));
        // still in the model
        assert!(app.doc.model().get(guides[0]).is_some());
        // picking the Guide tool brings them back
        let i = Tool::ALL.iter().position(|t| *t == Tool::Guide).unwrap();
        app.run_action(&ctx, &format!("tool:{i}"));
        assert!(!app.shell.settings.guides_hidden);
        assert!(!app.hidden_in_3d().contains(&guides[0]));
    }

    #[test]
    fn xray_and_hidden_elements_switch_on_and_off() {
        let mut app = app();
        let ctx = egui::Context::default();
        app.run_action(&ctx, "xray");
        assert!(app.shell.settings.xray);
        app.run_action(&ctx, "xray");
        assert!(!app.shell.settings.xray);
        let plan = app
            .doc
            .model()
            .iter()
            .find(|e| matches!(&e.kind, ElementKind::View(v) if matches!(v.kind, ViewKind::Plan { .. })))
            .unwrap()
            .id;
        app.active_view = Some(plan);
        app.run_action(&ctx, "view.reveal");
        assert!(app.shows_hidden_elements(plan));
        // one undo step
        app.run_command(&ctx, crate::commands::Command::Undo, None);
        assert!(!app.shows_hidden_elements(plan));
        let items = app.visibility_items(VisTarget::View(plan));
        assert_eq!(
            checked(&items, &format!("vis.reveal:{}", plan.0)),
            Some(false)
        );
    }

    #[test]
    fn a_custom_ground_grid_spacing_is_typed_in_any_unit() {
        let mut app = app();
        crate::units::set(creabim_core::Units::metric());
        assert_eq!(app.set_ground_spacing_text("1250"), Ok(1.25));
        assert_eq!(app.shell.settings.ground_grid_spacing, 1.25);
        assert!((app.set_ground_spacing_text("4'").unwrap() - 1.2192).abs() < 1e-9);
        assert!(app.set_ground_spacing_text("nonsense").is_err());
        assert!(app.set_ground_spacing_text("1 mm").is_err());
        // the menu shows it as the custom one
        let view = app.view_menu();
        let ground = view
            .iter()
            .find_map(|i| match i {
                Item::Sub { label, items, .. } if label == "Ground grid" => Some(items.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(checked(&ground, "ground.custom"), Some(true));
    }

    #[test]
    fn roofs_columns_and_beams_take_tags() {
        let model = creabim_core::sample::layered_house();
        for e in model.iter() {
            let member = creabim_core::members::is_member(&e.kind);
            if member || matches!(e.kind, ElementKind::Wall(_) | ElementKind::Slab(_)) {
                assert!(crate::planview::is_taggable(&model, e.id), "{}", e.name);
            }
            if matches!(e.kind, ElementKind::View(_) | ElementKind::Level(_)) {
                assert!(!crate::planview::is_taggable(&model, e.id));
            }
        }
    }

    /// Every icon a menu names, for the check below.
    fn icons(items: &[Item], out: &mut Vec<(String, &'static str)>) {
        for i in items {
            match i {
                Item::Action { label, icon, .. } => out.push((label.clone(), icon)),
                Item::Sub {
                    label, icon, items, ..
                } => {
                    out.push((label.clone(), icon));
                    icons(items, out);
                }
                _ => {}
            }
        }
    }

    #[test]
    fn every_menu_entry_has_an_icon_that_exists() {
        let app = app();
        let known = include_str!("../ui/icons.js");
        for key in [
            "menu:File",
            "menu:Edit",
            "menu:View",
            "menu:Create",
            "menu:Materials",
        ] {
            let items = app.menu_items(key);
            // entries on the menu itself, and the submenus they open; check
            // marks stand in for the icon of on/off entries
            for i in &items {
                match i {
                    Item::Action {
                        label,
                        icon,
                        checked: None,
                        ..
                    } => assert!(!icon.is_empty(), "{key}: {label} has no icon"),
                    Item::Sub { label, icon, .. } => {
                        assert!(!icon.is_empty(), "{key}: {label} has no icon")
                    }
                    _ => {}
                }
            }
            let mut all = vec![];
            icons(&items, &mut all);
            for (label, icon) in all.into_iter().filter(|(_, i)| !i.is_empty()) {
                assert!(
                    known.contains(&format!("\"{icon}\":")),
                    "{key}: {label} names {icon}, which icons.js lacks"
                );
            }
        }
        // the Create menu's schedules too
        let create = app.menu_items("menu:Create");
        let schedule = create
            .iter()
            .find_map(|i| match i {
                Item::Sub { label, items, .. } if label == "Schedule" => Some(items.clone()),
                _ => None,
            })
            .unwrap();
        assert!(schedule
            .iter()
            .all(|i| matches!(i, Item::Action { icon, .. } if !icon.is_empty())));
    }
}
