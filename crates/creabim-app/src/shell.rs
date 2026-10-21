//! The app's side of the web interface: what each pane shows, the menus,
//! and what the user's clicks there do. Menus are built as data here, so
//! the classic egui menu bar draws the same items.

use super::*;
use crate::theme::Theme;
use crate::ui_model::{self, Item};
use crate::webui::{Msg, Pane, WebUi};
use serde_json::{json, Value};

/// Height of the top bar (menus, tools and the tool options strip) and the
/// status bar, in points.
const TOP_H: f32 = 113.0;
const STATUS_H: f32 = 30.0;
/// Size of the 3D view's navigation buttons.
const NAV_W: f32 = 372.0;
const NAV_H: f32 = 34.0;

pub(super) struct Shell {
    pub web: Option<WebUi>,
    /// Why the web interface couldn't start, when it couldn't.
    pub failed: Option<String>,
    /// The classic egui panels, by choice.
    pub classic: bool,
    pub theme: Theme,
    /// The theme and panel widths, kept between sessions.
    pub settings: crate::files::Settings,
    popup: Option<Popup>,
    /// Keys pressed while a page had the keyboard, for the views.
    pub keys: Vec<egui::Event>,
    /// A right-click menu was asked for from the 3D view this frame.
    pub menu_3d: bool,
    /// The open right-click menu came from the 3D view (else a 2D pane).
    context_3d: bool,
    /// Give the popup the keyboard once it is shown (the palette).
    focus_popup: bool,
    /// The ground grid spacing being typed, while its window is open.
    pub ground_custom: Option<String>,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            web: None,
            failed: None,
            classic: std::env::var_os("CREABIM_CLASSIC_UI").is_some(),
            theme: Theme::default(),
            settings: Default::default(),
            popup: None,
            keys: vec![],
            menu_3d: false,
            context_3d: false,
            focus_popup: false,
            ground_custom: None,
        }
    }
}

struct Popup {
    key: String,
    /// Top-left corner, in points.
    at: egui::Pos2,
    /// When the popup opens upwards, where its bottom goes.
    up: Option<f32>,
    size: egui::Vec2,
}

fn tool_icon(t: Tool) -> &'static str {
    match t {
        Tool::Select => "mouse-pointer-2",
        Tool::Wall => "brick-wall",
        Tool::Door => "door-open",
        Tool::Window => "app-window",
        Tool::Slab => "layers-2",
        Tool::Mass => "box",
        Tool::Section => "scissors-line-dashed",
        Tool::Elevation => "building-2",
        Tool::Dimension => "ruler",
        Tool::Tag => "tag",
        Tool::Angle => "drafting-compass",
        Tool::Radius => "radius",
        Tool::Diameter => "diameter",
        Tool::ArcLength => "spline",
        Tool::Note => "message-square-text",
        Tool::Sketch => "pen-tool",
        Tool::Guide => "slash",
        Tool::Grid => "grid-3x3",
        Tool::Column => "rectangle-vertical",
        Tool::Beam => "rectangle-horizontal",
        Tool::Roof => "tent",
        Tool::Stair => "chart-no-axes-column-increasing",
        Tool::Railing => "fence",
        Tool::Level => "level",
        Tool::Align => "align-tool",
    }
}

fn tool_group(t: Tool) -> u8 {
    match t {
        Tool::Select | Tool::Align => 0,
        Tool::Wall
        | Tool::Door
        | Tool::Window
        | Tool::Slab
        | Tool::Mass
        | Tool::Column
        | Tool::Beam
        | Tool::Roof
        | Tool::Stair
        | Tool::Railing => 1,
        Tool::Section | Tool::Elevation => 2,
        Tool::Dimension
        | Tool::Tag
        | Tool::Angle
        | Tool::Radius
        | Tool::Diameter
        | Tool::ArcLength
        | Tool::Note => 3,
        Tool::Sketch | Tool::Guide | Tool::Grid | Tool::Level => 4,
    }
}

fn icon_name(i: Icon) -> &'static str {
    match i {
        Icon::Storey => "layers-3",
        Icon::Grid => "grid-3x3",
        Icon::Wall => "brick-wall",
        Icon::Slab => "layers-2",
        Icon::Door => "door-open",
        Icon::Window => "app-window",
        Icon::Roof => "house",
        Icon::Column => "columns-2",
        Icon::Beam => "minus",
        Icon::Mass => "box",
        Icon::Material => "palette",
        Icon::Plan => "map",
        Icon::Section => "scissors-line-dashed",
        Icon::Elevation => "building-2",
        Icon::View3d => "axis-3d",
        Icon::Schedule => "table-2",
        Icon::Sheet => "file-text",
        Icon::Viewport => "frame",
        Icon::Dimension => "ruler",
        Icon::Tag => "tag",
        Icon::Sketch => "pen-tool",
        Icon::Guide => "slash",
    }
}

/// A plain name for a group of the project browser.
fn group_title(class: &str) -> String {
    match class {
        "View" => "Views".into(),
        "Sheet" => "Sheets".into(),
        "Viewport" => "Viewports".into(),
        "IfcBuildingStorey" => "Levels".into(),
        "IfcBuildingElementProxy" => "Masses".into(),
        "Sketch" => "Sketches".into(),
        "IfcMaterial" => "Materials".into(),
        "IfcMaterialLayerSet" => "Assemblies".into(),
        "TextStyle" => "Text styles".into(),
        c => {
            let c = c.strip_prefix("Ifc").unwrap_or(c);
            if c.ends_with('s') {
                c.to_string()
            } else {
                format!("{c}s")
            }
        }
    }
}

/// The icon of a schedule of `c`.
fn schedule_icon(c: ScheduleCategory) -> &'static str {
    match c {
        ScheduleCategory::Doors => "door-open",
        ScheduleCategory::Windows => "app-window",
        ScheduleCategory::Walls => "brick-wall",
        ScheduleCategory::Slabs => "layers-2",
        ScheduleCategory::Structure => "rectangle-vertical",
    }
}

/// The New schedule submenu, one entry a category.
fn schedule_items() -> Vec<Item> {
    ScheduleCategory::ALL
        .iter()
        .enumerate()
        .map(|(i, c)| Item::new(format!("new.schedule:{i}"), c.label()).icon(schedule_icon(*c)))
        .collect()
}

fn command_icon(c: Command) -> &'static str {
    match c {
        Command::Undo => "undo-2",
        Command::Redo => "redo-2",
        Command::Move => "move",
        Command::Rotate => "rotate-cw",
        Command::Scale => "scaling",
        Command::Duplicate => "copy-plus",
        Command::Copy => "copy",
        Command::Cut => "scissors",
        Command::Paste => "clipboard-paste",
        Command::Delete => "trash-2",
        Command::Hide => "eye-off",
        Command::Isolate => "scan-eye",
        Command::ShowAll => "eye",
        Command::SelectAll => "square-dashed-mouse-pointer",
        Command::SelectNone => "square-dashed",
        Command::CursorHere => "crosshair",
        Command::WorkplaneHere => "square-dashed",
    }
}

impl CreaBimApp {
    fn cmd_item(&self, c: Command) -> Item {
        let enabled = match c {
            Command::Undo => self.doc.can_undo(),
            Command::Redo => self.doc.can_redo(),
            Command::Paste => !self.clipboard.is_empty(),
            Command::ShowAll => !self.hidden.is_empty(),
            c => self.selection.is_some() || !c.needs_selection(),
        };
        let label = match c {
            Command::Undo => self
                .doc
                .undo_label()
                .map_or("Undo".into(), |l| format!("Undo {l}")),
            Command::Redo => self
                .doc
                .redo_label()
                .map_or("Redo".into(), |l| format!("Redo {l}")),
            Command::Delete if self.selection.len() > 1 => {
                format!("Delete {} selected", self.selection.len())
            }
            c => c.label().into(),
        };
        Item::new(format!("cmd:{c:?}"), label)
            .key(c.shortcut())
            .enabled(enabled)
            .icon(command_icon(c))
    }

    /// The items of a menu or dropdown, by its key.
    pub(super) fn menu_items(&self, key: &str) -> Vec<Item> {
        match key {
            "menu:File" => self.file_menu(),
            "menu:Edit" => self.edit_menu(),
            "menu:View" => self.view_menu(),
            "menu:Materials" => self.materials_menu(),
            "menu:Create" => self.create_menu(),
            "underlay" => self.underlay_items(),
            "palette" => self.palette_items(),
            "look" => crate::camera::Look::ALL
                .iter()
                .enumerate()
                .map(|(i, l)| Item::new(format!("nav.look:{i}"), l.label()))
                .collect(),
            "views" => {
                let mut items = vec![];
                let model = self.doc.model();
                for (g, ids) in crate::browser::view_groups(model) {
                    items.push(Item::Header(g.title().into()));
                    for id in ids {
                        let Some(e) = model.get(id) else { continue };
                        items.push(
                            Item::new(format!("view:{}", id.0), e.name.clone())
                                .checked(self.active_view == Some(id))
                                .icon(icon_name(Icon::of(e))),
                        );
                    }
                }
                let sheets = self.sheet_list();
                if !sheets.is_empty() {
                    items.push(Item::Header("Sheets".into()));
                }
                for (id, name) in sheets {
                    items.push(
                        Item::new(format!("view:{}", id.0), name)
                            .checked(self.active_view == Some(id))
                            .icon("file-text"),
                    );
                }
                items
            }
            "placeview" => {
                let Some(sheet) = self.active_view.filter(|id| self.is_sheet(*id)) else {
                    return vec![];
                };
                let placed: Vec<ElementId> = self
                    .doc
                    .model()
                    .iter()
                    .filter_map(|e| match &e.kind {
                        ElementKind::Viewport(vp) if vp.sheet == sheet => Some(vp.view),
                        _ => None,
                    })
                    .collect();
                self.views()
                    .into_iter()
                    .map(|(id, name, _)| {
                        let label = if placed.contains(&id) {
                            format!("{name} (placed)")
                        } else {
                            name
                        };
                        Item::new(format!("place:{}", id.0), label)
                    })
                    .collect()
            }
            "context" => {
                let n = self.selection.len();
                let ctx = |c: Command| match self.cmd_item(c) {
                    Item::Action {
                        label,
                        shortcut,
                        enabled,
                        icon,
                        ..
                    } => Item::new(format!("ctx:{c:?}"), label)
                        .key(shortcut)
                        .enabled(enabled)
                        .icon(icon),
                    other => other,
                };
                let mut items = vec![];
                if n > 0 {
                    items.push(Item::Header(match n {
                        1 => "1 selected".into(),
                        n => format!("{n} selected"),
                    }));
                    for group in [
                        &[
                            Command::Move,
                            Command::Rotate,
                            Command::Scale,
                            Command::Duplicate,
                        ][..],
                        &[Command::Copy, Command::Cut, Command::Delete],
                        &[Command::Hide, Command::Isolate],
                    ] {
                        items.extend(group.iter().map(|c| ctx(*c)));
                        items.push(Item::Separator);
                    }
                    if !self.shell.context_3d {
                        items.push(align_item());
                        items.push(Item::Separator);
                    }
                    items.extend(self.hide_in_view_items(self.shell.context_3d));
                }
                items.extend([
                    ctx(Command::Paste),
                    ctx(Command::ShowAll),
                    Item::Separator,
                    ctx(Command::SelectAll),
                ]);
                if n > 0 {
                    items.push(ctx(Command::SelectNone));
                }
                items.push(Item::Separator);
                items.push(ctx(Command::CursorHere));
                items.push(ctx(Command::WorkplaneHere));
                items
            }
            "pivot" => {
                let mut items: Vec<Item> = crate::modal::Pivot::ALL
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        Item::new(format!("pivot:{i}"), p.label()).checked(self.plan.pivot == *p)
                    })
                    .collect();
                items.push(Item::Separator);
                items.push(
                    Item::new("cursor.clear", "Clear the pivot cursor")
                        .enabled(self.plan.cursor.is_some()),
                );
                items
            }
            "workplane" => {
                let mut items = vec![
                    Item::new("wp.level", "Level (follows the plan)")
                        .checked(self.workplane.is_none()),
                    Item::new("wp.face", "Pick a face…").icon("square-dashed-mouse-pointer"),
                    Item::new("wp.points", "Through three points…").icon("spline"),
                ];
                if let Some((_, z)) = self.work_level() {
                    items.push(
                        Item::new("wp.offset", "Level plane you can offset").hint(format!(
                            "A plane at {} to slide up and down",
                            crate::units::elevation(z)
                        )),
                    );
                }
                items
            }
            "snaps" => {
                let mut s = self.plan.snap;
                let mut items = vec![
                    Item::new("snap.on", "Snapping")
                        .key("F3")
                        .checked(s.on)
                        .keep_open()
                        .hint("Hold Alt while picking to suspend snaps for a moment"),
                    Item::Separator,
                    Item::new("snap.all", "Turn all on").keep_open(),
                    Item::new("snap.none", "Turn all off").keep_open(),
                    Item::Separator,
                ];
                const ICONS: [&str; 12] = [
                    "snap-endpoint",
                    "snap-midpoint",
                    "snap-intersection",
                    "snap-perpendicular",
                    "snap-nearest",
                    "snap-centre",
                    "snap-quadrant",
                    "hash",
                    "slash",
                    "snap-axis",
                    "snap-track",
                    "ruler",
                ];
                for (i, (label, v)) in s.switches().into_iter().enumerate() {
                    items.push(
                        Item::new(format!("snap:{i}"), label)
                            .checked(*v)
                            .icon(ICONS.get(i).copied().unwrap_or(""))
                            .keep_open()
                            .enabled(self.plan.snap.on),
                    );
                }
                items
            }
            key => self
                .drafting_menu(key)
                .or_else(|| self.grid_vis_menu(key))
                .unwrap_or_default(),
        }
    }

    fn file_menu(&self) -> Vec<Item> {
        let mut recent: Vec<Item> = self
            .recent
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| Item::new(format!("recent:{i}"), file_label(f)).hint(f.clone()))
            .collect();
        if !recent.is_empty() {
            recent.push(Item::Separator);
            recent.push(Item::new("recent.clear", "Clear the list"));
        }
        let mut items = vec![
            Item::new("new.empty", "New empty project").icon("file-plus"),
            Item::new("new.sample", "New sample house").icon("house"),
            Item::new("new.sheet", "New sheet").icon("file-text"),
            Item::sub("New schedule", schedule_items()).icon("table-2"),
            Item::Separator,
            Item::new("open", "Open…")
                .key("Ctrl+O")
                .icon("folder-open")
                .hint("Open a .creabim, .ifc or .ifcx file. Other files (.blend, .FCStd) are linked into this project."),
            Item::sub("Open recent", recent).icon("folder"),
        ];
        if self.shell.web.is_none() {
            items.push(
                Item::new("open.path", "Open the path in the File box")
                    .icon("folder-open")
                    .hint("For typing or pasting a path instead of browsing."),
            );
        }
        items.extend([
            Item::new("save", "Save")
                .key("Ctrl+S")
                .icon("save")
                .hint("Save as a .creabim project file, with its undo history."),
            Item::new("saveas", "Save as…").icon("save"),
            Item::Separator,
            Item::Header("Project".into()),
            Item::new("units", "Project units…")
                .icon("ruler")
                .hint("Metric or imperial: how lengths are shown and typed, in dimensions, schedules and on sheets"),
            Item::new("levels", "Levels…")
                .icon("level")
                .hint("Add, duplicate, rename, move and delete levels"),
            Item::new("styles", "Annotation styles…")
                .icon("anno-styles")
                .hint("Named styles for dimensions, notes and tags: text, ends, tag shape, colour"),
            Item::new("templates", "Sheet templates…")
                .icon("sheet-template")
                .hint("Title blocks for sheets: change the built-in one, or import one from DXF, SVG or another project"),
            Item::Separator,
            Item::Header("Linked files".into()),
            Item::new("link", "Import and link…")
                .icon("link")
                .hint("Bring in the objects of a .blend or .FCStd file. Saving it in Blender or FreeCAD updates them here."),
            Item::new("live", "Reload linked files when saved").checked(self.live_reload),
        ]);
        let linked = crate::files::linked_files(self.doc.model());
        if !linked.is_empty() {
            items.push(
                Item::sub(
                    "Reload a linked file",
                    linked
                        .iter()
                        .enumerate()
                        .map(|(i, f)| Item::new(format!("reload:{i}"), file_label(f)))
                        .collect(),
                )
                .icon("refresh-cw"),
            );
        }
        items.extend([
            Item::Separator,
            Item::Header("Export".into()),
            Item::new("export.ifc4", "IFC4 (.ifc)").icon("upload"),
            Item::new("export.ifcx", "IFC5 (.ifcx, alpha)")
                .icon("upload")
                .hint("The new JSON-based IFC. Its schema is still an alpha, so other tools may not read it yet."),
            Item::new("export.glb", "glTF (.glb) for Blender")
                .icon("box")
                .hint("Every element as a mesh with its CreaBIM id, so edits saved in Blender come back to the same elements."),
            Item::new("export.fcstd", "FreeCAD (.FCStd)")
                .icon("cuboid")
                .hint("Every element as a solid with its BIM type and CreaBIM id; open it in FreeCAD, edit, save, and link it back."),
            Item::new("export.pdf", "Sheets as PDF").icon("file-text"),
            Item::new("export.svg", "This view or sheet as SVG").icon("file"),
        ]);
        items
    }

    fn edit_menu(&self) -> Vec<Item> {
        let mut items = vec![];
        for group in [
            &[Command::Undo, Command::Redo][..],
            &[
                Command::Move,
                Command::Rotate,
                Command::Scale,
                Command::Duplicate,
            ],
            &[Command::Copy, Command::Cut, Command::Paste, Command::Delete],
            &[Command::SelectAll, Command::SelectNone],
            &[Command::Hide, Command::Isolate, Command::ShowAll],
        ] {
            items.extend(group.iter().map(|c| self.cmd_item(*c)));
            items.push(Item::Separator);
        }
        items.push(align_item());
        items.push(Item::Separator);
        let mut pivot = self.menu_items("pivot");
        pivot.insert(0, Item::Header("Rotate and scale about".into()));
        items.push(Item::sub("Pivot", pivot).icon("crosshair"));
        items
    }

    /// Everything that can be placed or made, by kind, as in the tool
    /// palette, with the views and sheets the File menu can also make.
    fn create_menu(&self) -> Vec<Item> {
        let mut items = vec![];
        for (group, title) in [
            (1, "Building elements"),
            (2, "Views"),
            (3, "Annotation"),
            (4, "Sketching"),
        ] {
            items.push(Item::Header(title.into()));
            for (i, t) in Tool::ALL.iter().enumerate() {
                if tool_group(*t) == group {
                    items.push(
                        Item::new(format!("tool:{i}"), t.label())
                            .icon(tool_icon(*t))
                            .hint(t.hint()),
                    );
                }
            }
            if group == 1 {
                items.push(
                    Item::new("level.add", "Level above the top one")
                        .icon("level")
                        .hint("A new level one storey up, with its floor plan"),
                );
                items.push(
                    Item::new("levels", "Levels…")
                        .icon("level")
                        .hint("Add, duplicate, rename, move and delete levels"),
                );
            }
            if group == 2 {
                items.push(Item::new("new.sheet", "Sheet").icon("file-text"));
                items.push(Item::sub("Schedule", schedule_items()).icon("table-2"));
            }
        }
        items
    }

    /// Every action of the menus in one list, with its menu path, for the
    /// command palette.
    fn palette_items(&self) -> Vec<Item> {
        fn flatten(items: Vec<Item>, path: &str, out: &mut Vec<Item>) {
            for item in items {
                match item {
                    Item::Action {
                        id,
                        label,
                        shortcut,
                        hint,
                        enabled,
                        icon,
                        ..
                    } if enabled
                        && !out
                            .iter()
                            .any(|o| matches!(o, Item::Action { id: i, .. } if *i == id)) =>
                    {
                        out.push(
                            Item::new(id, format!("{path}{label}"))
                                .key(shortcut)
                                .hint(hint)
                                .icon(icon),
                        )
                    }
                    Item::Sub { label, items, .. } => {
                        flatten(items, &format!("{path}{label} › "), out)
                    }
                    _ => {}
                }
            }
        }
        let mut out = vec![];
        flatten(self.create_menu(), "Create › ", &mut out);
        flatten(self.edit_menu(), "", &mut out);
        flatten(self.view_menu(), "View › ", &mut out);
        flatten(self.file_menu(), "File › ", &mut out);
        flatten(self.materials_menu(), "Materials › ", &mut out);
        flatten(self.menu_items("views"), "Open › ", &mut out);
        out
    }

    /// The 3D view's navigation buttons.
    pub(super) fn nav_state(&self) -> Value {
        let b = |id: &str, icon: &str, hint: &str, on: bool| json!({"id": id, "icon": icon, "hint": hint, "on": on});
        let sel = self.selection.is_some();
        let mut fitsel = b("nav.fitsel", "scan", "Frame the selection (.)", false);
        fitsel["off"] = json!(!sel);
        json!({
            "theme": self.theme(),
            "popup": self.popup_key(),
            "buttons": [
                b("frame", "maximize-2", "Frame all (Home)", false),
                fitsel,
                b("nav.orbitsel", "rotate-3d", "Orbit around the selection", self.orbit_selection),
                b("nav.zoomin", "zoom-in", "Zoom in", false),
                b("nav.zoomout", "zoom-out", "Zoom out", false),
                b("nav.ortho", if self.camera.ortho { "box" } else { "cuboid" },
                  if self.camera.ortho { "Orthographic: click for perspective" } else { "Perspective: click for orthographic" },
                  self.camera.ortho),
                b("ground.grid", "grid-3x3",
                  if self.shell.settings.ground_grid { "Ground grid shown: click to hide it" } else { "Ground grid hidden: click to show it" },
                  self.shell.settings.ground_grid),
                b("xray", "xray",
                  if self.shell.settings.xray { "X-ray on: click for solid surfaces (Alt+Z)" } else { "X-ray: see through walls, slabs and roofs to what is inside them (Alt+Z)" },
                  self.shell.settings.xray),
            ],
            "menus": [
                {"key": "vis3d", "icon": if self.vis3d.is_empty() { "eye" } else { "eye-off" },
                 "label": "Show", "on": !self.vis3d.is_empty(),
                 "hint": "What the 3D view shows: grids, annotation, guides, sketches and every category"},
            ],
            "look": {"key": "look", "hint": "Look from the top, a side or the corner"},
        })
    }

    /// The View menu: where the 2D view goes and how the 3D view looks,
    /// with the longer lists in submenus so it stays short.
    pub(super) fn view_menu(&self) -> Vec<Item> {
        let place = |p: PlanPlacement, i: u8, label: &str| {
            Item::new(format!("placement:{i}"), label).checked(self.placement == p)
        };
        let layout = vec![
            Item::new("layout.single", "One view at a time")
                .checked(self.shell.settings.single_view)
                .icon("square")
                .hint("Show the 3D view or the drawing, with tabs to switch, instead of both"),
            Item::new("layout.split", "Split: 3D and drawing")
                .checked(!self.shell.settings.single_view)
                .icon("columns-2"),
            Item::Separator,
            Item::new("layout.side", "Side by side").checked(!self.layout.stacked),
            Item::new("layout.stacked", "One above the other").checked(self.layout.stacked),
            Item::new("layout.swap", "2D view first").checked(self.layout.swapped),
        ];
        let now = ground_spacing(self.shell.settings.ground_grid_spacing);
        let custom = !GROUND_SPACINGS.iter().any(|s| (now - s).abs() < 1e-9);
        let mut ground = vec![Item::new("ground.grid", "Show the ground grid")
            .checked(self.shell.settings.ground_grid)
            .hint(
                "The grid on the working level (or the workplane), also on the 3D view's buttons",
            )];
        ground.push(Item::Separator);
        ground.push(Item::Header("Spacing".into()));
        ground.extend(GROUND_SPACINGS.iter().enumerate().map(|(i, s)| {
            Item::new(format!("ground.spacing:{i}"), format!("{s} m"))
                .checked((now - s).abs() < 1e-9)
                .keep_open()
        }));
        ground.push(
            Item::new(
                "ground.custom",
                if custom {
                    format!("Custom: {}…", crate::units::len(now))
                } else {
                    "Custom…".to_string()
                },
            )
            .checked(custom)
            .hint("Type any spacing, in the project's units"),
        );
        let mut camera = vec![
            Item::new("nav.orbitsel", "Orbit around the selection")
                .checked(self.orbit_selection)
                .hint(
                    "Orbiting swings round the selected elements instead of the middle of the view",
                ),
            Item::new("nav.ortho", "Orthographic")
                .checked(self.camera.ortho)
                .hint("Parallel projection, without perspective"),
            Item::Separator,
            Item::Header("Look from".into()),
        ];
        camera.extend(self.menu_items("look"));
        let mut themes: Vec<Item> = Theme::ALL
            .into_iter()
            .map(|t| {
                Item::new(format!("theme:{}", t.id()), t.label())
                    .checked(self.shell.theme == t)
                    .keep_open()
            })
            .collect();
        themes.push(Item::Separator);
        themes.push(
            Item::new("theme.next", "Next theme")
                .key("Ctrl+Shift+T")
                .icon("palette"),
        );
        vec![
            Item::Header("2D view".into()),
            place(PlanPlacement::Docked, 0, "Docked beside 3D"),
            place(PlanPlacement::Window, 1, "In its own window"),
            place(PlanPlacement::Overlay, 2, "Overlaid in the 3D workspace"),
            place(PlanPlacement::Hidden, 3, "Hidden"),
            Item::sub("Docked layout", layout).icon("columns-2"),
            Item::new("plan3d", "Show the 2D view in 3D space").checked(self.plan_in_3d),
            Item::sub("Underlay", self.underlay_items())
                .icon("layers")
                .enabled(self.active_plan().is_some()),
            Item::new("levels", "Levels…").icon("level").hint(
                "Add, rename, move and delete levels, and make their floor plans, from any view",
            ),
            Item::Separator,
            Item::Header("3D view".into()),
            Item::new("frame", "Frame all")
                .key("Home")
                .icon("maximize-2"),
            Item::new("nav.fitsel", "Frame the selection")
                .key(".")
                .icon("scan")
                .enabled(self.selection.is_some()),
            Item::sub(
                "3D view visibility",
                self.visibility_items(super::gridvis::VisTarget::Window3d),
            )
            .icon("eye"),
            Item::new("xray", "X-ray")
                .key("Alt+Z")
                .checked(self.shell.settings.xray)
                .icon("xray")
                .hint("Walls, slabs, roofs and masses see-through, so beams, columns and what is inside them show"),
            Item::sub("Camera", camera).icon("axis-3d"),
            Item::sub("Ground grid", ground).icon("grid-3x3"),
            Item::Separator,
            Item::Header("Every view".into()),
            Item::new("guides.toggle", "Guides")
                .key("Shift+G")
                .checked(!self.shell.settings.guides_hidden)
                .icon("slash")
                .hint("Hide the guides in every view without deleting them, and bring them back"),
            Item::sub(
                "2D view visibility",
                self.vis_target(false)
                    .map(|t| self.visibility_items(t))
                    .unwrap_or_default(),
            )
            .icon("eye"),
            Item::Separator,
            Item::Header("Interface".into()),
            Item::sub("Theme", themes).icon("palette"),
            Item::new("palette", "Command palette")
                .key("Ctrl+K")
                .icon("search"),
            Item::new("ui.classic", "Classic panels")
                .checked(self.shell.classic)
                .hint("The plain built-in panels instead of the new interface"),
        ]
    }

    /// Run an action picked in a menu, a toolbar or the project browser.
    pub(super) fn run_action(&mut self, ctx: &egui::Context, id: &str) {
        let (head, arg) = id.split_once(':').unwrap_or((id, ""));
        let index = arg.parse::<usize>().ok();
        let by_uuid = |s: &Self| {
            s.doc
                .model()
                .iter()
                .find(|e| e.id.0.to_string() == arg)
                .map(|e| e.id)
        };
        match head {
            "cmd" => {
                if let Some(c) = Command::ALL.into_iter().find(|c| format!("{c:?}") == arg) {
                    self.run_command(ctx, c, None);
                }
            }
            "ctx" => {
                let Some(c) = Command::ALL.into_iter().find(|c| format!("{c:?}") == arg) else {
                    return;
                };
                let at = if self.shell.context_3d {
                    self.menu_at_3d
                } else {
                    self.plan.menu_at
                };
                match (c, c.modal()) {
                    (_, Some(m)) => self.arm(c, m),
                    (Command::WorkplaneHere, None) if self.shell.context_3d => match self
                        .menu_plane_3d
                    {
                        Some(plane) => self.set_workplane(plane, "on a face".into()),
                        None => {
                            self.status = "Right-click a face to put the workplane on it.".into()
                        }
                    },
                    _ => self.run_command(ctx, c, at),
                }
            }
            "tool" => {
                if let Some(t) = index.and_then(|i| Tool::ALL.get(i)) {
                    self.plan.tool = *t;
                    self.plan.cancel();
                    self.status = t.hint().into();
                    if *t == Tool::Guide && self.shell.settings.guides_hidden {
                        // drawing guides you can't see helps no one
                        self.set_guides_hidden(false);
                        self.status = format!("Guides shown again. {}", t.hint());
                    }
                    if *t == Tool::Level && !self.shows_section() {
                        // level lines need a section or elevation; anywhere
                        // else the levels are edited in their window
                        self.open_levels();
                        self.status = "Levels window: add, rename and move levels here. To draw a level line, open a section or elevation.".into();
                    }
                }
            }
            "view" => {
                if let Some(v) = by_uuid(self) {
                    self.active_view = Some(v);
                    // with one view at a time, picking a drawing shows it
                    if self.shell.settings.single_view {
                        self.shell.settings.single_shows_3d = false;
                        self.shell.settings.store();
                    }
                }
            }
            "place" => {
                if let (Some(view), Some(sheet)) = (
                    by_uuid(self),
                    self.active_view.filter(|id| self.is_sheet(*id)),
                ) {
                    self.place_view(sheet, view);
                }
            }
            "pivot" => {
                if let Some(p) = index.and_then(|i| crate::modal::Pivot::ALL.get(i)) {
                    self.plan.pivot = *p;
                }
            }
            "cursor.clear" => self.plan.cursor = None,
            "wp.level" => {
                self.workplane = None;
                self.geometry_key = None;
                self.status = "Workplane: the working level.".into();
            }
            "wp.face" => {
                self.plane_pick = Some(PlanePick::Face);
                self.status = "Click a face in the 3D view to put the workplane on it.".into();
            }
            "wp.points" => {
                self.plane_pick = Some(PlanePick::Points(vec![]));
                self.status = "Click three points in the 3D view for the workplane.".into();
            }
            "wp.offset" => {
                if let Some((_, z)) = self.work_level() {
                    self.set_workplane(
                        Workplane::ground(z),
                        format!("level at {}", crate::units::elevation(z)),
                    );
                }
            }
            "snap.on" => self.plan.snap.on = !self.plan.snap.on,
            "snap.all" | "snap.none" => {
                let on = head == "snap.all";
                self.plan
                    .snap
                    .switches()
                    .into_iter()
                    .for_each(|(_, v)| *v = on);
            }
            "snap" => {
                if let Some((_, v)) =
                    index.and_then(|i| self.plan.snap.switches().into_iter().nth(i))
                {
                    *v = !*v;
                }
            }
            "new.sample" => {
                self.load_model(sample_house());
                self.status = "Loaded the sample house.".into();
            }
            "new.empty" => {
                let mut m = Document::new();
                let l = Element::new(
                    "Level 0",
                    ElementKind::Level(creabim_core::Level {
                        elevation: 0.0,
                        height: 3.0,
                    }),
                );
                let _ = m.commit(Transaction::new("New project").insert(l));
                self.load_model(m.model().clone());
                self.status =
                    "New project with one level. Pick the Wall tool to start drawing.".into();
            }
            "new.sheet" => self.new_sheet(),
            "levels" => self.open_levels(),
            "level.add" => self.add_level_above(),
            "browser.sort" => {
                if let Some(g) = crate::browser::ViewGroup::from_key(arg) {
                    let tx = crate::browser::sort_auto(self.doc.model(), g);
                    let r = self.doc.commit(tx);
                    self.report(
                        r,
                        match g {
                            crate::browser::ViewGroup::Plans => {
                                "Floor plans sorted by level again; new plans take their place by level.".to_string()
                            }
                            g => format!("{} back in their automatic order.", g.title()),
                        },
                    );
                }
            }
            "units" => self.levels.units_open = true,
            "underlay" => {
                if let Some((view, level)) = self.active_plan() {
                    let u = match arg {
                        "below" => Underlay::Below,
                        "above" => Underlay::Above,
                        "none" => Underlay::None,
                        _ => match by_uuid(self) {
                            Some(l) if l != level => Underlay::Level(l),
                            _ => return,
                        },
                    };
                    self.set_underlay(view, u);
                }
            }
            "new.schedule" => {
                if let Some(c) = index.and_then(|i| ScheduleCategory::ALL.get(i)) {
                    self.new_schedule(*c);
                }
            }
            "open" => self.open_dialog(),
            "open.path" => {
                let path = self.path.clone();
                self.open_path(&path);
            }
            "recent" => {
                if let Some(f) = index.and_then(|i| self.recent.files.get(i)).cloned() {
                    self.open_path(&f);
                }
            }
            "recent.clear" => self.recent.clear(),
            "save" => self.save(),
            "saveas" => self.save_as(),
            "link" => {
                if let Some(path) = crate::files::pick_open() {
                    self.link_file(&path);
                }
            }
            "live" => self.live_reload = !self.live_reload,
            "reload" => {
                let linked = crate::files::linked_files(self.doc.model());
                if let Some(f) = index.and_then(|i| linked.get(i)) {
                    self.reload(f);
                }
            }
            "export.glb" | "export.fcstd" | "export.ifcx" | "export.ifc4" | "export.pdf"
            | "export.svg" => self.export(head),
            "placement" => {
                self.placement = match index {
                    Some(1) => PlanPlacement::Window,
                    Some(2) => PlanPlacement::Overlay,
                    Some(3) => PlanPlacement::Hidden,
                    _ => PlanPlacement::Docked,
                }
            }
            "layout.single" | "layout.split" => self.set_single_view(head == "layout.single"),
            "layout.toggle" => self.set_single_view(!self.shell.settings.single_view),
            "single.3d" | "single.2d" => {
                self.shell.settings.single_shows_3d = head == "single.3d";
                self.shell.settings.store();
            }
            "ground.grid" => {
                let s = &mut self.shell.settings;
                s.ground_grid = !s.ground_grid;
                s.store();
                self.geometry_key = None;
                self.status = if self.shell.settings.ground_grid {
                    "Ground grid shown. View > 3D view sets its spacing.".into()
                } else {
                    "Ground grid hidden.".into()
                };
            }
            "xray" => self.set_xray(!self.shell.settings.xray),
            "guides.toggle" => self.set_guides_hidden(!self.shell.settings.guides_hidden),
            "ground.custom" => {
                let now = ground_spacing(self.shell.settings.ground_grid_spacing);
                self.shell.ground_custom = Some(crate::units::len(now));
            }
            "ground.spacing" => {
                if let Some(v) = index.and_then(|i| GROUND_SPACINGS.get(i)) {
                    let s = &mut self.shell.settings;
                    s.ground_grid_spacing = *v;
                    s.ground_grid = true;
                    s.store();
                    self.geometry_key = None;
                    self.status = format!("Ground grid every {v} m.");
                }
            }
            "layout.side" => self.layout.stacked = false,
            "layout.stacked" => self.layout.stacked = true,
            "layout.swap" => self.layout.swapped = !self.layout.swapped,
            "plan3d" => self.plan_in_3d = !self.plan_in_3d,
            "frame" => self.frame_all(),
            "nav.fitsel" => self.frame_selection(),
            "nav.orbitsel" => self.orbit_selection = !self.orbit_selection,
            "nav.ortho" => self.camera.ortho = !self.camera.ortho,
            "nav.zoomin" => self.camera.zoom(2.0),
            "nav.zoomout" => self.camera.zoom(-2.0),
            "nav.look" => {
                if let Some(l) = index.and_then(|i| crate::camera::Look::ALL.get(i)) {
                    self.camera.look(*l);
                }
            }
            "palette" => self.open_palette(ctx),
            "theme" | "theme.next" => {
                let t = match Theme::from_id(arg) {
                    Some(t) => t,
                    None => self.shell.theme.next(),
                };
                self.set_theme(ctx, t);
            }
            "ui.classic" => {
                self.shell.classic = !self.shell.classic;
                if let Some(web) = &mut self.shell.web {
                    web.hide_all(self.shell.classic);
                }
            }
            "view.shaded" | "view.tag" | "view.reveal" => self.view_action(head),
            "member" => crate::memberstool::toggle_option(&mut self.plan, arg),
            "slab.mode" => {
                if let Some(m) = index.and_then(|i| crate::slabtool::SlabMode::ALL.get(i)) {
                    crate::slabtool::pick(ctx, Some(*m), None);
                }
            }
            "slab.arc" => crate::slabtool::pick(ctx, None, Some(arg == "1")),
            "sketch.shape" => {
                if let Some(s) = index.and_then(|i| crate::sketch::Shape::ALL.get(i)) {
                    crate::sketch::pick_shape(ctx, *s);
                }
            }
            "note.leader" => self.plan.anno.leader = !self.plan.anno.leader,
            _ => {
                if !self.materials_action(head, arg) && !self.drafting_action(head, arg) {
                    self.grid_vis_action(head, arg);
                }
            }
        }
    }

    pub(super) fn place_view(&mut self, sheet: ElementId, view: ElementId) {
        let at = sheets::free_spot(self.doc.model(), sheet, view);
        let vp = sheets::new_viewport(self.doc.model(), sheet, view, at);
        let e = Element::new("Viewport", ElementKind::Viewport(vp));
        let id = e.id;
        let r = self
            .doc
            .commit(Transaction::new("Place view on sheet").insert(e));
        self.selection.only(id);
        self.report(
            r,
            "Placed the view. Drag it to where it belongs on the sheet.",
        );
    }

    fn export(&mut self, what: &str) {
        match what {
            "export.glb" => {
                let out = with_extension(&self.path, "glb");
                let r = std::fs::write(&out, creabim_exchange::gltf::export_glb(self.doc.model()));
                self.report(r, format!("Exported {out}. In Blender: File > Import > glTF, or use the CreaBIM add-on."));
            }
            "export.fcstd" => {
                let out = with_extension(&self.path, "FCStd");
                let r =
                    creabim_exchange::freecad::export_fcstd(self.doc.model(), &stem(&self.path))
                        .map_err(|e| e.to_string())
                        .and_then(|b| std::fs::write(&out, b).map_err(|e| e.to_string()));
                self.report(r, format!("Exported {out}"));
            }
            "export.ifcx" => {
                let out = with_extension(&self.path, "ifcx");
                let r = std::fs::write(&out, creabim_ifc::export_ifcx(self.doc.model()));
                self.report(r, format!("Exported {out}"));
            }
            "export.ifc4" => {
                let out = with_extension(&self.path, "ifc");
                let opts = creabim_ifc::ExportOptions {
                    project_name: stem(&self.path),
                    ..Default::default()
                };
                let r = std::fs::write(&out, creabim_ifc::export_ifc4(self.doc.model(), &opts));
                self.report(r, format!("Exported {out}"));
            }
            "export.pdf" => {
                let out = with_extension(&self.path, "pdf");
                if sheets::sheets(self.doc.model()).is_empty() {
                    self.status = "There are no sheets to export yet.".into();
                } else {
                    let pdf = sheets::sheets_pdf(self.doc.model(), &self.sheet_info());
                    let r = std::fs::write(&out, pdf);
                    let n = sheets::sheets(self.doc.model()).len();
                    self.report(r, format!("Exported {n} sheets to {out}"));
                }
            }
            "export.svg" => {
                if let Some(sheet) = self.active_view.filter(|id| self.is_sheet(*id)) {
                    let number = sheets::sheet_of(self.doc.model(), sheet)
                        .map(|s| s.number.clone())
                        .unwrap_or_default();
                    let out = with_extension(&self.path, &format!("{number}.svg"));
                    if let Some(svg) =
                        sheets::sheet_svg(self.doc.model(), sheet, &self.sheet_info())
                    {
                        let r = std::fs::write(&out, svg);
                        self.report(r, format!("Exported {out}"));
                    }
                } else if let Some(view) = self.active_view {
                    let name = self
                        .doc
                        .model()
                        .get(view)
                        .map(|e| e.name.replace(' ', "-"))
                        .unwrap_or_default();
                    let scale = self.view(view).map_or(100.0, |v| v.scale);
                    let out = with_extension(&self.path, &format!("{name}.svg"));
                    if let Some(d) = self.drawing(view) {
                        let r = std::fs::write(&out, d.to_svg(scale));
                        self.report(r, format!("Exported {out} at 1:{scale:.0}"));
                    }
                }
            }
            _ => {}
        }
    }

    /// The value the active view's number field shows: cut height, depth or
    /// altitude.
    fn view_number(&self) -> Option<(&'static str, f64, &'static str, ElementId)> {
        let id = self.active_view?;
        let v = self.view(id)?;
        match &v.kind {
            ViewKind::Plan { cut_height, .. } => Some(("Cut height", *cut_height, "m", id)),
            ViewKind::Section { depth, .. } => Some(("Depth", *depth, "m", id)),
            ViewKind::Axonometric { altitude, .. } => Some(("Altitude", *altitude, "°", id)),
            ViewKind::Schedule { .. } => None,
        }
    }

    fn set_view_number(&mut self, value: f64) {
        let Some(id) = self.active_view else { return };
        let Some(mut v) = self.view(id).cloned() else {
            return;
        };
        match &mut v.kind {
            ViewKind::Plan { cut_height, .. } => *cut_height = value.clamp(0.1, 200.0),
            ViewKind::Section { depth, .. } => *depth = value.clamp(0.1, 200.0),
            ViewKind::Axonometric { altitude, .. } => *altitude = value.clamp(-89.0, 89.0),
            ViewKind::Schedule { .. } => return,
        }
        let r = self
            .doc
            .edit_merged(id, "Change view", |e| e.kind = ElementKind::View(v.clone()));
        self.report(r, "View changed.");
    }

    fn view_action(&mut self, what: &str) {
        let Some(id) = self.active_view else { return };
        match what {
            "view.shaded" => {
                let Some(mut v) = self.view(id).cloned() else {
                    return;
                };
                v.style = if v.style == ViewStyle::Shaded {
                    ViewStyle::HiddenLine
                } else {
                    ViewStyle::Shaded
                };
                let r = self
                    .doc
                    .edit_merged(id, "Change view", |e| e.kind = ElementKind::View(v.clone()));
                self.report(r, "View changed.");
            }
            "view.reveal" => self.toggle_hidden_elements(id),
            "view.tag" => {
                let r = planview::tag_openings(&mut self.doc, id);
                self.status = match r {
                    Ok(0) => "Every door and window here is tagged already.".into(),
                    Ok(n) => format!("Tagged {n} doors and windows. Drag a tag to move it."),
                    Err(e) => format!("Couldn't do that: {e}"),
                };
            }
            _ => {}
        }
    }

    fn active_view_label(&self) -> String {
        let Some(id) = self.active_view else {
            return "No view".into();
        };
        let name = self
            .doc
            .model()
            .get(id)
            .map(|e| e.name.clone())
            .unwrap_or_default();
        match sheets::sheet_of(self.doc.model(), id) {
            Some(s) => format!("{} {name}", s.number),
            None => name,
        }
    }

    /// The tool options strip: what the tool in use (or the running Move
    /// or Rotate) can be set to. It has its own row under the tools, so it
    /// never covers a view or its buttons.
    fn tool_strip(&self, ctx: &egui::Context) -> (Value, Vec<Value>) {
        let seg = |items: Vec<Value>| json!({"kind": "seg", "items": items});
        let opt = |id: String, label: &str, on: bool, hint: &str| json!({"id": id, "label": label, "on": on, "hint": hint});
        let mut options = vec![];
        if let Some(m) = &self.plan.modal {
            let verb = format!("{:?}", m.mode);
            let icon = match m.mode {
                crate::modal::Mode::Move => "move",
                crate::modal::Mode::Rotate => "rotate-cw",
                crate::modal::Mode::Scale => "scaling",
            };
            return (json!({"label": verb, "icon": icon}), options);
        }
        if let Some((m, _)) = self.plan.armed {
            let verb = format!("{m:?}");
            return (
                json!({"label": verb, "icon": "move", "hint": self.status}),
                options,
            );
        }
        let tool = self.plan.tool;
        if tool == Tool::Wall {
            options.push(json!({
                "kind": "number", "key": "wall.thickness", "label": "Thickness",
                "value": self.plan.wall_thickness, "unit": "m", "step": 0.01, "min": 0.05, "max": 1.0,
                "hint": "Thickness of new walls. Drag the label or type a value.",
            }));
        }
        let redrawing = self.plan.outline.as_ref().is_some_and(|o| !o.is_wall());
        if tool.draws_outline() || redrawing {
            use crate::slabtool::SlabMode;
            if tool.draws_outline() {
                options.push(seg(SlabMode::ALL
                    .iter()
                    .enumerate()
                    .map(|(i, m)| {
                        opt(
                            format!("slab.mode:{i}"),
                            m.label(),
                            self.plan.slab_mode == *m,
                            m.hint(),
                        )
                    })
                    .collect()));
            }
            if !tool.draws_outline() || self.plan.slab_mode == SlabMode::Polygon {
                options.push(seg(vec![
                    opt(
                        "slab.arc:0".into(),
                        "Line (L)",
                        !self.plan.slab_arc,
                        "Straight sides",
                    ),
                    opt(
                        "slab.arc:1".into(),
                        "Arc (A)",
                        self.plan.slab_arc,
                        "The next side is an arc: click its end, then a point on it",
                    ),
                ]));
            }
        }
        if tool == Tool::Sketch {
            let now = crate::sketch::shape(ctx);
            options.push(seg(crate::sketch::Shape::ALL
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    opt(
                        format!("sketch.shape:{i}"),
                        s.label(),
                        now == *s,
                        "What the next clicks draw",
                    )
                })
                .collect()));
        }
        options.extend(crate::memberstool::tool_options(&self.plan));
        if tool == Tool::Note {
            options.push(json!({
                "kind": "toggle", "id": "note.leader", "label": "Leader",
                "on": self.plan.anno.leader, "icon": "arrow-up-right",
                "hint": "Draw an arrow to what the note is about: click its point first, then the text",
            }));
        }
        options.extend(self.drafting_options());
        // the Grid tool's grid system
        options.extend(
            self.grid_vis_options()
                .into_iter()
                .filter(|o| o["key"] == "gridgroups"),
        );
        while options.last().is_some_and(|o| o["kind"] == "sep") {
            options.pop();
        }
        let head = json!({"label": tool.label(), "icon": tool_icon(tool), "hint": tool.hint()});
        (head, options)
    }

    fn top_state(&self, ctx: &egui::Context) -> Value {
        let quick: Vec<Value> = [
            Some(Command::Undo),
            Some(Command::Redo),
            None,
            Some(Command::Move),
            Some(Command::Rotate),
            Some(Command::Scale),
            Some(Command::Duplicate),
            None,
            Some(Command::Copy),
            Some(Command::Cut),
            Some(Command::Paste),
            Some(Command::Delete),
            None,
            Some(Command::Hide),
            Some(Command::Isolate),
            Some(Command::ShowAll),
        ]
        .into_iter()
        .map(|c| match c {
            Some(c) => match self.cmd_item(c) {
                Item::Action {
                    id,
                    label,
                    shortcut,
                    enabled,
                    icon,
                    ..
                } => json!({"id": id, "label": label, "key": shortcut, "enabled": enabled, "icon": icon}),
                _ => Value::Null,
            },
            None => Value::Null,
        })
        .collect();
        let tools: Vec<Value> = Tool::ALL
            .iter()
            .enumerate()
            .map(|(i, t)| {
                json!({
                    "id": format!("tool:{i}"), "label": t.label(), "hint": t.hint(),
                    "icon": tool_icon(*t), "active": self.plan.tool == *t, "group": tool_group(*t),
                })
            })
            .collect();
        let (tool_head, tool_options) = self.tool_strip(ctx);
        let mut options = vec![];
        if let Some((label, value, unit, _)) = self.view_number() {
            let (step, min, max) = if unit == "°" {
                (1.0, -89.0, 89.0)
            } else {
                (0.05, 0.1, 200.0)
            };
            options.push(json!({
                "kind": "number", "key": "view.number", "label": label, "value": value,
                "unit": unit, "step": step, "min": min, "max": max,
                "hint": "For the view shown in the 2D pane",
            }));
            if let Some(v) = self.active_view.and_then(|id| self.view(id)) {
                if matches!(
                    v.kind,
                    ViewKind::Section { .. } | ViewKind::Axonometric { .. }
                ) {
                    options.push(json!({
                        "kind": "toggle", "id": "view.shaded", "label": "Shaded",
                        "on": v.style == ViewStyle::Shaded, "icon": "sun",
                        "hint": "Shade the view's surfaces",
                    }));
                }
                options.push(json!({
                    "kind": "toggle", "id": "view.reveal", "label": "Hidden",
                    "on": v.show_hidden, "icon": "xray",
                    "hint": "Show elements hidden inside others (a beam in a wall) with dashed lines",
                }));
            }
            options.push(json!({
                "kind": "button", "id": "view.tag", "label": "Tag openings", "icon": "tag",
                "hint": "Tag every door and window in this view that has no tag yet",
            }));
        }
        if let Some((view, level)) = self.active_plan() {
            let u = self.view(view).map(|v| v.underlay).unwrap_or_default();
            let value = self
                .underlay_choices(level)
                .into_iter()
                .find(|c| c.0 == u)
                .map_or("None".to_string(), |c| c.1);
            options.push(json!({
                "kind": "menu", "key": "underlay", "label": "Underlay", "value": value,
                "icon": "layers", "hint": "Show another level faintly under this plan, to trace and snap to",
            }));
        }
        options.extend(
            self.grid_vis_options()
                .into_iter()
                .filter(|o| o["key"] != "gridgroups" && o["kind"] != "sep"),
        );
        // lengths show and take the project's units; what is typed is read here
        for o in &mut options {
            if o["kind"] == "number" && o["unit"] == "m" {
                let v = o["value"].as_f64().unwrap_or(0.0);
                o["text"] = json!(crate::units::len(v));
                o["unit"] = json!("");
            }
        }
        if self.active_view.is_some_and(|id| self.is_sheet(id)) {
            options.push(json!({
                "kind": "menu", "key": "placeview", "label": "", "value": "Place view…",
                "icon": "frame", "hint": "Put a view on this sheet",
            }));
        }
        json!({
            "theme": self.theme(),
            "popup": self.popup_key(),
            "menus": (["File", "Edit", "View", "Create", "Materials"].map(|m| json!({"name": m, "key": format!("menu:{m}")}))),
            "quick": quick,
            "doc": {
                "name": file_label(&self.path),
                "path": self.path,
                "note": match self.selection.len() {
                    0 => String::new(),
                    1 => "1 selected".into(),
                    n => format!("{n} selected"),
                },
            },
            "right": [{
                "kind": "toggle", "id": "layout.toggle", "label": "",
                "on": self.shell.settings.single_view,
                "icon": if self.shell.settings.single_view { "square" } else { "columns-2" },
                "hint": if self.shell.settings.single_view {
                    "One view at a time: click to show the 3D view and the drawing side by side"
                } else {
                    "3D view and drawing side by side: click to show one view at a time"
                },
            }, {
                "kind": "menu", "key": "views", "label": "", "value": self.active_view_label(),
                "icon": "map", "big": true, "hint": "The view or sheet the 2D pane shows",
            }],
            "tools": tools,
            "tool": tool_head,
            "tool_options": tool_options,
            "options": options,
        })
    }

    fn left_state(&self) -> Value {
        let model = self.doc.model();
        let row = |e: &Element, opens: bool| {
            json!({
                "id": e.id.0.to_string(), "name": e.name, "icon": icon_name(Icon::of(e)),
                "sel": self.selection.contains(e.id),
                "active": opens && self.active_view == Some(e.id),
                "hint": if opens { format!("{}: click to open", e.name) } else { e.name.clone() },
            })
        };
        // views by kind, plans in their levels' order; drag a row to move it
        let mut out: Vec<Value> = crate::browser::view_groups(model)
            .into_iter()
            .map(|(g, ids)| {
                let rows: Vec<Value> = ids
                    .iter()
                    .filter_map(|id| model.get(*id))
                    .map(|e| row(e, true))
                    .collect();
                let manual = crate::browser::is_manual(model, g);
                let icon = match g {
                    crate::browser::ViewGroup::Plans => "map",
                    crate::browser::ViewGroup::Views3d => "axis-3d",
                    crate::browser::ViewGroup::Sections => "scissors-line-dashed",
                    crate::browser::ViewGroup::Elevations => "building-2",
                    crate::browser::ViewGroup::Schedules => "table-2",
                };
                let mut acts = vec![];
                if manual {
                    acts.push(json!({
                        "id": format!("browser.sort:{}", g.key()),
                        "icon": "sort-by-level",
                        "hint": match g {
                            crate::browser::ViewGroup::Plans => "Sort by level: put the plans back in the order of their levels, lowest first".to_string(),
                            _ => format!("{}: undo the order you dragged them into", g.auto_label()),
                        },
                    }));
                }
                json!({
                    "key": g.key(), "title": g.title(), "icon": icon, "open": true,
                    "rows": rows, "acts": acts, "sortable": true,
                    "note": if manual { "Your order" } else if g == crate::browser::ViewGroup::Plans { "By level" } else { "" },
                })
            })
            .collect();
        let mut groups: Vec<(String, Vec<&Element>)> = vec![];
        for e in model.iter() {
            if matches!(e.kind, ElementKind::Units(_) | ElementKind::View(_)) {
                continue;
            }
            let class = e.ifc_class().to_string();
            match groups.iter_mut().find(|(c, _)| *c == class) {
                Some((_, v)) => v.push(e),
                None => groups.push((class, vec![e])),
            }
        }
        let rank = |c: &str| match c {
            "View" => 0,
            "Sheet" => 1,
            "IfcBuildingStorey" => 2,
            _ => 3,
        };
        groups.sort_by_key(|(c, _)| rank(c));
        let groups: Vec<Value> = groups
            .into_iter()
            .map(|(class, items)| {
                let opens = class == "Sheet";
                let mut items = items;
                if class == "IfcBuildingStorey" {
                    // levels from the top down, as in a section
                    let z = |e: &Element| model.level(e.id).map_or(0.0, |l| l.elevation);
                    items.sort_by(|a, b| z(b).total_cmp(&z(a)));
                }
                let rows: Vec<Value> = items.iter().map(|e| row(e, opens)).collect();
                // levels and assemblies are managed from their groups
                let act = match class.as_str() {
                    "IfcBuildingStorey" => Some(json!([
                        {"id": "level.add", "icon": "plus", "hint": "Add a level above the top one, with its floor plan"},
                        {"id": "levels", "icon": "ellipsis", "hint": "Levels window: add, duplicate, rename, move and delete levels, and make their plans"},
                    ])),
                    "IfcMaterialLayerSet" => Some(json!([
                        {"id": "asm.browser", "icon": "ellipsis", "hint": "Assemblies: edit layers, add, duplicate, delete, assign"},
                    ])),
                    _ => None,
                };
                json!({
                    "key": class, "title": group_title(&class), "icon": icon_name(Icon::of_class(&class)),
                    "open": rank(&class) < 3, "rows": rows, "acts": act,
                })
            })
            .collect();
        out.extend(groups);
        let groups = out;
        json!({
            "theme": self.theme(),
            "count": format!("{} elements", model.len()),
            "groups": groups,
        })
    }

    fn status_state(&self) -> Value {
        let mut s = self.plan.snap;
        let n = s.switches().iter().filter(|(_, v)| **v).count();
        let total = s.switches().len();
        let snaps = match n {
            n if n == total => "All".to_string(),
            0 => "None".to_string(),
            n => format!("{n} of {total}"),
        };
        let workplane = match &self.workplane {
            Some((_, name)) => name.clone(),
            None => "Level".into(),
        };
        json!({
            "theme": self.theme(),
            "popup": self.popup_key(),
            "message": self.status,
            "controls": [
                {"kind": "toggle", "id": "snap.on", "label": "Snap", "on": self.plan.snap.on, "icon": "magnet",
                 "hint": "Object snaps on or off (F3). Hold Alt while picking to suspend them."},
                {"kind": "menu", "key": "snaps", "label": "", "value": snaps, "hint": "Which snaps are on"},
                {"kind": "sep"},
                {"kind": "menu", "key": "workplane", "label": "Workplane", "value": workplane, "icon": "square-dashed",
                 "hint": "Where sketches and guides drawn in 3D go"},
                {"kind": "menu", "key": "pivot", "label": "Pivot", "value": self.plan.pivot.label(), "icon": "crosshair",
                 "hint": "What rotate and scale turn about. Shift+right-click places the pivot cursor."},
                {"kind": "sep"},
                {"kind": "meta", "label": format!("{} elements", self.doc.model().len())},
            ],
        })
    }

    fn theme(&self) -> &'static str {
        self.shell.theme.id()
    }

    /// Switch theme everywhere and remember it for next time.
    pub(super) fn set_theme(&mut self, ctx: &egui::Context, t: Theme) {
        self.shell.theme = t;
        style(ctx, t);
        self.shell.settings.theme = t.id().into();
        self.shell.settings.store();
        self.status = format!("Theme: {}.", t.label());
    }

    fn popup_key(&self) -> Option<String> {
        self.shell.popup.as_ref().map(|p| p.key.clone())
    }

    /// While the palette is open, the keys typed in the views go to it: its
    /// page doesn't always get the keyboard when opened from a shortcut.
    pub(super) fn palette_keys(&mut self, raw: &mut egui::RawInput) {
        let Some(key) = self.popup_key() else { return };
        let Some(web) = &self.shell.web else { return };
        if key != "palette" {
            // arrows and Enter walk an open menu
            let mut keys = vec![];
            raw.events.retain(|e| match e {
                egui::Event::Key {
                    key, pressed: true, ..
                } if matches!(
                    key,
                    egui::Key::ArrowDown
                        | egui::Key::ArrowUp
                        | egui::Key::ArrowLeft
                        | egui::Key::ArrowRight
                        | egui::Key::Enter
                ) =>
                {
                    keys.push(match key {
                        egui::Key::ArrowDown => "ArrowDown",
                        egui::Key::ArrowUp => "ArrowUp",
                        egui::Key::ArrowLeft => "ArrowLeft",
                        egui::Key::ArrowRight => "ArrowRight",
                        _ => "Enter",
                    });
                    false
                }
                _ => true,
            });
            for k in keys {
                web.run(Pane::Popup, &format!("window.CB && CB.nav({})", json!(k)));
            }
            return;
        }
        let mut text = String::new();
        let mut keys = vec![];
        raw.events.retain(|e| match e {
            egui::Event::Text(t) => {
                text.push_str(t);
                false
            }
            egui::Event::Key { key, pressed, .. } => {
                if *pressed {
                    // the page's names, as a browser gives them
                    keys.push(match key {
                        egui::Key::ArrowDown => "ArrowDown",
                        egui::Key::ArrowUp => "ArrowUp",
                        k => k.name(),
                    });
                }
                false
            }
            _ => true,
        });
        if !text.is_empty() || !keys.is_empty() {
            web.run(
                Pane::Popup,
                &format!(
                    "window.CB && CB.feed({})",
                    json!({"text": text, "keys": keys})
                ),
            );
        }
    }

    /// The command palette: every action, found by typing.
    pub(super) fn open_palette(&mut self, ctx: &egui::Context) {
        let screen = ctx.content_rect();
        let w = 560.0f32.min(screen.width() - 32.0);
        self.shell.popup = Some(Popup {
            key: "palette".into(),
            at: egui::pos2(screen.center().x - w / 2.0, screen.top() + 90.0),
            up: None,
            size: egui::vec2(w, 380.0),
        });
        self.shell.focus_popup = true;
    }

    fn close_popup(&mut self) {
        self.shell.popup = None;
        if let Some(web) = &mut self.shell.web {
            web.hide_popup();
        }
    }

    fn handle_web(&mut self, ctx: &egui::Context, msg: Msg) {
        let b = &msg.body;
        let t = b["t"].as_str().unwrap_or("");
        match t {
            "ready" => {
                if let Some(web) = &mut self.shell.web {
                    web.resend(msg.pane);
                }
            }
            "down" if msg.pane != Pane::Popup => self.close_popup(),
            "focus" => {
                if let Some(web) = &mut self.shell.web {
                    web.focus_views();
                }
            }
            "close" => self.close_popup(),
            "reorder" => {
                let id = |k: &str| {
                    let s = b[k].as_str()?;
                    self.doc
                        .model()
                        .iter()
                        .find(|e| e.id.0.to_string() == s)
                        .map(|e| e.id)
                };
                if let Some(moved) = id("id") {
                    let tx = crate::browser::reorder(self.doc.model(), moved, id("before"));
                    let r = self.doc.commit(tx);
                    let name = self
                        .doc
                        .model()
                        .get(moved)
                        .map(|e| e.name.clone())
                        .unwrap_or_default();
                    self.report(
                        r,
                        format!("Moved {name}. The group keeps your order; its sort button puts it back."),
                    );
                }
            }
            "act" => {
                let id = b["id"].as_str().unwrap_or("").to_string();
                let keep = b["keep"].as_bool().unwrap_or(false);
                if msg.pane == Pane::Popup && !keep {
                    self.close_popup();
                }
                self.run_action(ctx, &id);
            }
            "popup" => {
                let Some(web) = &self.shell.web else { return };
                let origin = web.rects[msg.pane as usize].min;
                let x = b["x"].as_f64().unwrap_or(0.0) as f32;
                let y = b["y"].as_f64().unwrap_or(0.0) as f32;
                let w = b["w"].as_f64().unwrap_or(0.0) as f32;
                let up = b["up"].as_f64().map(|u| origin.y + u as f32);
                let key = b["key"].as_str().unwrap_or("").to_string();
                let n = self.menu_items(&key).len() as f32;
                self.shell.popup = Some(Popup {
                    key,
                    at: origin + egui::vec2(x, y),
                    up,
                    size: egui::vec2(w.max(220.0), 12.0 + 29.0 * n),
                });
            }
            "size" => {
                if let Some(p) = &mut self.shell.popup {
                    let h = b["h"].as_f64().unwrap_or(100.0) as f32;
                    // the palette keeps the width it was opened with
                    let w = match p.key.as_str() {
                        "palette" => p.size.x,
                        _ => b["w"].as_f64().unwrap_or(200.0) as f32,
                    };
                    p.size = egui::vec2(w, h);
                }
            }
            "set" => {
                // a length field sends what was typed, in any unit
                let v = match (b["value"].as_f64(), b["text"].as_str()) {
                    (Some(v), _) => v,
                    (None, Some(t)) => match crate::units::parse(t) {
                        Some(v) => v,
                        None => {
                            self.status = format!(
                                "Couldn't read \"{t}\" as a length. Try 3'-6 1/2\", 1067 or 1.2 m."
                            );
                            return;
                        }
                    },
                    _ => return,
                };
                match b["key"].as_str().unwrap_or("") {
                    "wall.thickness" => self.plan.wall_thickness = v.clamp(0.05, 1.0),
                    k if k.starts_with("member.") => {
                        crate::memberstool::set_option(&mut self.plan, k, v)
                    }
                    "view.number" => self.set_view_number(v),
                    _ => {}
                }
            }
            "pick" => {
                let want = b["id"].as_str().unwrap_or("");
                let Some(e) = self.doc.model().iter().find(|e| e.id.0.to_string() == want) else {
                    return;
                };
                let (id, opens) = (
                    e.id,
                    matches!(e.kind, ElementKind::View(_) | ElementKind::Sheet(_)),
                );
                if b["add"].as_bool().unwrap_or(false) {
                    self.selection.toggle(id);
                } else {
                    self.selection.only(id);
                    if opens {
                        self.active_view = Some(id);
                        // with one view at a time, opening a drawing shows it
                        if self.shell.settings.single_view && self.shell.settings.single_shows_3d {
                            self.shell.settings.single_shows_3d = false;
                            self.shell.settings.store();
                        }
                    }
                }
                self.face = None;
            }
            "key" => {
                let code = b["code"].as_str().unwrap_or("");
                let name = b["key"].as_str().unwrap_or("");
                let Some(key) = egui::Key::from_name(code).or_else(|| egui::Key::from_name(name))
                else {
                    return;
                };
                let ctrl = b["ctrl"].as_bool().unwrap_or(false);
                let modifiers = egui::Modifiers {
                    alt: b["alt"].as_bool().unwrap_or(false),
                    ctrl,
                    shift: b["shift"].as_bool().unwrap_or(false),
                    mac_cmd: false,
                    command: ctrl,
                };
                for pressed in [true, false] {
                    self.shell.keys.push(egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed,
                        repeat: b["repeat"].as_bool().unwrap_or(false),
                        modifiers,
                    });
                }
                if !ctrl && name.chars().count() == 1 {
                    self.shell.keys.push(egui::Event::Text(name.into()));
                }
                ctx.request_repaint();
            }
            _ => {}
        }
    }

    /// Lay out the web panes round the views. Returns false when there is
    /// no web interface, and the classic panels are drawn instead.
    pub(super) fn web_panels(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) -> bool {
        if self.shell.classic || self.shell.failed.is_some() {
            WebUi::pump();
            if let Some(web) = &self.shell.web {
                for m in web.take() {
                    let _ = m;
                }
            }
            return false;
        }
        if self.shell.web.is_none() {
            match WebUi::new(frame) {
                Ok(w) => self.shell.web = Some(w),
                Err(e) => {
                    self.status = format!("Using the classic panels: {e}");
                    self.shell.failed = Some(e);
                    return false;
                }
            }
        }
        let ctx = ui.ctx().clone();
        WebUi::pump();
        let msgs = self
            .shell
            .web
            .as_ref()
            .map(|w| w.take())
            .unwrap_or_default();
        for m in msgs {
            self.handle_web(&ctx, m);
        }
        if self.shell.classic {
            // just switched to the classic panels
            return false;
        }
        // A click in the views or Escape closes an open menu.
        if self.shell.popup.is_some()
            && ctx.input(|i| i.pointer.any_pressed() || i.key_pressed(egui::Key::Escape))
        {
            self.close_popup();
        }

        let ppp = ctx.pixels_per_point();
        let none = egui::Frame::NONE;
        let top = egui::Panel::top("web_top")
            .exact_size(TOP_H)
            .resizable(false)
            .show_separator_line(false)
            .frame(none)
            .show(ui, |_| ())
            .response
            .rect;
        let status = egui::Panel::bottom("web_status")
            .exact_size(STATUS_H)
            .resizable(false)
            .show_separator_line(false)
            .frame(none)
            .show(ui, |_| ())
            .response
            .rect;
        // drag the browser's right edge to widen it
        let left = egui::Panel::left("web_left")
            .default_size(self.shell.settings.left_width)
            .size_range(180.0..=520.0)
            .resizable(true)
            .show_separator_line(true)
            .frame(egui::Frame::NONE.fill(ui.visuals().panel_fill))
            .show(ui, |ui| ui.allocate_space(ui.available_size()))
            .response
            .rect;
        // the page sits just inside the edge so the edge stays grabbable
        let left = left.with_max_x(left.max.x - 3.0);
        let width = (left.width() + 3.0).round();
        if (width - self.shell.settings.left_width).abs() >= 1.0
            && !ctx.input(|i| i.pointer.any_down())
        {
            self.shell.settings.left_width = width;
            self.shell.settings.store();
        }

        // the 3D view's buttons, over its top-right corner (where it was
        // drawn last frame)
        let nav = self
            .rect_3d
            .filter(|r| r.width() > NAV_W + 40.0 && r.height() > 120.0)
            .map(|r| {
                // the overlaid 2D view takes the top-right corner
                let x = if self.placement == PlanPlacement::Overlay {
                    r.left() + 36.0
                } else {
                    r.right() - NAV_W - 8.0
                };
                egui::Rect::from_min_size(egui::pos2(x, r.top() + 8.0), egui::vec2(NAV_W, NAV_H))
            });
        let states = [
            (Pane::Top, top, self.top_state(&ctx)),
            (Pane::Left, left, self.left_state()),
            (Pane::Status, status, self.status_state()),
        ];
        let nav_state = self.nav_state();
        let screen = ctx.content_rect();
        let popup = self.shell.popup.as_ref().map(|p| {
            let size = popup_size(p, screen);
            let mut at = p.at;
            if let Some(bottom) = p.up {
                at.y = bottom - size.y;
            }
            at.x =
                at.x.min(screen.right() - size.x - 4.0)
                    .max(screen.left() + 4.0);
            at.y =
                at.y.min(screen.bottom() - size.y - 4.0)
                    .max(screen.top() + 4.0);
            let items = self.menu_items(&p.key);
            // the Create menu lists every tool: its sections side by side
            let columns = match p.key.as_str() {
                "menu:Create" => 3,
                _ => 1,
            };
            (
                egui::Rect::from_min_size(at, size),
                json!({"theme": self.theme(), "key": p.key, "items": ui_model::list(&items), "columns": columns}),
                items.is_empty(),
            )
        });
        let Some(web) = &mut self.shell.web else {
            return false;
        };
        for (pane, rect, state) in states {
            web.place(pane, rect, ppp);
            web.send(pane, &state);
        }
        match nav {
            Some(rect) => {
                web.place(Pane::Nav, rect, ppp);
                web.send(Pane::Nav, &nav_state);
            }
            None => web.park(Pane::Nav),
        }
        match popup {
            Some((_, _, true)) => self.close_popup(),
            Some((rect, state, false)) => {
                web.send(Pane::Popup, &state);
                web.show_popup(rect, ppp);
                if std::mem::take(&mut self.shell.focus_popup) {
                    web.focus(Pane::Popup);
                }
            }
            None => web.hide_popup(),
        }
        // Pages run on their own; keep frames coming so their messages are read.
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
        true
    }

    /// Show one view at a time, or the 3D view and the drawing side by side.
    fn set_single_view(&mut self, on: bool) {
        if on && self.placement != PlanPlacement::Docked {
            self.placement = PlanPlacement::Docked;
        }
        self.shell.settings.single_view = on;
        self.shell.settings.store();
        self.status = if on {
            "One view at a time: the tabs above the view switch between the 3D view and the drawing.".into()
        } else {
            "The 3D view and the drawing side by side.".into()
        };
    }

    /// The tabs over the single view: the 3D view, the drawing (its tab
    /// opens the list of views when it is already showing), and a way back
    /// to the split.
    pub(super) fn view_tabs(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let p = self.shell.theme.palette();
        ui.painter().rect_filled(rect, 0.0, p.panel);
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            egui::Stroke::new(1.0, p.border),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(egui::vec2(8.0, 3.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let three = self.shell.settings.single_shows_3d;
        let mut pick = None;
        if child
            .selectable_label(three, "3D view")
            .on_hover_text("The model in 3D")
            .clicked()
        {
            pick = Some(true);
        }
        let drawing = format!("{}  ⏷", self.active_view_label());
        let r = child.selectable_label(!three, drawing).on_hover_text(
            "The plan, section, sheet or schedule. Click again for the list of views.",
        );
        if r.clicked() {
            if !three && self.shell.web.is_some() {
                self.shell.popup = Some(Popup {
                    key: "views".into(),
                    at: r.rect.left_bottom() + egui::vec2(0.0, 4.0),
                    up: None,
                    size: egui::vec2(260.0, 300.0),
                });
            }
            pick = Some(false);
        }
        child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("Split")
                .on_hover_text("Show the 3D view and the drawing side by side")
                .clicked()
            {
                pick = None;
                self.set_single_view(false);
            }
        });
        if let Some(t) = pick {
            if t != three {
                self.shell.settings.single_shows_3d = t;
                self.shell.settings.store();
            }
        }
    }

    /// Open the right-click menu a view asked for this frame.
    pub(super) fn open_view_menu(&mut self) {
        let Some(pos) = self.plan.menu_request.take() else {
            return;
        };
        if self.shell.web.is_none() {
            return;
        }
        let n = self.menu_items("context").len() as f32;
        self.shell.context_3d = self.shell.menu_3d;
        self.shell.popup = Some(Popup {
            key: "context".into(),
            at: pos + egui::vec2(2.0, 2.0),
            up: None,
            size: egui::vec2(230.0, 12.0 + 28.0 * n),
        });
    }

    /// The classic egui menu bar, drawn from the same items as the web one.
    pub(super) fn classic_menu(&mut self, ui: &mut egui::Ui) {
        let mut picked = None;
        egui::MenuBar::new().ui(ui, |ui| {
            for m in ["File", "Edit", "View", "Materials"] {
                let items = self.menu_items(&format!("menu:{m}"));
                ui.menu_button(m, |ui| {
                    if let Some(id) = ui_model::show(ui, &items) {
                        picked = Some(id);
                    }
                });
            }
            ui.separator();
            ui.label("File:");
            ui.add(egui::TextEdit::singleline(&mut self.path).desired_width(220.0));
        });
        if let Some(id) = picked {
            let ctx = ui.ctx().clone();
            self.run_action(&ctx, &id);
        }
    }
}

/// How big a popup is shown: as its page asks, but never past the window.
/// A menu opening down keeps to the room below where it opens (or the
/// whole window height when that is too little); the page scrolls the rest.
fn popup_size(p: &Popup, screen: egui::Rect) -> egui::Vec2 {
    let margin = 6.0;
    let room = match p.up {
        Some(bottom) => bottom - screen.top() - margin,
        None => screen.bottom() - p.at.y - margin,
    };
    let room = if room < 240.0 {
        screen.height() - 2.0 * margin
    } else {
        room
    };
    egui::vec2(
        p.size.x.min(screen.width() - 2.0 * margin),
        p.size.y.min(room).max(40.0),
    )
}

/// egui's spacing and type, roomier than its defaults.
pub(super) fn style(ctx: &egui::Context, theme: Theme) {
    let p = theme.palette();
    let mode = if p.dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(mode);
    ctx.set_visuals_of(mode, visuals(theme));
    ctx.all_styles_mut(|s| {
        use egui::{FontId, TextStyle};
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
        s.spacing.interact_size.y = 24.0;
        // a slider's track thick enough to read as one
        s.spacing.slider_rail_height = 6.0;
        s.text_styles
            .insert(TextStyle::Body, FontId::proportional(13.0));
        s.text_styles
            .insert(TextStyle::Button, FontId::proportional(13.0));
        s.text_styles
            .insert(TextStyle::Heading, FontId::proportional(16.0));
    });
}

/// `a` moved `t` of the way to `b`.
fn mix(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    egui::Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// egui's look, matched to the web interface so the properties panel and
/// the views' own controls sit with it. Every control has a fill and an
/// outline that stand out from the window it is in, so a button reads as
/// a button, a dropdown as a field with its arrow, a text box as a box and
/// a slider as a track; hovering strengthens both.
pub(super) fn visuals(theme: Theme) -> egui::Visuals {
    use egui::{CornerRadius, Stroke};
    let p = theme.palette();
    let mut v = if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = p.panel;
    v.window_fill = p.panel2;
    v.faint_bg_color = p.panel2;
    v.extreme_bg_color = p.field;
    v.text_edit_bg_color = Some(p.field);
    v.slider_trailing_fill = true;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(8);
    // also the filled part of a slider's track
    v.selection.bg_fill = p.accent.gamma_multiply(if p.dark { 0.45 } else { 0.4 });
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.hyperlink_color = p.accent;
    v.weak_text_color = Some(p.muted);
    v.override_text_color = None;
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke.color = p.text;
    w.noninteractive.bg_fill = p.panel;
    w.inactive.fg_stroke.color = p.text;
    // buttons, dropdowns and slider tracks are filled a step off the
    // window; text fields stay light (dark in the dark themes), so the two
    // read apart
    let control = if p.dark {
        p.raise
    } else {
        mix(p.panel, p.raise2, 0.7)
    };
    let hover = if p.dark {
        p.raise2
    } else {
        mix(p.raise2, p.edge, 0.6)
    };
    for (state, fill, edge) in [
        (&mut w.inactive, control, p.edge),
        (&mut w.hovered, hover, p.muted),
        (&mut w.active, hover, p.accent),
        (&mut w.open, control, p.accent),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.corner_radius = CornerRadius::same(6);
        state.bg_stroke = Stroke::new(1.0, edge);
    }
    w.noninteractive.corner_radius = CornerRadius::same(6);
    v
}

/// The Align tool, in the Edit and right-click menus.
fn align_item() -> Item {
    Item::new("align", "Align to a reference…")
        .icon("align-tool")
        .hint("Pick a reference line (a wall face, an edge, a guide or a grid line), then an edge to bring flush with it")
}
