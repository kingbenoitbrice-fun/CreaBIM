//! Marks in tags and schedules, tag and note leaders, spot levels,
//! dimension breaks and the layout of level heads in sections.

use creabim_core::*;
use creabim_geom::annotate::{break_dimensions, tag_geometry, tag_text};
use creabim_geom::annotation::{Shape, TextLook};
use creabim_geom::section::{spread, view_drawing};
use creabim_geom::PlanItem;
use glam::DVec2;

fn named(m: &Model, name: &str) -> ElementId {
    m.iter().find(|e| e.name == name).unwrap().id
}

fn first(m: &Model, f: impl Fn(&ElementKind) -> bool) -> ElementId {
    m.iter().find(|e| f(&e.kind)).unwrap().id
}

fn is_door(k: &ElementKind) -> bool {
    matches!(k, ElementKind::Opening(o) if o.kind == OpeningKind::Door)
}

fn add(doc: &mut Document, e: Element) -> ElementId {
    let id = e.id;
    doc.commit(Transaction::new("Add").insert(e)).unwrap();
    id
}

#[test]
fn tags_and_schedules_read_typed_marks_and_the_rules() {
    let mut doc = Document::from_model(sample::sample_house());
    let door = first(doc.model(), is_door);
    assert_eq!(tag_text(doc.model(), door), ("D01".into(), true));
    doc.edit(door, "Mark", |e| e.mark = Some("FD-1".into()))
        .unwrap();
    assert_eq!(tag_text(doc.model(), door).0, "FD-1");
    // level-based rules renumber the rest
    let tx = marks::set_rules(doc.model(), MarkRules::level_based());
    doc.commit(tx).unwrap();
    let other = doc
        .model()
        .iter()
        .find(|e| is_door(&e.kind) && e.id != door)
        .unwrap()
        .id;
    assert!(tag_text(doc.model(), other).0.starts_with("D-"));
    // the door schedule shows the same marks
    let table = creabim_geom::schedule::schedule_by(doc.model(), ScheduleCategory::Doors, false);
    let marks: Vec<&str> = table.rows.iter().map(|r| r.cells[0].as_str()).collect();
    assert!(marks.contains(&"FD-1"), "{marks:?}");
    assert!(marks.contains(&tag_text(doc.model(), other).0.as_str()));
    // a wall with a mark reads it, and the wall schedule grows a column
    let wall = first(doc.model(), |k| matches!(k, ElementKind::Wall(_)));
    let thickness = tag_text(doc.model(), wall).0;
    doc.edit(wall, "Mark", |e| e.mark = Some("WT1".into()))
        .unwrap();
    assert_eq!(tag_text(doc.model(), wall).0, "WT1");
    let walls = creabim_geom::schedule::schedule_by(doc.model(), ScheduleCategory::Walls, false);
    assert_eq!(walls.columns[1].title, "Mark");
    doc.undo();
    assert_eq!(tag_text(doc.model(), wall).0, thickness);
}

#[test]
fn tag_leaders_bend_at_their_elbow_and_end_in_their_mark() {
    let look = TextLook {
        leader_end: Some(Terminator::Dot),
        ..TextLook::default()
    };
    let anchor = DVec2::new(0.0, -2.0);
    let elbow = DVec2::new(1.0, -1.0);
    let g = tag_geometry(anchor, DVec2::ZERO, Some(elbow), "D01", true, &look, 0.01);
    assert_eq!(g.leader.len(), 3);
    assert_eq!(g.leader[1], elbow);
    assert_eq!(*g.leader.last().unwrap(), anchor);
    assert_eq!(g.ends.fills.len(), 1, "a dot at the end");
    // no end mark: a plain line, as tags always had
    let plain = tag_geometry(
        anchor,
        DVec2::ZERO,
        None,
        "D01",
        true,
        &TextLook::default(),
        0.01,
    );
    assert_eq!(plain.leader.len(), 2);
    assert!(plain.ends.fills.is_empty() && plain.ends.thin.is_empty());

    // a tag's own leader end and elbow reach its drawing
    let mut doc = Document::from_model(sample::sample_house());
    let plan = named(doc.model(), "Ground Floor plan");
    let door = first(doc.model(), is_door);
    let mut tag = Tag::new(plan, door, DVec2::new(2.0, 2.0));
    tag.elbow = Some(DVec2::new(2.0, 0.0));
    tag.end = Some(DVec2::new(0.1, 0.0));
    tag.leader_end = Some(Terminator::Arrow);
    let id = add(&mut doc, Element::new("Tag", ElementKind::Tag(tag)));
    let d = view_drawing(doc.model(), plan).unwrap();
    let (anchor, at, elbow, look) = d
        .items
        .iter()
        .find_map(|i| match i {
            PlanItem::Tag {
                id: t,
                anchor,
                at,
                elbow,
                look,
                ..
            } if *t == id => Some((*anchor, *at, *elbow, *look)),
            _ => None,
        })
        .unwrap();
    assert!((at - anchor - DVec2::new(1.9, 2.0)).length() < 1e-9);
    assert!((elbow.unwrap() - anchor - DVec2::new(1.9, 0.0)).length() < 1e-9);
    assert_eq!(look.leader_end, Some(Terminator::Arrow));
}

#[test]
fn notes_bend_their_leaders_at_an_elbow() {
    let mut doc = Document::from_model(sample::sample_house());
    let plan = named(doc.model(), "Ground Floor plan");
    let mut n = Note::new(plan, DVec2::new(5.0, 5.0), "Note");
    n.leader = Some(HostPoint::free(DVec2::new(0.0, 0.0)));
    n.elbow = Some(DVec2::new(2.0, 1.0));
    n.leader_end = Some(Terminator::DotOpen);
    let id = add(&mut doc, Element::new("Note", ElementKind::Note(n)));
    let d = view_drawing(doc.model(), plan).unwrap();
    let item = d
        .items
        .iter()
        .find_map(|i| match i {
            PlanItem::Annotation { id: n, item } if *n == id => Some(item.clone()),
            _ => None,
        })
        .unwrap();
    let m = item.marks(0.1);
    let leader = m.lines.last().unwrap();
    assert!(leader.contains(&DVec2::new(2.0, 1.0)), "{leader:?}");
    assert_eq!(*leader.last().unwrap(), DVec2::ZERO);
    assert!(
        m.arrows.is_empty() && !m.ends.thin.is_empty(),
        "an open dot"
    );
}

#[test]
fn spot_levels_read_heights_from_their_pattern() {
    let mut doc = Document::from_model(sample::sample_house());
    let model = doc.model().clone();
    let plan = named(&model, "Ground Floor plan");
    let (level, ground) = model
        .levels()
        .map(|(e, l)| (e.id, l.elevation))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    // a level marker in the plan: the plan's level
    let free = HostPoint::free(DVec2::new(-3.0, -3.0));
    let marker = Dimension::spot(
        plan,
        free,
        HostPoint::free(DVec2::new(-1.0, -2.0)),
        Some("{m} FFL".into()),
    );
    let marker = add(
        &mut doc,
        Element::new("Spot", ElementKind::Dimension(marker)),
    );
    // a section: the point's own height, 0.45 up
    let section = View::section(DVec2::new(-5.0, 2.0), DVec2::new(20.0, 2.0), 20.0, true);
    let section = add(&mut doc, Element::new("S", ElementKind::View(section)));
    let p = HostPoint::free(DVec2::new(1.0, ground + 0.45));
    let spot = Dimension::spot(
        section,
        p,
        HostPoint::free(DVec2::new(3.0, 1.0)),
        Some("{m} FFL ({level})".into()),
    );
    let spot = add(&mut doc, Element::new("Spot", ElementKind::Dimension(spot)));
    let text = |view: ElementId, id: ElementId| {
        view_drawing(doc.model(), view)
            .unwrap()
            .items
            .into_iter()
            .find_map(|i| match i {
                PlanItem::Annotation { id: n, item } if n == id => {
                    assert!(matches!(item.shape, Shape::Spot { .. }));
                    Some(item.text)
                }
                _ => None,
            })
            .unwrap()
    };
    let level_name = doc.model().get(level).unwrap().name.clone();
    assert_eq!(
        text(plan, marker),
        format!("{} FFL", annotation::spot_text(doc.model(), "{m}", ground))
    );
    assert_eq!(
        text(section, spot),
        format!(
            "{} FFL ({level_name})",
            annotation::spot_text(doc.model(), "{m}", ground + 0.45)
        )
    );
    assert_eq!(annotation::spot_text(doc.model(), "{m}", 0.45), "+0.45");
    assert_eq!(annotation::spot_text(doc.model(), "{m}", 0.0), "±0.00");
    assert_eq!(annotation::spot_text(doc.model(), "{mm}", -0.45), "-450");
    // older dimensions read without a pattern
    let old = r#"{"view":"236746a3-3a36-452f-801d-b4b755309b62","a":{"host":null,"local":[0.0,0.0],"from_end":false},"b":{"host":null,"local":[3.0,0.0],"from_end":false},"offset":-1.5}"#;
    let d: Dimension = serde_json::from_str(old).unwrap();
    assert_eq!(d.pattern, None);
}

#[test]
fn dimension_lines_break_where_tags_sit_across_them() {
    let look = TextLook {
        breaks: true,
        ..TextLook::default()
    };
    let tag = PlanItem::Tag {
        id: ElementId::new(),
        anchor: DVec2::new(5.0, 1.0),
        at: DVec2::new(5.0, 1.0),
        elbow: None,
        text: "D01".into(),
        round: true,
        look: TextLook::default(),
    };
    let dim = |look: TextLook| PlanItem::Dimension {
        id: ElementId::new(),
        a: DVec2::ZERO,
        b: DVec2::new(10.0, 0.0),
        offset: 1.0,
        text: "10000".into(),
        look,
        witness: None,
        breaks: vec![],
    };
    let mut items = vec![tag, dim(look), dim(TextLook::default())];
    break_dimensions(&mut items, 0.1);
    let PlanItem::Dimension { breaks, .. } = &items[1] else {
        unreachable!()
    };
    assert_eq!(breaks.len(), 1);
    let [t0, t1] = breaks[0];
    assert!(t0 < 0.5 && t1 > 0.5 && t1 - t0 < 0.2, "{t0} {t1}");
    let PlanItem::Dimension { breaks, .. } = &items[2] else {
        unreachable!()
    };
    assert!(breaks.is_empty(), "a style without breaks runs through");
    // the line is drawn in two pieces
    let mut g = creabim_geom::annotate::dimension_geometry(
        DVec2::ZERO,
        DVec2::new(10.0, 0.0),
        1.0,
        None,
        &look,
        0.1,
    );
    g.break_line(&[[t0, t1]]);
    let along: Vec<_> = g
        .lines
        .iter()
        .filter(|(p, q)| p.y == 1.0 && q.y == 1.0)
        .collect();
    assert_eq!(along.len(), 2);
}

#[test]
fn close_level_heads_spread_apart_and_follow_the_view() {
    // two levels 0.2 m apart, one well clear
    let ys = spread(&[0.0, 0.2, 3.0], 0.7);
    assert!((ys[1] - ys[0] - 0.7).abs() < 1e-9);
    assert!(
        ((ys[0] + ys[1]) / 2.0 - 0.1).abs() < 1e-9,
        "centred on the pair"
    );
    assert_eq!(ys[2], 3.0);

    let mut doc = Document::from_model(sample::sample_house());
    let ground = doc
        .model()
        .levels()
        .min_by(|a, b| a.1.elevation.total_cmp(&b.1.elevation))
        .unwrap()
        .0
        .id;
    let near = add(
        &mut doc,
        Element::new(
            "Mezzanine",
            ElementKind::Level(Level {
                elevation: 0.2,
                height: 3.0,
            }),
        ),
    );
    let section = View::section(DVec2::new(-5.0, 2.0), DVec2::new(20.0, 2.0), 20.0, true);
    let view = add(&mut doc, Element::new("S", ElementKind::View(section)));
    let heads = |m: &Model| -> Vec<(ElementId, DVec2, Option<DVec2>)> {
        view_drawing(m, view)
            .unwrap()
            .items
            .into_iter()
            .filter_map(|i| match i {
                PlanItem::Annotation { id, item } => match item.shape {
                    Shape::Level { at, line_end, .. } => Some((id, at, line_end)),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    };
    let h = heads(doc.model());
    let of = |h: &[(ElementId, DVec2, Option<DVec2>)], id| h.iter().find(|x| x.0 == id).copied();
    let (_, a, ea) = of(&h, ground).unwrap();
    let (_, b, eb) = of(&h, near).unwrap();
    assert!(ea.is_some() && eb.is_some(), "both get a dog-leg");
    assert!(b.y - a.y > 0.5, "spread apart: {a} {b}");
    // drag one, hide the other
    doc.edit(view, "Heads", |e| {
        if let ElementKind::View(v) = &mut e.kind {
            v.level_heads.insert(
                near,
                LevelHead {
                    hidden: false,
                    offset: Some(DVec2::new(1.0, 2.0)),
                },
            );
            v.level_heads.insert(
                ground,
                LevelHead {
                    hidden: true,
                    offset: None,
                },
            );
        }
    })
    .unwrap();
    let h = heads(doc.model());
    assert!(of(&h, ground).is_none(), "hidden in this view");
    let (_, b, eb) = of(&h, near).unwrap();
    assert!((b - eb.unwrap() - DVec2::new(1.0, 2.0)).length() < 1e-9);
    // the view's settings survive saving, and older views read without them
    let e = doc.model().get(view).unwrap();
    let back: Element = serde_json::from_str(&serde_json::to_string(e).unwrap()).unwrap();
    assert_eq!(&back, e);
    let old = r#"{"kind":{"Plan":{"level":"9801fb10-8165-4b45-b92c-b50f14e9679d","cut_height":1.2}},"scale":100.0,"style":"HiddenLine"}"#;
    let v: View = serde_json::from_str(old).unwrap();
    assert!(v.level_heads.is_empty());
    assert!(!serde_json::to_string(&v).unwrap().contains("level_heads"));
}
