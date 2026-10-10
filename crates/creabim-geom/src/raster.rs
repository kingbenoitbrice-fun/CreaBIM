//! Shaded images of views, and the encodings the exports need.

/// An RGBA image, rows from the top. Transparent where nothing was drawn.
#[derive(Debug, PartialEq)]
pub struct Raster {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    /// Identifies what the image shows (its view), so a viewer can keep one
    /// texture per view and replace it when the image changes.
    pub key: u64,
}

impl Raster {
    /// The colour channels and the alpha channel as separate planes, for PDF.
    pub fn split(&self) -> (Vec<u8>, Vec<u8>) {
        let mut rgb = Vec::with_capacity(self.width * self.height * 3);
        let mut alpha = Vec::with_capacity(self.width * self.height);
        for px in self.rgba.chunks(4) {
            rgb.extend_from_slice(&px[..3]);
            alpha.push(px[3]);
        }
        (rgb, alpha)
    }

    /// The image as a PNG file.
    pub fn png(&self) -> Vec<u8> {
        let mut raw = Vec::with_capacity((self.width * 4 + 1) * self.height);
        for row in self.rgba.chunks(self.width * 4) {
            raw.push(0); // no filter
            raw.extend_from_slice(row);
        }
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&(self.width as u32).to_be_bytes());
        ihdr.extend_from_slice(&(self.height as u32).to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
        chunk(&mut out, b"IHDR", &ihdr);
        chunk(&mut out, b"IDAT", &deflate(&raw));
        chunk(&mut out, b"IEND", &[]);
        out
    }
}

/// zlib-compressed data, as PDF's FlateDecode and PNG expect.
pub fn deflate(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(data, 6)
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_and_base64_match_known_values() {
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn png_round_trips_through_inflate() {
        let r = Raster {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 0],
            key: 0,
        };
        let png = r.png();
        assert!(png.starts_with(b"\x89PNG"));
        let idat = png.windows(4).position(|w| w == b"IDAT").unwrap();
        let len = u32::from_be_bytes(png[idat - 4..idat].try_into().unwrap()) as usize;
        let raw =
            miniz_oxide::inflate::decompress_to_vec_zlib(&png[idat + 4..idat + 4 + len]).unwrap();
        assert_eq!(raw, [0, 255, 0, 0, 255, 0, 0, 255, 0]);
    }
}
