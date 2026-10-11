//! The `.blend` container: optional compression, file header and the
//! list of BHead blocks.

use std::collections::HashMap;
use std::io::Read;

use crate::sdna::Sdna;
use crate::BlendError;

/// Upper bound for decompressed data, so a hostile file cannot make us
/// allocate without limit. Real scenes are far below this.
const MAX_DECOMPRESSED: u64 = 4 << 30;

/// Returns the raw `.blend` bytes, decompressing gzip (Blender < 3.0) or
/// zstd (Blender 3.0+) when the magic says so.
pub(crate) fn decompress(bytes: &[u8]) -> Result<Vec<u8>, BlendError> {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::MultiGzDecoder::new(bytes)
            .take(MAX_DECOMPRESSED)
            .read_to_end(&mut out)
            .map_err(|e| BlendError::Decompress(e.to_string()))?;
        Ok(out)
    } else if bytes.starts_with(&ZSTD_MAGIC) {
        unzstd(bytes)
    } else {
        Ok(bytes.to_vec())
    }
}

const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

/// Blender writes zstd as many independent frames followed by a skippable
/// seek-table frame, so frames are decoded one after another.
fn unzstd(mut input: &[u8]) -> Result<Vec<u8>, BlendError> {
    let mut out = Vec::new();
    while !input.is_empty() {
        let magic = input
            .get(..4)
            .map(|m| u32::from_le_bytes([m[0], m[1], m[2], m[3]]))
            .ok_or(BlendError::Truncated)?;
        if magic & 0xFFFF_FFF0 == 0x184D_2A50 {
            let len = input
                .get(4..8)
                .map(|m| u32::from_le_bytes([m[0], m[1], m[2], m[3]]) as usize)
                .ok_or(BlendError::Truncated)?;
            input = input.get(8 + len..).ok_or(BlendError::Truncated)?;
            continue;
        }
        let mut decoder = ruzstd::decoding::StreamingDecoder::new(&mut input)
            .map_err(|e| BlendError::Decompress(e.to_string()))?;
        let room = MAX_DECOMPRESSED.saturating_sub(out.len() as u64);
        (&mut decoder)
            .take(room)
            .read_to_end(&mut out)
            .map_err(|e| BlendError::Decompress(e.to_string()))?;
    }
    Ok(out)
}

/// One BHead block: a 4-byte code, the SDNA struct index of its contents,
/// the memory address it had when saved, and its payload.
#[derive(Debug, Clone)]
pub(crate) struct Block {
    pub code: [u8; 4],
    pub sdna: usize,
    pub old: u64,
    pub count: usize,
    pub start: usize,
    pub len: usize,
    /// Index of the ID block this data block belongs to (the last ID block
    /// before it), or `NO_SCOPE`.
    pub scope: usize,
}

/// Scope of blocks that precede every ID block (and of ID blocks' lookups
/// from file-level structs such as `GLOB`).
pub(crate) const NO_SCOPE: usize = usize::MAX;

/// ID blocks have a two-letter code padded with NULs (`OB\0\0`).
fn is_id_code(code: &[u8; 4]) -> bool {
    code[2] == 0 && code[3] == 0 && code[0] != 0
}

/// A parsed `.blend` file: header facts, blocks and the SDNA catalogue.
pub(crate) struct BlendFile<'a> {
    pub buf: &'a [u8],
    pub ptr_size: usize,
    pub little_endian: bool,
    /// Blender version as `major * 100 + minor`, e.g. 402.
    pub version: u32,
    pub blocks: Vec<Block>,
    pub sdna: Sdna,
    /// ID blocks by address. Their addresses are unique in the file.
    ids: HashMap<u64, usize>,
    /// Data blocks by (owning ID, address). Blender 5 writes addresses that
    /// are only unique within one ID's data, like its own reader expects.
    local: HashMap<(usize, u64), usize>,
    /// Every block by address (first wins), for data shared across IDs.
    global: HashMap<u64, usize>,
    /// Block indices sorted by (scope, address), for pointers into a block.
    sorted: Vec<usize>,
}

impl<'a> BlendFile<'a> {
    pub fn parse(buf: &'a [u8]) -> Result<Self, BlendError> {
        if buf.len() < 12 || &buf[..7] != b"BLENDER" {
            return Err(BlendError::NotBlend);
        }
        // Classic header: `BLENDER-v402` (pointer char, endian char, 3
        // digits). Blender 5.x: `BLENDER17-01v0500` (header size, pointer
        // char, file format version, endian char, 4 digits) with 64-bit
        // "large" BHeads.
        let (header_len, ptr_char, endian_char, version, large) = if buf[7].is_ascii_digit() {
            let header_len = digits(buf.get(7..9).ok_or(BlendError::NotBlend)?)? as usize;
            let head = buf.get(..header_len).ok_or(BlendError::NotBlend)?;
            if header_len < 17 {
                return Err(BlendError::NotBlend);
            }
            let format = digits(&head[10..12])?;
            if format != 1 {
                return Err(BlendError::Unsupported(format!(
                    "file format version {format}"
                )));
            }
            (header_len, head[9], head[12], digits(&head[13..17])?, true)
        } else {
            (12, buf[7], buf[8], digits(&buf[9..12])?, false)
        };
        let ptr_size = match ptr_char {
            b'_' => 4,
            b'-' => 8,
            _ => return Err(BlendError::NotBlend),
        };
        let little_endian = match endian_char {
            b'v' => true,
            b'V' => false,
            _ => return Err(BlendError::NotBlend),
        };

        let mut file = BlendFile {
            buf,
            ptr_size,
            little_endian,
            version,
            blocks: Vec::new(),
            sdna: Sdna::default(),
            ids: HashMap::new(),
            local: HashMap::new(),
            global: HashMap::new(),
            sorted: Vec::new(),
        };
        let mut pos = header_len;
        let mut dna = None;
        let mut scope = NO_SCOPE;
        loop {
            let (mut block, next) = file.read_bhead(pos, large)?;
            if is_id_code(&block.code) {
                scope = file.blocks.len();
            }
            block.scope = scope;
            if &block.code == b"ENDB" {
                break;
            }
            if &block.code == b"DNA1" {
                dna = Some(block.start..block.start + block.len);
            }
            file.blocks.push(block);
            pos = next;
        }
        let dna = dna.ok_or(BlendError::Malformed("no DNA1 block".into()))?;
        file.sdna = Sdna::parse(&buf[dna], ptr_size, little_endian)?;

        for (i, b) in file.blocks.iter().enumerate() {
            if b.old == 0 {
                continue;
            }
            if is_id_code(&b.code) {
                file.ids.entry(b.old).or_insert(i);
            } else {
                file.local.entry((b.scope, b.old)).or_insert(i);
            }
            file.global.entry(b.old).or_insert(i);
        }
        file.sorted = (0..file.blocks.len())
            .filter(|&i| file.blocks[i].old != 0)
            .collect();
        file.sorted
            .sort_by_key(|&i| (file.blocks[i].scope, file.blocks[i].old));
        Ok(file)
    }

    /// Reads one BHead at `pos`, returning the block and the next offset.
    fn read_bhead(&self, pos: usize, large: bool) -> Result<(Block, usize), BlendError> {
        let code_bytes = self.buf.get(pos..pos + 4).ok_or(BlendError::Truncated)?;
        let code = [code_bytes[0], code_bytes[1], code_bytes[2], code_bytes[3]];
        let (sdna, old, len, count, head) = if large {
            let sdna = self.i32_at(pos + 4)? as i64;
            let old = self.u64_at(pos + 8)?;
            let len = self.u64_at(pos + 16)? as i64;
            let count = self.u64_at(pos + 24)? as i64;
            (sdna, old, len, count, 32)
        } else {
            let len = self.i32_at(pos + 4)? as i64;
            let (old, p) = match self.ptr_size {
                4 => (self.u32_at(pos + 8)? as u64, pos + 12),
                _ => (self.u64_at(pos + 8)?, pos + 16),
            };
            let sdna = self.i32_at(p)? as i64;
            let count = self.i32_at(p + 4)? as i64;
            (sdna, old, len, count, p + 8 - pos)
        };
        if &code == b"ENDB" {
            let block = Block {
                code,
                sdna: 0,
                old: 0,
                count: 0,
                start: pos + head,
                len: 0,
                scope: NO_SCOPE,
            };
            return Ok((block, pos + head));
        }
        if sdna < 0 || len < 0 || count < 0 {
            return Err(BlendError::Malformed(format!("bad block header at {pos}")));
        }
        let start = pos + head;
        let end = start
            .checked_add(len as usize)
            .filter(|&e| e <= self.buf.len())
            .ok_or(BlendError::Truncated)?;
        let block = Block {
            code,
            sdna: sdna as usize,
            old,
            count: count as usize,
            start,
            len: len as usize,
            scope: NO_SCOPE,
        };
        Ok((block, end))
    }

    pub fn data(&self, block: &Block) -> &'a [u8] {
        &self.buf[block.start..block.start + block.len]
    }

    /// Finds the block a pointer saved inside `scope` refers to, and the
    /// offset inside it: the ID's own data first, then other IDs, then any
    /// block (shared arrays), then a block the address points into.
    pub fn resolve(&self, addr: u64, scope: usize) -> Option<(&Block, usize)> {
        if addr == 0 {
            return None;
        }
        let exact = self
            .local
            .get(&(scope, addr))
            .or_else(|| self.ids.get(&addr))
            .or_else(|| self.global.get(&addr));
        if let Some(&i) = exact {
            return Some((&self.blocks[i], 0));
        }
        let key = (scope, addr);
        let at = self
            .sorted
            .partition_point(|&i| (self.blocks[i].scope, self.blocks[i].old) <= key);
        let block = &self.blocks[*self.sorted.get(at.checked_sub(1)?)?];
        if block.scope != scope {
            return None;
        }
        let offset = (addr - block.old) as usize;
        (offset < block.len).then_some((block, offset))
    }

    fn bytes<const N: usize>(&self, pos: usize) -> Result<[u8; N], BlendError> {
        let s = self.buf.get(pos..pos + N).ok_or(BlendError::Truncated)?;
        let mut b = [0u8; N];
        b.copy_from_slice(s);
        if !self.little_endian {
            b.reverse();
        }
        Ok(b)
    }

    fn i32_at(&self, pos: usize) -> Result<i32, BlendError> {
        self.bytes::<4>(pos).map(i32::from_le_bytes)
    }

    fn u32_at(&self, pos: usize) -> Result<u32, BlendError> {
        self.bytes::<4>(pos).map(u32::from_le_bytes)
    }

    fn u64_at(&self, pos: usize) -> Result<u64, BlendError> {
        self.bytes::<8>(pos).map(u64::from_le_bytes)
    }
}

fn digits(b: &[u8]) -> Result<u32, BlendError> {
    std::str::from_utf8(b)
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or(BlendError::NotBlend)
}
