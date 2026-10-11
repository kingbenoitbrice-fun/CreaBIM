//! Typed, bounds-checked access to struct instances by field name.

use crate::container::{BlendFile, Block};
use crate::sdna::{Field, StructDef};

/// One struct instance inside a block, read through its SDNA definition.
/// Every accessor returns `None` when the field is missing in this file's
/// SDNA or the bytes are out of range, which keeps version differences
/// and malformed files on the same, non-panicking path.
#[derive(Clone, Copy)]
pub(crate) struct View<'f, 'a> {
    pub file: &'f BlendFile<'a>,
    pub def: &'f StructDef,
    pub bytes: &'a [u8],
    /// Owning ID block, for resolving the pointers this struct holds.
    pub scope: usize,
}

impl<'f, 'a> View<'f, 'a> {
    pub fn type_name(&self) -> &'f str {
        self.file.sdna.type_name(self.def.type_idx)
    }

    pub fn has(&self, name: &str) -> bool {
        self.def.field(name).is_some()
    }

    fn field(&self, name: &str) -> Option<(&'f Field, &'a [u8])> {
        let f = self.def.field(name)?;
        let b = self.bytes.get(f.offset..f.offset + f.size)?;
        Some((f, b))
    }

    /// First element of a numeric field, as an integer.
    pub fn int(&self, name: &str) -> Option<i64> {
        let (f, b) = self.field(name)?;
        if f.is_ptr {
            return None;
        }
        let ty = self.file.sdna.type_name(f.type_idx);
        let n = scalar(self.file, ty, self.file.sdna.type_len[f.type_idx], b)?;
        Some(match n {
            Num::Int(i) => i,
            Num::Float(x) => x as i64,
        })
    }

    /// All elements of a numeric field, as floats (`co[3]`, `mat[4][4]`).
    pub fn floats(&self, name: &str) -> Option<Vec<f64>> {
        let (f, b) = self.field(name)?;
        if f.is_ptr {
            return None;
        }
        let ty = self.file.sdna.type_name(f.type_idx);
        let elem = self.file.sdna.type_len[f.type_idx];
        if elem == 0 {
            return None;
        }
        b.chunks_exact(elem)
            .map(|c| {
                scalar(self.file, ty, elem, c).map(|n| match n {
                    Num::Int(i) => i as f64,
                    Num::Float(x) => x,
                })
            })
            .collect()
    }

    pub fn float(&self, name: &str) -> Option<f64> {
        self.floats(name)?.first().copied()
    }

    /// A pointer field's saved address; `None` for null.
    pub fn ptr(&self, name: &str) -> Option<u64> {
        let (f, b) = self.field(name)?;
        if !f.is_ptr {
            return None;
        }
        let p = read_ptr(self.file, b)?;
        (p != 0).then_some(p)
    }

    /// A `char name[N]` field up to its first NUL.
    pub fn string(&self, name: &str) -> Option<String> {
        let (f, b) = self.field(name)?;
        if f.is_ptr {
            return None;
        }
        Some(c_string(b))
    }

    /// An embedded struct field such as `Object.id` or `Scene.unit`.
    pub fn sub(&self, name: &str) -> Option<View<'f, 'a>> {
        let (f, b) = self.field(name)?;
        if f.is_ptr {
            return None;
        }
        let def = &self.file.sdna.structs[self.file.sdna.struct_of_type(f.type_idx)?];
        Some(View {
            file: self.file,
            def,
            bytes: b.get(..def.size)?,
            scope: self.scope,
        })
    }

    /// The struct a pointer field points to, typed by the target block's
    /// SDNA index (or `fallback` when the block is untyped).
    pub fn deref(&self, name: &str, fallback: &str) -> Option<View<'f, 'a>> {
        self.view_at(self.ptr(name)?, fallback)
    }
}

enum Num {
    Int(i64),
    Float(f64),
}

fn scalar(file: &BlendFile, ty: &str, len: usize, b: &[u8]) -> Option<Num> {
    if len > 8 {
        return None;
    }
    let le = file.little_endian;
    let get = |n: usize| -> Option<[u8; 8]> {
        let s = b.get(..n)?;
        let mut out = [0u8; 8];
        if le {
            out[..n].copy_from_slice(s);
        } else {
            for (i, &x) in s.iter().rev().enumerate() {
                out[i] = x;
            }
        }
        Some(out)
    };
    let raw = get(len)?;
    Some(match (ty, len) {
        ("float", 4) => Num::Float(f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as f64),
        ("double", 8) => Num::Float(f64::from_le_bytes(raw)),
        ("char" | "int8_t", 1) => Num::Int(raw[0] as i8 as i64),
        (_, 1) => Num::Int(raw[0] as i64),
        ("short" | "int16_t", 2) => Num::Int(i16::from_le_bytes([raw[0], raw[1]]) as i64),
        (_, 2) => Num::Int(u16::from_le_bytes([raw[0], raw[1]]) as i64),
        ("int" | "int32_t" | "long", 4) => {
            Num::Int(i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as i64)
        }
        (_, 4) => Num::Int(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as i64),
        (_, 8) => Num::Int(i64::from_le_bytes(raw)),
        _ => return None,
    })
}

pub(crate) fn read_ptr(file: &BlendFile, b: &[u8]) -> Option<u64> {
    let b = b.get(..file.ptr_size)?;
    let mut out = [0u8; 8];
    if file.little_endian {
        out[..b.len()].copy_from_slice(b);
    } else {
        for (i, &x) in b.iter().rev().enumerate() {
            out[i] = x;
        }
    }
    Some(u64::from_le_bytes(out))
}

pub(crate) fn c_string(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

impl<'a> BlendFile<'a> {
    /// The SDNA struct a block holds; `None` for raw data, which Blender
    /// tags with struct index 0.
    pub fn block_struct(&self, block: &Block) -> Option<&StructDef> {
        if block.sdna == 0 {
            return None;
        }
        let def = self.sdna.structs.get(block.sdna)?;
        (def.size > 0 && block.len >= def.size).then_some(def)
    }

    /// All struct instances stored in a block (`nr` elements).
    pub fn block_views<'f>(&'f self, block: &Block) -> Vec<View<'f, 'a>> {
        let Some(def) = self.block_struct(block) else {
            return Vec::new();
        };
        self.data(block)
            .chunks_exact(def.size)
            .take(block.count)
            .map(|bytes| View {
                file: self,
                def,
                bytes,
                scope: block.scope,
            })
            .collect()
    }
}

/// Pointer following. Pointers are resolved in the scope (owning ID) of
/// the struct they were read from.
impl<'f, 'a> View<'f, 'a> {
    /// The struct at a saved address. The block's own SDNA index wins;
    /// `fallback` names the type for untyped (raw) blocks.
    pub fn view_at(&self, addr: u64, fallback: &str) -> Option<View<'f, 'a>> {
        let file = self.file;
        let (block, offset) = file.resolve(addr, self.scope)?;
        let def = file.block_struct(block).or_else(|| {
            file.sdna
                .struct_by_name(fallback)
                .map(|i| &file.sdna.structs[i])
        })?;
        let bytes = file.data(block).get(offset..offset + def.size)?;
        Some(View {
            file,
            def,
            bytes,
            scope: block.scope,
        })
    }

    /// The bytes a pointer refers to, to the end of its block.
    pub fn bytes_at(&self, addr: u64) -> Option<&'a [u8]> {
        let (block, offset) = self.file.resolve(addr, self.scope)?;
        self.file.data(block).get(offset..)
    }

    /// An array of `n` structs of type `ty` at `addr` (`Mesh.mvert` etc.).
    pub fn array_at(&self, addr: u64, ty: &str, n: usize) -> Option<Vec<View<'f, 'a>>> {
        let file = self.file;
        let def = &file.sdna.structs[file.sdna.struct_by_name(ty)?];
        let (block, offset) = file.resolve(addr, self.scope)?;
        let bytes = file.data(block).get(offset..)?;
        if def.size == 0 || bytes.len() / def.size < n {
            return None;
        }
        Some(
            bytes
                .chunks_exact(def.size)
                .take(n)
                .map(|bytes| View {
                    file,
                    def,
                    bytes,
                    scope: block.scope,
                })
                .collect(),
        )
    }

    /// `n` pointers stored at `addr` (e.g. `Object.mat`).
    pub fn ptr_array(&self, addr: u64, n: usize) -> Vec<u64> {
        let Some(bytes) = self.bytes_at(addr) else {
            return Vec::new();
        };
        bytes
            .chunks_exact(self.file.ptr_size)
            .take(n)
            .map(|c| read_ptr(self.file, c).unwrap_or(0))
            .collect()
    }

    pub fn f32s_at(&self, addr: u64, n: usize) -> Option<Vec<f32>> {
        Some(
            self.u32s_at(addr, n)?
                .into_iter()
                .map(f32::from_bits)
                .collect(),
        )
    }

    pub fn i32s_at(&self, addr: u64, n: usize) -> Option<Vec<i32>> {
        Some(
            self.u32s_at(addr, n)?
                .into_iter()
                .map(|v| v as i32)
                .collect(),
        )
    }

    fn u32s_at(&self, addr: u64, n: usize) -> Option<Vec<u32>> {
        let b = self.bytes_at(addr)?.get(..n.checked_mul(4)?)?;
        let le = self.file.little_endian;
        Some(
            b.chunks_exact(4)
                .map(|c| {
                    let a = [c[0], c[1], c[2], c[3]];
                    if le {
                        u32::from_le_bytes(a)
                    } else {
                        u32::from_be_bytes(a)
                    }
                })
                .collect(),
        )
    }
}
