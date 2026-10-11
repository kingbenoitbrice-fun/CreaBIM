//! Reader for ISO 10303-21 (STEP physical file) exchange structures.
//!
//! The reader is schema-agnostic: it turns `#12=IFCWALL(...)` records into
//! [`Entity`] values and leaves their meaning to the importer. It works on
//! bytes in a single pass and never panics: a malformed record is skipped up
//! to its terminating `;` and reported in [`StepFile::errors`], so one bad
//! line does not lose the rest of a building.

use std::collections::HashMap;

/// One parameter value of an entity instance.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// `$`: no value.
    Null,
    /// `*`: value derived from other attributes.
    Derived,
    Int(i64),
    Real(f64),
    Str(String),
    /// `.ENUM.`, without the dots. Booleans are `.T.` / `.F.`.
    Enum(String),
    /// `"0A3F"`: a binary, kept as its hex digits.
    Binary(String),
    /// `#123`: a reference to another instance.
    Ref(u64),
    List(Vec<Value>),
    /// A typed value such as `IFCLENGTHMEASURE(2.)`. Several parameters are
    /// wrapped in a [`Value::List`].
    Typed(String, Box<Value>),
}

static NULL: Value = Value::Null;

impl Value {
    pub fn as_ref(&self) -> Option<u64> {
        match self {
            Value::Ref(r) => Some(*r),
            _ => None,
        }
    }

    /// A number, looking through typed wrappers like `IFCREAL(1.)`.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Real(v) => Some(*v),
            Value::Int(v) => Some(*v as f64),
            Value::Typed(_, v) => v.as_f64(),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            Value::Typed(_, v) => v.as_str(),
            _ => None,
        }
    }

    pub fn as_enum(&self) -> Option<&str> {
        match self {
            Value::Enum(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self.as_enum()? {
            "T" => Some(true),
            "F" => Some(false),
            _ => None,
        }
    }

    pub fn as_list(&self) -> &[Value] {
        match self {
            Value::List(v) => v,
            _ => &[],
        }
    }

    /// The references in a list, skipping anything else.
    pub fn refs(&self) -> impl Iterator<Item = u64> + '_ {
        self.as_list().iter().filter_map(Value::as_ref)
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null | Value::Derived)
    }
}

/// One entity instance: its upper-case class name and its parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub class: String,
    pub args: Vec<Value>,
}

impl Entity {
    /// Parameter `i`, or `$` if the record is too short.
    pub fn arg(&self, i: usize) -> &Value {
        self.args.get(i).unwrap_or(&NULL)
    }

    pub fn is(&self, class: &str) -> bool {
        self.class == class
    }
}

/// A parsed file: the header's schema identifiers and every data instance.
#[derive(Clone, Debug, Default)]
pub struct StepFile {
    pub schemas: Vec<String>,
    pub entities: HashMap<u64, Entity>,
    /// Problems found while reading; each names its line.
    pub errors: Vec<String>,
}

impl StepFile {
    pub fn get(&self, id: u64) -> Option<&Entity> {
        self.entities.get(&id)
    }

    /// The instance a value refers to, if it refers to one that exists.
    pub fn deref(&self, v: &Value) -> Option<&Entity> {
        self.get(v.as_ref()?)
    }

    /// Instance ids by class, sorted, for the importer's passes.
    pub fn by_class(&self) -> HashMap<&str, Vec<u64>> {
        let mut out: HashMap<&str, Vec<u64>> = HashMap::new();
        for (id, e) in &self.entities {
            out.entry(e.class.as_str()).or_default().push(*id);
        }
        for ids in out.values_mut() {
            ids.sort_unstable();
        }
        out
    }
}

/// Why a file could not be read at all.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StepError {
    #[error("not a STEP file: no DATA section found")]
    NoData,
}

/// Read a STEP physical file. Only a file with no DATA section is an error;
/// damaged records are skipped and listed in [`StepFile::errors`].
pub fn parse(text: &str) -> Result<StepFile, StepError> {
    let mut p = Parser {
        s: text.as_bytes(),
        i: 0,
    };
    let mut file = StepFile::default();
    let mut saw_data = false;
    loop {
        p.skip_ws();
        if p.eof() {
            break;
        }
        let start = p.i;
        let Some(word) = p.keyword() else {
            // stray characters between sections
            p.skip_record();
            file.errors
                .push(format!("line {}: unexpected text", p.line_of(start)));
            continue;
        };
        match word.as_str() {
            "ISO-10303-21" | "END-ISO-10303-21" | "ENDSEC" => {
                p.skip_ws();
                p.eat(b';');
            }
            "HEADER" => {
                p.skip_ws();
                p.eat(b';');
                p.header(&mut file);
            }
            "DATA" => {
                saw_data = true;
                p.skip_ws();
                if p.peek() == Some(b'(') {
                    // DATA('name', ('SCHEMA')) in multi-section files
                    let _ = p.value(0);
                    p.skip_ws();
                }
                p.eat(b';');
                p.data(&mut file);
            }
            _ => {
                p.skip_record();
            }
        }
    }
    if saw_data {
        Ok(file)
    } else {
        Err(StepError::NoData)
    }
}

/// Lists nested deeper than this are rejected, so hostile input cannot
/// overflow the stack.
const MAX_DEPTH: usize = 64;

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn eof(&self) -> bool {
        self.i >= self.s.len()
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn line_of(&self, pos: usize) -> usize {
        1 + self.s[..pos.min(self.s.len())]
            .iter()
            .filter(|&&c| c == b'\n')
            .count()
    }

    /// Skip whitespace and `/* comments */`.
    fn skip_ws(&mut self) {
        loop {
            while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
                self.i += 1;
            }
            if self.s.get(self.i..self.i + 2) == Some(b"/*") {
                match find(&self.s[self.i + 2..], b"*/") {
                    Some(end) => self.i += 2 + end + 2,
                    None => self.i = self.s.len(),
                }
            } else {
                return;
            }
        }
    }

    /// A keyword like `IFCWALL`, `END-ISO-10303-21` or `!USERDEFINED`.
    fn keyword(&mut self) -> Option<String> {
        let start = self.i;
        if self.peek() == Some(b'!') {
            self.i += 1;
        }
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            self.i += 1;
        }
        if self.i == start {
            return None;
        }
        Some(String::from_utf8_lossy(&self.s[start..self.i]).to_ascii_uppercase())
    }

    /// Skip to just past the next `;` that is not inside a string.
    fn skip_record(&mut self) {
        while let Some(c) = self.peek() {
            self.i += 1;
            match c {
                b';' => return,
                b'\'' => self.skip_string_body(),
                b'/' if self.peek() == Some(b'*') => {
                    self.i -= 1;
                    self.skip_ws();
                }
                _ => {}
            }
        }
    }

    fn skip_string_body(&mut self) {
        while let Some(c) = self.peek() {
            self.i += 1;
            if c == b'\'' {
                if self.peek() == Some(b'\'') {
                    self.i += 1;
                } else {
                    return;
                }
            }
        }
    }

    fn header(&mut self, file: &mut StepFile) {
        loop {
            self.skip_ws();
            if self.eof() {
                return;
            }
            let start = self.i;
            let Some(name) = self.keyword() else {
                self.skip_record();
                continue;
            };
            if name == "ENDSEC" {
                self.skip_ws();
                self.eat(b';');
                return;
            }
            self.skip_ws();
            match self.value(0) {
                Ok(Value::List(args)) => {
                    if name == "FILE_SCHEMA" {
                        let schemas = args.first().map(Value::as_list).unwrap_or(&[]);
                        file.schemas = schemas
                            .iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect();
                    }
                    self.skip_ws();
                    if !self.eat(b';') {
                        self.skip_record();
                    }
                }
                _ => {
                    file.errors.push(format!(
                        "line {}: malformed header record {name}",
                        self.line_of(start)
                    ));
                    self.i = start;
                    self.skip_record();
                }
            }
        }
    }

    fn data(&mut self, file: &mut StepFile) {
        loop {
            self.skip_ws();
            if self.eof() {
                return;
            }
            let start = self.i;
            if self.peek() != Some(b'#') {
                if let Some(word) = self.keyword() {
                    if word == "ENDSEC" {
                        self.skip_ws();
                        self.eat(b';');
                        return;
                    }
                }
                self.i = start;
                file.errors.push(format!(
                    "line {}: expected an instance",
                    self.line_of(start)
                ));
                self.skip_record();
                continue;
            }
            match self.instance() {
                Ok((id, entity)) => {
                    if file.entities.insert(id, entity).is_some() {
                        file.errors.push(format!(
                            "line {}: instance #{id} defined twice; the last one wins",
                            self.line_of(start)
                        ));
                    }
                }
                Err(msg) => {
                    file.errors
                        .push(format!("line {}: {msg}", self.line_of(start)));
                    self.i = start;
                    self.skip_record();
                }
            }
        }
    }

    /// `#12 = NAME(params);` or a complex `#12 = (A(...) B(...));`.
    fn instance(&mut self) -> Result<(u64, Entity), String> {
        self.eat(b'#');
        let id = self.unsigned().ok_or("bad instance id")?;
        self.skip_ws();
        if !self.eat(b'=') {
            return Err(format!("missing '=' after #{id}"));
        }
        self.skip_ws();
        let entity = if self.eat(b'(') {
            // complex instance: a list of partial records, kept as typed values
            let mut parts = Vec::new();
            loop {
                self.skip_ws();
                if self.eat(b')') {
                    break;
                }
                let name = self.keyword().ok_or("bad complex instance")?;
                self.skip_ws();
                let params = self.value(1)?;
                parts.push(Value::Typed(name, Box::new(params)));
            }
            Entity {
                class: String::new(),
                args: parts,
            }
        } else {
            let class = self.keyword().ok_or("missing entity name")?;
            self.skip_ws();
            match self.value(0)? {
                Value::List(args) => Entity { class, args },
                _ => return Err(format!("{class} has no parameter list")),
            }
        };
        self.skip_ws();
        if !self.eat(b';') {
            return Err(format!("#{id} is not terminated by ';'"));
        }
        Ok((id, entity))
    }

    fn unsigned(&mut self) -> Option<u64> {
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        std::str::from_utf8(&self.s[start..self.i])
            .ok()?
            .parse()
            .ok()
    }

    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err("lists nested too deeply".into());
        }
        self.skip_ws();
        let c = self.peek().ok_or("unexpected end of file")?;
        match c {
            b'$' => {
                self.i += 1;
                Ok(Value::Null)
            }
            b'*' => {
                self.i += 1;
                Ok(Value::Derived)
            }
            b'#' => {
                self.i += 1;
                self.unsigned()
                    .map(Value::Ref)
                    .ok_or_else(|| "bad reference".into())
            }
            b'\'' => {
                self.i += 1;
                self.string().map(Value::Str)
            }
            b'"' => {
                self.i += 1;
                let start = self.i;
                while self.peek().is_some_and(|c| c != b'"') {
                    self.i += 1;
                }
                let hex = String::from_utf8_lossy(&self.s[start..self.i]).into_owned();
                if !self.eat(b'"') {
                    return Err("unterminated binary".into());
                }
                Ok(Value::Binary(hex))
            }
            b'.' if self
                .s
                .get(self.i + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_') =>
            {
                self.i += 1;
                let start = self.i;
                while self
                    .peek()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
                {
                    self.i += 1;
                }
                let name = String::from_utf8_lossy(&self.s[start..self.i]).to_ascii_uppercase();
                if !self.eat(b'.') {
                    return Err("unterminated enumeration".into());
                }
                Ok(Value::Enum(name))
            }
            b'(' => {
                self.i += 1;
                let mut items = Vec::new();
                self.skip_ws();
                if self.eat(b')') {
                    return Ok(Value::List(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.skip_ws();
                    match self.peek() {
                        Some(b',') => self.i += 1,
                        Some(b')') => {
                            self.i += 1;
                            return Ok(Value::List(items));
                        }
                        _ => return Err("expected ',' or ')' in a list".into()),
                    }
                }
            }
            b'+' | b'-' | b'.' | b'0'..=b'9' => self.number(),
            c if c.is_ascii_alphabetic() || c == b'!' => {
                let name = self.keyword().ok_or("bad typed value")?;
                self.skip_ws();
                if self.peek() != Some(b'(') {
                    return Err(format!("{name} is not followed by '('"));
                }
                let inner = self.value(depth + 1)?;
                let inner = match inner {
                    Value::List(mut v) if v.len() == 1 => v.pop().unwrap_or(Value::Null),
                    other => other,
                };
                Ok(Value::Typed(name, Box::new(inner)))
            }
            _ => Err(format!("unexpected character '{}'", c as char)),
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.i;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.i += 1;
        }
        let mut real = false;
        while let Some(c) = self.peek() {
            match c {
                b'0'..=b'9' => {}
                b'.' => real = true,
                b'E' | b'e' => {
                    real = true;
                    if matches!(self.s.get(self.i + 1), Some(b'+' | b'-')) {
                        self.i += 1;
                    }
                }
                _ => break,
            }
            self.i += 1;
        }
        let text = std::str::from_utf8(&self.s[start..self.i]).map_err(|_| "bad number")?;
        let bad = || format!("bad number '{text}'");
        if real {
            // STEP allows "1." and "1.E-5"; Rust's parser wants a digit after the point
            let fixed;
            let t = if text.contains(".E") || text.contains(".e") || text.ends_with('.') {
                fixed = text.replacen(".E", ".0E", 1).replacen(".e", ".0e", 1);
                if fixed.ends_with('.') {
                    fixed.clone() + "0"
                } else {
                    fixed.clone()
                }
            } else {
                text.to_string()
            };
            t.parse::<f64>().map(Value::Real).map_err(|_| bad())
        } else {
            match text.parse::<i64>() {
                Ok(v) => Ok(Value::Int(v)),
                // integers too large for i64 still carry a usable magnitude
                Err(_) => text.parse::<f64>().map(Value::Real).map_err(|_| bad()),
            }
        }
    }

    /// The body of a string after its opening quote, with escapes decoded.
    fn string(&mut self) -> Result<String, String> {
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = self.peek().ok_or("unterminated string")?;
            self.i += 1;
            match c {
                b'\'' => {
                    if self.eat(b'\'') {
                        out.push(b'\'');
                    } else {
                        return Ok(String::from_utf8_lossy(&out).into_owned());
                    }
                }
                b'\\' => self.escape(&mut out),
                b'\r' | b'\n' => {} // line breaks inside strings are not content
                _ => out.push(c),
            }
        }
    }

    /// Decode one escape after its backslash: `\\`, `\X2\…\X0\` (UTF-16),
    /// `\X4\…\X0\` (UTF-32), `\X\hh` (ISO 8859-1), `\S\c` and `\P?\`.
    fn escape(&mut self, out: &mut Vec<u8>) {
        let rest = &self.s[self.i..];
        let push = |out: &mut Vec<u8>, ch: char| {
            let mut buf = [0u8; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        };
        if rest.first() == Some(&b'\\') {
            self.i += 1;
            out.push(b'\\');
        } else if rest.starts_with(b"X2\\") || rest.starts_with(b"X4\\") {
            let width = if rest[1] == b'2' { 4 } else { 8 };
            self.i += 3;
            let mut units: Vec<u32> = Vec::new();
            while let Some(hex) = self.s.get(self.i..self.i + width) {
                if hex.first() == Some(&b'\\') {
                    break;
                }
                match std::str::from_utf8(hex)
                    .ok()
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                {
                    Some(u) => units.push(u),
                    None => break,
                }
                self.i += width;
            }
            if self.s[self.i..].starts_with(b"\\X0\\") {
                self.i += 4;
            }
            if width == 4 {
                let wide: Vec<u16> = units.iter().map(|&u| u as u16).collect();
                for ch in char::decode_utf16(wide) {
                    push(out, ch.unwrap_or(char::REPLACEMENT_CHARACTER));
                }
            } else {
                for u in units {
                    push(
                        out,
                        char::from_u32(u).unwrap_or(char::REPLACEMENT_CHARACTER),
                    );
                }
            }
        } else if rest.starts_with(b"X\\") && rest.len() >= 4 {
            let hex = std::str::from_utf8(&rest[2..4]).ok();
            match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                Some(b) => {
                    self.i += 4;
                    push(out, b as char);
                }
                None => out.push(b'\\'),
            }
        } else if rest.starts_with(b"S\\") && rest.len() >= 3 {
            self.i += 3;
            push(out, (rest[2] as u32 + 128).try_into().unwrap_or('?'));
        } else if rest.len() >= 3 && rest[0] == b'P' && rest[2] == b'\\' {
            self.i += 3; // code page switch; \S\ text is read as Latin-1 anyway
        } else if rest.starts_with(b"N\\") {
            self.i += 2;
        } else {
            out.push(b'\\');
        }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(src: &str) -> Entity {
        let text = format!("ISO-10303-21;HEADER;ENDSEC;DATA;{src}ENDSEC;END-ISO-10303-21;");
        let f = parse(&text).unwrap();
        assert!(f.errors.is_empty(), "{:?}", f.errors);
        f.entities.into_values().next().unwrap()
    }

    #[test]
    fn reads_header_and_instances() {
        let text = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('ViewDefinition [X]'),'2;1');\n\
            FILE_NAME('a.ifc','2020-01-01T00:00:00',(''),(''),'x','y','');\n\
            FILE_SCHEMA(('IFC4'));\nENDSEC;\nDATA;\n#1=IFCCARTESIANPOINT((0.,1.5,-2.E-3));\n\
            #2 = IFCWALL('abc',$,'W;1',*,$,#1,$,$,.STANDARD.);\nENDSEC;\nEND-ISO-10303-21;\n";
        let f = parse(text).unwrap();
        assert_eq!(f.schemas, vec!["IFC4"]);
        assert_eq!(f.entities.len(), 2);
        let p = f.get(1).unwrap();
        assert_eq!(p.class, "IFCCARTESIANPOINT");
        let coords: Vec<f64> = p
            .arg(0)
            .as_list()
            .iter()
            .filter_map(Value::as_f64)
            .collect();
        assert_eq!(coords, vec![0.0, 1.5, -0.002]);
        let w = f.get(2).unwrap();
        assert_eq!(w.arg(2).as_str(), Some("W;1"));
        assert_eq!(w.arg(3), &Value::Derived);
        assert_eq!(w.arg(5).as_ref(), Some(1));
        assert_eq!(w.arg(8).as_enum(), Some("STANDARD"));
        assert_eq!(w.arg(42), &Value::Null);
    }

    #[test]
    fn typed_values_nested_lists_and_numbers() {
        let e = one("#5=IFCPROPERTYSINGLEVALUE('H',$,IFCLENGTHMEASURE(2.),$);");
        assert_eq!(
            e.arg(2),
            &Value::Typed("IFCLENGTHMEASURE".into(), Box::new(Value::Real(2.0)))
        );
        assert_eq!(e.arg(2).as_f64(), Some(2.0));
        let e = one("#6=IFCINDEXEDPOLYCURVE(#1,(IFCLINEINDEX((1,2)),IFCARCINDEX((2,3,4))),.F.);");
        let segs = e.arg(1).as_list();
        assert_eq!(segs.len(), 2);
        match &segs[1] {
            Value::Typed(name, inner) => {
                assert_eq!(name, "IFCARCINDEX");
                assert_eq!(inner.as_list().len(), 3);
            }
            v => panic!("{v:?}"),
        }
        let e = one("#7=X(1,-2,+3.5,1.E5,1.5e-3,0.,\"0FF\",.T.,());");
        assert_eq!(e.arg(0), &Value::Int(1));
        assert_eq!(e.arg(1), &Value::Int(-2));
        assert_eq!(e.arg(2), &Value::Real(3.5));
        assert_eq!(e.arg(3), &Value::Real(1e5));
        assert_eq!(e.arg(4), &Value::Real(1.5e-3));
        assert_eq!(e.arg(5), &Value::Real(0.0));
        assert_eq!(e.arg(6), &Value::Binary("0FF".into()));
        assert_eq!(e.arg(7).as_bool(), Some(true));
        assert_eq!(e.arg(8), &Value::List(vec![]));
    }

    #[test]
    fn strings_decode_escapes() {
        let e = one(r"#1=X('it''s','Caf\X2\00E9\X0\','\X2\D83DDE00\X0\','a\\b','\X\E9t\S\i');");
        assert_eq!(e.arg(0).as_str(), Some("it's"));
        assert_eq!(e.arg(1).as_str(), Some("Café"));
        assert_eq!(e.arg(2).as_str(), Some("😀"));
        assert_eq!(e.arg(3).as_str(), Some("a\\b"));
        assert_eq!(e.arg(4).as_str(), Some("été"));
        // round trip through our own writer
        let s = crate::step::string("Ünïcödé 'q' \\");
        let e = one(&format!("#1=X({s});"));
        assert_eq!(e.arg(0).as_str(), Some("Ünïcödé 'q' \\"));
    }

    #[test]
    fn comments_and_multiline_records() {
        let e = one("/* a; comment */ #9 =\n IFCDIRECTION( /* inline */\n (0.,\n 0., 1.)\n) ;");
        assert_eq!(e.class, "IFCDIRECTION");
        assert_eq!(e.arg(0).as_list().len(), 3);
    }

    #[test]
    fn complex_instances_are_kept() {
        let e = one("#3=(IFCA(1)IFCB('x'));");
        assert_eq!(e.class, "");
        assert_eq!(e.args.len(), 2);
    }

    #[test]
    fn malformed_records_are_skipped_not_fatal() {
        let text = "ISO-10303-21;HEADER;ENDSEC;DATA;\n#1=IFCA(1,2;\n#2=IFCB((1,2),'ok;');\n\
            #x=IFCC();\n#3=IFCD(#);\n#4=IFCE('unterminated\n";
        let f = parse(text).unwrap();
        assert!(f.get(2).is_some());
        assert_eq!(f.get(2).unwrap().arg(1).as_str(), Some("ok;"));
        assert!(!f.errors.is_empty());
        assert!(parse("hello").is_err());
        // no panics on arbitrary prefixes of a valid file
        let good = "ISO-10303-21;HEADER;FILE_SCHEMA(('IFC4'));ENDSEC;DATA;#1=IFCX('a\\X2\\00E9\\X0\\',(1.,2.),.T.,IFCREAL(1.E-3));ENDSEC;";
        for n in 0..good.len() {
            if good.is_char_boundary(n) {
                let _ = parse(&good[..n]);
            }
        }
    }

    #[test]
    fn deep_nesting_is_rejected() {
        let deep = format!(
            "DATA;#1=X({}1{});ENDSEC;",
            "(".repeat(10_000),
            ")".repeat(10_000)
        );
        let f = parse(&deep).unwrap();
        assert!(f.get(1).is_none());
        assert_eq!(f.errors.len(), 1);
    }
}
