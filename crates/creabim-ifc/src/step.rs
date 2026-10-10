//! Minimal writer for ISO 10303-21 (STEP physical file) data sections.

use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ref(pub usize);

impl std::fmt::Display for Ref {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

#[derive(Default)]
pub struct StepWriter {
    entities: Vec<String>,
}

impl StepWriter {
    /// Add an entity, e.g. `add("IFCCARTESIANPOINT", "((0.,0.,0.))")`.
    pub fn add(&mut self, name: &str, args: impl AsRef<str>) -> Ref {
        self.entities.push(format!("{}({})", name, args.as_ref()));
        Ref(self.entities.len())
    }

    pub fn finish(self, header: &str) -> String {
        let mut s = String::new();
        s.push_str("ISO-10303-21;\nHEADER;\n");
        s.push_str(header);
        s.push_str("ENDSEC;\nDATA;\n");
        for (i, e) in self.entities.iter().enumerate() {
            let _ = writeln!(s, "#{}={};", i + 1, e);
        }
        s.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
        s
    }
}

/// A STEP string literal, with quotes and backslashes escaped and non-ASCII
/// text encoded as `\X2\` UTF-16 hex.
pub fn string(s: &str) -> String {
    let mut out = String::from("'");
    let mut wide: Vec<u16> = Vec::new();
    let flush = |out: &mut String, wide: &mut Vec<u16>| {
        if !wide.is_empty() {
            out.push_str("\\X2\\");
            for u in wide.drain(..) {
                let _ = write!(out, "{u:04X}");
            }
            out.push_str("\\X0\\");
        }
    };
    for c in s.chars() {
        if c.is_ascii() && !c.is_ascii_control() {
            flush(&mut out, &mut wide);
            match c {
                '\'' => out.push_str("''"),
                '\\' => out.push_str("\\\\"),
                _ => out.push(c),
            }
        } else if !c.is_ascii() {
            let mut buf = [0u16; 2];
            wide.extend_from_slice(c.encode_utf16(&mut buf));
        }
    }
    flush(&mut out, &mut wide);
    out.push('\'');
    out
}

/// A STEP real: always has a decimal point, exponent written as `E`.
pub fn real(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return "0.".into();
    }
    let s = format!("{v:.9}");
    s.trim_end_matches('0').to_string()
}

pub fn point3(x: f64, y: f64, z: f64) -> String {
    format!("({},{},{})", real(x), real(y), real(z))
}

pub fn point2(x: f64, y: f64) -> String {
    format!("({},{})", real(x), real(y))
}

pub fn list(items: impl IntoIterator<Item = impl std::fmt::Display>) -> String {
    let v: Vec<String> = items.into_iter().map(|i| i.to_string()).collect();
    format!("({})", v.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reals_always_have_a_point() {
        assert_eq!(real(0.0), "0.");
        assert_eq!(real(3.0), "3.");
        assert_eq!(real(-0.25), "-0.25");
        assert_eq!(real(1e-7), "0.0000001");
    }

    #[test]
    fn strings_are_escaped() {
        assert_eq!(string("it's"), "'it''s'");
        assert_eq!(string("a\\b"), "'a\\\\b'");
        assert_eq!(string("Café"), "'Caf\\X2\\00E9\\X0\\'");
    }
}
