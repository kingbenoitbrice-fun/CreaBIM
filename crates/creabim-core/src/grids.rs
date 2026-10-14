//! Grid systems: named groups of grid lines with their own labelling, so
//! one project can carry building A's grid (A-1, A-2…) beside building B's
//! (B-A, B-B…) without the labels running into each other.
//!
//! A grid line belongs to at most one group ([`Grid::group`]). Its stored
//! label is the part after the group's prefix, so renaming the prefix
//! relabels every line of the group at once. Lines running north–south
//! (more along y than x) and lines running east–west are counted
//! separately, each in its own style: letters or numbers.

use crate::{Element, ElementId, ElementKind, Grid, Model, Transaction};
use glam::DVec2;
use serde::{Deserialize, Serialize};

/// How a set of grid lines is counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LabelStyle {
    /// 1, 2, 3…
    #[default]
    Numbers,
    /// A, B, C… (I and O are skipped, as they read like 1 and 0), then AA, AB…
    Letters,
}

/// Letters used for grid labels: I and O are left out.
const LETTERS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";

impl LabelStyle {
    pub const ALL: [LabelStyle; 2] = [LabelStyle::Numbers, LabelStyle::Letters];

    pub fn label(self) -> &'static str {
        match self {
            LabelStyle::Numbers => "1, 2, 3…",
            LabelStyle::Letters => "A, B, C…",
        }
    }

    /// The `n`th label, counting from 1.
    pub fn format(self, n: u32) -> String {
        match self {
            LabelStyle::Numbers => n.to_string(),
            LabelStyle::Letters => {
                // bijective base 24: A..Z, AA, AB…
                let base = LETTERS.len() as u32;
                let mut n = n.max(1);
                let mut out = Vec::new();
                while n > 0 {
                    n -= 1;
                    out.push(LETTERS[(n % base) as usize]);
                    n /= base;
                }
                out.reverse();
                String::from_utf8(out).unwrap_or_default()
            }
        }
    }

    /// Which label `text` is in this style, counting from 1.
    pub fn parse(self, text: &str) -> Option<u32> {
        let text = text.trim();
        match self {
            LabelStyle::Numbers => text.parse::<u32>().ok().filter(|n| *n > 0),
            LabelStyle::Letters => {
                if text.is_empty() {
                    return None;
                }
                let base = LETTERS.len() as u32;
                let mut n: u32 = 0;
                for c in text.bytes() {
                    let d = LETTERS.iter().position(|l| *l == c.to_ascii_uppercase())? as u32;
                    n = n.checked_mul(base)?.checked_add(d + 1)?;
                }
                Some(n)
            }
        }
    }
}

/// A grid system: its name is the element's name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GridGroup {
    /// Put before every label of the group: "A-" gives A-1, A-2…
    #[serde(default)]
    pub prefix: String,
    /// How lines running north–south (along y) are counted, west to east.
    #[serde(default = "letters")]
    pub vertical: LabelStyle,
    /// How lines running east–west (along x) are counted, south to north.
    #[serde(default)]
    pub horizontal: LabelStyle,
    /// sRGB, 0..1: the group's lines and bubbles in 3D and in lists.
    #[serde(default = "default_color")]
    pub color: [f32; 3],
}

fn letters() -> LabelStyle {
    LabelStyle::Letters
}

fn default_color() -> [f32; 3] {
    PALETTE[0]
}

/// Colours given to new groups in turn.
pub const PALETTE: [[f32; 3]; 6] = [
    [0.20, 0.42, 0.78],
    [0.82, 0.36, 0.16],
    [0.16, 0.60, 0.38],
    [0.62, 0.30, 0.70],
    [0.76, 0.60, 0.10],
    [0.15, 0.58, 0.66],
];

/// The colour of ungrouped grid lines.
pub const UNGROUPED_COLOR: [f32; 3] = [0.40, 0.46, 0.58];

impl Default for GridGroup {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            vertical: LabelStyle::Letters,
            horizontal: LabelStyle::Numbers,
            color: default_color(),
        }
    }
}

impl GridGroup {
    /// The style lines from `start` to `end` are counted in.
    pub fn style_for(&self, start: DVec2, end: DVec2) -> LabelStyle {
        if runs_north_south(start, end) {
            self.vertical
        } else {
            self.horizontal
        }
    }
}

/// Does a line run more along y than along x?
pub fn runs_north_south(start: DVec2, end: DVec2) -> bool {
    let d = end - start;
    d.y.abs() >= d.x.abs()
}

/// Every grid group in the model, in model order.
pub fn groups(model: &Model) -> Vec<(&Element, &GridGroup)> {
    model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::GridGroup(g) => Some((e, g)),
            _ => None,
        })
        .collect()
}

/// The group a grid line belongs to, if it (still) exists.
pub fn group_of<'a>(model: &'a Model, grid: &Grid) -> Option<(&'a Element, &'a GridGroup)> {
    let e = model.get(grid.group?)?;
    match &e.kind {
        ElementKind::GridGroup(g) => Some((e, g)),
        _ => None,
    }
}

/// A grid line's label as drawn: its group's prefix, then its own label.
pub fn display_label(model: &Model, grid: &Grid) -> String {
    match group_of(model, grid) {
        Some((_, g)) => format!("{}{}", g.prefix, grid.label),
        None => grid.label.clone(),
    }
}

/// The colour a grid line is drawn in: its group's, or the ungrouped one.
pub fn color_of(model: &Model, grid: &Grid) -> [f32; 3] {
    group_of(model, grid).map_or(UNGROUPED_COLOR, |(_, g)| g.color)
}

/// The label a new line from `start` to `end` gets in `group` (`None` for
/// ungrouped lines): one past the highest label of the lines of that group
/// running the same way. Without a prefix, as stored.
pub fn next_label(model: &Model, group: Option<ElementId>, start: DVec2, end: DVec2) -> String {
    let group = group.filter(|g| {
        matches!(
            model.get(*g).map(|e| &e.kind),
            Some(ElementKind::GridGroup(_))
        )
    });
    let scheme = group
        .and_then(|g| match &model.get(g)?.kind {
            ElementKind::GridGroup(gg) => Some(gg.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let ns = runs_north_south(start, end);
    let style = scheme.style_for(start, end);
    let highest = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Grid(g) if g.group == group => Some(g),
            _ => None,
        })
        .filter(|g| runs_north_south(g.start, g.end) == ns)
        .filter_map(|g| style.parse(&g.label))
        .max()
        .unwrap_or(0);
    style.format(highest + 1)
}

/// A new grid line in `group`, labelled to follow on from its group.
pub fn new_grid(model: &Model, group: Option<ElementId>, start: DVec2, end: DVec2) -> Element {
    let group = group.filter(|g| {
        matches!(
            model.get(*g).map(|e| &e.kind),
            Some(ElementKind::GridGroup(_))
        )
    });
    let label = next_label(model, group, start, end);
    let grid = Grid {
        label,
        start,
        end,
        group,
    };
    let name = format!("Grid {}", display_label(model, &grid));
    Element::new(name, ElementKind::Grid(grid))
}

/// A new, empty grid group with a name, prefix and colour no other group has.
pub fn new_group(model: &Model) -> Element {
    let existing = groups(model);
    let n = existing.len();
    let taken = |prefix: &str| existing.iter().any(|(_, g)| g.prefix == prefix);
    let mut k = n as u32 + 1;
    while taken(&format!("{}-", LabelStyle::Letters.format(k))) {
        k += 1;
    }
    let letter = LabelStyle::Letters.format(k);
    Element::new(
        format!("Building {letter} grid"),
        ElementKind::GridGroup(GridGroup {
            prefix: format!("{letter}-"),
            color: PALETTE[n % PALETTE.len()],
            ..GridGroup::default()
        }),
    )
}

/// Relabel the lines of `group` (`None`: the ungrouped ones) in order:
/// north–south lines west to east, east–west lines south to north.
/// Returns one undo step, empty when nothing changes.
pub fn renumber(model: &Model, group: Option<ElementId>) -> Transaction {
    let scheme = group
        .and_then(|g| match &model.get(g)?.kind {
            ElementKind::GridGroup(gg) => Some(gg.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let mut tx = Transaction::new("Renumber grid lines");
    for ns in [true, false] {
        let mut lines: Vec<(&Element, &Grid)> = model
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::Grid(g) if g.group == group => Some((e, g)),
                _ => None,
            })
            .filter(|(_, g)| runs_north_south(g.start, g.end) == ns)
            .collect();
        let key = |g: &Grid| {
            let m = (g.start + g.end) / 2.0;
            if ns {
                m.x
            } else {
                m.y
            }
        };
        lines.sort_by(|a, b| key(a.1).total_cmp(&key(b.1)));
        let style = if ns {
            scheme.vertical
        } else {
            scheme.horizontal
        };
        for (i, (e, g)) in lines.into_iter().enumerate() {
            let label = style.format(i as u32 + 1);
            if g.label != label {
                let mut after = e.clone();
                if let ElementKind::Grid(ag) = &mut after.kind {
                    ag.label = label;
                    after.name = format!("Grid {}", display_label(model, ag));
                }
                tx = tx.replace(e.clone(), after);
            }
        }
    }
    tx
}

/// Ops that take the grid lines of the groups in `removed` out of their
/// group, for a transaction that deletes those groups: the lines stay.
pub fn release_lines(model: &Model, removed: &[ElementId]) -> Vec<crate::Op> {
    model
        .iter()
        .filter(|e| !removed.contains(&e.id))
        .filter_map(|e| match &e.kind {
            ElementKind::Grid(g) if g.group.is_some_and(|gr| removed.contains(&gr)) => {
                let mut after = e.clone();
                if let ElementKind::Grid(ag) = &mut after.kind {
                    // keep the label as it read, prefix included
                    ag.label = display_label(model, g);
                    ag.group = None;
                }
                Some(crate::Op::Replace {
                    before: e.clone(),
                    after,
                })
            }
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(model: &mut Model, group: Option<ElementId>, a: DVec2, b: DVec2) -> Element {
        let e = new_grid(model, group, a, b);
        model.insert(e.clone()).unwrap();
        e
    }

    fn label(model: &Model, e: &Element) -> String {
        match &model.get(e.id).unwrap().kind {
            ElementKind::Grid(g) => display_label(model, g),
            _ => unreachable!(),
        }
    }

    #[test]
    fn letters_skip_i_and_o_and_roll_over() {
        let l = LabelStyle::Letters;
        assert_eq!(l.format(1), "A");
        assert_eq!(l.format(8), "H");
        assert_eq!(l.format(9), "J");
        assert_eq!(l.format(24), "Z");
        assert_eq!(l.format(25), "AA");
        for n in 1..2000 {
            assert_eq!(l.parse(&l.format(n)), Some(n));
            assert_eq!(
                LabelStyle::Numbers.parse(&LabelStyle::Numbers.format(n)),
                Some(n)
            );
        }
        assert_eq!(l.parse("I"), None);
        assert_eq!(LabelStyle::Numbers.parse("A"), None);
    }

    #[test]
    fn labels_continue_within_each_group_and_direction() {
        let mut m = Model::new();
        let a = new_group(&m);
        m.insert(a.clone()).unwrap();
        let b = new_group(&m);
        m.insert(b.clone()).unwrap();
        // the second group gets its own prefix and colour
        let (ElementKind::GridGroup(ga), ElementKind::GridGroup(gb)) = (&a.kind, &b.kind) else {
            unreachable!()
        };
        assert_eq!(ga.prefix, "A-");
        assert_eq!(gb.prefix, "B-");
        assert_ne!(ga.color, gb.color);

        let ns = |x: f64| (DVec2::new(x, -1.0), DVec2::new(x, 10.0));
        let ew = |y: f64| (DVec2::new(-1.0, y), DVec2::new(10.0, y));
        let a1 = line(&mut m, Some(a.id), ns(0.0).0, ns(0.0).1);
        let a2 = line(&mut m, Some(a.id), ns(5.0).0, ns(5.0).1);
        let a3 = line(&mut m, Some(a.id), ew(0.0).0, ew(0.0).1);
        let b1 = line(&mut m, Some(b.id), ns(30.0).0, ns(30.0).1);
        let u1 = line(&mut m, None, ew(40.0).0, ew(40.0).1);
        assert_eq!(label(&m, &a1), "A-A");
        assert_eq!(label(&m, &a2), "A-B");
        assert_eq!(label(&m, &a3), "A-1");
        assert_eq!(label(&m, &b1), "B-A");
        assert_eq!(label(&m, &u1), "1");

        // a new prefix relabels the whole group
        let mut renamed = m.get(a.id).unwrap().clone();
        if let ElementKind::GridGroup(g) = &mut renamed.kind {
            g.prefix = "North ".into();
        }
        m.replace(renamed).unwrap();
        assert_eq!(label(&m, &a2), "North B");
        assert_eq!(label(&m, &b1), "B-A");
    }

    #[test]
    fn renumber_orders_lines_and_deleting_a_group_keeps_its_lines() {
        let mut m = Model::new();
        let g = new_group(&m);
        m.insert(g.clone()).unwrap();
        let far = line(
            &mut m,
            Some(g.id),
            DVec2::new(9.0, 0.0),
            DVec2::new(9.0, 5.0),
        );
        let near = line(
            &mut m,
            Some(g.id),
            DVec2::new(1.0, 0.0),
            DVec2::new(1.0, 5.0),
        );
        assert_eq!(label(&m, &far), "A-A");
        renumber(&m, Some(g.id)).apply(&mut m).unwrap();
        assert_eq!(label(&m, &near), "A-A");
        assert_eq!(label(&m, &far), "A-B");

        let tx = Transaction::new("Delete").remove_all_cascade(&m, &[g.id]);
        tx.apply(&mut m).unwrap();
        assert!(m.get(g.id).is_none());
        match &m.get(far.id).unwrap().kind {
            ElementKind::Grid(gr) => {
                assert_eq!(gr.group, None);
                assert_eq!(gr.label, "A-B");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn grids_and_views_from_older_files_still_read() {
        let old = r#"{"label":"A","start":[0.0,0.0],"end":[0.0,5.0]}"#;
        let g: Grid = serde_json::from_str(old).unwrap();
        assert_eq!(g.group, None);
        // an ungrouped line writes no group
        assert!(!serde_json::to_string(&g).unwrap().contains("group"));
        let group: GridGroup = serde_json::from_str("{}").unwrap();
        assert_eq!(group, GridGroup::default());
    }
}
