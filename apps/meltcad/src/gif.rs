//! A small animated GIF writer (no dependencies): one shared palette, LZW, looping forever.

use std::collections::HashMap;

/// `frames` are RGBA pictures of `w` x `h`; `delay_cs` is the time of every frame in 1/100 s.
#[cfg(test)]
pub fn encode(w: usize, h: usize, frames: &[Vec<u8>], delay_cs: u16) -> Result<Vec<u8>, String> {
    encode_timed(w, h, frames, &vec![delay_cs; frames.len()])
}

/// The delay of each of `count` frames (1/100 s) when a frame lasts `cs` hundredths on average.
/// A GIF counts whole hundredths, so the rounding error is handed on from frame to frame: the
/// whole animation lasts as long as it should. (Viewers play a delay under 2 as 10, so callers
/// keep `cs` at 2 or more.)
pub fn delays(count: usize, cs: f32) -> Vec<u16> {
    let cs = if cs.is_finite() { cs.clamp(2.0, 6000.0) } else { 2.0 };
    let at = |i: usize| (f64::from(cs) * i as f64).round();
    (0..count).map(|i| (at(i + 1) - at(i)).clamp(2.0, 6000.0) as u16).collect()
}

/// Like [`encode`], with a delay for every frame.
pub fn encode_timed(w: usize, h: usize, frames: &[Vec<u8>], delays: &[u16]) -> Result<Vec<u8>, String> {
    if delays.len() != frames.len() {
        return Err("Every frame needs a delay".into());
    }
    if w == 0 || h == 0 || w > 4096 || h > 4096 || frames.is_empty() {
        return Err("Bad picture size".into());
    }
    if frames.iter().any(|f| f.len() != w * h * 4) {
        return Err("A frame has the wrong size".into());
    }
    let palette = make_palette(frames);
    let mut lut: Vec<u16> = vec![u16::MAX; 32768];
    let mut out = Vec::new();
    out.extend_from_slice(b"GIF89a");
    out.extend_from_slice(&(w as u16).to_le_bytes());
    out.extend_from_slice(&(h as u16).to_le_bytes());
    out.extend_from_slice(&[0xF7, 0, 0]); // global table of 256 colours
    for c in &palette {
        out.extend_from_slice(c);
    }
    out.extend_from_slice(&[0x21, 0xFF, 0x0B]);
    out.extend_from_slice(b"NETSCAPE2.0");
    out.extend_from_slice(&[3, 1, 0, 0, 0]); // loop forever
    for (f, delay_cs) in frames.iter().zip(delays) {
        out.extend_from_slice(&[0x21, 0xF9, 4, 0]);
        out.extend_from_slice(&delay_cs.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out.push(0x2C);
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&(w as u16).to_le_bytes());
        out.extend_from_slice(&(h as u16).to_le_bytes());
        out.push(0);
        let idx: Vec<u8> = f.as_chunks::<4>().0.iter().map(|p| index_of(&palette, &mut lut, p)).collect();
        out.push(8);
        for chunk in lzw(&idx).chunks(255) {
            out.push(chunk.len() as u8);
            out.extend_from_slice(chunk);
        }
        out.push(0);
    }
    out.push(0x3B);
    Ok(out)
}

fn key(p: &[u8]) -> usize {
    let g = |i: usize| usize::from(p.get(i).copied().unwrap_or(0) >> 3);
    (g(0) << 10) | (g(1) << 5) | g(2)
}

/// The 256 most used colours (at 5 bits per channel) over all frames.
fn make_palette(frames: &[Vec<u8>]) -> Vec<[u8; 3]> {
    let mut count = vec![0u32; 32768];
    for f in frames {
        for p in f.as_chunks::<4>().0.iter() {
            if let Some(c) = count.get_mut(key(p)) {
                *c += 1;
            }
        }
    }
    let mut order: Vec<usize> = (0..32768).filter(|&k| count.get(k).copied().unwrap_or(0) > 0).collect();
    order.sort_by_key(|&k| std::cmp::Reverse(count.get(k).copied().unwrap_or(0)));
    order.truncate(256);
    let mut pal: Vec<[u8; 3]> =
        order.iter().map(|&k| [(((k >> 10) & 31) << 3 | 4) as u8, (((k >> 5) & 31) << 3 | 4) as u8, ((k & 31) << 3 | 4) as u8]).collect();
    pal.resize(256, [0, 0, 0]);
    pal
}

fn index_of(pal: &[[u8; 3]], lut: &mut [u16], p: &[u8]) -> u8 {
    let k = key(p);
    if let Some(&v) = lut.get(k)
        && v != u16::MAX
    {
        return v as u8;
    }
    let rgb = [p.first().copied().unwrap_or(0), p.get(1).copied().unwrap_or(0), p.get(2).copied().unwrap_or(0)];
    let best =
        pal.iter().enumerate().min_by_key(|(_, c)| (0..3).map(|i| (i32::from(c[i]) - i32::from(rgb[i])).pow(2)).sum::<i32>()).map_or(0, |(i, _)| i);
    if let Some(slot) = lut.get_mut(k) {
        *slot = best as u16;
    }
    best as u8
}

/// GIF-flavoured LZW with 8-bit codes (clear = 256, end = 257).
fn lzw(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let (mut acc, mut nbits) = (0u32, 0u32);
    let mut put = |code: u32, size: u32, out: &mut Vec<u8>| {
        acc |= code << nbits;
        nbits += size;
        while nbits >= 8 {
            out.push((acc & 255) as u8);
            acc >>= 8;
            nbits -= 8;
        }
    };
    let mut dict: HashMap<(u32, u8), u32> = HashMap::new();
    let (mut next, mut size) = (258u32, 9u32);
    put(256, size, &mut out);
    let mut iter = data.iter();
    let Some(&first) = iter.next() else {
        put(257, size, &mut out);
        if nbits > 0 {
            out.push((acc & 255) as u8);
        }
        return out;
    };
    let mut cur = u32::from(first);
    for &b in iter {
        if let Some(&c) = dict.get(&(cur, b)) {
            cur = c;
            continue;
        }
        put(cur, size, &mut out);
        dict.insert((cur, b), next);
        if next == (1 << size) && size < 12 {
            size += 1;
        }
        next += 1;
        if next >= 4095 {
            put(256, size, &mut out);
            dict.clear();
            next = 258;
            size = 9;
        }
        cur = u32::from(b);
    }
    put(cur, size, &mut out);
    put(257, size, &mut out);
    if nbits > 0 {
        out.push((acc & 255) as u8);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reference decoder, only to prove the encoder's output.
    fn unlzw(bytes: &[u8]) -> Vec<u8> {
        let (mut acc, mut nbits, mut pos) = (0u32, 0u32, 0usize);
        let mut size = 9u32;
        let mut table: Vec<Vec<u8>> = (0..258).map(|i| if i < 256 { vec![i as u8] } else { Vec::new() }).collect();
        let mut out = Vec::new();
        let mut prev: Option<Vec<u8>> = None;
        loop {
            while nbits < size {
                let Some(&b) = bytes.get(pos) else { return out };
                acc |= u32::from(b) << nbits;
                nbits += 8;
                pos += 1;
            }
            let code = (acc & ((1 << size) - 1)) as usize;
            acc >>= size;
            nbits -= size;
            if code == 256 {
                table.truncate(258);
                size = 9;
                prev = None;
                continue;
            }
            if code == 257 {
                return out;
            }
            let entry = match table.get(code) {
                Some(e) if !e.is_empty() => e.clone(),
                _ => {
                    let mut e = prev.clone().unwrap_or_default();
                    let f = e.first().copied().unwrap_or(0);
                    e.push(f);
                    e
                }
            };
            out.extend_from_slice(&entry);
            if let Some(p) = prev {
                let mut n = p;
                n.push(entry.first().copied().unwrap_or(0));
                table.push(n);
                if table.len() == (1 << size) && size < 12 {
                    size += 1;
                }
            }
            prev = Some(entry);
        }
    }

    #[test]
    fn lzw_round_trips_small_and_large_inputs() {
        let small: Vec<u8> = vec![1, 1, 1, 2, 2, 3, 1, 1, 1, 2];
        assert_eq!(unlzw(&lzw(&small)), small);
        let big: Vec<u8> = (0..60_000u32).map(|i| ((i * i / 7 + i / 13) % 251) as u8).collect();
        assert_eq!(unlzw(&lzw(&big)), big);
        let flat = vec![7u8; 100_000];
        assert_eq!(unlzw(&lzw(&flat)), flat);
    }

    #[test]
    fn a_gif_has_the_right_header_and_trailer() {
        let f: Vec<u8> = (0..16 * 16).flat_map(|i| [(i % 256) as u8, 40, 200, 255]).collect();
        let g = encode(16, 16, &[f.clone(), f], 8).unwrap();
        assert_eq!(&g[..6], b"GIF89a");
        assert_eq!(g.last(), Some(&0x3B));
        assert!(encode(0, 4, &[], 8).is_err());
        assert!(encode(2, 2, &[vec![0; 3]], 8).is_err());
    }

    #[test]
    fn delays_add_up_to_the_real_time() {
        // 60 fps, 48 frames: 0.8 s, not the 0.96 s that 2 cs for each frame would give
        let d = delays(48, 100.0 / 60.0 * 2.0);
        assert!(d.iter().all(|c| *c >= 2));
        // two frames per picture here: 24 pictures of 3.33 cs
        let d = delays(24, 100.0 * 2.0 / 60.0);
        assert_eq!(d.iter().map(|c| u32::from(*c)).sum::<u32>(), 80);
        assert!(d.iter().all(|c| *c == 3 || *c == 4));
        assert!(delays(3, f32::NAN).iter().all(|c| *c == 2));
    }
}
