//! A small animated GIF writer (no dependencies): 256 colours per picture, LZW, looping forever.
//!
//! Pictures are encoded as they arrive ([`Writer`]): a long animation holds one picture and one
//! 8-bit index picture, never the whole run (#217).

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

/// Like [`encode`], with a delay for every frame (for tests that hand in a whole run at once).
#[cfg(test)]
pub fn encode_timed(w: usize, h: usize, frames: &[Vec<u8>], delays: &[u16]) -> Result<Vec<u8>, String> {
    if delays.len() != frames.len() {
        return Err("Every frame needs a delay".into());
    }
    if frames.is_empty() {
        return Err("Bad picture size".into());
    }
    let mut writer = Writer::new(w, h)?;
    for (f, &delay_cs) in frames.iter().zip(delays) {
        writer.push(f, delay_cs)?;
    }
    Ok(writer.finish())
}

/// A GIF written picture by picture. The palette, LZW dictionary, index picture and output buffer
/// are reused for every picture, so the memory a run needs does not grow with its length: one RGBA
/// picture handed in, one 8-bit index picture here.
pub struct Writer {
    w: usize,
    h: usize,
    out: Vec<u8>,
    /// The current picture's 256 colours.
    palette: Vec<[u8; 3]>,
    /// The colours of the (first) global table; a picture that shares it needs no local table.
    global: Vec<[u8; 3]>,
    /// 5-bit colour key to palette index, reset for every picture with u16::MAX.
    lut: Vec<u16>,
    counts: Vec<u32>,
    order: Vec<usize>,
    idx: Vec<u8>,
    lzw: Vec<u8>,
    dict: HashMap<(u32, u8), u32>,
    frames: usize,
    started: bool,
}

impl Writer {
    /// A writer for `w` x `h` pictures. Nothing is written until the first [`Writer::push`], so the
    /// global table can be the first picture's palette.
    pub fn new(w: usize, h: usize) -> Result<Writer, String> {
        if w == 0 || h == 0 || w > 4096 || h > 4096 {
            return Err("Bad picture size".into());
        }
        let pixels = w.checked_mul(h).ok_or("Bad picture size")?;
        if pixels.checked_mul(4).is_none() {
            return Err("Bad picture size".into());
        }
        Ok(Writer {
            w,
            h,
            out: Vec::new(),
            palette: Vec::with_capacity(256),
            global: Vec::with_capacity(256),
            lut: vec![u16::MAX; 32768],
            counts: vec![0u32; 32768],
            order: Vec::with_capacity(4096),
            idx: Vec::with_capacity(pixels),
            lzw: Vec::new(),
            dict: HashMap::new(),
            frames: 0,
            started: false,
        })
    }

    /// Bytes in one RGBA picture of this writer's size.
    pub fn picture_len(&self) -> usize {
        // `new` proved that this fits (both are at most 4096).
        self.w.saturating_mul(self.h).saturating_mul(4)
    }

    /// Pictures written so far.
    #[cfg(test)]
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Most full pictures the writer itself keeps alive at once: its 8-bit index picture, once a
    /// picture has been encoded (0 before that). It never keeps the RGBA pictures, so a run's peak
    /// is this plus the one picture its caller is drawing.
    pub fn peak_pictures(&self) -> usize {
        usize::from(self.frames > 0)
    }

    /// Write one RGBA picture, shown for `delay_cs` hundredths of a second.
    pub fn push(&mut self, rgba: &[u8], delay_cs: u16) -> Result<(), String> {
        if rgba.len() != self.picture_len() {
            return Err("A frame has the wrong size".into());
        }
        if !self.started {
            // The first picture's palette is the global table: a one-picture GIF, or an animation
            // whose colours do not change, then needs no local table at all.
            self.palette_of(rgba);
            self.global.clone_from(&self.palette);
            self.write_header();
            self.started = true;
        } else {
            self.palette_of(rgba);
        }
        let local = self.palette != self.global;
        self.out.extend_from_slice(&[0x21, 0xF9, 4, 0]);
        self.out.extend_from_slice(&delay_cs.to_le_bytes());
        self.out.extend_from_slice(&[0, 0]);
        self.out.push(0x2C);
        self.out.extend_from_slice(&[0, 0, 0, 0]);
        self.out.extend_from_slice(&(self.w as u16).to_le_bytes());
        self.out.extend_from_slice(&(self.h as u16).to_le_bytes());
        // a local table of 256 colours (colour resolution is the global table's), or none
        self.out.push(if local { 0x87 } else { 0 });
        if local {
            for c in &self.palette {
                self.out.extend_from_slice(c);
            }
        }
        let lut = &mut self.lut;
        lut.fill(u16::MAX);
        let palette = &self.palette;
        self.idx.clear();
        self.idx.extend(rgba.as_chunks::<4>().0.iter().map(|p| index_of(palette, lut, p)));
        self.out.push(8);
        lzw_into(&self.idx, &mut self.lzw, &mut self.dict);
        for chunk in self.lzw.chunks(255) {
            self.out.push(chunk.len() as u8);
            self.out.extend_from_slice(chunk);
        }
        self.out.push(0);
        self.frames += 1;
        Ok(())
    }

    /// Close the GIF (loop forever, trailer) and hand back its bytes.
    pub fn finish(mut self) -> Vec<u8> {
        if !self.started {
            self.write_header();
            self.started = true;
        }
        self.out.push(0x3B);
        self.out
    }

    fn write_header(&mut self) {
        self.out.extend_from_slice(b"GIF89a");
        self.out.extend_from_slice(&(self.w as u16).to_le_bytes());
        self.out.extend_from_slice(&(self.h as u16).to_le_bytes());
        self.out.extend_from_slice(&[0xF7, 0, 0]); // global table of 256 colours
        for i in 0..256 {
            // until a picture arrives the table is black; `push` replaces it before writing one
            self.out.extend_from_slice(&self.global.get(i).copied().unwrap_or([0, 0, 0]));
        }
        self.out.extend_from_slice(&[0x21, 0xFF, 0x0B]);
        self.out.extend_from_slice(b"NETSCAPE2.0");
        self.out.extend_from_slice(&[3, 1, 0, 0, 0]); // loop forever
    }

    /// The 256 most used colours of one picture (at 5 bits per channel).
    fn palette_of(&mut self, rgba: &[u8]) {
        self.counts.fill(0);
        for p in rgba.as_chunks::<4>().0.iter() {
            if let Some(c) = self.counts.get_mut(key(p)) {
                *c = c.saturating_add(1);
            }
        }
        self.order.clear();
        self.order.extend((0..32768).filter(|&k| self.counts.get(k).copied().unwrap_or(0) > 0));
        let counts = &self.counts;
        self.order.sort_by_key(|&k| std::cmp::Reverse(counts.get(k).copied().unwrap_or(0)));
        self.order.truncate(256);
        self.palette.clear();
        self.palette
            .extend(self.order.iter().map(|&k| [(((k >> 10) & 31) << 3 | 4) as u8, (((k >> 5) & 31) << 3 | 4) as u8, ((k & 31) << 3 | 4) as u8]));
        self.palette.resize(256, [0, 0, 0]);
    }
}

fn key(p: &[u8]) -> usize {
    let g = |i: usize| usize::from(p.get(i).copied().unwrap_or(0) >> 3);
    (g(0) << 10) | (g(1) << 5) | g(2)
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
#[cfg(test)]
fn lzw(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut dict = HashMap::new();
    lzw_into(data, &mut out, &mut dict);
    out
}

/// [`lzw`], reusing the output and dictionary of the previous picture.
fn lzw_into(data: &[u8], out: &mut Vec<u8>, dict: &mut HashMap<(u32, u8), u32>) {
    out.clear();
    dict.clear();
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
    let (mut next, mut size) = (258u32, 9u32);
    put(256, size, out);
    let mut iter = data.iter();
    let Some(&first) = iter.next() else {
        put(257, size, out);
        if nbits > 0 {
            out.push((acc & 255) as u8);
        }
        return;
    };
    let mut cur = u32::from(first);
    for &b in iter {
        if let Some(&c) = dict.get(&(cur, b)) {
            cur = c;
            continue;
        }
        put(cur, size, out);
        dict.insert((cur, b), next);
        if next == (1 << size) && size < 12 {
            size += 1;
        }
        next += 1;
        if next >= 4095 {
            put(256, size, out);
            dict.clear();
            next = 258;
            size = 9;
        }
        cur = u32::from(b);
    }
    put(cur, size, out);
    put(257, size, out);
    if nbits > 0 {
        out.push((acc & 255) as u8);
    }
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

    /// Walk the GIF's blocks: every picture's delay (1/100 s), its 256-colour table and its
    /// decoded colour indices.
    fn read_pictures(bytes: &[u8]) -> Vec<(u16, Vec<[u8; 3]>, Vec<u8>)> {
        let byte = |i: usize| bytes.get(i).copied().unwrap_or(0);
        let mut pictures = Vec::new();
        let mut pos = 13usize;
        let packed = byte(10);
        let mut global = Vec::new();
        if packed & 0x80 != 0 {
            let n = 2usize << (packed & 7);
            for i in 0..n {
                global.push([byte(pos + 3 * i), byte(pos + 3 * i + 1), byte(pos + 3 * i + 2)]);
            }
            pos += 3 * n;
        }
        let mut delay = 0u16;
        while let Some(&b) = bytes.get(pos) {
            pos += 1;
            match b {
                0x3B => break,
                0x21 => {
                    let label = byte(pos);
                    pos += 1;
                    if label == 0xF9 {
                        let n = usize::from(byte(pos));
                        pos += 1;
                        if n >= 4 {
                            delay = u16::from_le_bytes([byte(pos + 1), byte(pos + 2)]);
                        }
                        pos += n;
                        if byte(pos) == 0 {
                            pos += 1;
                        }
                    } else {
                        // a sub-block list ending with a zero-length one
                        loop {
                            let n = usize::from(byte(pos));
                            pos += 1;
                            if n == 0 {
                                break;
                            }
                            pos += n;
                        }
                    }
                }
                0x2C => {
                    let flags = byte(pos + 8);
                    pos += 9;
                    let table = if flags & 0x80 != 0 {
                        let n = 2usize << (flags & 7);
                        let mut t = Vec::new();
                        for i in 0..n {
                            t.push([byte(pos + 3 * i), byte(pos + 3 * i + 1), byte(pos + 3 * i + 2)]);
                        }
                        pos += 3 * n;
                        t
                    } else {
                        global.clone()
                    };
                    pos += 1; // minimum code size
                    let mut lzw = Vec::new();
                    loop {
                        let n = usize::from(byte(pos));
                        pos += 1;
                        if n == 0 {
                            break;
                        }
                        if let Some(part) = bytes.get(pos..pos + n) {
                            lzw.extend_from_slice(part);
                        }
                        pos += n;
                    }
                    pictures.push((delay, table, unlzw(&lzw)));
                }
                _ => break,
            }
        }
        pictures
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
    fn a_long_animation_is_encoded_one_picture_at_a_time() {
        // 60 pictures of 32 x 24: the writer never holds more than its one index picture, and the
        // pictures keep their order and their delays (#217).
        let (w, h, n) = (32usize, 24usize, 60usize);
        let mut writer = Writer::new(w, h).unwrap();
        for i in 0..n {
            let pic: Vec<u8> = (0..w * h).flat_map(|p| [(p % 200) as u8, i as u8, (i * 3) as u8, 255]).collect();
            writer.push(&pic, 2 + (i % 5) as u16).unwrap();
            assert_eq!(writer.frames(), i + 1);
            assert_eq!(writer.peak_pictures(), 1, "only the 8-bit index picture is kept");
        }
        let g = writer.finish();
        let pictures = read_pictures(&g);
        assert_eq!(pictures.len(), n);
        for (i, (delay, _, pixels)) in pictures.iter().enumerate() {
            assert_eq!(*delay, 2 + (i % 5) as u16, "picture {i} keeps its delay");
            assert_eq!(pixels.len(), w * h, "picture {i} decodes to a whole picture");
        }
    }

    #[test]
    fn a_picture_round_trips_through_its_own_palette() {
        // Colours the 5-bit table holds exactly (252 = 31 << 3 | 4), so the round trip is exact
        // even though every picture carries its own 256-colour table.
        let (w, h) = (8usize, 6usize);
        let mut writer = Writer::new(w, h).unwrap();
        let mut want = Vec::new();
        for i in 0..3u8 {
            // each picture has two colours that no other picture uses, so it needs its own table
            let a = [4 + i * 80, 4, 252, 255];
            let b = [252, 4 + i * 80, 4, 255];
            let pic: Vec<u8> = (0..w * h).flat_map(|p| if (p + usize::from(i)) % 3 == 0 { a } else { b }).collect();
            want.push(pic.clone());
            writer.push(&pic, 4).unwrap();
        }
        let pictures = read_pictures(&writer.finish());
        assert_eq!(pictures.len(), 3);
        for (k, (_, table, pixels)) in pictures.iter().enumerate() {
            let got: Vec<u8> = pixels
                .iter()
                .flat_map(|&i| {
                    let c = table.get(usize::from(i)).copied().unwrap_or([0, 0, 0]);
                    [c[0], c[1], c[2], 255]
                })
                .collect();
            assert_eq!(got, want[k], "picture {k} decodes to its own colours");
        }
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
