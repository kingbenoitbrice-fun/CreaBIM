//! Menus described as data, so the same menu can be drawn by the web
//! interface or by egui, and each item runs one named action.

use eframe::egui;
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Action {
        id: String,
        label: String,
        shortcut: &'static str,
        hint: String,
        enabled: bool,
        /// Some for a check or radio item.
        checked: Option<bool>,
        /// Leave the menu open after picking it (for check lists).
        keep_open: bool,
        icon: &'static str,
    },
    Separator,
    Header(String),
    Sub {
        label: String,
        enabled: bool,
        items: Vec<Item>,
        icon: &'static str,
    },
}

impl Item {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Item::Action {
            id: id.into(),
            label: label.into(),
            shortcut: "",
            hint: String::new(),
            enabled: true,
            checked: None,
            keep_open: false,
            icon: "",
        }
    }

    pub fn sub(label: impl Into<String>, items: Vec<Item>) -> Self {
        Item::Sub {
            enabled: !items.is_empty(),
            label: label.into(),
            items,
            icon: "",
        }
    }

    pub fn key(mut self, k: &'static str) -> Self {
        if let Item::Action { shortcut, .. } = &mut self {
            *shortcut = k;
        }
        self
    }

    pub fn hint(mut self, h: impl Into<String>) -> Self {
        if let Item::Action { hint, .. } = &mut self {
            *hint = h.into();
        }
        self
    }

    pub fn enabled(mut self, on: bool) -> Self {
        match &mut self {
            Item::Action { enabled, .. } | Item::Sub { enabled, .. } => *enabled = on,
            _ => {}
        }
        self
    }

    pub fn checked(mut self, on: bool) -> Self {
        if let Item::Action { checked, .. } = &mut self {
            *checked = Some(on);
        }
        self
    }

    pub fn keep_open(mut self) -> Self {
        if let Item::Action { keep_open, .. } = &mut self {
            *keep_open = true;
        }
        self
    }

    pub fn icon(mut self, name: &'static str) -> Self {
        if let Item::Action { icon, .. } | Item::Sub { icon, .. } = &mut self {
            *icon = name;
        }
        self
    }

    pub fn to_json(&self) -> Value {
        match self {
            Item::Action {
                id,
                label,
                shortcut,
                hint,
                enabled,
                checked,
                keep_open,
                icon,
            } => json!({
                "id": id, "label": label, "key": shortcut, "hint": hint,
                "enabled": enabled, "checked": checked, "keep": keep_open, "icon": icon,
            }),
            Item::Separator => json!({ "sep": true }),
            Item::Header(t) => json!({ "header": t }),
            Item::Sub {
                label,
                enabled,
                items,
                icon,
            } => json!({ "label": label, "enabled": enabled, "items": list(items), "icon": icon }),
        }
    }
}

pub fn list(items: &[Item]) -> Value {
    Value::Array(items.iter().map(Item::to_json).collect())
}

/// Draw the items in an egui menu. Returns the action picked, if any.
pub fn show(ui: &mut egui::Ui, items: &[Item]) -> Option<String> {
    let mut picked = None;
    for item in items {
        match item {
            Item::Action {
                id,
                label,
                shortcut,
                hint,
                enabled,
                checked,
                keep_open,
                ..
            } => {
                let text = match checked {
                    Some(true) => format!("✔ {label}"),
                    Some(false) => format!("    {label}"),
                    None => label.clone(),
                };
                let mut r =
                    ui.add_enabled(*enabled, egui::Button::new(text).shortcut_text(*shortcut));
                if !hint.is_empty() {
                    r = r.on_hover_text(hint);
                }
                if r.clicked() {
                    picked = Some(id.clone());
                    if !keep_open {
                        ui.close();
                    }
                }
            }
            Item::Separator => {
                ui.separator();
            }
            Item::Header(t) => {
                ui.label(egui::RichText::new(t).weak());
            }
            Item::Sub {
                label,
                enabled,
                items,
                ..
            } => {
                ui.add_enabled_ui(*enabled, |ui| {
                    ui.menu_button(label, |ui| {
                        if let Some(id) = show(ui, items) {
                            picked = Some(id);
                        }
                    });
                });
            }
        }
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_turn_into_json_the_web_menu_reads() {
        let items = vec![
            Item::new("save", "Save").key("Ctrl+S"),
            Item::Separator,
            Item::sub("Open recent", vec![Item::new("recent:0", "a.creabim")]).icon("folder"),
            Item::new("snap:0", "Endpoint").checked(true).keep_open(),
        ];
        let j = list(&items);
        assert_eq!(j[0]["id"], "save");
        assert_eq!(j[0]["key"], "Ctrl+S");
        assert_eq!(j[1]["sep"], true);
        assert_eq!(j[2]["items"][0]["id"], "recent:0");
        assert_eq!(j[2]["icon"], "folder");
        assert_eq!(j[3]["checked"], true);
        assert_eq!(j[3]["keep"], true);
    }
}
