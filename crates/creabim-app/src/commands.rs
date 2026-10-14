//! Edits that can be started from a key, a toolbar button or the right-click
//! menu alike, so everything can be done with the mouse alone.

use crate::modal::Mode;
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Move,
    Rotate,
    Scale,
    Duplicate,
    Copy,
    Cut,
    Paste,
    Delete,
    Hide,
    Isolate,
    ShowAll,
    SelectAll,
    SelectNone,
    Undo,
    Redo,
    /// Put the pivot cursor where the menu was opened.
    CursorHere,
    /// Put the workplane on the face (in 3D) or level (in a plan) where the
    /// menu was opened.
    WorkplaneHere,
}

impl Command {
    pub fn label(self) -> &'static str {
        match self {
            Command::Move => "Move",
            Command::Rotate => "Rotate",
            Command::Scale => "Scale",
            Command::Duplicate => "Duplicate",
            Command::Copy => "Copy",
            Command::Cut => "Cut",
            Command::Paste => "Paste in place",
            Command::Delete => "Delete",
            Command::Hide => "Hide",
            Command::Isolate => "Show only selected",
            Command::ShowAll => "Show everything",
            Command::SelectAll => "Select all",
            Command::SelectNone => "Select nothing",
            Command::Undo => "Undo",
            Command::Redo => "Redo",
            Command::CursorHere => "Place pivot cursor here",
            Command::WorkplaneHere => "Set workplane here",
        }
    }

    pub fn shortcut(self) -> &'static str {
        match self {
            Command::Move => "G",
            Command::Rotate => "R",
            Command::Scale => "S",
            Command::Duplicate => "Shift+D",
            Command::Copy => "Ctrl+C",
            Command::Cut => "Ctrl+X",
            Command::Paste => "Ctrl+V",
            Command::Delete => "Del",
            Command::Hide => "H",
            Command::Isolate => "Shift+H",
            Command::ShowAll => "Alt+H",
            Command::SelectAll => "Ctrl+A",
            Command::SelectNone => "Alt+A",
            Command::Undo => "Ctrl+Z",
            Command::Redo => "Ctrl+Y",
            Command::CursorHere => "Shift+right-click",
            Command::WorkplaneHere => "",
        }
    }

    /// Does it act on the selection, so it needs one?
    pub fn needs_selection(self) -> bool {
        matches!(
            self,
            Command::Move
                | Command::Rotate
                | Command::Scale
                | Command::Duplicate
                | Command::Copy
                | Command::Cut
                | Command::Delete
                | Command::Hide
                | Command::Isolate
                | Command::SelectNone
        )
    }

    /// The pointer edit it starts, if it is one: (mode, copy first).
    pub fn modal(self) -> Option<(Mode, bool)> {
        match self {
            Command::Move => Some((Mode::Move, false)),
            Command::Rotate => Some((Mode::Rotate, false)),
            Command::Scale => Some((Mode::Scale, false)),
            Command::Duplicate => Some((Mode::Move, true)),
            _ => None,
        }
    }

    /// A button for it, greyed out when it can't run.
    pub fn button(self, ui: &mut egui::Ui, selected: bool) -> bool {
        let enabled = selected || !self.needs_selection();
        ui.add_enabled(
            enabled,
            egui::Button::new(self.label()).shortcut_text(self.shortcut()),
        )
        .clicked()
    }
}

/// The right-click menu on a view in the Select tool. Returns the command
/// picked, if any.
pub fn context_menu(ui: &mut egui::Ui, selected: usize) -> Option<Command> {
    let some = selected > 0;
    let mut picked = None;
    let mut item = |ui: &mut egui::Ui, c: Command| {
        if c.button(ui, some) {
            picked = Some(c);
            ui.close();
        }
    };
    if some {
        ui.label(
            egui::RichText::new(match selected {
                1 => "1 selected".to_string(),
                n => format!("{n} selected"),
            })
            .weak(),
        );
        for c in [
            Command::Move,
            Command::Rotate,
            Command::Scale,
            Command::Duplicate,
        ] {
            item(ui, c);
        }
        ui.separator();
        for c in [Command::Copy, Command::Cut, Command::Delete] {
            item(ui, c);
        }
        ui.separator();
        for c in [Command::Hide, Command::Isolate] {
            item(ui, c);
        }
        ui.separator();
    }
    item(ui, Command::Paste);
    item(ui, Command::ShowAll);
    ui.separator();
    item(ui, Command::SelectAll);
    if some {
        item(ui, Command::SelectNone);
    }
    item(ui, Command::CursorHere);
    item(ui, Command::WorkplaneHere);
    picked
}

/// The row of edit buttons under the tools.
pub fn modify_bar(ui: &mut egui::Ui, selected: bool) -> Option<Command> {
    let mut picked = None;
    for group in [
        &[Command::Undo, Command::Redo][..],
        &[
            Command::Move,
            Command::Rotate,
            Command::Scale,
            Command::Duplicate,
        ],
        &[Command::Copy, Command::Cut, Command::Paste, Command::Delete],
        &[Command::Hide, Command::Isolate, Command::ShowAll],
        &[Command::SelectAll, Command::SelectNone],
    ] {
        for c in group {
            let enabled = selected || !c.needs_selection();
            if ui
                .add_enabled(enabled, egui::Button::new(c.label()).small())
                .on_hover_text(c.shortcut())
                .clicked()
            {
                picked = Some(*c);
            }
        }
        ui.separator();
    }
    picked
}
