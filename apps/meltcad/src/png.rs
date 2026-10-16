//! Minimal, dependency-free PNG encoder.
//!
//! Writes 8-bit RGBA, colour type 6, no interlacing, filter type 0 on every
//! scanline. The IDAT payload is a zlib stream made of stored (uncompressed)
//! deflate blocks, so the output is larger than a compressed PNG but needs no
//! compression code.

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];

/// Largest side accepted, in pixels.
const MAX_SIDE: u32 = 16_384;

/// Largest payload of one stored deflate block.
const MAX_STORED_BLOCK: usize = 65_535;

/// Encodes an 8-bit RGBA image (row-major, 4 bytes per pixel, top row first)
/// as a PNG file held in memory.
pub fn encode_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 {
        return Err(format!("PNG size must be non-zero, got {width}x{height}"));
    }
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(format!("PNG sides must be at most {MAX_SIDE} pixels, got {width}x{height}"));
    }

    let w = usize::try_from(width).map_err(|_| "width does not fit in usize".to_string())?;
    let h = usize::try_from(height).map_err(|_| "height does not fit in usize".to_string())?;
    let row_len = w.checked_mul(4).ok_or_else(|| "image row size overflows".to_string())?;
    let expected = row_len.checked_mul(h).ok_or_else(|| "image size overflows".to_string())?;
    if rgba.len() != expected {
        return Err(format!("{width}x{height} RGBA needs {expected} bytes, got {}", rgba.len()));
    }

    // Scanlines: one filter byte (0 = None) followed by the row's pixels.
    let raw_cap = expected.checked_add(h).ok_or_else(|| "image size overflows".to_string())?;
    let mut raw = Vec::with_capacity(raw_cap);
    for row in rgba.chunks_exact(row_len) {
        raw.push(0);
        raw.extend_from_slice(row);
    }

    let zlib = zlib_stored(&raw)?;

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    // bit depth 8, colour type 6 (RGBA), compression 0, filter 0, interlace 0
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);

    let mut out = Vec::with_capacity(PNG_SIGNATURE.len() + zlib.len() + 64);
    out.extend_from_slice(&PNG_SIGNATURE);
    write_chunk(&mut out, b"IHDR", &ihdr)?;
    write_chunk(&mut out, b"IDAT", &zlib)?;
    write_chunk(&mut out, b"IEND", &[])?;
    Ok(out)
}

/// Wraps `data` in a zlib stream made only of stored deflate blocks.
fn zlib_stored(data: &[u8]) -> Result<Vec<u8>, String> {
    let blocks = data.len() / MAX_STORED_BLOCK + 1;
    let mut out = Vec::with_capacity(data.len() + blocks * 5 + 6);
    // CMF 0x78 (deflate, 32 KiB window); FLG 0x01 makes the header a multiple of 31.
    out.extend_from_slice(&[0x78, 0x01]);

    let mut chunks = data.chunks(MAX_STORED_BLOCK).peekable();
    while let Some(block) = chunks.next() {
        let last = chunks.peek().is_none();
        // First byte: BFINAL in bit 0, BTYPE 00 (stored) in bits 1-2, rest padding.
        out.push(u8::from(last));
        let len = u16::try_from(block.len()).map_err(|_| "stored deflate block too large".to_string())?;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
    if data.is_empty() {
        // Not reachable from encode_rgba (rows are never empty), but keep the
        // stream valid: one final empty stored block.
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    Ok(out)
}

/// Appends one PNG chunk: length, type, data, CRC over type and data.
fn write_chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) -> Result<(), String> {
    let len = u32::try_from(data.len()).map_err(|_| "PNG chunk too large".to_string())?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(&[kind, data]).to_be_bytes());
    Ok(())
}

/// Adler-32 checksum (RFC 1950).
fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    // Largest run that cannot overflow u32 before the modulo reduction.
    const NMAX: usize = 5_552;
    let (mut a, mut b) = (1u32, 0u32);
    for run in data.chunks(NMAX) {
        for &byte in run {
            a = a.wrapping_add(u32::from(byte));
            b = b.wrapping_add(a);
        }
        a %= MOD;
        b %= MOD;
    }
    (b << 16) | a
}

/// CRC-32 (IEEE, reflected, polynomial 0xEDB88320) over the concatenation of `parts`.
fn crc32(parts: &[&[u8]]) -> u32 {
    let table = crc_table();
    let mut crc = 0xFFFF_FFFFu32;
    for part in parts {
        for &byte in *part {
            let idx = usize::from((crc ^ u32::from(byte)) as u8);
            let entry = table.get(idx).copied().unwrap_or(0);
            crc = entry ^ (crc >> 8);
        }
    }
    !crc
}

fn crc_table() -> [u32; 256] {
    std::array::from_fn(|n| {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        c
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Splits a PNG into (type, data) pairs, returning None if the layout or any CRC is wrong.
    fn parse_chunks(png: &[u8]) -> Option<Vec<([u8; 4], Vec<u8>)>> {
        let mut rest = png.get(8..)?;
        let mut out = Vec::new();
        while !rest.is_empty() {
            let len = usize::try_from(u32::from_be_bytes(rest.get(0..4)?.try_into().ok()?)).ok()?;
            let kind: [u8; 4] = rest.get(4..8)?.try_into().ok()?;
            let data = rest.get(8..8 + len)?;
            let crc = u32::from_be_bytes(rest.get(8 + len..12 + len)?.try_into().ok()?);
            if crc != crc32(&[&kind[..], data]) {
                return None;
            }
            out.push((kind, data.to_vec()));
            rest = rest.get(12 + len..)?;
        }
        Some(out)
    }

    /// Inflates a zlib stream made of stored blocks, checking headers and Adler-32.
    fn inflate_stored(zlib: &[u8]) -> Option<Vec<u8>> {
        if zlib.get(0..2)? != [0x78, 0x01] {
            return None;
        }
        let mut rest = zlib.get(2..)?;
        let mut out = Vec::new();
        loop {
            let header = *rest.first()?;
            if header > 1 {
                return None; // BTYPE must be 00 and padding bits zero
            }
            let len = u16::from_le_bytes(rest.get(1..3)?.try_into().ok()?);
            let nlen = u16::from_le_bytes(rest.get(3..5)?.try_into().ok()?);
            if nlen != !len {
                return None;
            }
            let end = 5 + usize::from(len);
            out.extend_from_slice(rest.get(5..end)?);
            rest = rest.get(end..)?;
            if header == 1 {
                break;
            }
        }
        let adler = u32::from_be_bytes(rest.try_into().ok()?);
        (adler == adler32(&out)).then_some(out)
    }

    fn pattern_rgba(width: usize, height: usize) -> Vec<u8> {
        (0..width * height * 4).map(|i| (i * 7 % 251) as u8).collect()
    }

    #[test]
    fn encodes_2x2_with_signature_and_ihdr() {
        let rgba = [
            255, 0, 0, 255, 0, 255, 0, 255, //
            0, 0, 255, 255, 255, 255, 255, 128,
        ];
        let png = encode_rgba(2, 2, &rgba).unwrap_or_default();

        assert!(png.starts_with(&PNG_SIGNATURE));
        let chunks = parse_chunks(&png).unwrap_or_default();
        let kinds: Vec<&[u8]> = chunks.iter().map(|(k, _)| &k[..]).collect();
        assert_eq!(kinds, [&b"IHDR"[..], b"IDAT", b"IEND"]);

        let ihdr = chunks.first().map(|(_, d)| d.clone()).unwrap_or_default();
        assert_eq!(ihdr.get(0..4), Some(&2u32.to_be_bytes()[..]), "width");
        assert_eq!(ihdr.get(4..8), Some(&2u32.to_be_bytes()[..]), "height");
        assert_eq!(ihdr.get(8..13), Some(&[8u8, 6, 0, 0, 0][..]), "depth/colour/compression/filter/interlace");
    }

    #[test]
    fn crc32_check_value() {
        assert_eq!(crc32(&[b"123456789"]), 0xCBF4_3926);
        // Splitting the input must not change the result.
        assert_eq!(crc32(&[b"1234", b"56789"]), 0xCBF4_3926);
    }

    #[test]
    fn adler32_check_value() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(b""), 1);
    }

    #[test]
    fn rejects_zero_size_and_bad_lengths() {
        assert!(encode_rgba(0, 2, &[]).is_err());
        assert!(encode_rgba(2, 0, &[]).is_err());
        assert!(encode_rgba(0, 0, &[]).is_err());
        assert!(encode_rgba(2, 2, &[0; 15]).is_err());
        assert!(encode_rgba(2, 2, &[0; 17]).is_err());
        assert!(encode_rgba(2, 2, &[0; 16]).is_ok());
        assert!(encode_rgba(MAX_SIDE + 1, 1, &[]).is_err());
        assert!(encode_rgba(1, MAX_SIDE + 1, &[]).is_err());
    }

    #[test]
    fn stored_blocks_round_trip_pixels() {
        // 100 x 200 gives 80 200 raw bytes, so the zlib stream needs two stored blocks.
        let (w, h) = (100usize, 200usize);
        let rgba = pattern_rgba(w, h);
        let png = encode_rgba(w as u32, h as u32, &rgba).unwrap_or_default();

        let chunks = parse_chunks(&png).unwrap_or_default();
        let idat = chunks.iter().find(|(k, _)| k == b"IDAT").map(|(_, d)| d.clone()).unwrap_or_default();
        let raw = inflate_stored(&idat).unwrap_or_default();

        let expected: Vec<u8> = rgba.chunks(w * 4).flat_map(|row| std::iter::once(0u8).chain(row.iter().copied())).collect();
        assert_eq!(raw.len(), h * (w * 4 + 1));
        assert!(raw == expected, "decoded scanlines differ from input");
    }

    #[test]
    fn single_pixel_image() {
        let png = encode_rgba(1, 1, &[10, 20, 30, 40]).unwrap_or_default();
        let chunks = parse_chunks(&png).unwrap_or_default();
        let idat = chunks.iter().find(|(k, _)| k == b"IDAT").map(|(_, d)| d.clone()).unwrap_or_default();
        assert_eq!(inflate_stored(&idat), Some(vec![0, 10, 20, 30, 40]));
    }
}
