//! Schedules: tables of doors, windows, walls or slabs read live from the
//! model. The same table is drawn on sheets, edited in the app and
//! exported as CSV.

use crate::paper::Anchor;
use crate::plan::{Pen, PlanItem, PlanView};
use creabim_core::{Element, ElementId, ElementKind, Model, OpeningKind, ScheduleCategory};
use glam::DVec2;

/// An element property a schedule cell shows and can change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    Width,
    Height,
    Sill,
    Thickness,
    WallHeight,
    Material,
}

impl Field {
    /// Lengths, shown and edited in the project's units.
    pub fn is_length(self) -> bool {
        matches!(
            self,
            Field::Width | Field::Height | Field::Sill | Field::Thickness | Field::WallHeight
        )
    }
}

#[derive(Clone, Debug)]
pub struct Column {
    pub title: &'static str,
    /// The editable property behind this column, if any.
    pub field: Option<Field>,
    /// Width on paper, mm.
    pub width: f64,
    pub numeric: bool,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub id: ElementId,
    pub cells: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Schedule {
    pub category: ScheduleCategory,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    /// The last row: counts and sums.
    pub totals: Vec<String>,
}

fn col(title: &'static str, field: Option<Field>, width: f64, numeric: bool) -> Column {
    Column {
        title,
        field,
        width,
        numeric,
    }
}

/// The current value of a numeric field, metres.
pub fn value(e: &Element, field: Field) -> Option<f64> {
    Some(match (&e.kind, field) {
        (ElementKind::Opening(o), Field::Width) => o.width,
        (ElementKind::Opening(o), Field::Height) => o.height,
        (ElementKind::Opening(o), Field::Sill) => o.sill,
        (ElementKind::Wall(w), Field::Thickness) => w.thickness,
        (ElementKind::Wall(w), Field::WallHeight) => w.height,
        (ElementKind::Slab(s), Field::Thickness) => s.thickness,
        _ => return None,
    })
}

/// Change a numeric field, metres.
pub fn set_value(e: &mut Element, field: Field, v: f64) {
    match (&mut e.kind, field) {
        (ElementKind::Opening(o), Field::Width) => o.width = v,
        (ElementKind::Opening(o), Field::Height) => o.height = v,
        (ElementKind::Opening(o), Field::Sill) => o.sill = v,
        (ElementKind::Wall(w), Field::Thickness) => w.thickness = v,
        (ElementKind::Wall(w), Field::WallHeight) => w.height = v,
        (ElementKind::Slab(s), Field::Thickness) => s.thickness = v,
        _ => {}
    }
}

/// Change what an element is made of.
pub fn set_material(e: &mut Element, material: Option<ElementId>) {
    match &mut e.kind {
        ElementKind::Wall(w) => w.material = material,
        ElementKind::Slab(s) => s.material = material,
        ElementKind::FreeMesh(m) => m.material = material,
        k => {
            if let Some(m) = creabim_core::members::material_mut(k) {
                *m = material;
            }
        }
    }
}

/// The schedule of one category, rows in mark or model order.
pub fn schedule(model: &Model, category: ScheduleCategory) -> Schedule {
    let units = model.units();
    let mm = |v: f64| units.dimension(v);
    let area = |v: f64| units.area(v);
    let name = |id: ElementId| model.get(id).map(|e| e.name.clone()).unwrap_or_default();
    let material = |e: &Element| {
        // A layered element is listed by its core layer's material.
        let core = creabim_core::assembly_of(model, e)
            .and_then(|(_, a)| a.layers.get(a.core().0)?.material)
            .and_then(|m| model.get(m));
        if let Some(m) = core {
            return m.name.clone();
        }
        creabim_core::material_of(model, e)
            .map(|(m, _)| m.name.clone())
            .unwrap_or_else(|| "-".into())
    };
    let level_of = |e: &Element| match &e.kind {
        ElementKind::Wall(w) => Some(w.level),
        ElementKind::Slab(s) => Some(s.level),
        ElementKind::Opening(o) => match model.get(o.host).map(|h| &h.kind) {
            Some(ElementKind::Wall(w)) => Some(w.level),
            _ => None,
        },
        k => creabim_core::members::level_of(k),
    };
    let elevation = |e: &Element| {
        level_of(e)
            .and_then(|l| model.level(l))
            .map_or(0.0, |l| l.elevation)
    };
    let mut rows: Vec<(f64, String, Row)> = Vec::new();
    let (columns, totals) = match category {
        ScheduleCategory::Doors | ScheduleCategory::Windows => {
            let kind = if category == ScheduleCategory::Doors {
                OpeningKind::Door
            } else {
                OpeningKind::Window
            };
            let marks = creabim_core::marks(model);
            let mut columns = vec![
                col("Mark", None, 16.0, false),
                col("Level", None, 28.0, false),
                col("Wall", None, 26.0, false),
                col("Width", Some(Field::Width), 18.0, true),
                col("Height", Some(Field::Height), 18.0, true),
                col("Sill", Some(Field::Sill), 16.0, true),
            ];
            let doors = kind == OpeningKind::Door;
            if doors {
                // hinge side seen from where it opens to, into or out of the building
                columns.push(col("Swing", None, 22.0, false));
            }
            for e in model.iter() {
                let ElementKind::Opening(o) = &e.kind else {
                    continue;
                };
                if o.kind != kind {
                    continue;
                }
                let mark = marks.get(&e.id).cloned().unwrap_or_default();
                let mut cells = vec![
                    mark.clone(),
                    level_of(e).map(name).unwrap_or_default(),
                    name(o.host),
                    mm(o.width),
                    mm(o.height),
                    mm(o.sill),
                ];
                if doors {
                    cells.push(match model.get(o.host).map(|h| &h.kind) {
                        Some(ElementKind::Wall(w)) => crate::opening::door_hand(o, w).into(),
                        _ => String::new(),
                    });
                }
                rows.push((0.0, mark, Row { id: e.id, cells }));
            }
            let what = if kind == OpeningKind::Door {
                "doors"
            } else {
                "windows"
            };
            (columns, vec![format!("{} {what}", rows.len())])
        }
        ScheduleCategory::Walls => {
            // The assembly column shows up once any wall has layers, so
            // single-material projects keep their old table.
            let layered = model
                .iter()
                .any(|e| matches!(&e.kind, ElementKind::Wall(w) if w.assembly.is_some()));
            let mut columns = vec![
                col("Name", Some(Field::Name), 30.0, false),
                col("Level", None, 28.0, false),
                col("Material", Some(Field::Material), 30.0, false),
                col("Length", None, 18.0, true),
                col("Thickness", Some(Field::Thickness), 20.0, true),
                col("Height", Some(Field::WallHeight), 18.0, true),
                col("Area", None, 22.0, true),
            ];
            if layered {
                columns.insert(3, col("Assembly", None, 44.0, false));
            }
            let mut total_area = 0.0;
            let mut total_length = 0.0;
            for e in model.iter() {
                let ElementKind::Wall(w) = &e.kind else {
                    continue;
                };
                let openings: f64 = model
                    .dependents(e.id)
                    .into_iter()
                    .filter_map(|d| match &model.get(d)?.kind {
                        ElementKind::Opening(o) => Some(o.width * o.height),
                        _ => None,
                    })
                    .sum();
                let a = (w.length() * w.height - openings).max(0.0);
                total_area += a;
                total_length += w.length();
                let mut cells = vec![
                    e.name.clone(),
                    name(w.level),
                    material(e),
                    mm(w.length()),
                    mm(w.thickness),
                    mm(w.height),
                    area(a),
                ];
                if layered {
                    let asm = w
                        .assembly
                        .and_then(|id| model.get(id))
                        .map_or_else(|| "-".to_string(), |a| a.name.clone());
                    cells.insert(3, asm);
                }
                rows.push((elevation(e), String::new(), Row { id: e.id, cells }));
            }
            let mut totals = vec![
                format!("{} walls", rows.len()),
                String::new(),
                String::new(),
                mm(total_length),
                String::new(),
                String::new(),
                area(total_area),
            ];
            if layered {
                totals.insert(3, String::new());
            }
            (columns, totals)
        }
        ScheduleCategory::Slabs => {
            let columns = vec![
                col("Name", Some(Field::Name), 30.0, false),
                col("Level", None, 28.0, false),
                col("Material", Some(Field::Material), 30.0, false),
                col("Thickness", Some(Field::Thickness), 20.0, true),
                col("Area", None, 22.0, true),
            ];
            let mut total = 0.0;
            for e in model.iter() {
                let ElementKind::Slab(s) = &e.kind else {
                    continue;
                };
                let a = s.area();
                total += a;
                rows.push((
                    elevation(e),
                    String::new(),
                    Row {
                        id: e.id,
                        cells: vec![
                            e.name.clone(),
                            name(s.level),
                            material(e),
                            mm(s.thickness),
                            area(a),
                        ],
                    },
                ));
            }
            (
                columns,
                vec![
                    format!("{} slabs", rows.len()),
                    String::new(),
                    String::new(),
                    String::new(),
                    area(total),
                ],
            )
        }
        ScheduleCategory::Structure => {
            let columns = vec![
                col("Name", Some(Field::Name), 30.0, false),
                col("Type", None, 16.0, false),
                col("Level", None, 28.0, false),
                col("Material", Some(Field::Material), 30.0, false),
                col("Section", None, 22.0, false),
                col("Length", None, 18.0, true),
            ];
            let mut total = 0.0;
            for e in model.iter() {
                let (kind, section, length) = match &e.kind {
                    ElementKind::Column(c) => {
                        ("Column", c.profile.label_in(&units), c.height_in(model))
                    }
                    ElementKind::Beam(b) => (
                        "Beam",
                        format!("{} × {}", mm(b.width), mm(b.depth)),
                        b.length(),
                    ),
                    _ => continue,
                };
                total += length;
                rows.push((
                    elevation(e),
                    kind.to_string(),
                    Row {
                        id: e.id,
                        cells: vec![
                            e.name.clone(),
                            kind.into(),
                            level_of(e).map(name).unwrap_or_default(),
                            material(e),
                            section,
                            mm(length),
                        ],
                    },
                ));
            }
            (
                columns,
                vec![
                    format!("{} members", rows.len()),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    mm(total),
                ],
            )
        }
    };
    // marks sort as text (D01 < D02); walls and slabs by level, then model order
    rows.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    // feet and inches take more room than millimetres
    let mut columns = columns;
    if units.is_imperial() {
        for c in columns.iter_mut().filter(|c| c.numeric) {
            c.width *= 1.4;
        }
    }
    Schedule {
        category,
        columns,
        rows: rows.into_iter().map(|(_, _, r)| r).collect(),
        totals,
    }
}

pub const ROW: f64 = 6.0;
const TITLE_ROW: f64 = 8.0;
const PAD: f64 = 1.5;

/// The schedule as a table drawn in paper millimetres, its top-left corner
/// at the origin. Row text carries the row's element, so clicking a row on a
/// sheet selects that element.
pub fn schedule_drawing(view: ElementId, title: &str, s: &Schedule) -> PlanView {
    let width: f64 = s.columns.iter().map(|c| c.width).sum();
    let mut items = Vec::new();
    let line = |items: &mut Vec<PlanItem>, a: DVec2, b: DVec2, heavy: bool| {
        items.push(PlanItem::Line {
            id: view,
            pen: if heavy { Pen::Cut } else { Pen::Symbol },
            a,
            b,
        })
    };
    let text = |items: &mut Vec<PlanItem>, id, at, t: &str, size, align| {
        items.push(PlanItem::Text {
            id,
            at,
            text: t.to_string(),
            size,
            align,
        })
    };
    text(
        &mut items,
        view,
        DVec2::new(width / 2.0, -TITLE_ROW / 2.0),
        &title.to_uppercase(),
        3.5,
        Anchor::Middle,
    );
    let rows = s.rows.len() + 2; // header, rows, totals
    let bottom = -TITLE_ROW - rows as f64 * ROW;
    let mut x = 0.0;
    let cell = |c: &Column, x: f64| {
        if c.numeric {
            (x + c.width - PAD, Anchor::End)
        } else {
            (x + PAD, Anchor::Start)
        }
    };
    for (i, c) in s.columns.iter().enumerate() {
        let (tx, align) = cell(c, x);
        text(
            &mut items,
            view,
            DVec2::new(tx, -TITLE_ROW - ROW / 2.0),
            c.title,
            2.5,
            align,
        );
        for (r, row) in s.rows.iter().enumerate() {
            let y = -TITLE_ROW - (r as f64 + 1.5) * ROW;
            text(
                &mut items,
                row.id,
                DVec2::new(tx, y),
                &row.cells[i],
                2.5,
                align,
            );
        }
        if let Some(t) = s.totals.get(i).filter(|t| !t.is_empty()) {
            text(
                &mut items,
                view,
                DVec2::new(tx, bottom + ROW / 2.0),
                t,
                2.5,
                align,
            );
        }
        if i > 0 {
            line(
                &mut items,
                DVec2::new(x, -TITLE_ROW),
                DVec2::new(x, bottom),
                false,
            );
        }
        x += c.width;
    }
    for r in 0..=rows {
        let y = -TITLE_ROW - r as f64 * ROW;
        let heavy = r == 0 || r == 1 || r == rows - 1 || r == rows;
        line(&mut items, DVec2::new(0.0, y), DVec2::new(width, y), heavy);
    }
    // outer frame
    for (a, b) in [
        (DVec2::ZERO, DVec2::new(width, 0.0)),
        (DVec2::ZERO, DVec2::new(0.0, bottom)),
        (DVec2::new(width, 0.0), DVec2::new(width, bottom)),
    ] {
        line(&mut items, a, b, true);
    }
    PlanView { items }
}

/// The schedule as CSV, header first, totals last.
pub fn to_csv(s: &Schedule) -> String {
    let quote = |v: &str| {
        if v.contains([',', '"', '\n']) {
            format!("\"{}\"", v.replace('"', "\"\""))
        } else {
            v.to_string()
        }
    };
    let line = |cells: Vec<&str>| cells.into_iter().map(quote).collect::<Vec<_>>().join(",");
    let mut out = line(s.columns.iter().map(|c| c.title).collect()) + "\n";
    for r in &s.rows {
        out += &(line(r.cells.iter().map(String::as_str).collect()) + "\n");
    }
    let mut totals: Vec<&str> = s.totals.iter().map(String::as_str).collect();
    totals.resize(s.columns.len(), "");
    out += &(line(totals) + "\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::sample::sample_house;

    #[test]
    fn door_schedule_lists_marks_in_order_and_follows_edits() {
        let mut doc = creabim_core::Document::from_model(sample_house());
        let doors = schedule(doc.model(), ScheduleCategory::Doors);
        let marks: Vec<&str> = doors.rows.iter().map(|r| r.cells[0].as_str()).collect();
        assert_eq!(marks, ["D01", "D02"]);
        assert_eq!(doors.totals[0], "2 doors");
        let front = doors.rows[0].id;
        assert_eq!(doors.rows[0].cells[3], "1000");
        doc.edit(front, "Widen", |e| set_value(e, Field::Width, 1.2))
            .unwrap();
        let doors = schedule(doc.model(), ScheduleCategory::Doors);
        assert_eq!(doors.rows[0].cells[3], "1200");
    }

    #[test]
    fn wall_schedule_totals_areas_and_exports_csv() {
        let m = sample_house();
        let walls = schedule(&m, ScheduleCategory::Walls);
        assert_eq!(walls.rows.len(), 9);
        let csv = to_csv(&walls);
        assert!(csv.starts_with("Name,Level,Material,Length"));
        assert!(csv.contains("GF Wall 1,Ground Floor,Brick,10000,300,3000"));
        assert!(csv.trim_end().ends_with("m²"));
    }

    #[test]
    fn layered_walls_list_their_assembly() {
        let m = creabim_core::sample::layered_house();
        let walls = schedule(&m, ScheduleCategory::Walls);
        assert_eq!(walls.columns[3].title, "Assembly");
        let row = walls
            .rows
            .iter()
            .find(|r| r.cells[0] == "GF Wall 1")
            .unwrap();
        assert_eq!(row.cells[2], "Block, concrete", "listed by its core");
        assert_eq!(row.cells[3], "Exterior wall 365, brick cavity");
        assert_eq!(row.cells[5], "365");
        assert_eq!(walls.totals.len(), walls.columns.len());
    }

    #[test]
    fn an_imperial_project_schedules_in_feet_and_inches() {
        let mut doc = creabim_core::Document::from_model(sample_house());
        let tx = creabim_core::units::set_units(doc.model(), creabim_core::Units::imperial());
        doc.commit(tx).unwrap();
        let walls = schedule(doc.model(), ScheduleCategory::Walls);
        // 10 m long, 300 thick, 3 m high
        assert_eq!(
            walls.rows[0].cells[..6],
            [
                "GF Wall 1",
                "Ground Floor",
                "Brick",
                "32'-9 3/4\"",
                "0'-11 3/4\"",
                "9'-10 1/8\""
            ]
        );
        assert!(walls.totals.last().unwrap().ends_with("ft²"));
        // inch marks are quoted in the CSV
        assert!(to_csv(&walls).contains(",\"32'-9 3/4\"\"\","));
    }

    #[test]
    fn schedule_drawing_is_a_table() {
        let m = sample_house();
        let s = schedule(&m, ScheduleCategory::Windows);
        let d = schedule_drawing(ElementId::NIL, "Window Schedule", &s);
        let (lo, hi) = d.bounds().unwrap();
        let width: f64 = s.columns.iter().map(|c| c.width).sum();
        assert!((hi.x - lo.x - width).abs() < 1e-9);
        assert!((lo.y + TITLE_ROW + (s.rows.len() + 2) as f64 * ROW).abs() < 1e-9);
    }
}
