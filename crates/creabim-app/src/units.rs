//! The project's units in the app. The app sets them once a frame from the
//! model ([`set`]); every field, readout and status message then formats
//! and reads lengths through this module, so they all follow the project's
//! units without passing them round. The model itself is always metres.

use creabim_core::Units;
use eframe::egui;
use std::cell::Cell;

thread_local! {
    static CURRENT: Cell<Units> = Cell::new(Units::metric());
}

/// Use these units from now on (called each frame with the project's).
pub fn set(u: Units) {
    CURRENT.with(|c| c.set(u));
}

pub fn get() -> Units {
    CURRENT.with(|c| c.get())
}

/// A length with its unit: `1067 mm`, `3'-6 1/2"`.
pub fn len(metres: f64) -> String {
    get().length(metres)
}

/// A signed height, as level elevations are written: `+3000`, `+9'-10"`.
pub fn elevation(metres: f64) -> String {
    let u = get();
    let text = u.elevation(metres);
    if u.is_imperial() {
        text
    } else {
        format!("{text} {}", unit_symbol(&u))
    }
}

/// The metric unit lengths are written in (`mm`, `cm` or `m`).
pub fn symbol() -> &'static str {
    unit_symbol(&get())
}

fn unit_symbol(u: &Units) -> &'static str {
    match u.length {
        creabim_core::LengthUnit::Centimetres => "cm",
        creabim_core::LengthUnit::Metres => "m",
        _ => "mm",
    }
}

/// An area: `12.50 m²`, `134.55 ft²`.
pub fn area(square_metres: f64) -> String {
    get().area(square_metres)
}

/// A typed length, metres.
pub fn parse(text: &str) -> Option<f64> {
    get().parse(text)
}

/// A field for a length in metres, shown and typed in the project's units.
/// Any unit can be typed (`3'-6 1/2"`, `1067`, `1.2 m`); dragging moves it
/// about a centimetre (or half an inch) a step.
pub fn drag(v: &mut f64) -> egui::DragValue<'_> {
    let u = get();
    let speed = if u.is_imperial() { 0.0127 } else { 0.01 };
    egui::DragValue::new(v)
        .speed(speed)
        .custom_formatter(move |v, _| u.length(v))
        .custom_parser(move |s| u.parse(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_show_and_read_the_project_units() {
        set(Units::imperial());
        assert_eq!(len(1.0668), "3'-6\"");
        assert!((parse("3'-6\"").unwrap() - 1.0668).abs() < 1e-9);
        assert_eq!(elevation(3.0), "+9'-10 1/8\"");
        set(Units::metric());
        assert_eq!(len(1.0668), "1067 mm");
        assert_eq!(elevation(3.0), "+3000 mm");
        assert_eq!(area(2.0), "2.00 m²");
    }
}
