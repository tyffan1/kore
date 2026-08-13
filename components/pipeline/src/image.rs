//! Hand-rolled image decoding for `<img>` elements.
//!
//! No external decoder crates are used: PNG decoding (zlib inflate,
//! unfiltering and pixel format conversion) is implemented from scratch
//! here. JPEG and other formats are not supported yet and fall back to
//! the placeholder rect in the display list builder.

use kore_gpu::GpuImage;

const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

/// Decode raw image bytes into RGBA pixels. Returns `None` for unsupported
/// formats or corrupt data.
pub fn decode_image_bytes(bytes: &[u8]) -> Option<GpuImage> {
    if bytes.starts_with(&PNG_SIGNATURE) {
        return decode_png(bytes);
    }
    None
}

/// Decode a `data:` URL (e.g. `data:image/png;base64,...`).
pub fn decode_data_url(url: &str) -> Option<GpuImage> {
    let rest = url.strip_prefix("data:")?;
    let (media_type, payload) = rest.split_once(',')?;
    if !media_type.contains(";base64") {
        return None;
    }
    let bytes = base64_decode(payload)?;
    decode_image_bytes(&bytes)
}

// ─────────────────────────────── PNG ───────────────────────────────

fn decode_png(data: &[u8]) -> Option<GpuImage> {
    if !data.starts_with(&PNG_SIGNATURE) {
        return None;
    }
    let mut offset = 8usize;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut bit_depth = 0u8;
    let mut color_type = 0u8;
    let mut interlace = 0u8;
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut trns: Vec<u8> = Vec::new();
    let mut idat = Vec::new();

    while offset + 8 <= data.len() {
        let length = u32::from_be_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]) as usize;
        let chunk_end = offset + 12 + length;
        if chunk_end > data.len() {
            return None;
        }
        let chunk_type = &data[offset + 4..offset + 8];
        let chunk_data = &data[offset + 8..offset + 8 + length];
        match chunk_type {
            b"IHDR" => {
                if chunk_data.len() != 13 {
                    return None;
                }
                width = u32::from_be_bytes([chunk_data[0], chunk_data[1], chunk_data[2], chunk_data[3]]);
                height = u32::from_be_bytes([chunk_data[4], chunk_data[5], chunk_data[6], chunk_data[7]]);
                bit_depth = chunk_data[8];
                color_type = chunk_data[9];
                interlace = chunk_data[12];
            }
            b"PLTE" => {
                palette.extend(chunk_data.chunks_exact(3).map(|c| [c[0], c[1], c[2]]));
            }
            b"tRNS" => trns.extend_from_slice(chunk_data),
            b"IDAT" => idat.extend_from_slice(chunk_data),
            b"IEND" => break,
            _ => {}
        }
        offset = chunk_end;
    }

    if width == 0 || height == 0 {
        return None;
    }
    // Adam7 interlacing is not supported.
    if interlace != 0 {
        return None;
    }
    let channels = match color_type {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => return None,
    };
    let supports_depth = bit_depth == 8
        || bit_depth == 16
        || (color_type == 0 && (bit_depth == 1 || bit_depth == 2 || bit_depth == 4));
    if !supports_depth {
        return None;
    }

    let width = width as usize;
    let height = height as usize;
    let raw = inflate_zlib(&idat)?;

    let row_bytes = if bit_depth < 8 {
        (width * bit_depth as usize).div_ceil(8)
    } else {
        width * channels * (bit_depth as usize / 8)
    };
    let bpp = if bit_depth < 8 {
        1
    } else {
        channels * (bit_depth as usize / 8)
    };
    let filtered = unfilter_rows(&raw, height, bpp, row_bytes)?;

    let pixels = to_rgba(&filtered, width, height, bit_depth, color_type, &palette, &trns)?;
    Some(GpuImage {
        width: width as u32,
        height: height as u32,
        pixels,
    })
}

/// Reverse PNG row filtering (None/Sub/Up/Average/Paeth).
fn unfilter_rows(raw: &[u8], height: usize, bpp: usize, row_bytes: usize) -> Option<Vec<u8>> {
    if height == 0 || row_bytes == 0 {
        return None;
    }
    let stride = row_bytes + 1;
    if raw.len() < height * stride {
        return None;
    }
    let mut out = Vec::with_capacity(height * row_bytes);
    let mut prev = vec![0u8; row_bytes];
    for row in 0..height {
        let start = row * stride;
        let filter = raw[start];
        let mut current = raw[start + 1..start + stride].to_vec();
        match filter {
            0 => {}
            1 => {
                for i in bpp..row_bytes {
                    current[i] = current[i].wrapping_add(current[i - bpp]);
                }
            }
            2 => {
                for i in 0..row_bytes {
                    current[i] = current[i].wrapping_add(prev[i]);
                }
            }
            3 => {
                for i in 0..row_bytes {
                    let left = if i >= bpp { current[i - bpp] } else { 0 };
                    current[i] = current[i].wrapping_add(((left as u16 + prev[i] as u16) / 2) as u8);
                }
            }
            4 => {
                for i in 0..row_bytes {
                    let left = if i >= bpp { current[i - bpp] } else { 0 };
                    let up = prev[i];
                    let up_left = if i >= bpp { prev[i - bpp] } else { 0 };
                    current[i] = current[i].wrapping_add(paeth(left, up, up_left));
                }
            }
            _ => return None,
        }
        out.extend_from_slice(&current);
        prev = current;
    }
    Some(out)
}

fn paeth(left: u8, up: u8, up_left: u8) -> u8 {
    let p = left as i32 + up as i32 - up_left as i32;
    let pa = (p - left as i32).abs();
    let pb = (p - up as i32).abs();
    let pc = (p - up_left as i32).abs();
    if pa <= pb && pa <= pc {
        left
    } else if pb <= pc {
        up
    } else {
        up_left
    }
}

/// Convert filtered scanlines into 8-bit RGBA.
fn to_rgba(
    filtered: &[u8],
    width: usize,
    height: usize,
    bit_depth: u8,
    color_type: u8,
    palette: &[[u8; 3]],
    trns: &[u8],
) -> Option<Vec<u8>> {
    // Expand sub-byte grayscale into full bytes first.
    let expanded;
    let source = if bit_depth < 8 {
        expanded = expand_gray_bits(filtered, width, height, bit_depth)?;
        &expanded
    } else {
        filtered
    };

    let sample_bytes = if bit_depth < 8 { 1 } else { bit_depth as usize / 8 };
    let mut out = Vec::with_capacity(width * height * 4);
    let mut idx = 0usize;
    for _ in 0..width * height {
        let mut push = |r: u8, g: u8, b: u8, a: u8| {
            out.extend_from_slice(&[r, g, b, a]);
        };
        match color_type {
            0 => {
                let v = read_sample(source, idx);
                idx += sample_bytes;
                push(v, v, v, 255);
            }
            2 => {
                let r = read_sample(source, idx);
                let g = read_sample(source, idx + sample_bytes);
                let b = read_sample(source, idx + 2 * sample_bytes);
                idx += 3 * sample_bytes;
                push(r, g, b, 255);
            }
            3 => {
                let p = *source.get(idx)? as usize;
                idx += 1;
                let color = *palette.get(p)?;
                let alpha = trns.get(p).copied().unwrap_or(255);
                push(color[0], color[1], color[2], alpha);
            }
            4 => {
                let g = read_sample(source, idx);
                let a = read_sample(source, idx + sample_bytes);
                idx += 2 * sample_bytes;
                push(g, g, g, a);
            }
            6 => {
                let r = read_sample(source, idx);
                let g = read_sample(source, idx + sample_bytes);
                let b = read_sample(source, idx + 2 * sample_bytes);
                let a = read_sample(source, idx + 3 * sample_bytes);
                idx += 4 * sample_bytes;
                push(r, g, b, a);
            }
            _ => return None,
        }
    }
    Some(out)
}

/// For 16-bit samples keep the high byte; 8-bit passes through.
fn read_sample(data: &[u8], offset: usize) -> u8 {
    data.get(offset).copied().unwrap_or(0)
}

fn expand_gray_bits(data: &[u8], width: usize, height: usize, bit_depth: u8) -> Option<Vec<u8>> {
    let bits = bit_depth as usize;
    let mut out = Vec::with_capacity(width * height);
    let mut bit_index = 0usize;
    for _ in 0..width * height {
        let byte = *data.get(bit_index / 8)?;
        let shift = 8 - bits - (bit_index % 8);
        let value = (byte >> shift) & ((1 << bits) - 1);
        let expanded = if bits == 1 {
            if value == 1 { 255 } else { 0 }
        } else if bits == 2 {
            value * 85
        } else {
            value * 17
        };
        out.push(expanded);
        bit_index += bits;
    }
    Some(out)
}

// ─────────────────────────────── inflate ───────────────────────────────

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
const CODE_LENGTH_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// Decompress a zlib stream (RFC 1950 wrapper around raw deflate).
fn inflate_zlib(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 6 {
        return None;
    }
    let cmf = data[0];
    let flg = data[1];
    if cmf & 0x0f != 8 {
        return None;
    }
    if ((cmf as u16) << 8 | flg as u16) % 31 != 0 {
        return None;
    }
    inflate_raw(&data[2..data.len() - 4])
}

/// Decompress a raw DEFLATE stream (RFC 1951).
fn inflate_raw(data: &[u8]) -> Option<Vec<u8>> {
    let mut reader = BitReader::new(data);
    let mut out = Vec::new();
    loop {
        let final_block = reader.read_bits(1)? == 1;
        let block_type = reader.read_bits(2)?;
        match block_type {
            0 => {
                reader.align_to_byte();
                let len = reader.read_le_u16()? as usize;
                let nlen = reader.read_le_u16()? as usize;
                if len != (!nlen) & 0xffff {
                    return None;
                }
                out.extend_from_slice(reader.read_bytes(len)?);
            }
            1 => {
                inflate_block(&mut reader, &mut out, fixed_length_table()?, fixed_dist_table()?)?;
            }
            2 => {
                let hlit = reader.read_bits(5)? as usize + 257;
                let hdist = reader.read_bits(5)? as usize + 1;
                let hclen = reader.read_bits(4)? as usize + 4;
                let mut code_lengths = [0u8; 19];
                for i in 0..hclen {
                    code_lengths[CODE_LENGTH_ORDER[i]] = reader.read_bits(3)? as u8;
                }
                let code_table = build_huffman(&code_lengths)?;
                let mut lengths = Vec::with_capacity(hlit + hdist);
                while lengths.len() < hlit + hdist {
                    let symbol = code_table.decode(&mut reader)? as usize;
                    match symbol {
                        0..=15 => lengths.push(symbol as u8),
                        16 => {
                            let prev = *lengths.last()?;
                            let count = reader.read_bits(2)? as usize + 3;
                            for _ in 0..count {
                                lengths.push(prev);
                            }
                        }
                        17 => {
                            let count = reader.read_bits(3)? as usize + 3;
                            for _ in 0..count {
                                lengths.push(0);
                            }
                        }
                        18 => {
                            let count = reader.read_bits(7)? as usize + 11;
                            for _ in 0..count {
                                lengths.push(0);
                            }
                        }
                        _ => return None,
                    }
                }
                let length_table = build_huffman(&lengths[..hlit])?;
                let dist_table = build_huffman(&lengths[hlit..])?;
                inflate_block(&mut reader, &mut out, length_table, dist_table)?;
            }
            _ => return None,
        }
        if final_block {
            break;
        }
    }
    Some(out)
}

fn inflate_block(
    reader: &mut BitReader<'_>,
    out: &mut Vec<u8>,
    length_table: Huffman,
    dist_table: Huffman,
) -> Option<()> {
    loop {
        let symbol = length_table.decode(reader)? as usize;
        match symbol {
            0..=255 => out.push(symbol as u8),
            256 => return Some(()),
            257..=285 => {
                let index = symbol - 257;
                let length = LENGTH_BASE[index] as usize + reader.read_bits(LENGTH_EXTRA[index] as usize)? as usize;
                let dist_symbol = dist_table.decode(reader)? as usize;
                let distance = if dist_symbol < DIST_BASE.len() {
                    DIST_BASE[dist_symbol] as usize + reader.read_bits(DIST_EXTRA[dist_symbol] as usize)? as usize
                } else {
                    return None;
                };
                if distance > out.len() {
                    return None;
                }
                let start = out.len() - distance;
                for i in 0..length {
                    let byte = out[start + i];
                    out.push(byte);
                }
            }
            _ => return None,
        }
    }
}

/// Canonical Huffman table decoded from per-symbol code lengths.
struct Huffman {
    entries: Vec<(u32, u8, u16)>,
    max_len: u8,
}

fn build_huffman(lengths: &[u8]) -> Option<Huffman> {
    let mut count = [0u16; 16];
    for &len in lengths {
        if len > 15 {
            return None;
        }
        if len > 0 {
            count[len as usize] += 1;
        }
    }
    let mut next_code = [0u32; 16];
    let mut code = 0u32;
    for bits in 1..=15 {
        code = (code + count[bits - 1] as u32) << 1;
        next_code[bits] = code;
    }
    let mut entries = Vec::new();
    let mut max_len = 0u8;
    for (symbol, &len) in lengths.iter().enumerate() {
        if len == 0 {
            continue;
        }
        let entry_code = next_code[len as usize];
        next_code[len as usize] += 1;
        entries.push((entry_code, len, symbol as u16));
        max_len = max_len.max(len);
    }
    Some(Huffman { entries, max_len })
}

impl Huffman {
    fn decode(&self, reader: &mut BitReader<'_>) -> Option<u16> {
        let mut code = 0u32;
        for len in 1..=self.max_len {
            code = (code << 1) | reader.read_bit()? as u32;
            if let Some((_, _, symbol)) = self.entries.iter().find(|(entry_code, entry_len, _)| {
                *entry_len == len && *entry_code == code
            }) {
                return Some(*symbol);
            }
        }
        None
    }
}

fn fixed_length_table() -> Option<Huffman> {
    let mut lengths = [0u8; 288];
    for i in 0..144 {
        lengths[i] = 8;
    }
    for i in 144..256 {
        lengths[i] = 9;
    }
    for i in 256..280 {
        lengths[i] = 7;
    }
    for i in 280..288 {
        lengths[i] = 8;
    }
    build_huffman(&lengths)
}

fn fixed_dist_table() -> Option<Huffman> {
    let lengths = [5u8; 30];
    build_huffman(&lengths)
}

/// LSB-first bit reader over a byte slice.
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    fn read_bit(&mut self) -> Option<u8> {
        let byte = *self.data.get(self.byte_pos)?;
        let bit = (byte >> self.bit_pos) & 1;
        self.bit_pos += 1;
        if self.bit_pos == 8 {
            self.byte_pos += 1;
            self.bit_pos = 0;
        }
        Some(bit)
    }

    fn read_bits(&mut self, count: usize) -> Option<u32> {
        let mut value = 0u32;
        for i in 0..count {
            value |= (self.read_bit()? as u32) << i;
        }
        Some(value)
    }

    fn read_le_u16(&mut self) -> Option<u16> {
        let lo = *self.data.get(self.byte_pos)?;
        let hi = *self.data.get(self.byte_pos + 1)?;
        self.byte_pos += 2;
        Some((lo as u16) | ((hi as u16) << 8))
    }

    fn read_bytes(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.byte_pos.checked_add(count)?;
        let slice = self.data.get(self.byte_pos..end)?;
        self.byte_pos = end;
        Some(slice)
    }

    fn align_to_byte(&mut self) {
        if self.bit_pos > 0 {
            self.byte_pos += 1;
            self.bit_pos = 0;
        }
    }
}

// ─────────────────────────────── base64 ───────────────────────────────

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn value(ch: u8) -> Option<u8> {
        match ch {
            b'A'..=b'Z' => Some(ch - b'A'),
            b'a'..=b'z' => Some(ch - b'a' + 26),
            b'0'..=b'9' => Some(ch - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for ch in input.bytes() {
        if ch == b'=' || ch.is_ascii_whitespace() {
            continue;
        }
        let v = value(ch)?;
        buffer = (buffer << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

/// A 2x2 RGBA PNG (red, green, blue, white) generated by .NET `System.Drawing`.
#[cfg(test)]
const PNG_2X2: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
    0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x06, 0x00, 0x00, 0x00, 0x72,
    0xb6, 0x0d, 0x24, 0x00, 0x00, 0x00, 0x01, 0x73, 0x52, 0x47, 0x42, 0x00, 0xae, 0xce, 0x1c,
    0xe9, 0x00, 0x00, 0x00, 0x04, 0x67, 0x41, 0x4d, 0x41, 0x00, 0x00, 0xb1, 0x8f, 0x0b, 0xfc,
    0x61, 0x05, 0x00, 0x00, 0x00, 0x09, 0x70, 0x48, 0x59, 0x73, 0x00, 0x00, 0x0e, 0xc3, 0x00,
    0x00, 0x0e, 0xc3, 0x01, 0xc7, 0x6f, 0xa8, 0x64, 0x00, 0x00, 0x00, 0x13, 0x49, 0x44, 0x41,
    0x54, 0x18, 0x57, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0x0c, 0x19, 0x18, 0xfe, 0x83, 0x01,
    0x00, 0x49, 0xc8, 0x09, 0xf7, 0x96, 0xde, 0x4d, 0x2e, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

#[cfg(test)]
pub(crate) fn test_png_bytes() -> Vec<u8> {
    PNG_2X2.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A zlib stream containing `"hello world"` in a single stored block.
    /// Header 0x78 0x01, then BFINAL=1 BTYPE=00, LEN=0x0b 0x00, NLEN=0xf4 0xff,
    /// followed by the 11 payload bytes and a dummy Adler-32 checksum.
    const STORED_ZLIB: &[u8] = &[
        0x78, 0x01, 0x01, 0x0b, 0x00, 0xf4, 0xff, b'h', b'e', b'l', b'l', b'o', b' ', b'w', b'o',
        b'r', b'l', b'd', 0x00, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn inflate_stored_block() -> Result<(), String> {
        let out = inflate_zlib(STORED_ZLIB).ok_or("stream should inflate")?;
        assert_eq!(out, b"hello world");
        Ok(())
    }

    #[test]
    fn inflate_rejects_truncated_stream() {
        assert!(inflate_zlib(&STORED_ZLIB[..10]).is_none());
    }

    #[test]
    fn base64_roundtrip() -> Result<(), String> {
        let decoded = base64_decode("aGVsbG8=").ok_or("should decode")?;
        assert_eq!(decoded, b"hello");
        Ok(())
    }

    #[test]
    fn decode_png_rgba_small() -> Result<(), String> {
        let image = decode_image_bytes(PNG_2X2).ok_or("png should decode")?;
        assert_eq!(image.width, 2);
        assert_eq!(image.height, 2);
        assert_eq!(image.pixels.len(), 16);
        // Top-left red, top-right green, bottom-left blue, bottom-right white.
        assert_eq!(&image.pixels[0..4], &[255, 0, 0, 255]);
        assert_eq!(&image.pixels[4..8], &[0, 255, 0, 255]);
        assert_eq!(&image.pixels[8..12], &[0, 0, 255, 255]);
        assert_eq!(&image.pixels[12..16], &[255, 255, 255, 255]);
        Ok(())
    }

    #[test]
    fn decode_png_gray8() -> Result<(), String> {
        let png = build_png(3, 1, 8, 0, &[0, 128, 255]);
        let image = decode_image_bytes(&png).ok_or("gray png should decode")?;
        assert_eq!(image.width, 3);
        assert_eq!(image.height, 1);
        assert_eq!(&image.pixels, &[0, 0, 0, 255, 128, 128, 128, 255, 255, 255, 255, 255]);
        Ok(())
    }

    #[test]
    fn decode_png_gray_alpha() -> Result<(), String> {
        let png = build_png(2, 1, 8, 4, &[50, 255, 200, 100]);
        let image = decode_image_bytes(&png).ok_or("gray+alpha png should decode")?;
        assert_eq!(&image.pixels, &[50, 50, 50, 255, 200, 200, 200, 100]);
        Ok(())
    }

    #[test]
    fn decode_png_palette_with_trns() -> Result<(), String> {
        let png = build_palette_png();
        let image = decode_image_bytes(&png).ok_or("palette png should decode")?;
        assert_eq!(&image.pixels, &[255, 0, 0, 255, 0, 0, 255, 128]);
        Ok(())
    }

    #[test]
    fn decode_png_subbyte_gray() -> Result<(), String> {
        // 4 pixels of 2-bit grayscale: values 0, 1, 2, 3 packed into one byte 0b00011011.
        let expanded = expand_gray_bits(&[0b00011011], 4, 1, 2).ok_or("expand failed")?;
        assert_eq!(&expanded, &[0, 85, 170, 255]);
        let unfiltered = unfilter_rows(&[0, 0b00011011], 1, 1, 1).ok_or("unfilter failed")?;
        assert_eq!(&unfiltered, &[0b00011011]);
        let raw = inflate_zlib(&[0x78, 0x01, 0x01, 0x02, 0x00, 0xfd, 0xff, 0x00, 0x1b, 0, 0, 0, 0]).ok_or("inflate failed")?;
        assert_eq!(&raw, &[0, 0b00011011]);
        let png = build_png(4, 1, 2, 0, &[0b00011011]);
        let image = decode_image_bytes(&png).ok_or("2-bit png should decode")?;
        assert_eq!(
            &image.pixels,
            &[0, 0, 0, 255, 85, 85, 85, 255, 170, 170, 170, 255, 255, 255, 255, 255]
        );
        Ok(())
    }

    #[test]
    fn decode_png_16bit_rgb_takes_high_byte() -> Result<(), String> {
        let png = build_png(1, 1, 16, 2, &[0x12, 0x34, 0xab, 0xcd, 0xef, 0x01]);
        let image = decode_image_bytes(&png).ok_or("16-bit png should decode")?;
        assert_eq!(&image.pixels, &[0x12, 0xab, 0xef, 255]);
        Ok(())
    }

    #[test]
    fn decode_rejects_unknown_format() {
        assert!(decode_image_bytes(&[0xff, 0xd8, 0xff, 0xe0]).is_none());
    }

    #[test]
    fn decodes_base64_data_url() -> Result<(), String> {
        let mut encoded = String::from("data:image/png;base64,");
        encoded.push_str(&base64_encode(PNG_2X2));
        let image = decode_data_url(&encoded).ok_or("data url should decode")?;
        assert_eq!(image.width, 2);
        assert_eq!(image.height, 2);
        Ok(())
    }

    fn base64_encode(data: &[u8]) -> String {
        const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
            let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
            let triple = (b0 << 16) | (b1 << 8) | b2;
            out.push(TABLE[(triple >> 18) as usize & 63] as char);
            out.push(TABLE[(triple >> 12) as usize & 63] as char);
            out.push(if chunk.len() > 1 { TABLE[(triple >> 6) as usize & 63] as char } else { '=' });
            out.push(if chunk.len() > 2 { TABLE[triple as usize & 63] as char } else { '=' });
        }
        out
    }

    /// Build a small non-interlaced PNG with unfiltered (filter 0) scanlines.
    fn build_png(width: u32, height: u32, bit_depth: u8, color_type: u8, rows: &[u8]) -> Vec<u8> {
        let row_bytes = rows.len() / height as usize;
        let mut filtered = Vec::new();
        for row in 0..height as usize {
            filtered.push(0);
            filtered.extend_from_slice(&rows[row * row_bytes..(row + 1) * row_bytes]);
        }
        let mut out = Vec::new();
        out.extend_from_slice(&[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.push(bit_depth);
        ihdr.push(color_type);
        ihdr.push(0);
        ihdr.push(0);
        ihdr.push(0);
        out.extend_from_slice(&chunk(b"IHDR", &ihdr));
        out.extend_from_slice(&chunk(b"IDAT", &zlib_stored(&filtered)));
        out.extend_from_slice(&chunk(b"IEND", &[]));
        out
    }

    /// 2x1 palette PNG: indices [0, 1] with palette red/blue and tRNS [255, 128].
    fn build_palette_png() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.push(8);
        ihdr.push(3);
        ihdr.push(0);
        ihdr.push(0);
        ihdr.push(0);
        out.extend_from_slice(&chunk(b"IHDR", &ihdr));
        out.extend_from_slice(&chunk(b"PLTE", &[255, 0, 0, 0, 0, 255]));
        out.extend_from_slice(&chunk(b"tRNS", &[255, 128]));
        out.extend_from_slice(&chunk(b"IDAT", &zlib_stored(&[0, 0, 1])));
        out.extend_from_slice(&chunk(b"IEND", &[]));
        out
    }

    fn zlib_stored(payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(0x78);
        out.push(0x01);
        out.push(0x01); // BFINAL=1, BTYPE=00
        out.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        out.extend_from_slice(&(!(payload.len() as u16)).to_le_bytes());
        out.extend_from_slice(payload);
        out.extend_from_slice(&[0, 0, 0, 0]); // Adler-32 (not verified by the decoder)
        out
    }

    fn chunk(chunk_type: &[u8], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(chunk_type);
        out.extend_from_slice(data);
        let mut crc_input = chunk_type.to_vec();
        crc_input.extend_from_slice(data);
        out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
        out
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for &byte in data {
            crc ^= byte as u32;
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        !crc
    }
}