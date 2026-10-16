//! A small DEFLATE compressor (RFC 1951): LZ77 with hash chains and the fixed Huffman code, so that
//! 3MF files (XML text, which repeats a lot) come out several times smaller without a new dependency.

struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn put(&mut self, value: u32, count: u32) {
        self.acc |= u64::from(value) << self.n;
        self.n += count;
        while self.n >= 8 {
            self.out.push((self.acc & 0xFF) as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    /// A Huffman code goes out most significant bit first.
    fn code(&mut self, code: u32, len: u32) {
        let mut rev = 0u32;
        for i in 0..len {
            rev |= ((code >> i) & 1) << (len - 1 - i);
        }
        self.put(rev, len);
    }
    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push((self.acc & 0xFF) as u8);
        }
        self.out
    }
}

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

fn literal(b: &mut Bits, sym: u32) {
    match sym {
        0..=143 => b.code(0x30 + sym, 8),
        144..=255 => b.code(0x190 + (sym - 144), 9),
        256..=279 => b.code(sym - 256, 7),
        _ => b.code(0xC0 + (sym - 280), 8),
    }
}

fn length_symbol(len: usize) -> usize {
    let mut k = 28;
    while k > 0 && usize::from(LEN_BASE.get(k).copied().unwrap_or(3)) > len {
        k -= 1;
    }
    k
}

fn dist_symbol(dist: usize) -> usize {
    let mut k = 29;
    while k > 0 && usize::from(DIST_BASE.get(k).copied().unwrap_or(1)) > dist {
        k -= 1;
    }
    k
}

const WINDOW: usize = 32768;
const HASH_BITS: u32 = 15;

fn hash3(d: &[u8], i: usize) -> usize {
    let a = u32::from(d.get(i).copied().unwrap_or(0));
    let b = u32::from(d.get(i + 1).copied().unwrap_or(0));
    let c = u32::from(d.get(i + 2).copied().unwrap_or(0));
    (((a << 16) | (b << 8) | c).wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
}

/// The data as one DEFLATE stream (a single block with the fixed Huffman code).
pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut b = Bits { out: Vec::with_capacity(data.len() / 3 + 16), acc: 0, n: 0 };
    b.put(1, 1); // last block
    b.put(1, 2); // fixed Huffman
    let mut head = vec![u32::MAX; 1 << HASH_BITS];
    let mut prev = vec![u32::MAX; WINDOW];
    let n = data.len();
    let mut i = 0usize;
    let insert = |head: &mut Vec<u32>, prev: &mut Vec<u32>, p: usize| {
        if p + 2 < n {
            let h = hash3(data, p);
            if let (Some(hs), Some(pv)) = (head.get_mut(h), prev.get_mut(p % WINDOW)) {
                *pv = *hs;
                *hs = p as u32;
            }
        }
    };
    while i < n {
        let mut best_len = 0usize;
        let mut best_dist = 0usize;
        if i + 2 < n {
            let mut cand = head.get(hash3(data, i)).copied().unwrap_or(u32::MAX);
            let mut depth = 24;
            let max_len = (n - i).min(258);
            while cand != u32::MAX && depth > 0 {
                let c = cand as usize;
                if c >= i || i - c > WINDOW {
                    break;
                }
                let mut l = 0;
                while l < max_len && data.get(c + l) == data.get(i + l) {
                    l += 1;
                }
                if l > best_len {
                    best_len = l;
                    best_dist = i - c;
                    if l == max_len {
                        break;
                    }
                }
                cand = prev.get(c % WINDOW).copied().unwrap_or(u32::MAX);
                depth -= 1;
            }
        }
        if best_len >= 4 || (best_len == 3 && best_dist < 4096) {
            let ls = length_symbol(best_len);
            literal(&mut b, 257 + ls as u32);
            let extra = u32::from(LEN_EXTRA.get(ls).copied().unwrap_or(0));
            if extra > 0 {
                b.put((best_len - usize::from(LEN_BASE.get(ls).copied().unwrap_or(3))) as u32, extra);
            }
            let ds = dist_symbol(best_dist);
            b.code(ds as u32, 5);
            let dextra = u32::from(DIST_EXTRA.get(ds).copied().unwrap_or(0));
            if dextra > 0 {
                b.put((best_dist - usize::from(DIST_BASE.get(ds).copied().unwrap_or(1))) as u32, dextra);
            }
            for p in i..i + best_len {
                insert(&mut head, &mut prev, p);
            }
            i += best_len;
        } else {
            literal(&mut b, u32::from(data.get(i).copied().unwrap_or(0)));
            insert(&mut head, &mut prev, i);
            i += 1;
        }
    }
    literal(&mut b, 256);
    b.finish()
}

#[cfg(test)]
pub mod tests {
    use super::*;

    struct Reader<'a> {
        d: &'a [u8],
        pos: usize,
    }

    impl Reader<'_> {
        fn bit(&mut self) -> u32 {
            let byte = self.d[self.pos / 8];
            let v = (byte >> (self.pos % 8)) & 1;
            self.pos += 1;
            u32::from(v)
        }
        fn bits(&mut self, n: u32) -> u32 {
            (0..n).map(|i| self.bit() << i).sum()
        }
        fn code_msb(&mut self, n: u32) -> u32 {
            (0..n).fold(0, |a, _| (a << 1) | self.bit())
        }
        /// A fixed-Huffman literal/length symbol.
        fn sym(&mut self) -> u32 {
            let mut v = self.code_msb(7);
            if v <= 0b0010111 {
                return 256 + v;
            }
            v = (v << 1) | self.bit();
            if (0b0011_0000..=0b1011_1111).contains(&v) {
                return v - 0x30;
            }
            if (0b1100_0000..=0b1100_0111).contains(&v) {
                return 280 + (v - 0xC0);
            }
            v = (v << 1) | self.bit();
            144 + (v - 0x190)
        }
    }

    /// A reader of what `deflate` writes (stored and fixed-Huffman blocks).
    pub fn inflate(d: &[u8]) -> Vec<u8> {
        let mut r = Reader { d, pos: 0 };
        let mut out: Vec<u8> = Vec::new();
        loop {
            let last = r.bit();
            let kind = r.bits(2);
            if kind == 0 {
                r.pos = r.pos.div_ceil(8) * 8;
                let len = (d[r.pos / 8] as usize) | ((d[r.pos / 8 + 1] as usize) << 8);
                out.extend_from_slice(&d[r.pos / 8 + 4..r.pos / 8 + 4 + len]);
                r.pos += (4 + len) * 8;
            } else {
                assert_eq!(kind, 1, "only the fixed code is written");
                loop {
                    let s = r.sym();
                    if s < 256 {
                        out.push(s as u8);
                    } else if s == 256 {
                        break;
                    } else {
                        let k = (s - 257) as usize;
                        let len = LEN_BASE[k] as usize + r.bits(u32::from(LEN_EXTRA[k])) as usize;
                        let dk = r.code_msb(5) as usize;
                        let dist = DIST_BASE[dk] as usize + r.bits(u32::from(DIST_EXTRA[dk])) as usize;
                        for _ in 0..len {
                            out.push(out[out.len() - dist]);
                        }
                    }
                }
            }
            if last == 1 {
                return out;
            }
        }
    }

    #[test]
    fn deflate_round_trips_text_noise_and_the_empty_input() {
        let text: Vec<u8> = (0..20000).flat_map(|i| format!("<vertex x=\"{}.5\" y=\"2\" z=\"{}\"/>\n", i % 97, i % 13).into_bytes()).collect();
        let z = deflate(&text);
        assert!(z.len() * 4 < text.len(), "{} -> {}", text.len(), z.len());
        assert_eq!(inflate(&z), text);
        let mut x = 12345u32;
        let noise: Vec<u8> = (0..5000).map(|_| { x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223); (x >> 24) as u8 }).collect();
        assert_eq!(inflate(&deflate(&noise)), noise);
        assert_eq!(inflate(&deflate(&[])), Vec::<u8>::new());
        assert_eq!(inflate(&deflate(&[7])), vec![7]);
        let zeros = vec![0u8; 100_000];
        let z = deflate(&zeros);
        assert!(z.len() < 1000);
        assert_eq!(inflate(&z), zeros);
    }
}
