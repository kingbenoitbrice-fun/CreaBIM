//! Excel workbooks (`.xlsx`) of the project's schedules: a summary sheet,
//! then one sheet for each category that lists something. Numbers go in
//! as numbers, with their unit in the column heading, so the sums and
//! sorting of a spreadsheet work on them.

use crate::schedule::{self, Schedule};
use creabim_core::{takeoff, ElementId, Model, ScheduleCategory};
use rust_xlsxwriter::{Format, FormatBorder, Workbook, Worksheet, XlsxError};

/// A cell read back from a schedule's text: a number with the unit it was
/// written in, or text.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Number(f64, Option<String>),
    Text(String),
}

/// Units a schedule writes after a number.
const UNITS: [&str; 9] = ["m²", "m³", "ft²", "ft³", "mm", "cm", "m", "ft", "in"];

/// `1200` is a number, `12.50 m²` a number in m², `3'-6"` and `GF Wall 1`
/// are text.
pub fn cell(text: &str) -> Cell {
    let t = text.trim();
    if let Ok(v) = t.parse::<f64>() {
        if v.is_finite() {
            return Cell::Number(v, None);
        }
    }
    if let Some((num, unit)) = t.rsplit_once(' ') {
        if UNITS.contains(&unit) {
            if let Ok(v) = num.trim().parse::<f64>() {
                if v.is_finite() {
                    return Cell::Number(v, Some(unit.to_string()));
                }
            }
        }
    }
    Cell::Text(t.to_string())
}

/// A sheet's name: at most 31 characters, none of `[]:*?/\`.
fn sheet_name(label: &str) -> String {
    let clean: String = label
        .chars()
        .map(|c| if "[]:*?/\\".contains(c) { '-' } else { c })
        .collect();
    clean.chars().take(31).collect()
}

/// Numeric columns that hold lengths, written without their unit in metric.
const LENGTHS: [&str; 6] = [
    "Width",
    "Height",
    "Sill",
    "Thickness",
    "Length",
    "Perimeter",
];

/// The heading of each column, with the unit its numbers are in when they
/// all share one: "Width (mm)", "Area (m²)".
fn headings(model: &Model, s: &Schedule) -> Vec<String> {
    units_of(model, s)
        .into_iter()
        .zip(&s.columns)
        .map(|(u, c)| match u {
            Some(u) => format!("{} ({u})", c.title),
            None => c.title.to_string(),
        })
        .collect()
}

/// The unit each column's numbers share, if they share one.
fn units_of(model: &Model, s: &Schedule) -> Vec<Option<String>> {
    let units = model.units();
    s.columns
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if !c.numeric {
                return None;
            }
            let mut seen: Option<Option<String>> = None;
            let mut same = true;
            let mut any = false;
            for r in s.rows.iter().filter(|r| !r.group) {
                if let Some(Cell::Number(_, u)) = r.cells.get(i).map(|t| cell(t)) {
                    any = true;
                    match &seen {
                        None => seen = Some(u),
                        Some(prev) if *prev != u => same = false,
                        _ => {}
                    }
                }
            }
            match seen.flatten() {
                Some(u) if same => Some(u),
                // metric lengths are written without their unit
                None if any
                    && same
                    && !units.is_imperial()
                    && (c.field.is_some_and(|f| f.is_length())
                        || (c.numeric && LENGTHS.contains(&c.title))) =>
                {
                    Some(units.length.symbol().to_string())
                }
                _ => None,
            }
        })
        .collect()
}

/// A schedule's totals in words for the summary: "25 walls · Length
/// 149800 mm · Area 354.92 m²".
fn totals_line(model: &Model, s: &Schedule) -> String {
    let units = units_of(model, s);
    s.totals
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.trim().is_empty())
        .map(|(i, t)| {
            let title = s.columns.get(i).map_or("", |c| c.title);
            match (cell(t), units.get(i).cloned().flatten()) {
                (Cell::Number(_, None), Some(u)) => format!("{title} {} {u}", t.trim()),
                (Cell::Number(..), _) => format!("{title} {}", t.trim()),
                (Cell::Text(t), _) => t,
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// "Door schedule"; "Columns and beams schedule" stays plural, as in the app.
fn title(s: &Schedule) -> String {
    let label = s.category.label();
    if label.contains(' ') {
        format!("{label} schedule")
    } else {
        format!("{} schedule", label.trim_end_matches('s'))
    }
}

fn write_cell(
    ws: &mut Worksheet,
    row: u32,
    col: u16,
    text: &str,
    numeric: bool,
    format: &Format,
) -> Result<(), XlsxError> {
    // a room number like 0.10 stays text
    let read = if numeric {
        cell(text)
    } else {
        Cell::Text(text.trim().to_string())
    };
    match read {
        Cell::Number(v, _) => ws.write_number_with_format(row, col, v, format)?,
        Cell::Text(t) if t.is_empty() => ws.write_blank(row, col, format)?,
        Cell::Text(t) => ws.write_string_with_format(row, col, t, format)?,
    };
    Ok(())
}

/// Write one schedule as a sheet: a title, the headings, its rows (a type's
/// heading row in bold when grouped) and its totals in bold.
fn write_schedule(
    ws: &mut Worksheet,
    model: &Model,
    s: &Schedule,
    title: &str,
) -> Result<(), XlsxError> {
    let bold = Format::new().set_bold();
    let head = Format::new()
        .set_bold()
        .set_background_color("E8ECE6")
        .set_border_bottom(FormatBorder::Thin);
    let plain = Format::new();
    let total = Format::new().set_bold().set_border_top(FormatBorder::Thin);
    ws.write_string_with_format(0, 0, title, &Format::new().set_bold().set_font_size(14))?;
    let heads = headings(model, s);
    for (c, h) in heads.iter().enumerate() {
        ws.write_string_with_format(2, c as u16, h, &head)?;
    }
    let mut row = 3;
    for r in &s.rows {
        let f = if r.group { &bold } else { &plain };
        for (c, t) in r.cells.iter().enumerate() {
            let numeric = s.columns.get(c).is_some_and(|c| c.numeric);
            write_cell(ws, row, c as u16, t, numeric, f)?;
        }
        row += 1;
    }
    for c in 0..s.columns.len() {
        let t = s.totals.get(c).map(String::as_str).unwrap_or("");
        let numeric = s.columns[c].numeric;
        write_cell(ws, row, c as u16, t, numeric, &total)?;
    }
    // each column as wide as its widest text, within reason
    for (c, h) in heads.iter().enumerate() {
        let widest = std::iter::once(h.chars().count())
            .chain(
                s.rows
                    .iter()
                    .filter_map(|r| r.cells.get(c))
                    .map(|t| t.chars().count()),
            )
            .chain(s.totals.get(c).map(|t| t.chars().count()))
            .max()
            .unwrap_or(8);
        ws.set_column_width(c as u16, (widest as f64 + 2.0).clamp(8.0, 48.0))?;
    }
    ws.set_freeze_panes(3, 0)?;
    Ok(())
}

/// The schedules that list something, in the order of the app's list.
pub fn schedules(model: &Model) -> Vec<Schedule> {
    ScheduleCategory::ALL
        .iter()
        // the take-off has sheets of its own
        .filter(|c| **c != ScheduleCategory::Materials)
        .map(|c| schedule::schedule(model, *c))
        .filter(|s| !s.rows.is_empty())
        .collect()
}

/// The project's schedules as an Excel workbook: a *Summary* sheet listing
/// each schedule with its count and totals, then a sheet for each.
/// `project` heads the summary.
pub fn schedules_xlsx(model: &Model, project: &str) -> Result<Vec<u8>, String> {
    build(model, project).map_err(|e| e.to_string())
}

fn build(model: &Model, project: &str) -> Result<Vec<u8>, XlsxError> {
    let mut book = Workbook::new();
    let all = schedules(model);
    {
        let ws = book.add_worksheet();
        ws.set_name("Summary")?;
        let head = Format::new()
            .set_bold()
            .set_background_color("E8ECE6")
            .set_border_bottom(FormatBorder::Thin);
        ws.write_string_with_format(0, 0, project, &Format::new().set_bold().set_font_size(14))?;
        ws.write_string(1, 0, "Schedules, read from the model when exported")?;
        for (c, h) in ["Schedule", "Listed", "Totals"].iter().enumerate() {
            ws.write_string_with_format(3, c as u16, *h, &head)?;
        }
        for (i, s) in all.iter().enumerate() {
            let r = 4 + i as u32;
            ws.write_string(r, 0, s.category.label())?;
            ws.write_number(r, 1, s.listed as f64)?;
            ws.write_string(r, 2, totals_line(model, s))?;
        }
        ws.set_column_width(0, 22)?;
        ws.set_column_width(1, 10)?;
        ws.set_column_width(2, 60)?;
    }
    for s in &all {
        let ws = book.add_worksheet();
        ws.set_name(sheet_name(s.category.label()))?;
        write_schedule(ws, model, s, &title(s))?;
    }
    let lines = takeoff::lines(model);
    if !lines.is_empty() {
        write_takeoff(book.add_worksheet(), model, &lines)?;
        write_takeoff_lines(book.add_worksheet(), model, &lines)?;
    }
    book.save_to_buffer()
}

fn name_of(model: &Model, id: Option<ElementId>, none: &str) -> String {
    id.and_then(|id| model.get(id))
        .map_or_else(|| none.to_string(), |e| e.name.clone())
}

/// The *Take-off* sheet: totals by material, priced where a material has a
/// rate, then totals by level.
fn write_takeoff(
    ws: &mut Worksheet,
    model: &Model,
    lines: &[takeoff::Line],
) -> Result<(), XlsxError> {
    ws.set_name("Take-off")?;
    let head = Format::new()
        .set_bold()
        .set_background_color("E8ECE6")
        .set_border_bottom(FormatBorder::Thin);
    let two = Format::new().set_num_format("0.00");
    let total = Format::new()
        .set_bold()
        .set_border_top(FormatBorder::Thin)
        .set_num_format("0.00");
    let bold = Format::new().set_bold().set_font_size(14);
    ws.write_string_with_format(0, 0, "Take-off by material", &bold)?;
    let heads = [
        "Material",
        "Elements",
        "Area (m²)",
        "Volume (m³)",
        "Length (m)",
        "Rate",
        "Per",
        "Cost",
    ];
    let write_heads = |ws: &mut Worksheet, row: u32, first: &str| -> Result<(), XlsxError> {
        for (c, h) in heads.iter().enumerate() {
            let h = if c == 0 { first } else { h };
            ws.write_string_with_format(row, c as u16, h, &head)?;
        }
        Ok(())
    };
    write_heads(ws, 2, "Material")?;
    let mut row = 3;
    let write_total = |ws: &mut Worksheet,
                       row: u32,
                       name: &str,
                       t: &takeoff::Total,
                       f: &Format|
     -> Result<(), XlsxError> {
        ws.write_string(row, 0, name)?;
        ws.write_number(row, 1, t.elements as f64)?;
        ws.write_number_with_format(row, 2, t.quantity.area, f)?;
        ws.write_number_with_format(row, 3, t.quantity.volume, f)?;
        ws.write_number_with_format(row, 4, t.quantity.length, f)?;
        if let Some(r) = t.rate {
            ws.write_number_with_format(row, 5, r.amount, f)?;
            ws.write_string(row, 6, r.per.label())?;
        }
        if let Some(c) = t.cost {
            ws.write_number_with_format(row, 7, c, f)?;
        }
        Ok(())
    };
    for t in takeoff::by_material(model, lines) {
        write_total(ws, row, &name_of(model, t.key, "No material"), &t, &two)?;
        row += 1;
    }
    ws.write_string_with_format(row, 0, "Priced total", &total)?;
    ws.write_number_with_format(row, 7, takeoff::total_cost(model, lines), &total)?;
    row += 3;
    ws.write_string_with_format(row, 0, "Take-off by level", &bold)?;
    row += 2;
    write_heads(ws, row, "Level")?;
    row += 1;
    for t in takeoff::by_level(model, lines) {
        write_total(ws, row, &name_of(model, t.key, "No level"), &t, &two)?;
        row += 1;
    }
    ws.set_column_width(0, 34)?;
    for c in 1..8 {
        ws.set_column_width(c, 13)?;
    }
    Ok(())
}

/// The *Take-off lines* sheet: every element's materials, layer by layer.
fn write_takeoff_lines(
    ws: &mut Worksheet,
    model: &Model,
    lines: &[takeoff::Line],
) -> Result<(), XlsxError> {
    ws.set_name("Take-off lines")?;
    let head = Format::new()
        .set_bold()
        .set_background_color("E8ECE6")
        .set_border_bottom(FormatBorder::Thin);
    let two = Format::new().set_num_format("0.00");
    let heads = [
        "Element",
        "Category",
        "Level",
        "Layer",
        "Material",
        "Thickness (mm)",
        "Area (m²)",
        "Volume (m³)",
        "Length (m)",
    ];
    for (c, h) in heads.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, *h, &head)?;
    }
    for (i, l) in lines.iter().enumerate() {
        let r = 1 + i as u32;
        ws.write_string(r, 0, name_of(model, Some(l.element), ""))?;
        ws.write_string(r, 1, l.category)?;
        ws.write_string(r, 2, name_of(model, l.level, ""))?;
        if let Some((n, f)) = l.layer {
            ws.write_string(r, 3, format!("{} {}", n + 1, f.label()))?;
        }
        ws.write_string(r, 4, name_of(model, l.material, "-"))?;
        if l.thickness > 0.0 {
            ws.write_number(r, 5, (l.thickness * 1000.0 * 10.0).round() / 10.0)?;
        }
        ws.write_number_with_format(r, 6, l.quantity.area, &two)?;
        ws.write_number_with_format(r, 7, l.quantity.volume, &two)?;
        ws.write_number_with_format(r, 8, l.quantity.length, &two)?;
    }
    for (c, w) in [28, 12, 16, 16, 30, 15, 12, 12, 12].iter().enumerate() {
        ws.set_column_width(c as u16, *w)?;
    }
    ws.set_freeze_panes(1, 0)?;
    ws.autofilter(0, 0, lines.len() as u32, heads.len() as u16 - 1)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::sample::sample_house;

    #[test]
    fn cells_read_back_as_numbers_with_their_units_or_as_text() {
        assert_eq!(cell("1200"), Cell::Number(1200.0, None));
        assert_eq!(cell(" 12.50 m² "), Cell::Number(12.5, Some("m²".into())));
        assert_eq!(cell("3'-6\""), Cell::Text("3'-6\"".into()));
        assert_eq!(cell("GF Wall 1"), Cell::Text("GF Wall 1".into()));
        assert_eq!(cell("2 doors"), Cell::Text("2 doors".into()));
        assert_eq!(cell("NaN"), Cell::Text("NaN".into()));
    }

    #[test]
    fn sheet_names_are_short_and_clean() {
        assert_eq!(sheet_name("Columns and beams"), "Columns and beams");
        assert_eq!(sheet_name("a/b:c"), "a-b-c");
        assert_eq!(sheet_name(&"x".repeat(40)).len(), 31);
    }

    #[test]
    fn the_workbook_has_a_summary_and_a_sheet_per_listed_schedule() {
        let model = sample_house();
        let bytes = schedules_xlsx(&model, "Sample house").unwrap();
        // an xlsx is a zip: its local file headers start with PK\x03\x04
        assert_eq!(&bytes[..4], b"PK\x03\x04");
        let all = schedules(&model);
        assert!(all.iter().any(|s| s.category == ScheduleCategory::Doors));
        // file names in a zip are stored as is: one worksheet part for the
        // summary and one for each schedule (each name shows twice, in its
        // local header and in the central directory)
        let part = b"xl/worksheets/sheet";
        let parts = bytes.windows(part.len()).filter(|w| w == part).count();
        // and the take-off's two sheets
        assert_eq!(parts, 2 * (1 + all.len() + 2));
        let doors = all
            .iter()
            .find(|s| s.category == ScheduleCategory::Doors)
            .unwrap();
        let heads = headings(&model, doors);
        assert!(heads.iter().any(|h| h == "Width (mm)"), "{heads:?}");
        assert!(heads.iter().any(|h| h == "Mark"));
    }
}
