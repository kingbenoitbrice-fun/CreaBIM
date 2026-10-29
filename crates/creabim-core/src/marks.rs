//! Derived properties shared by tags, schedules and exports: marks, and
//! the materials elements are made of.
//!
//! An element's mark is the one typed on it ([`Element::mark`]) when it has
//! one; otherwise the project's numbering rules ([`MarkRules`]) give it one.
//! The rules are project data, not code: an office's numbering (D01, or
//! D-101 for level 1 door 01) is a [`MarkRules`] value that can be saved
//! to a file and loaded into another project.

use crate::{Element, ElementId, ElementKind, Material, Model, OpeningKind, Transaction};
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

/// The kinds of element numbered by a rule of their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MarkClass {
    Door,
    Window,
    Column,
    Beam,
    Stair,
    Railing,
    Footing,
    Wall,
    Slab,
    Roof,
}

impl MarkClass {
    pub const ALL: [MarkClass; 10] = [
        MarkClass::Door,
        MarkClass::Window,
        MarkClass::Column,
        MarkClass::Beam,
        MarkClass::Stair,
        MarkClass::Railing,
        MarkClass::Footing,
        MarkClass::Wall,
        MarkClass::Slab,
        MarkClass::Roof,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MarkClass::Door => "Doors",
            MarkClass::Window => "Windows",
            MarkClass::Column => "Columns",
            MarkClass::Beam => "Beams",
            MarkClass::Stair => "Stairs",
            MarkClass::Railing => "Railings",
            MarkClass::Footing => "Footings",
            MarkClass::Wall => "Walls",
            MarkClass::Slab => "Slabs",
            MarkClass::Roof => "Roofs",
        }
    }

    /// The class an element is numbered in, if any.
    pub fn of(kind: &ElementKind) -> Option<MarkClass> {
        Some(match kind {
            ElementKind::Opening(o) => match o.kind {
                OpeningKind::Door => MarkClass::Door,
                OpeningKind::Window => MarkClass::Window,
                OpeningKind::Hole => return None,
            },
            ElementKind::Column(_) => MarkClass::Column,
            ElementKind::Beam(_) => MarkClass::Beam,
            ElementKind::Stair(_) => MarkClass::Stair,
            ElementKind::Railing(_) => MarkClass::Railing,
            ElementKind::Footing(_) => MarkClass::Footing,
            ElementKind::Wall(_) => MarkClass::Wall,
            ElementKind::Slab(_) => MarkClass::Slab,
            ElementKind::Roof(_) => MarkClass::Roof,
            _ => return None,
        })
    }
}

/// The order elements are counted in, within a level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarkOrder {
    /// The order they were made in.
    #[default]
    Model,
    /// Clockwise round the middle of the level's elements of that kind,
    /// starting at the top of the plan (north, +Y).
    Clockwise,
}

impl MarkOrder {
    pub const ALL: [MarkOrder; 2] = [MarkOrder::Model, MarkOrder::Clockwise];

    pub fn label(self) -> &'static str {
        match self {
            MarkOrder::Model => "Model order",
            MarkOrder::Clockwise => "Clockwise per level",
        }
    }
}

/// How the elements of one [`MarkClass`] are numbered.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarkRule {
    pub class: MarkClass,
    /// Number these automatically; when off, only marks typed on them show
    /// (and a tag shows what it showed before, such as a wall's thickness).
    #[serde(default)]
    pub auto: bool,
    pub prefix: String,
    /// Between the prefix and the number: "" for D01, "-" for D-101.
    #[serde(default)]
    pub separator: String,
    /// Digits of the count, zero-padded: 2 gives 01.
    #[serde(default = "two")]
    pub padding: u8,
    /// Put the level's number before the count, restarting the count on
    /// each level: D-101 is level 1, door 01.
    #[serde(default)]
    pub by_level: bool,
    #[serde(default)]
    pub order: MarkOrder,
    /// The first number of a count.
    #[serde(default = "one")]
    pub start: u32,
}

fn two() -> u8 {
    2
}

fn one() -> u32 {
    1
}

impl MarkRule {
    pub fn new(class: MarkClass, prefix: &str, auto: bool) -> Self {
        Self {
            class,
            auto,
            prefix: prefix.into(),
            separator: String::new(),
            padding: 2,
            by_level: false,
            order: MarkOrder::Model,
            start: 1,
        }
    }

    /// The mark of the `n`th element (from [`MarkRule::start`]) on the
    /// level numbered `level`.
    pub fn format(&self, level: i32, n: u32) -> String {
        let count = format!("{n:0width$}", width = self.padding.min(9) as usize);
        if self.by_level {
            format!("{}{}{level}{count}", self.prefix, self.separator)
        } else {
            format!("{}{}{count}", self.prefix, self.separator)
        }
    }
}

/// A project's numbering rules: one [`MarkRule`] per kind of element, and
/// how levels are numbered for level-based marks. Stored as the project's
/// [`ElementKind::MarkRules`] element, read through [`rules`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarkRules {
    #[serde(default)]
    pub rules: Vec<MarkRule>,
    /// The number of the ground level: the lowest one at or above ±0.
    /// Levels above it count up from it, levels below count down.
    #[serde(default = "one_i")]
    pub ground_number: i32,
}

fn one_i() -> i32 {
    1
}

impl Default for MarkRules {
    /// Doors D01, D02… and windows W01… in model order, lowest level
    /// first; the other kinds unnumbered: how marks always worked.
    fn default() -> Self {
        let rule = |c, p, auto| MarkRule::new(c, p, auto);
        Self {
            rules: vec![
                rule(MarkClass::Door, "D", true),
                rule(MarkClass::Window, "W", true),
                rule(MarkClass::Column, "C", false),
                rule(MarkClass::Beam, "B", false),
                rule(MarkClass::Stair, "ST", false),
                rule(MarkClass::Railing, "R", false),
                rule(MarkClass::Footing, "F", false),
                rule(MarkClass::Wall, "WT", false),
                rule(MarkClass::Slab, "SL", false),
                rule(MarkClass::Roof, "RF", false),
            ],
            ground_number: 1,
        }
    }
}

impl MarkRules {
    /// The rule for `class`; a class the rules leave out is not numbered.
    pub fn rule(&self, class: MarkClass) -> Option<&MarkRule> {
        self.rules.iter().find(|r| r.class == class)
    }

    pub fn rule_mut(&mut self, class: MarkClass) -> &mut MarkRule {
        if let Some(i) = self.rules.iter().position(|r| r.class == class) {
            return &mut self.rules[i];
        }
        self.rules.push(MarkRule::new(class, "", false));
        self.rules.last_mut().expect("just pushed")
    }

    /// Level-based numbering for doors and windows, D-101 and W-101.
    pub fn level_based() -> Self {
        let mut r = Self::default();
        for c in [MarkClass::Door, MarkClass::Window] {
            let rule = r.rule_mut(c);
            rule.separator = "-".into();
            rule.by_level = true;
        }
        r
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut seen = HashSet::new();
        for r in &self.rules {
            if !seen.insert(r.class) {
                return Err(format!("two numbering rules for {}", r.class.label()));
            }
            if r.padding > 9 {
                return Err("mark padding must be at most 9 digits".into());
            }
        }
        Ok(())
    }

    /// The rules as JSON, to save for another project or a style pack.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("rules serialise")
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let r: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        r.validate()?;
        Ok(r)
    }
}

/// The project's numbering rules, or the default ones.
pub fn rules(model: &Model) -> MarkRules {
    model
        .iter()
        .find_map(|e| match &e.kind {
            ElementKind::MarkRules(r) => Some(r.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// The edit that gives `model` these rules, as one undo step.
pub fn set_rules(model: &Model, rules: MarkRules) -> Transaction {
    let tx = Transaction::new("Mark numbering");
    let current = model
        .iter()
        .find(|e| matches!(e.kind, ElementKind::MarkRules(_)));
    match current {
        Some(e) if e.kind == ElementKind::MarkRules(rules.clone()) => tx,
        Some(e) => {
            let mut after = e.clone();
            after.kind = ElementKind::MarkRules(rules);
            tx.replace(e.clone(), after)
        }
        None => tx.insert(Element::new(
            "Mark numbering",
            ElementKind::MarkRules(rules),
        )),
    }
}

/// The mark typed on an element, if it has a non-blank one.
pub fn manual(e: &Element) -> Option<&str> {
    e.mark.as_deref().map(str::trim).filter(|m| !m.is_empty())
}

/// The level an element of a numbered class stands on.
fn level_of(model: &Model, e: &Element) -> Option<ElementId> {
    match &e.kind {
        ElementKind::Opening(o) => match &model.get(o.host)?.kind {
            ElementKind::Wall(w) => Some(w.level),
            _ => None,
        },
        ElementKind::Wall(w) => Some(w.level),
        ElementKind::Slab(s) => Some(s.level),
        k => crate::members::level_of(k),
    }
}

/// Where an element sits in plan, for clockwise numbering.
fn plan_position(model: &Model, e: &Element) -> Option<DVec2> {
    let mid = |pts: &[DVec2]| {
        (!pts.is_empty()).then(|| pts.iter().copied().sum::<DVec2>() / pts.len() as f64)
    };
    match &e.kind {
        ElementKind::Opening(o) => match &model.get(o.host)?.kind {
            ElementKind::Wall(w) => Some(w.start + w.direction() * (o.offset + o.width / 2.0)),
            _ => None,
        },
        ElementKind::Wall(w) => Some((w.start + w.end) / 2.0),
        ElementKind::Slab(s) => mid(&s.boundary),
        ElementKind::Roof(r) => mid(&r.boundary),
        ElementKind::Column(c) => Some(c.position),
        ElementKind::Beam(b) => Some((b.start + b.end) / 2.0),
        ElementKind::Stair(s) => Some(s.start),
        ElementKind::Railing(r) => {
            let pts: Vec<DVec2> = r.path.iter().map(|p| p.truncate()).collect();
            mid(&pts)
        }
        ElementKind::Footing(f) => Some((f.start + f.end) / 2.0),
        _ => None,
    }
}

/// Each level's number for level-based marks: the lowest level at or
/// above ±0 is `ground`, those above count up and those below count down.
pub fn level_numbers(model: &Model, ground: i32) -> HashMap<ElementId, i32> {
    let mut levels: Vec<(f64, usize, ElementId)> = model
        .levels()
        .enumerate()
        .map(|(i, (e, l))| (l.elevation, i, e.id))
        .collect();
    levels.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let base = levels
        .iter()
        .position(|l| l.0 >= -1e-6)
        .unwrap_or(levels.len().saturating_sub(1));
    levels
        .iter()
        .enumerate()
        .map(|(i, l)| (l.2, ground + i as i32 - base as i32))
        .collect()
}

/// The marks `rules` give the elements of each numbered class. With
/// `keep_manual`, elements with a typed mark keep it and the count skips
/// the marks they hold; without, everything is numbered afresh.
pub fn auto_marks(
    model: &Model,
    rules: &MarkRules,
    keep_manual: bool,
) -> HashMap<ElementId, String> {
    let numbers = level_numbers(model, rules.ground_number);
    let elevation = |l: Option<ElementId>| {
        l.and_then(|l| model.level(l))
            .map_or(f64::NEG_INFINITY, |l| l.elevation)
    };
    let mut out = HashMap::new();
    for rule in rules.rules.iter().filter(|r| r.auto) {
        let mut taken: HashSet<String> = HashSet::new();
        // (level, its elevation, model index, id, plan position)
        type Item = (Option<ElementId>, f64, usize, ElementId, Option<DVec2>);
        let mut list: Vec<Item> = Vec::new();
        for (i, e) in model.iter().enumerate() {
            if MarkClass::of(&e.kind) != Some(rule.class) {
                continue;
            }
            if keep_manual {
                if let Some(m) = manual(e) {
                    taken.insert(m.to_string());
                    continue;
                }
            }
            let level = level_of(model, e);
            list.push((level, elevation(level), i, e.id, plan_position(model, e)));
        }
        list.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.2.cmp(&b.2)));
        let mut groups: Vec<Vec<Item>> = Vec::new();
        for item in list {
            match groups.last_mut() {
                Some(g) if g[0].0 == item.0 => g.push(item),
                _ => groups.push(vec![item]),
            }
        }
        let mut n = rule.start;
        for mut group in groups {
            if rule.order == MarkOrder::Clockwise {
                clockwise(&mut group, |g: &Item| g.4);
            }
            let level = group[0].0.and_then(|l| numbers.get(&l).copied());
            if rule.by_level {
                n = rule.start;
            }
            for item in group {
                let mut mark = rule.format(level.unwrap_or(0), n);
                while taken.contains(&mark) {
                    n += 1;
                    mark = rule.format(level.unwrap_or(0), n);
                }
                n += 1;
                out.insert(item.3, mark);
            }
        }
    }
    out
}

/// Sort `items` clockwise round their middle, from the top of the plan;
/// items without a position go last, in the order they were.
fn clockwise<T>(items: &mut [T], at: impl Fn(&T) -> Option<DVec2>) {
    let pts: Vec<DVec2> = items.iter().filter_map(&at).collect();
    if pts.is_empty() {
        return;
    }
    let c = pts.iter().copied().sum::<DVec2>() / pts.len() as f64;
    // bearing from north, turning clockwise, in [0, 2π)
    let key = |p: Option<DVec2>| match p {
        Some(p) => {
            let d = p - c;
            if d.length() < 1e-9 {
                (0.0, 0.0)
            } else {
                (d.x.atan2(d.y).rem_euclid(std::f64::consts::TAU), d.length())
            }
        }
        None => (f64::INFINITY, 0.0),
    };
    items.sort_by(|a, b| {
        let (ka, kb) = (key(at(a)), key(at(b)));
        ka.0.total_cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
    });
}

/// Every element's mark: its typed one, or the one the project's rules
/// give it. Tags and schedules both use these, so they always agree.
pub fn marks(model: &Model) -> HashMap<ElementId, String> {
    let mut out = auto_marks(model, &rules(model), true);
    for e in model.iter() {
        if let Some(m) = manual(e) {
            out.insert(e.id, m.to_string());
        }
    }
    out
}

/// One element's mark (see [`marks`]); for an object, its own typed mark
/// or its type's.
pub fn mark_of(model: &Model, id: ElementId) -> Option<String> {
    let e = model.get(id)?;
    if matches!(e.kind, ElementKind::Object(_)) {
        return object_mark(model, e);
    }
    marks(model).remove(&id)
}

/// Object type marks: E01, E02, … for electrical types, L01 for lights,
/// P01 plumbing, M01 heating and ventilation, S01 sanitary ware, K01
/// kitchen and F01 furniture (see [`crate::ObjectCategory::mark_prefix`]),
/// numbered in name order within each letter. A mark typed on a type wins
/// and takes its number out of the count. Each type's objects share its
/// mark, so a tag on a socket and the fixture schedule agree.
pub fn object_marks(model: &Model) -> HashMap<ElementId, String> {
    let mut types: Vec<(&'static str, &str, ElementId)> = Vec::new();
    let mut out = HashMap::new();
    let mut taken = HashSet::new();
    for e in model.iter() {
        let Some(t) = crate::objects::type_of_id(model, e.id) else {
            continue;
        };
        match manual(e) {
            Some(m) => {
                taken.insert(m.to_string());
                out.insert(e.id, m.to_string());
            }
            None => types.push((t.category.mark_prefix(), e.name.as_str(), e.id)),
        }
    }
    types.sort();
    for group in types.chunk_by(|a, b| a.0 == b.0) {
        let mut n = 1;
        for (prefix, _, id) in group {
            let mut mark = format!("{prefix}{n:02}");
            while taken.contains(&mark) {
                n += 1;
                mark = format!("{prefix}{n:02}");
            }
            n += 1;
            out.insert(*id, mark);
        }
    }
    out
}

/// The mark of a placed object: its own typed one, else its type's (see
/// [`object_marks`]).
pub fn object_mark(model: &Model, e: &Element) -> Option<String> {
    if let Some(m) = manual(e) {
        return Some(m.to_string());
    }
    let t = e.of_type.as_ref()?.id;
    object_marks(model).remove(&t)
}

/// Marks held by more than one element, with the elements holding each,
/// in model order. Objects of one type share its mark on purpose, so they
/// count once (as their type) unless one has a mark of its own.
pub fn duplicates(model: &Model) -> Vec<(String, Vec<ElementId>)> {
    let mut by_mark: BTreeMap<String, Vec<ElementId>> = BTreeMap::new();
    let marks = marks(model);
    let object_types = object_marks(model);
    let mut seen_types = HashSet::new();
    for e in model.iter() {
        let mark = if matches!(e.kind, ElementKind::Object(_)) {
            match manual(e) {
                Some(m) => Some(m.to_string()),
                None => {
                    // a type counts once, however many objects it has
                    let t = e.of_type.as_ref().map(|t| t.id);
                    match t {
                        Some(t) if seen_types.insert(t) => object_types.get(&t).cloned(),
                        _ => None,
                    }
                }
            }
        } else if matches!(e.kind, ElementKind::ElementType(_)) {
            // types are counted through their objects
            None
        } else {
            marks.get(&e.id).cloned()
        };
        if let Some(m) = mark {
            by_mark.entry(m).or_default().push(e.id);
        }
    }
    by_mark.into_iter().filter(|(_, ids)| ids.len() > 1).collect()
}

/// Is `id`'s mark also held by another element?
pub fn is_duplicate(model: &Model, id: ElementId) -> bool {
    duplicates(model).iter().any(|(_, ids)| ids.contains(&id))
}

/// The edit that writes the rules' numbers onto elements as their marks,
/// so later changes to the model leave them as they are. It numbers only
/// elements without a mark of their own; with `replace`, every element of
/// a numbered class is numbered afresh, typed marks too. `classes` limits
/// it to those kinds (all of them when empty).
pub fn renumber(model: &Model, replace: bool, classes: &[MarkClass]) -> Transaction {
    let rules = rules(model);
    let fresh = auto_marks(model, &rules, !replace);
    let mut tx = Transaction::new("Renumber marks");
    for e in model.iter() {
        let Some(class) = MarkClass::of(&e.kind) else {
            continue;
        };
        if !classes.is_empty() && !classes.contains(&class) {
            continue;
        }
        let Some(m) = fresh.get(&e.id) else { continue };
        if e.mark.as_deref() != Some(m.as_str()) && (replace || manual(e).is_none()) {
            let mut after = e.clone();
            after.mark = Some(m.clone());
            tx = tx.replace(e.clone(), after);
        }
    }
    tx
}

/// The mark at the start of an element's name, like the "D-101" of
/// "D-101 sliding doors": a first word of letters then digits, maybe with
/// a dash, dot or underscore between them.
pub fn mark_in_name(name: &str) -> Option<&str> {
    let word = name.split_whitespace().next()?;
    let letters = word.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    let rest = &word[letters..];
    let rest = rest.strip_prefix(['-', '.', '_']).unwrap_or(rest);
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    (letters > 0 && letters <= 4 && digits > 0 && digits == rest.len()).then_some(word)
}

/// The edit that takes marks from names (see [`mark_in_name`]) for
/// elements of a numbered class with no mark yet: for models whose marks
/// were written into the names.
pub fn marks_from_names(model: &Model) -> Transaction {
    let mut tx = Transaction::new("Marks from names");
    for e in model.iter() {
        if MarkClass::of(&e.kind).is_none() && !matches!(e.kind, ElementKind::Object(_)) {
            continue;
        }
        if manual(e).is_some() {
            continue;
        }
        if let Some(m) = mark_in_name(&e.name) {
            let mut after = e.clone();
            after.mark = Some(m.to_string());
            tx = tx.replace(e.clone(), after);
        }
    }
    tx
}

/// The material an element is made of, if it has one and it still exists.
pub fn material_of<'a>(model: &'a Model, e: &Element) -> Option<(&'a Element, &'a Material)> {
    let id = crate::library::material_slot(e)??;
    let m = model.get(id)?;
    match &m.kind {
        ElementKind::Material(mat) => Some((m, mat)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::sample_house;
    use crate::Document;

    fn openings(m: &Model, kind: OpeningKind) -> Vec<ElementId> {
        m.iter()
            .filter(|e| matches!(&e.kind, ElementKind::Opening(o) if o.kind == kind))
            .map(|e| e.id)
            .collect()
    }

    #[test]
    fn default_rules_number_doors_and_windows_as_before() {
        let m = sample_house();
        let marks = marks(&m);
        let doors = openings(&m, OpeningKind::Door);
        assert_eq!(marks[&doors[0]], "D01");
        assert_eq!(marks[&doors[1]], "D02");
        assert_eq!(marks[&openings(&m, OpeningKind::Window)[0]], "W01");
        // walls are not numbered unless a rule says so
        let wall = m
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Wall(_)))
            .unwrap();
        assert!(!marks.contains_key(&wall.id));
    }

    #[test]
    fn a_typed_mark_wins_and_the_count_skips_it() {
        let mut doc = Document::from_model(sample_house());
        let doors = openings(doc.model(), OpeningKind::Door);
        // the second door is typed D01: the first becomes D02 and no two match
        doc.edit(doors[1], "Mark", |e| e.mark = Some("D01".into()))
            .unwrap();
        let m = marks(doc.model());
        assert_eq!(m[&doors[1]], "D01");
        assert_eq!(m[&doors[0]], "D02");
        assert!(duplicates(doc.model()).is_empty());
        // a blank mark is no mark
        doc.edit(doors[1], "Mark", |e| e.mark = Some("  ".into()))
            .unwrap();
        assert_eq!(marks(doc.model())[&doors[0]], "D01");
        doc.undo();
        doc.undo();
        assert_eq!(marks(doc.model())[&doors[0]], "D01");
    }

    #[test]
    fn duplicates_are_found_and_renumber_fixes_marks() {
        let mut doc = Document::from_model(sample_house());
        let doors = openings(doc.model(), OpeningKind::Door);
        let windows = openings(doc.model(), OpeningKind::Window);
        doc.edit(windows[0], "Mark", |e| e.mark = Some("X1".into()))
            .unwrap();
        doc.edit(windows[1], "Mark", |e| e.mark = Some("X1".into()))
            .unwrap();
        let d = duplicates(doc.model());
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].0, "X1");
        assert!(is_duplicate(doc.model(), windows[1]));
        assert!(!is_duplicate(doc.model(), doors[0]));
        // renumbering keeps typed marks and writes the rest down
        let tx = renumber(doc.model(), false, &[]);
        doc.commit(tx).unwrap();
        let e = doc.model().get(doors[0]).unwrap();
        assert_eq!(e.mark.as_deref(), Some("D01"));
        assert_eq!(
            doc.model().get(windows[0]).unwrap().mark.as_deref(),
            Some("X1")
        );
        // replacing renumbers everything, and clears the duplicate
        let tx = renumber(doc.model(), true, &[MarkClass::Window]);
        doc.commit(tx).unwrap();
        assert_eq!(
            doc.model().get(windows[0]).unwrap().mark.as_deref(),
            Some("W01")
        );
        assert!(duplicates(doc.model()).is_empty());
        // and it all undoes in one step each
        doc.undo();
        assert!(!duplicates(doc.model()).is_empty());
    }

    #[test]
    fn level_based_rules_give_d_101_and_restart_on_each_level() {
        let mut m = sample_house();
        let tx = set_rules(&m, MarkRules::level_based());
        tx.apply(&mut m).unwrap();
        let marks = marks(&m);
        let doors = openings(&m, OpeningKind::Door);
        let level = |id: ElementId| match &m.get(id).unwrap().kind {
            ElementKind::Opening(o) => match &m.get(o.host).unwrap().kind {
                ElementKind::Wall(w) => w.level,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        };
        let ground = level(doors[0]);
        let first: Vec<&str> = doors
            .iter()
            .filter(|d| level(**d) == ground)
            .map(|d| marks[d].as_str())
            .collect();
        assert_eq!(first[0], "D-101");
        assert_eq!(first[1], "D-102");
        if let Some(up) = doors.iter().find(|d| level(**d) != ground) {
            assert!(marks[up].starts_with("D-201"), "{}", marks[up]);
        }
        // the rules round-trip through their file
        let r = MarkRules::from_json(&rules(&m).to_json()).unwrap();
        assert_eq!(r, rules(&m));
        assert!(MarkRules::from_json("{\"rules\":[{\"class\":\"Door\",\"prefix\":\"D\",\"padding\":12}]}").is_err());
    }

    #[test]
    fn levels_are_numbered_from_the_ground() {
        let mut m = Model::new();
        let mk = |name: &str, z: f64| {
            Element::new(
                name,
                ElementKind::Level(crate::Level {
                    elevation: z,
                    height: 3.0,
                }),
            )
        };
        let (site, ground, up) = (mk("Site", -0.45), mk("Ground", 0.0), mk("Up", 3.0));
        let ids = (site.id, ground.id, up.id);
        Transaction::new("Levels")
            .insert(up)
            .insert(site)
            .insert(ground)
            .apply(&mut m)
            .unwrap();
        let n = level_numbers(&m, 1);
        assert_eq!((n[&ids.0], n[&ids.1], n[&ids.2]), (0, 1, 2));
    }

    #[test]
    fn clockwise_numbering_goes_round_from_the_top() {
        // four points round the middle, made in an odd order
        let mut pts = vec![
            (3, Some(DVec2::new(-1.0, 0.0))),
            (1, Some(DVec2::new(1.0, 0.0))),
            (0, Some(DVec2::new(0.0, 1.0))),
            (9, None),
            (2, Some(DVec2::new(0.0, -1.0))),
        ];
        clockwise(&mut pts, |p| p.1);
        let order: Vec<i32> = pts.iter().map(|p| p.0).collect();
        assert_eq!(order, vec![0, 1, 2, 3, 9]);
    }

    #[test]
    fn marks_come_out_of_names() {
        assert_eq!(mark_in_name("D-101 sliding doors"), Some("D-101"));
        assert_eq!(mark_in_name("W01"), Some("W01"));
        assert_eq!(mark_in_name("C.12 clerestory"), Some("C.12"));
        assert_eq!(mark_in_name("Door"), None);
        assert_eq!(mark_in_name("GF Wall 1"), None);
        assert_eq!(mark_in_name("101"), None);
        let mut doc = Document::from_model(sample_house());
        let door = openings(doc.model(), OpeningKind::Door)[0];
        doc.edit(door, "Name", |e| e.name = "D-7 front door".into())
            .unwrap();
        let tx = marks_from_names(doc.model());
        doc.commit(tx).unwrap();
        assert_eq!(mark_of(doc.model(), door).as_deref(), Some("D-7"));
    }

    #[test]
    fn elements_and_rules_from_older_files_still_read() {
        let old = r#"{"id":"236746a3-3a36-452f-801d-b4b755309b62","name":"Grid","kind":{"Grid":{"label":"A","start":[0.0,0.0],"end":[1.0,0.0]}}}"#;
        let e: Element = serde_json::from_str(old).unwrap();
        assert_eq!(e.mark, None);
        // and an element without a mark writes none
        assert!(!serde_json::to_string(&e).unwrap().contains("mark"));
        let mut marked = e.clone();
        marked.mark = Some("G1".into());
        let back: Element = serde_json::from_str(&serde_json::to_string(&marked).unwrap()).unwrap();
        assert_eq!(back.mark.as_deref(), Some("G1"));
        // a sparse rules file fills in the rest
        let r = MarkRules::from_json(r#"{"rules":[{"class":"Door","prefix":"DR"}]}"#).unwrap();
        assert_eq!(r.ground_number, 1);
        let d = r.rule(MarkClass::Door).unwrap();
        assert_eq!((d.padding, d.start, d.auto, d.by_level), (2, 1, false, false));
        assert_eq!(d.format(1, 3), "DR03");
    }
}
