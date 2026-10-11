//! The SDNA catalogue stored in the `DNA1` block: every struct the file
//! uses, with its fields' types, names and array sizes. Field offsets are
//! derived from it, so nothing about Blender's struct layout is hard-coded.

use std::collections::HashMap;

use crate::BlendError;

#[derive(Debug, Clone)]
pub(crate) struct Field {
    /// Bare name: `*next` and `mat[4][4]` become `next` and `mat`.
    pub name: String,
    pub type_idx: usize,
    pub is_ptr: bool,
    pub offset: usize,
    pub size: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct StructDef {
    pub type_idx: usize,
    pub size: usize,
    pub fields: Vec<Field>,
    by_name: HashMap<String, usize>,
}

impl StructDef {
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.by_name.get(name).map(|&i| &self.fields[i])
    }
}

#[derive(Debug, Default)]
pub(crate) struct Sdna {
    pub types: Vec<String>,
    pub type_len: Vec<usize>,
    pub structs: Vec<StructDef>,
    /// Type index -> struct index, for types that are structs.
    struct_of_type: HashMap<usize, usize>,
    struct_by_name: HashMap<String, usize>,
}

impl Sdna {
    pub fn parse(data: &[u8], ptr_size: usize, little_endian: bool) -> Result<Self, BlendError> {
        let mut r = Reader {
            data,
            pos: 0,
            little_endian,
        };
        r.tag(b"SDNA")?;
        r.tag(b"NAME")?;
        let n = r.count()?;
        let names = (0..n).map(|_| r.cstr()).collect::<Result<Vec<_>, _>>()?;
        r.align();
        r.tag(b"TYPE")?;
        let n = r.count()?;
        let types = (0..n).map(|_| r.cstr()).collect::<Result<Vec<_>, _>>()?;
        r.align();
        r.tag(b"TLEN")?;
        let type_len = (0..types.len())
            .map(|_| r.u16().map(usize::from))
            .collect::<Result<Vec<_>, _>>()?;
        r.align();
        r.tag(b"STRC")?;
        let n = r.count()?;
        let mut structs = Vec::with_capacity(n);
        for _ in 0..n {
            let type_idx = r.u16()? as usize;
            let nfields = r.u16()? as usize;
            let mut fields = Vec::with_capacity(nfields);
            let mut offset = 0;
            for _ in 0..nfields {
                let ftype = r.u16()? as usize;
                let fname = r.u16()? as usize;
                let raw = names.get(fname).ok_or(bad("field name index"))?;
                let elem = *type_len.get(ftype).ok_or(bad("field type index"))?;
                let (name, is_ptr, count) = parse_name(raw);
                let size = if is_ptr { ptr_size } else { elem } * count;
                fields.push(Field {
                    name,
                    type_idx: ftype,
                    is_ptr,
                    offset,
                    size,
                });
                offset += size;
            }
            let size = *type_len.get(type_idx).ok_or(bad("struct type index"))?;
            let mut by_name = HashMap::new();
            for (i, f) in fields.iter().enumerate() {
                by_name.entry(f.name.clone()).or_insert(i);
            }
            structs.push(StructDef {
                type_idx,
                size,
                fields,
                by_name,
            });
        }
        let mut sdna = Sdna {
            types,
            type_len,
            structs,
            ..Default::default()
        };
        for (i, s) in sdna.structs.iter().enumerate() {
            sdna.struct_of_type.insert(s.type_idx, i);
            sdna.struct_by_name
                .insert(sdna.types[s.type_idx].clone(), i);
        }
        Ok(sdna)
    }

    pub fn struct_by_name(&self, name: &str) -> Option<usize> {
        self.struct_by_name.get(name).copied()
    }

    pub fn struct_of_type(&self, type_idx: usize) -> Option<usize> {
        self.struct_of_type.get(&type_idx).copied()
    }

    pub fn type_name(&self, type_idx: usize) -> &str {
        self.types.get(type_idx).map_or("", String::as_str)
    }
}

/// Splits an SDNA field name such as `*next`, `(*func)()` or `co[3]` into
/// its bare name, pointer-ness and element count.
fn parse_name(raw: &str) -> (String, bool, usize) {
    let is_ptr = raw.starts_with('*') || raw.starts_with("(*");
    let bare: String = raw
        .split('[')
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| !matches!(c, '*' | '(' | ')'))
        .collect();
    let mut count = 1usize;
    // Function pointers carry `()` but no array dimensions.
    if !raw.starts_with("(*") {
        for dim in raw.split('[').skip(1) {
            let n = dim.trim_end_matches(']').parse::<usize>().unwrap_or(1);
            count = count.saturating_mul(n);
        }
    }
    (bare, is_ptr, count)
}

fn bad(what: &str) -> BlendError {
    BlendError::Malformed(format!("SDNA: bad {what}"))
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    little_endian: bool,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], BlendError> {
        let s = self
            .data
            .get(self.pos..self.pos + n)
            .ok_or(BlendError::Truncated)?;
        self.pos += n;
        Ok(s)
    }

    fn tag(&mut self, tag: &[u8; 4]) -> Result<(), BlendError> {
        if self.take(4)? == tag {
            Ok(())
        } else {
            Err(BlendError::Malformed(format!(
                "SDNA: expected {}",
                String::from_utf8_lossy(tag)
            )))
        }
    }

    fn count(&mut self) -> Result<usize, BlendError> {
        let b = self.take(4)?;
        let b = [b[0], b[1], b[2], b[3]];
        let n = if self.little_endian {
            i32::from_le_bytes(b)
        } else {
            i32::from_be_bytes(b)
        };
        // Each entry takes at least one byte, which bounds the allocation.
        usize::try_from(n)
            .ok()
            .filter(|&n| n <= self.data.len())
            .ok_or(bad("count"))
    }

    fn u16(&mut self) -> Result<u16, BlendError> {
        let b = self.take(2)?;
        Ok(if self.little_endian {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        })
    }

    fn cstr(&mut self) -> Result<String, BlendError> {
        let rest = self.data.get(self.pos..).ok_or(BlendError::Truncated)?;
        let end = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or(BlendError::Truncated)?;
        let s = String::from_utf8_lossy(&rest[..end]).into_owned();
        self.pos += end + 1;
        Ok(s)
    }

    fn align(&mut self) {
        self.pos = (self.pos + 3) & !3;
    }
}

#[cfg(test)]
mod tests {
    use super::parse_name;

    #[test]
    fn field_names() {
        assert_eq!(parse_name("*next"), ("next".into(), true, 1));
        assert_eq!(parse_name("mat[4][4]"), ("mat".into(), false, 16));
        assert_eq!(parse_name("(*func)()"), ("func".into(), true, 1));
        assert_eq!(parse_name("**mat"), ("mat".into(), true, 1));
        assert_eq!(parse_name("*mtex[18]"), ("mtex".into(), true, 18));
    }
}
