//! The tool options strip under the tools: the tool's own settings at
//! the left, the active view's at the right. Toggles and buttons with an
//! icon show only the icon (their tooltip says the rest), and what doesn't
//! fit the window's width goes in a More menu at the strip's end, so
//! nothing is ever pushed out of sight.

use serde_json::{json, Value};

/// A control in the strip as the page measured it.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub key: String,
    /// Its width, with its margins, in points.
    pub width: f32,
    /// One of the view's settings (else the tool's).
    pub view: bool,
}

/// What the page measured: every control and the room the strip has for
/// them, the gap between two, and the width of the More button.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Measure {
    pub slots: Vec<Slot>,
    pub avail: f32,
    pub gap: f32,
    pub more: f32,
}

impl Measure {
    /// From the page's message `{avail, gap, more, items: [{k, w, v}]}`.
    pub fn from_json(b: &Value) -> Self {
        let f = |k: &str| b[k].as_f64().unwrap_or(0.0) as f32;
        let slots = b["items"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|i| Slot {
                        key: i["k"].as_str().unwrap_or("").to_string(),
                        width: i["w"].as_f64().unwrap_or(0.0) as f32,
                        view: i["v"].as_bool().unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Measure {
            slots,
            avail: f("avail"),
            gap: f("gap"),
            more: f("more"),
        }
    }
}

/// Which slots go in the More menu, by index, for everything else to fit
/// in `avail`. The view's settings go first, from the right, then the
/// tool's, from the right: the tool in use keeps its options in sight
/// longest. A divider left at the end of what is shown goes too.
pub fn overflow(slots: &[Slot], avail: f32, gap: f32, more: f32) -> Vec<usize> {
    let width = |shown: &[bool]| -> f32 {
        let (n, w) = slots
            .iter()
            .zip(shown)
            .filter(|(_, s)| **s)
            .fold((0, 0.0), |(n, w), (slot, _)| (n + 1, w + slot.width));
        w + gap * (n.max(1) - 1) as f32
    };
    let mut shown = vec![true; slots.len()];
    if width(&shown) <= avail {
        return vec![];
    }
    // the More button takes room of its own
    let room = avail - more;
    let order = (0..slots.len())
        .rev()
        .filter(|i| slots[*i].view)
        .chain((0..slots.len()).rev().filter(|i| !slots[*i].view));
    for i in order {
        if width(&shown) <= room {
            break;
        }
        shown[i] = false;
    }
    // a divider with nothing after it in its group is no use
    for i in 0..slots.len() {
        let last_of_group = !slots[i + 1..]
            .iter()
            .zip(&shown[i + 1..])
            .any(|(s, on)| *on && s.view == slots[i].view);
        if shown[i] && slots[i].key.starts_with("sep") && last_of_group {
            shown[i] = false;
        }
    }
    (0..slots.len()).filter(|i| !shown[*i]).collect()
}

/// Give each control a key the page measures it by, and say which are
/// drawn as an icon alone: in the view's settings every control with an
/// icon; in the tool's the toggles and buttons with one, and menus with no
/// label. A control can keep its words with `"wide": true`.
pub fn prepare(controls: &mut [Value], view: bool) {
    for (i, c) in controls.iter_mut().enumerate() {
        let kind = c["kind"].as_str().unwrap_or("").to_string();
        let name = c["id"]
            .as_str()
            .or_else(|| c["key"].as_str())
            .unwrap_or("")
            .to_string();
        let slot = match kind.as_str() {
            "sep" => format!("sep{i}"),
            _ => format!("{}{i}:{name}", if view { "v" } else { "t" }),
        };
        let has_icon = c["icon"].as_str().is_some_and(|s| !s.is_empty());
        let wide = c["wide"].as_bool().unwrap_or(false);
        let compact = has_icon
            && !wide
            && match kind.as_str() {
                "toggle" | "button" => true,
                "menu" => view || c["label"].as_str().unwrap_or("").is_empty(),
                _ => false,
            };
        c["slot"] = json!(slot);
        if compact {
            c["compact"] = json!(true);
        }
    }
}

/// Mark the controls of `tool` and `view` that go in More, given what the
/// page last measured. Returns how many. Nothing is moved while the
/// measure is of other controls than these (the page measures again as
/// soon as it draws them).
pub fn place(tool: &mut [Value], view: &mut [Value], m: Option<&Measure>) -> usize {
    let Some(m) = m else { return 0 };
    let keys: Vec<&str> = tool
        .iter()
        .chain(view.iter())
        .map(|c| c["slot"].as_str().unwrap_or(""))
        .collect();
    if keys.len() != m.slots.len() || keys.iter().zip(&m.slots).any(|(k, s)| *k != s.key) {
        return 0;
    }
    let hidden = overflow(&m.slots, m.avail, m.gap, m.more);
    let n = tool.len();
    for &i in &hidden {
        let c = if i < n {
            &mut tool[i]
        } else {
            &mut view[i - n]
        };
        c["more"] = json!(true);
    }
    hidden
        .iter()
        .filter(|i| !m.slots[**i].key.starts_with("sep"))
        .count()
}

/// The More menu's rows: the controls marked for it, the tool's then the
/// view's, each group under its heading.
pub fn more_rows(tool: &[Value], view: &[Value], tool_name: &str) -> Vec<Value> {
    let mut out = vec![];
    for (list, head) in [
        (tool, format!("{tool_name} tool")),
        (view, "This view".into()),
    ] {
        let rows: Vec<&Value> = list
            .iter()
            .filter(|c| c["more"].as_bool() == Some(true) && c["kind"] != "sep")
            .collect();
        if rows.is_empty() {
            continue;
        }
        out.push(json!({"kind": "head", "label": head}));
        out.extend(rows.into_iter().cloned());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(key: &str, width: f32, view: bool) -> Slot {
        Slot {
            key: key.into(),
            width,
            view,
        }
    }

    #[test]
    fn everything_shows_when_it_fits() {
        let s = [slot("t0", 100.0, false), slot("v0", 30.0, true)];
        assert!(overflow(&s, 134.0, 4.0, 60.0).is_empty());
    }

    #[test]
    fn the_views_settings_go_first_from_the_right() {
        let s = [
            slot("t0", 100.0, false),
            slot("t1", 100.0, false),
            slot("v0", 30.0, true),
            slot("v1", 30.0, true),
            slot("v2", 30.0, true),
        ];
        // all: 290 + 4 gaps of 5 = 310; with the More button (40), the two
        // last view controls go
        assert_eq!(overflow(&s, 300.0, 5.0, 40.0), vec![3, 4]);
        // narrower: every view control, then the tool's last
        assert_eq!(overflow(&s, 200.0, 5.0, 40.0), vec![1, 2, 3, 4]);
        // so narrow nothing fits: all of it is in More
        assert_eq!(overflow(&s, 20.0, 5.0, 40.0), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn a_divider_left_at_the_end_goes_too() {
        let s = [
            slot("t0", 100.0, false),
            slot("sep1", 13.0, false),
            slot("t2", 100.0, false),
            slot("v0", 30.0, true),
        ];
        let hidden = overflow(&s, 160.0, 4.0, 40.0);
        assert_eq!(hidden, vec![1, 2, 3]);
    }

    #[test]
    fn icons_alone_where_they_say_enough() {
        let mut tool = vec![
            json!({"kind": "toggle", "id": "member:footing.strip", "label": "Strip", "icon": "footing"}),
            json!({"kind": "number", "key": "member.footing_width", "label": "Width"}),
            json!({"kind": "menu", "key": "type.tool", "label": "Type", "value": "Strip", "icon": "x"}),
            json!({"kind": "menu", "key": "guides", "label": "", "value": "Guides", "icon": "snap-guides"}),
            json!({"kind": "toggle", "id": "object:catalogue", "label": "Basin", "icon": "sofa", "wide": true}),
            json!({"kind": "sep"}),
        ];
        prepare(&mut tool, false);
        let compact: Vec<bool> = tool.iter().map(|c| c["compact"] == true).collect();
        assert_eq!(compact, [true, false, false, true, false, false]);
        assert_eq!(tool[0]["slot"], "t0:member:footing.strip");
        assert_eq!(tool[5]["slot"], "sep5");
        let mut view = vec![
            json!({"kind": "menu", "key": "underlay", "label": "Underlay", "value": "None", "icon": "layers"}),
        ];
        prepare(&mut view, true);
        assert_eq!(view[0]["compact"], true);
    }

    #[test]
    fn controls_are_marked_only_against_their_own_measure() {
        let mut tool = vec![json!({"kind": "number", "key": "a", "label": "A"})];
        let mut view = vec![
            json!({"kind": "toggle", "id": "b", "label": "B", "icon": "eye"}),
            json!({"kind": "button", "id": "c", "label": "C", "icon": "tag"}),
        ];
        prepare(&mut tool, false);
        prepare(&mut view, true);
        let m = Measure {
            slots: vec![
                slot("t0:a", 120.0, false),
                slot("v0:b", 28.0, true),
                slot("v1:c", 28.0, true),
            ],
            avail: 182.0,
            gap: 4.0,
            more: 30.0,
        };
        assert_eq!(place(&mut tool, &mut view, Some(&m)), 1);
        assert_eq!(view[1]["more"], true);
        assert!(view[0].get("more").is_none());
        let rows = more_rows(&tool, &view, "Footing");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["label"], "This view");
        assert_eq!(rows[1]["id"], "c");
        // a measure of other controls moves nothing
        let mut other = vec![json!({"kind": "number", "key": "z", "label": "Z"})];
        prepare(&mut other, false);
        let mut none: Vec<Value> = vec![];
        assert_eq!(place(&mut other, &mut none, Some(&m)), 0);
    }
}
