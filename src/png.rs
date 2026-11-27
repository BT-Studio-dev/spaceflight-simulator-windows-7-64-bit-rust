//! Minimal, dependency-free PNG writer.
//!
//! Screenshots are written with "stored" (uncompressed) DEFLATE blocks, which
//! keeps the encoder tiny while remaining a perfectly valid PNG. Files are
//! bigger than a real compressor would produce, but screenshots are debug
//! artefacts.
//!
//! Also contains a tiny PNG *reader* for the self-test round-trip check.

use std::io::{self, Write};
use std::path::Path;

fn crc32(data: &[u8]) -> u32 {
    let mut c: u32 = 0xFFFF_FFFF;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for chunk in data.chunks(5552) {
        for &byte in chunk {
            a += byte as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn zlib_store(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + raw.len() / 65_535 * 5 + 16);
    out.push(0x78); // CMF: deflate, 32k window
    out.push(0x01); // FLG: no dictionary, fastest
    let mut rest = raw;
    if rest.is_empty() {
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    }
    while !rest.is_empty() {
        let take = rest.len().min(65_535);
        let last = take == rest.len();
        out.push(if last { 1 } else { 0 });
        out.extend_from_slice(&(take as u16).to_le_bytes());
        out.extend_from_slice(&(!(take as u16)).to_le_bytes());
        out.extend_from_slice(&rest[..take]);
        rest = &rest[take..];
    }
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

/// Encode `argb` (0x00RRGGBB or 0xAARRGGBB, alpha ignored) as RGB8 PNG bytes.
pub fn encode_rgb(w: u32, h: u32, argb: &[u32]) -> Vec<u8> {
    assert!(w > 0 && h > 0, "image must be non-empty");
    assert!(
        argb.len() >= (w as usize) * (h as usize),
        "pixel buffer too small"
    );
    let mut raw = Vec::with_capacity((w as usize) * (h as usize) * 3 + h as usize);
    for y in 0..h as usize {
        raw.push(0); // filter type 0 (None)
        for x in 0..w as usize {
            let p = argb[y * w as usize + x];
            raw.push(((p >> 16) & 0xFF) as u8);
            raw.push(((p >> 8) & 0xFF) as u8);
            raw.push((p & 0xFF) as u8);
        }
    }
    let mut out = Vec::with_capacity(raw.len() / 2 + 128);
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit, truecolour, no interlace
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_store(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

pub fn write_png(path: &Path, w: u32, h: u32, argb: &[u32]) -> io::Result<()> {
    let bytes = encode_rgb(w, h, argb);
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)?;
        }
    }
    let mut f = std::fs::File::create(path)?;
    f.write_all(&bytes)?;
    f.flush()
}

/// Result of decoding a PNG produced by [`encode_rgb`].
pub struct Decoded {
    pub w: u32,
    pub h: u32,
    pub rgb: Vec<u8>,
}

/// Very small PNG reader: 8-bit truecolour, filter 0 only, stored deflate.
/// Used by the self-test to verify that written files are really valid PNGs.
pub fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    if bytes.len() < 8 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err("bad signature".into());
    }
    let mut pos = 8;
    let mut w = 0u32;
    let mut h = 0u32;
    let mut idat: Vec<u8> = Vec::new();
    while pos + 8 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
            as usize;
        let kind = &bytes[pos + 4..pos + 8];
        let data_start = pos + 8;
        if data_start + len + 4 > bytes.len() {
            return Err("truncated chunk".into());
        }
        let data = &bytes[data_start..data_start + len];
        let want = u32::from_be_bytes([
            bytes[data_start + len],
            bytes[data_start + len + 1],
            bytes[data_start + len + 2],
            bytes[data_start + len + 3],
        ]);
        if crc32(&bytes[pos + 4..data_start + len]) != want {
            return Err("bad crc".into());
        }
        match kind {
            b"IHDR" => {
                if data.len() < 13 {
                    return Err("short IHDR".into());
                }
                w = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                h = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
                if data[8] != 8 || data[9] != 2 {
                    return Err("unsupported bit depth / colour type".into());
                }
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            _ => {}
        }
        pos = data_start + len + 4;
    }
    // Verify the zlib wrapper and inflate stored blocks.
    if idat.len() < 6 || idat[0] != 0x78 {
        return Err("bad zlib header".into());
    }
    let mut raw: Vec<u8> = Vec::with_capacity((w as usize * 3 + 1) * h as usize);
    let mut i = 2;
    loop {
        if i + 5 > idat.len() {
            return Err("truncated deflate stream".into());
        }
        let bfinal = idat[i] & 1;
        let btype = (idat[i] >> 1) & 3;
        if btype != 0 {
            return Err("non-stored deflate block".into());
        }
        let len = u16::from_le_bytes([idat[i + 1], idat[i + 2]]) as usize;
        let nlen = u16::from_le_bytes([idat[i + 3], idat[i + 4]]) as usize;
        if nlen != !len & 0xFFFF {
            return Err("bad stored block length".into());
        }
        let start = i + 5;
        if start + len > idat.len() {
            return Err("truncated stored block".into());
        }
        raw.extend_from_slice(&idat[start..start + len]);
        i = start + len;
        if bfinal == 1 {
            break;
        }
    }
    let row_bytes = w as usize * 3;
    if raw.len() < (row_bytes + 1) * h as usize {
        return Err("short image data".into());
    }
    let mut rgb = Vec::with_capacity(row_bytes * h as usize);
    for y in 0..h as usize {
        let row = &raw[y * (row_bytes + 1)..(y + 1) * (row_bytes + 1)];
        if row[0] != 0 {
            return Err("unsupported filter".into());
        }
        rgb.extend_from_slice(&row[1..]);
    }
    Ok(Decoded { w, h, rgb })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let (w, h) = (37u32, 19u32);
        let mut px = vec![0u32; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                px[(y * w + x) as usize] = ((x * 7) << 16) | ((y * 13) << 8) | ((x ^ y) & 0xFF);
            }
        }
        let bytes = encode_rgb(w, h, &px);
        let dec = decode(&bytes).expect("decode");
        assert_eq!(dec.w, w);
        assert_eq!(dec.h, h);
        for y in 0..h as usize {
            for x in 0..w as usize {
                let p = px[y * w as usize + x];
                let o = (y * w as usize + x) * 3;
                assert_eq!(dec.rgb[o], ((p >> 16) & 0xFF) as u8);
                assert_eq!(dec.rgb[o + 1], ((p >> 8) & 0xFF) as u8);
                assert_eq!(dec.rgb[o + 2], (p & 0xFF) as u8);
            }
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode(b"not a png").is_err());
    }
}
