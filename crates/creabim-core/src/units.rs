//! Project units: how lengths, areas and volumes are written and read.
//!
//! The model always stores metres. A project picks a [`LengthUnit`] and a
//! precision; every view, drawing, schedule and field formats through
//! [`Units`] and parses what is typed with [`Units::parse`], which also
//! accepts any other unit written out (`3'-6 1/2"`, `1067 mm`, `1.2 m`).
//! Older files have no units element and read as millimetres, which is
//! how their dimensions and schedules were always written.

use serde::{Deserialize, Serialize};

const MM_PER_INCH: f64 = 25.4;
const M_PER_INCH: f64 = MM_PER_INCH / 1000.0;
const M_PER_FOOT: f64 = M_PER_INCH * 12.0;

/// The unit lengths are written in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LengthUnit {
    #[default]
    Millimetres,
    Centimetres,
    Metres,
    /// Feet and fractional inches: 3'-6 1/2".
    FeetInches,
    /// Decimal feet: 3.54'.
    Feet,
    /// Fractional inches: 42 1/2".
    Inches,
}

impl LengthUnit {
    pub const ALL: [LengthUnit; 6] = [
        LengthUnit::Millimetres,
        LengthUnit::Centimetres,
        LengthUnit::Metres,
        LengthUnit::FeetInches,
        LengthUnit::Feet,
        LengthUnit::Inches,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LengthUnit::Millimetres => "Millimetres (mm)",
            LengthUnit::Centimetres => "Centimetres (cm)",
            LengthUnit::Metres => "Metres (m)",
            LengthUnit::FeetInches => "Feet and fractional inches (3'-6 1/2\")",
            LengthUnit::Feet => "Decimal feet (3.54')",
            LengthUnit::Inches => "Fractional inches (42 1/2\")",
        }
    }

    pub fn is_imperial(self) -> bool {
        matches!(
            self,
            LengthUnit::FeetInches | LengthUnit::Feet | LengthUnit::Inches
        )
    }

    /// Lengths in fractions of an inch rather than decimals.
    pub fn is_fractional(self) -> bool {
        matches!(self, LengthUnit::FeetInches | LengthUnit::Inches)
    }

    /// The precision a project switching to this unit starts with.
    pub fn default_precision(self) -> u8 {
        match self {
            LengthUnit::Millimetres => 0,
            LengthUnit::Centimetres => 1,
            LengthUnit::Metres => 3,
            LengthUnit::FeetInches | LengthUnit::Inches => 3,
            LengthUnit::Feet => 2,
        }
    }

    /// How many metres one of this unit is (an inch for feet and inches).
    fn metres(self) -> f64 {
        match self {
            LengthUnit::Millimetres => 0.001,
            LengthUnit::Centimetres => 0.01,
            LengthUnit::Metres => 1.0,
            LengthUnit::Feet => M_PER_FOOT,
            LengthUnit::FeetInches | LengthUnit::Inches => M_PER_INCH,
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            LengthUnit::Millimetres => "mm",
            LengthUnit::Centimetres => "cm",
            LengthUnit::Metres => "m",
            LengthUnit::Feet => "'",
            LengthUnit::FeetInches | LengthUnit::Inches => "\"",
        }
    }
}

/// A project's units. Stored as the project's [`crate::ElementKind::Units`]
/// element, so changing them undoes and saves like any edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Units {
    #[serde(default)]
    pub length: LengthUnit,
    /// Decimal places for metric units and decimal feet; for feet and
    /// inches, the smallest fraction of an inch as a power of two
    /// (0 = whole inches, 1 = 1/2", 3 = 1/8", 4 = 1/16").
    #[serde(default)]
    pub precision: u8,
}

impl Default for Units {
    fn default() -> Self {
        Self::metric()
    }
}

/// Fractions of an inch offered for feet and inches: 1" to 1/64".
pub const MAX_FRACTION: u8 = 6;
/// Decimal places offered for decimal units.
pub const MAX_DECIMALS: u8 = 4;

impl Units {
    /// Millimetres, whole: what CreaBIM always wrote dimensions in.
    pub fn metric() -> Self {
        Self::of(LengthUnit::Millimetres)
    }

    /// Feet and inches to 1/8".
    pub fn imperial() -> Self {
        Self::of(LengthUnit::FeetInches)
    }

    pub fn of(length: LengthUnit) -> Self {
        Self {
            length,
            precision: length.default_precision(),
        }
    }

    pub fn is_imperial(&self) -> bool {
        self.length.is_imperial()
    }

    /// The precisions to choose from for this unit, with how they read.
    pub fn precisions(&self) -> Vec<(u8, String)> {
        if self.length.is_fractional() {
            (0..=MAX_FRACTION)
                .map(|p| {
                    let label = match p {
                        0 => "1\"".to_string(),
                        p => format!("1/{}\"", 1u32 << p),
                    };
                    (p, label)
                })
                .collect()
        } else {
            (0..=MAX_DECIMALS)
                .map(|p| {
                    let label = match p {
                        0 => "0".to_string(),
                        p => format!("0.{}", "0".repeat(p as usize)),
                    };
                    (p, label)
                })
                .collect()
        }
    }

    /// The smallest step a length is written to, metres.
    pub fn step(&self) -> f64 {
        if self.length.is_fractional() {
            M_PER_INCH / f64::from(1u32 << self.precision.min(MAX_FRACTION))
        } else {
            self.length.metres() / 10f64.powi(i32::from(self.precision.min(MAX_DECIMALS)))
        }
    }

    /// A length with its unit: `1067 mm`, `1.067 m`, `3'-6 1/2"`.
    pub fn length(&self, metres: f64) -> String {
        match self.length {
            LengthUnit::FeetInches | LengthUnit::Inches | LengthUnit::Feet => {
                self.dimension(metres)
            }
            u => format!("{} {}", self.dimension(metres), u.symbol()),
        }
    }

    /// A length as dimensions write it: metric without its unit (`1067`),
    /// imperial with its marks (`3'-6 1/2"`).
    pub fn dimension(&self, metres: f64) -> String {
        if !metres.is_finite() {
            return "?".into();
        }
        match self.length {
            LengthUnit::FeetInches => self.feet_inches(metres),
            LengthUnit::Inches => {
                let (neg, whole, num, den) = self.inch_parts(metres);
                let mut s = String::new();
                if neg {
                    s.push('-');
                }
                match (whole, num) {
                    (w, 0) => s.push_str(&w.to_string()),
                    (0, n) => s.push_str(&format!("{n}/{den}")),
                    (w, n) => s.push_str(&format!("{w} {n}/{den}")),
                }
                s.push('"');
                s
            }
            LengthUnit::Feet => format!("{}'", self.decimal(metres)),
            _ => self.decimal(metres),
        }
    }

    /// A signed level elevation: `+3000`, `+9'-10"`, `±0`.
    pub fn elevation(&self, metres: f64) -> String {
        let text = self.dimension(metres.abs());
        if self.dimension(metres) == self.dimension(0.0) {
            format!("±{text}")
        } else if metres < 0.0 {
            format!("-{text}")
        } else {
            format!("+{text}")
        }
    }

    /// An area: `12.50 m²` or `134.5 ft²`.
    pub fn area(&self, square_metres: f64) -> String {
        if self.is_imperial() {
            format!("{:.2} ft²", square_metres / (M_PER_FOOT * M_PER_FOOT))
        } else {
            format!("{square_metres:.2} m²")
        }
    }

    /// A volume: `3.20 m³` or `113.0 ft³`.
    pub fn volume(&self, cubic_metres: f64) -> String {
        if self.is_imperial() {
            format!("{:.2} ft³", cubic_metres / M_PER_FOOT.powi(3))
        } else {
            format!("{cubic_metres:.2} m³")
        }
    }

    /// The decimal number of this unit, without a symbol, at the precision.
    fn decimal(&self, metres: f64) -> String {
        let p = usize::from(self.precision.min(MAX_DECIMALS));
        let v = metres / self.length.metres();
        let s = format!("{v:.p$}");
        // no "-0"
        if s.trim_start_matches('-')
            .chars()
            .all(|c| c == '0' || c == '.')
        {
            s.trim_start_matches('-').to_string()
        } else {
            s
        }
    }

    /// Sign, whole inches and the fraction num/den, rounded to the precision
    /// and reduced.
    fn inch_parts(&self, metres: f64) -> (bool, u64, u64, u64) {
        let den0 = 1u64 << self.precision.min(MAX_FRACTION);
        let steps = (metres.abs() / M_PER_INCH * den0 as f64).round() as u64;
        let neg = metres < 0.0 && steps > 0;
        let (whole, mut num) = (steps / den0, steps % den0);
        let mut den = den0;
        while num > 0 && num % 2 == 0 {
            num /= 2;
            den /= 2;
        }
        (neg, whole, num, den)
    }

    fn feet_inches(&self, metres: f64) -> String {
        let (neg, inches, num, den) = self.inch_parts(metres);
        let (feet, inches) = (inches / 12, inches % 12);
        let frac = if num > 0 {
            format!(" {num}/{den}")
        } else {
            String::new()
        };
        format!("{}{feet}'-{inches}{frac}\"", if neg { "-" } else { "" })
    }

    /// Read a typed length, metres. A bare number is in the project's
    /// unit (feet for feet and inches, where `3 6` is 3'-6"); any unit can
    /// be written out instead: `1067mm`, `106.7 cm`, `1.2 m`, `3'-6 1/2"`,
    /// `3' 6"`, `42"`, `3.5'`, `3 ft 6 in`. A comma works as the decimal point.
    pub fn parse(&self, text: &str) -> Option<f64> {
        let s = text.trim().replace(',', ".").replace('−', "-");
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let (neg, body) = match s.strip_prefix('-') {
            Some(rest) => (true, rest.trim_start()),
            None => (false, s.strip_prefix('+').unwrap_or(s).trim_start()),
        };
        let v = self.parse_unsigned(&body.to_ascii_lowercase())?;
        v.is_finite().then_some(if neg { -v } else { v })
    }

    fn parse_unsigned(&self, s: &str) -> Option<f64> {
        // feet, maybe followed by inches
        for mark in ["'", "feet", "foot", "ft"] {
            if let Some(i) = s.find(mark) {
                let feet = number(s[..i].trim())?;
                let rest = s[i + mark.len()..].trim();
                let rest = rest.strip_prefix('-').unwrap_or(rest).trim();
                let inches = if rest.is_empty() {
                    0.0
                } else {
                    inches_text(rest)?
                };
                return Some(feet * M_PER_FOOT + inches * M_PER_INCH);
            }
        }
        if let Some(rest) = ["\"", "inches", "inch", "in"]
            .iter()
            .find_map(|m| s.strip_suffix(m))
        {
            return Some(mixed(rest.trim())? * M_PER_INCH);
        }
        for (mark, k) in [("mm", 0.001), ("cm", 0.01), ("m", 1.0)] {
            if let Some(rest) = s.strip_suffix(mark) {
                return Some(number(rest.trim())? * k);
            }
        }
        // a bare number, in the project's unit
        match self.length {
            LengthUnit::FeetInches => {
                // "3 6" is 3'-6", "3 6 1/2" is 3'-6 1/2"; "6 1/2" stays a
                // mixed number of feet
                let mut parts = s.splitn(2, char::is_whitespace);
                let first = parts.next()?;
                match parts.next().map(str::trim) {
                    Some(rest) if !first.contains('/') && !is_fraction_only(rest) => {
                        Some(number(first)? * M_PER_FOOT + mixed(rest)? * M_PER_INCH)
                    }
                    _ => Some(mixed(s)? * M_PER_FOOT),
                }
            }
            LengthUnit::Inches => Some(mixed(s)? * M_PER_INCH),
            u => Some(mixed(s)? * u.metres()),
        }
    }
}

/// The edit that gives `model` these units, as one undo step: its units
/// element changed, or one added.
pub fn set_units(model: &crate::Model, units: Units) -> crate::Transaction {
    let tx = crate::Transaction::new("Project units");
    let current = model
        .iter()
        .find(|e| matches!(e.kind, crate::ElementKind::Units(_)));
    match current {
        Some(e) if e.kind == crate::ElementKind::Units(units) => tx,
        Some(e) => {
            let mut after = e.clone();
            after.kind = crate::ElementKind::Units(units);
            tx.replace(e.clone(), after)
        }
        None => tx.insert(crate::Element::new(
            "Project units",
            crate::ElementKind::Units(units),
        )),
    }
}

fn is_fraction_only(s: &str) -> bool {
    s.contains('/') && !s.contains(char::is_whitespace) && !s.contains('-')
}

/// Inches after a feet mark: `6`, `6 1/2`, `6-1/2`, `6 1/2"`, `6in`.
fn inches_text(s: &str) -> Option<f64> {
    let s = ["\"", "inches", "inch", "in"]
        .iter()
        .find_map(|m| s.strip_suffix(m))
        .unwrap_or(s)
        .trim();
    mixed(s)
}

/// A plain decimal number.
fn number(s: &str) -> Option<f64> {
    if s.is_empty() || s.contains(|c: char| !(c.is_ascii_digit() || c == '.')) {
        return None;
    }
    s.parse::<f64>().ok()
}

/// A number with an optional fraction: `6`, `6.5`, `1/2`, `6 1/2`, `6-1/2`.
fn mixed(s: &str) -> Option<f64> {
    let s = s.trim();
    let fraction = |f: &str| -> Option<f64> {
        let (n, d) = f.split_once('/')?;
        let (n, d) = (number(n.trim())?, number(d.trim())?);
        (d > 0.0).then_some(n / d)
    };
    if !s.contains('/') {
        return number(s);
    }
    match s.rsplit_once([' ', '-']) {
        Some((whole, f)) if !whole.trim().is_empty() => Some(number(whole.trim())? + fraction(f)?),
        _ => fraction(s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn metric_lengths_read_as_before() {
        let u = Units::metric();
        assert_eq!(u.dimension(1.0674), "1067");
        assert_eq!(u.length(2.5), "2500 mm");
        let m = Units {
            length: LengthUnit::Metres,
            precision: 2,
        };
        assert_eq!(m.length(1.2), "1.20 m");
        assert_eq!(m.dimension(-0.001), "0.00");
        let cm = Units::of(LengthUnit::Centimetres);
        assert_eq!(cm.length(1.067), "106.7 cm");
    }

    #[test]
    fn feet_and_inches_round_to_the_fraction() {
        let u = Units::imperial();
        let m = |ft: f64, inch: f64| ft * M_PER_FOOT + inch * M_PER_INCH;
        assert_eq!(u.dimension(m(3.0, 6.5)), "3'-6 1/2\"");
        assert_eq!(u.dimension(m(10.0, 0.0)), "10'-0\"");
        assert_eq!(u.dimension(m(0.0, 3.25)), "0'-3 1/4\"");
        // 11 15/16" rounds up to the next foot at 1/8"
        assert_eq!(u.dimension(m(0.0, 11.97)), "1'-0\"");
        assert_eq!(u.dimension(-m(2.0, 1.0)), "-2'-1\"");
        assert_eq!(u.dimension(1.0), "3'-3 3/8\"");
        let whole = Units { precision: 0, ..u };
        assert_eq!(whole.dimension(m(3.0, 6.5)), "3'-7\"");
        let inches = Units::of(LengthUnit::Inches);
        assert_eq!(inches.dimension(m(3.0, 6.5)), "42 1/2\"");
        assert_eq!(inches.dimension(m(0.0, 0.5)), "1/2\"");
        let feet = Units::of(LengthUnit::Feet);
        assert_eq!(feet.dimension(m(3.0, 6.0)), "3.50'");
        assert_eq!(u.elevation(m(9.0, 10.0)), "+9'-10\"");
        assert_eq!(u.elevation(0.0), "±0'-0\"");
        assert_eq!(Units::metric().elevation(-0.45), "-450");
    }

    #[test]
    fn typed_lengths_parse_in_any_unit() {
        let imp = Units::imperial();
        let met = Units::metric();
        let ft_in = |ft: f64, inch: f64| ft * M_PER_FOOT + inch * M_PER_INCH;
        for (u, text, want) in [
            (imp, "3'-6 1/2\"", ft_in(3.0, 6.5)),
            (imp, "3' 6\"", ft_in(3.0, 6.0)),
            (imp, "3'6", ft_in(3.0, 6.0)),
            (imp, "3'-6-1/2\"", ft_in(3.0, 6.5)),
            (imp, "3 6", ft_in(3.0, 6.0)),
            (imp, "3 6 1/2", ft_in(3.0, 6.5)),
            (imp, "3", ft_in(3.0, 0.0)),
            (imp, "3.5'", ft_in(3.5, 0.0)),
            (imp, "42\"", ft_in(0.0, 42.0)),
            (imp, "1/2\"", ft_in(0.0, 0.5)),
            (imp, "3 ft 6 in", ft_in(3.0, 6.0)),
            (imp, "-2'-1\"", -ft_in(2.0, 1.0)),
            (imp, "1067 mm", 1.067),
            (imp, "1.2 m", 1.2),
            (met, "1067", 1.067),
            (met, "1.2 m", 1.2),
            (met, "1,5m", 1.5),
            (met, "106.7cm", 1.067),
            (met, "3'-6\"", ft_in(3.0, 6.0)),
            (Units::of(LengthUnit::Metres), "2.4", 2.4),
            (Units::of(LengthUnit::Inches), "6 1/2", ft_in(0.0, 6.5)),
        ] {
            let got = u.parse(text);
            assert!(
                got.is_some_and(|g| close(g, want)),
                "{text:?} as {:?}: got {got:?}, want {want}",
                u.length
            );
        }
        for bad in ["", "abc", "3'-x\"", "1/0\"", "12 mm m"] {
            assert_eq!(imp.parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn what_is_written_reads_back() {
        for length in LengthUnit::ALL {
            let u = Units::of(length);
            for v in [0.0, 0.0125, 0.2, 1.0674, 3.0, 12.345, -2.5] {
                let text = u.length(v);
                let back = u.parse(&text).unwrap_or_else(|| panic!("{text:?}"));
                // read back to within the precision written
                assert!(
                    (back - v).abs() <= u.step() / 2.0 + 1e-9,
                    "{length:?}: {v} → {text:?} → {back}"
                );
                // and writing it again gives the same text
                assert_eq!(u.length(back), text);
            }
        }
    }

    #[test]
    fn areas_and_volumes_follow_the_system() {
        assert_eq!(Units::metric().area(12.5), "12.50 m²");
        assert_eq!(
            Units::imperial().area(M_PER_FOOT * M_PER_FOOT * 100.0),
            "100.00 ft²"
        );
        assert_eq!(
            Units::imperial().volume(M_PER_FOOT.powi(3) * 2.0),
            "2.00 ft³"
        );
    }

    #[test]
    fn units_without_fields_read_as_metric() {
        let u: Units = serde_json::from_str("{}").unwrap();
        assert_eq!(u.length, LengthUnit::Millimetres);
    }

    #[test]
    fn a_project_saved_before_units_opens_metric() {
        // the sample house has no units element, as files written before
        // units existed; nor do its plan views carry an underlay
        let old = crate::sample::sample_house().to_json();
        assert!(!old.contains("\"Units\"") && !old.contains("underlay"));
        let m = crate::Model::from_json(&old).unwrap();
        assert_eq!(m.units(), Units::metric());
        assert!(m.iter().all(|e| match &e.kind {
            crate::ElementKind::View(v) => v.underlay.is_none(),
            _ => true,
        }));
    }

    #[test]
    fn project_units_are_saved_and_undone() {
        let mut doc = crate::Document::from_model(crate::sample::sample_house());
        doc.commit(set_units(doc.model(), Units::imperial()))
            .unwrap();
        assert_eq!(doc.model().units(), Units::imperial());
        let back = crate::Model::from_json(&doc.model().to_json()).unwrap();
        assert_eq!(back.units(), Units::imperial());
        // changing them again replaces the one element
        doc.commit(set_units(doc.model(), Units::of(LengthUnit::Metres)))
            .unwrap();
        let count = |m: &crate::Model| {
            m.iter()
                .filter(|e| matches!(e.kind, crate::ElementKind::Units(_)))
                .count()
        };
        assert_eq!(count(doc.model()), 1);
        doc.undo();
        assert_eq!(doc.model().units(), Units::imperial());
        doc.undo();
        assert_eq!(doc.model().units(), Units::metric());
        assert_eq!(count(doc.model()), 0);
    }
}
