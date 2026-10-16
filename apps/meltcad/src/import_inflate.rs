//! Reading compressed data: a DEFLATE decoder (RFC 1951: stored, fixed and dynamic Huffman blocks),
//! the zlib wrapper (RFC 1950) and a small ZIP reader (stored and deflated entries). Everything is
//! written for hostile input: every read is bounds checked, the output size is capped (so a "zip
//! bomb" fails instead of eating the memory) and nothing panics.

use crate::export_formats::crc32;

/// Why a stream could not be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InflateError {
    /// The data ended in the middle of the stream.
    Truncated,
    /// The stream is broken (the text says where).
    Corrupt(&'static str),
    /// The result would be bigger than the allowed size.
    TooBig,
}

impl std::fmt::Display for InflateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InflateError::Truncated => write!(f, "圧縮データが途中で切れています"),
            InflateError::Corrupt(why) => write!(f, "圧縮データがこわれています（{why}）"),
            InflateError::TooBig => write!(f, "展開すると大きすぎます"),
        }
    }
}

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] =
    [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
const CL_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// The bits of the input, least significant first, in a 64-bit buffer. Past the end it feeds zeros
/// and remembers it, so that a truncated stream is reported once the zeros have been used.
struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    acc: u64,
    n: u32,
    padded: usize,
}

impl<'a> Bits<'a> {
    fn new(d: &'a [u8]) -> Bits<'a> {
        Bits { d, pos: 0, acc: 0, n: 0, padded: 0 }
    }
    fn refill(&mut self) {
        while self.n <= 56 {
            let b = match self.d.get(self.pos) {
                Some(b) => {
                    self.pos += 1;
                    *b
                }
                None => {
                    self.padded += 1;
                    0
                }
            };
            self.acc |= u64::from(b) << self.n;
            self.n += 8;
        }
    }
    /// True when more bits were used than the input holds.
    fn overrun(&self) -> bool {
        // zero bytes loaded past the end minus the bits still unused
        self.padded * 8 > self.n as usize
    }
    fn take(&mut self, count: u32) -> u32 {
        if self.n < count {
            self.refill();
        }
        let v = (self.acc & ((1u64 << count) - 1)) as u32;
        self.acc >>= count;
        self.n -= count;
        v
    }
    /// Drop the bits up to the next byte boundary and hand the whole unused input bytes back.
    fn align_bytes(&mut self) {
        let drop = self.n % 8;
        self.acc >>= drop;
        self.n -= drop;
        // the last `padded` buffered bytes are zeros made up past the end of the input
        let real_unused = ((self.n / 8) as usize).saturating_sub(self.padded);
        self.pos -= real_unused;
        self.acc = 0;
        self.n = 0;
        self.padded = 0;
    }
}

/// A canonical Huffman code as a lookup table indexed by the next `bits` input bits.
struct Huff {
    table: Vec<u16>,
    bits: u32,
}

impl Huff {
    /// Entry: `symbol << 4 | length`; length 0 marks a bit pattern no code starts with.
    fn build(lengths: &[u8]) -> Result<Huff, InflateError> {
        let mut count = [0u32; 16];
        let mut max = 0usize;
        for &l in lengths {
            let l = usize::from(l);
            if l > 15 {
                return Err(InflateError::Corrupt("符号の長さ"));
            }
            if let Some(c) = count.get_mut(l) {
                *c += 1;
            }
            if l > max {
                max = l;
            }
        }
        if let Some(c) = count.get_mut(0) {
            *c = 0;
        }
        let mut left: i64 = 1;
        for c in count.iter().skip(1) {
            left = (left << 1) - i64::from(*c);
            if left < 0 {
                return Err(InflateError::Corrupt("符号が多すぎます"));
            }
        }
        let bits = max.max(1) as u32;
        let mut table = vec![0u16; 1usize << bits];
        let mut next = [0u32; 17];
        let mut code = 0u32;
        for l in 1..=15usize {
            code = (code + count.get(l - 1).copied().unwrap_or(0)) << 1;
            if let Some(n) = next.get_mut(l) {
                *n = code;
            }
        }
        for (sym, &l) in lengths.iter().enumerate() {
            if l == 0 {
                continue;
            }
            let l = u32::from(l);
            let Some(slot) = next.get_mut(l as usize) else { continue };
            let c = *slot;
            *slot += 1;
            let mut rev = 0u32;
            for i in 0..l {
                rev |= ((c >> i) & 1) << (l - 1 - i);
            }
            let entry = ((sym as u16) << 4) | l as u16;
            let mut idx = rev as usize;
            while idx < table.len() {
                if let Some(t) = table.get_mut(idx) {
                    *t = entry;
                }
                idx += 1usize << l;
            }
        }
        Ok(Huff { table, bits })
    }
    fn decode(&self, b: &mut Bits) -> Result<usize, InflateError> {
        if b.n < self.bits {
            b.refill();
        }
        let e = self.table.get((b.acc & ((1u64 << self.bits) - 1)) as usize).copied().unwrap_or(0);
        let len = u32::from(e & 15);
        if len == 0 {
            return Err(InflateError::Corrupt("符号"));
        }
        b.acc >>= len;
        b.n -= len;
        Ok(usize::from(e >> 4))
    }
}

fn fixed_tables() -> Result<(Huff, Huff), InflateError> {
    let mut l = [0u8; 288];
    for (i, v) in l.iter_mut().enumerate() {
        *v = match i {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    Ok((Huff::build(&l)?, Huff::build(&[5u8; 30])?))
}

fn dynamic_tables(b: &mut Bits) -> Result<(Huff, Huff), InflateError> {
    let hlit = b.take(5) as usize + 257;
    let hdist = b.take(5) as usize + 1;
    let hclen = b.take(4) as usize + 4;
    if hlit > 286 || hdist > 30 {
        return Err(InflateError::Corrupt("符号の数"));
    }
    let mut cl = [0u8; 19];
    for &o in CL_ORDER.iter().take(hclen) {
        if let Some(v) = cl.get_mut(o) {
            *v = b.take(3) as u8;
        }
    }
    let clh = Huff::build(&cl)?;
    let mut lens = [0u8; 320];
    let total = hlit + hdist;
    let mut i = 0usize;
    while i < total {
        if b.overrun() {
            return Err(InflateError::Truncated);
        }
        let sym = clh.decode(b)?;
        let (value, rep) = match sym {
            0..=15 => (sym as u8, 1usize),
            16 => {
                let prev = if i > 0 { lens.get(i - 1).copied().unwrap_or(0) } else { return Err(InflateError::Corrupt("くり返し")) };
                (prev, 3 + b.take(2) as usize)
            }
            17 => (0, 3 + b.take(3) as usize),
            _ => (0, 11 + b.take(7) as usize),
        };
        if i + rep > total {
            return Err(InflateError::Corrupt("くり返し"));
        }
        for k in 0..rep {
            if let Some(v) = lens.get_mut(i + k) {
                *v = value;
            }
        }
        i += rep;
    }
    if lens.get(256).copied().unwrap_or(0) == 0 {
        return Err(InflateError::Corrupt("終わりの符号がありません"));
    }
    let lit = Huff::build(lens.get(..hlit).ok_or(InflateError::Corrupt("符号"))?)?;
    let dist = Huff::build(lens.get(hlit..total).ok_or(InflateError::Corrupt("符号"))?)?;
    Ok((lit, dist))
}

/// Decode a raw DEFLATE stream. The result may not be longer than `max_out` bytes.
pub fn inflate(data: &[u8], max_out: usize) -> Result<Vec<u8>, InflateError> {
    inflate_counted(data, max_out).map(|(v, _)| v)
}

/// Like `inflate`, and also says how many input bytes the stream used.
pub fn inflate_counted(data: &[u8], max_out: usize) -> Result<(Vec<u8>, usize), InflateError> {
    let mut b = Bits::new(data);
    let mut out: Vec<u8> = Vec::with_capacity(data.len().saturating_mul(3).min(max_out).min(1 << 26));
    let mut fixed: Option<(Huff, Huff)> = None;
    loop {
        let last = b.take(1);
        let kind = b.take(2);
        if b.overrun() {
            return Err(InflateError::Truncated);
        }
        match kind {
            0 => {
                b.align_bytes();
                let len = data.get(b.pos..b.pos + 4).ok_or(InflateError::Truncated)?;
                let l = usize::from(u16::from_le_bytes([len[0], len[1]]));
                let nl = usize::from(u16::from_le_bytes([len[2], len[3]]));
                if l != (!nl & 0xFFFF) {
                    return Err(InflateError::Corrupt("ブロックの長さ"));
                }
                let start = b.pos + 4;
                let body = data.get(start..start + l).ok_or(InflateError::Truncated)?;
                if out.len() + l > max_out {
                    return Err(InflateError::TooBig);
                }
                out.extend_from_slice(body);
                b.pos = start + l;
            }
            1 | 2 => {
                let dynamic;
                let (lit, dist) = if kind == 1 {
                    if fixed.is_none() {
                        fixed = Some(fixed_tables()?);
                    }
                    match fixed.as_ref() {
                        Some(t) => (&t.0, &t.1),
                        None => return Err(InflateError::Corrupt("固定の符号")),
                    }
                } else {
                    dynamic = dynamic_tables(&mut b)?;
                    (&dynamic.0, &dynamic.1)
                };
                loop {
                    let sym = lit.decode(&mut b)?;
                    if b.overrun() {
                        return Err(InflateError::Truncated);
                    }
                    if sym < 256 {
                        if out.len() >= max_out {
                            return Err(InflateError::TooBig);
                        }
                        out.push(sym as u8);
                    } else if sym == 256 {
                        break;
                    } else {
                        let k = sym - 257;
                        let (Some(&base), Some(&extra)) = (LEN_BASE.get(k), LEN_EXTRA.get(k)) else {
                            return Err(InflateError::Corrupt("長さの符号"));
                        };
                        let len = usize::from(base) + b.take(u32::from(extra)) as usize;
                        let ds = dist.decode(&mut b)?;
                        let (Some(&dbase), Some(&dextra)) = (DIST_BASE.get(ds), DIST_EXTRA.get(ds)) else {
                            return Err(InflateError::Corrupt("距離の符号"));
                        };
                        let d = usize::from(dbase) + b.take(u32::from(dextra)) as usize;
                        if d > out.len() {
                            return Err(InflateError::Corrupt("距離が先頭を越えています"));
                        }
                        if out.len() + len > max_out {
                            return Err(InflateError::TooBig);
                        }
                        let start = out.len() - d;
                        if d >= len {
                            out.extend_from_within(start..start + len);
                        } else {
                            for i in 0..len {
                                let v = out.get(start + i).copied().unwrap_or(0);
                                out.push(v);
                            }
                        }
                    }
                }
            }
            _ => return Err(InflateError::Corrupt("ブロックの種類")),
        }
        if last == 1 {
            break;
        }
    }
    // whole bytes of the buffer that were not used belong to whatever follows the stream
    let unused = (b.n / 8) as usize;
    let used = b.pos.saturating_sub(unused.saturating_sub(b.padded));
    Ok((out, used.min(data.len())))
}

/// Adler-32 (the checksum of the zlib wrapper).
#[allow(dead_code)] // part of the zlib wrapper; used by the tests
pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut s) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += u32::from(x);
            s += a;
        }
        a %= 65521;
        s %= 65521;
    }
    (s << 16) | a
}

/// Decode a zlib stream (2-byte header, DEFLATE, Adler-32).
#[allow(dead_code)] // the zlib wrapper (GLB images, PNG); used by the tests
pub fn inflate_zlib(data: &[u8], max_out: usize) -> Result<Vec<u8>, InflateError> {
    let (Some(&cmf), Some(&flg)) = (data.first(), data.get(1)) else { return Err(InflateError::Truncated) };
    if cmf & 15 != 8 || (u32::from(cmf) * 256 + u32::from(flg)) % 31 != 0 || flg & 0x20 != 0 {
        return Err(InflateError::Corrupt("zlib のヘッダー"));
    }
    let body = data.get(2..).ok_or(InflateError::Truncated)?;
    let (out, used) = inflate_counted(body, max_out)?;
    let tail = body.get(used..used + 4).ok_or(InflateError::Truncated)?;
    if u32::from_be_bytes([tail[0], tail[1], tail[2], tail[3]]) != adler32(&out) {
        return Err(InflateError::Corrupt("チェックサム"));
    }
    Ok(out)
}

// ---------------------------------------------------------------- ZIP

/// More entries than this are refused.
pub const MAX_ZIP_ENTRIES: usize = 20_000;

/// One file of a ZIP archive.
#[derive(Clone, Debug)]
pub struct ZipEntry {
    pub name: String,
    method: u16,
    flags: u16,
    crc: u32,
    csize: usize,
    usize_: usize,
    local: usize,
}

impl ZipEntry {
    /// The size of the file once unpacked, as the archive says.
    pub fn size(&self) -> usize {
        self.usize_
    }
}

/// A ZIP archive held in memory (the central directory is read, the files are unpacked on demand).
pub struct Zip<'a> {
    data: &'a [u8],
    pub entries: Vec<ZipEntry>,
}

fn u16at(d: &[u8], o: usize) -> Option<u16> {
    let b = d.get(o..o.checked_add(2)?)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

fn u32at(d: &[u8], o: usize) -> Option<u32> {
    let b = d.get(o..o.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

const BAD_ZIP: &str = "ZIP ファイルがこわれています";

impl<'a> Zip<'a> {
    pub fn open(data: &'a [u8]) -> Result<Zip<'a>, String> {
        if data.len() < 22 {
            return Err(BAD_ZIP.into());
        }
        // the end-of-central-directory record is within the last 64 KB + 22 bytes
        let lowest = data.len().saturating_sub(22 + 65535);
        let mut eocd = None;
        let mut o = data.len() - 22;
        loop {
            if data.get(o..o + 4) == Some(b"PK\x05\x06") {
                eocd = Some(o);
                break;
            }
            if o <= lowest {
                break;
            }
            o -= 1;
        }
        let eocd = eocd.ok_or_else(|| BAD_ZIP.to_string())?;
        let count = usize::from(u16at(data, eocd + 10).ok_or_else(|| BAD_ZIP.to_string())?);
        let cd_size = u32at(data, eocd + 12).ok_or_else(|| BAD_ZIP.to_string())? as usize;
        let cd_off = u32at(data, eocd + 16).ok_or_else(|| BAD_ZIP.to_string())? as usize;
        if count == 0xFFFF || cd_size == 0xFFFF_FFFF || cd_off == 0xFFFF_FFFF {
            return Err("ZIP64 の大きなファイルには対応していません".into());
        }
        if count > MAX_ZIP_ENTRIES {
            return Err("ZIP の中のファイルが多すぎます".into());
        }
        let mut entries = Vec::with_capacity(count);
        let mut off = cd_off;
        for _ in 0..count {
            if u32at(data, off) != Some(0x0201_4b50) {
                return Err(BAD_ZIP.into());
            }
            let get = |o: usize| u16at(data, off + o);
            let get32 = |o: usize| u32at(data, off + o);
            let (Some(flags), Some(method), Some(crc), Some(cs), Some(us), Some(nl), Some(xl), Some(cl), Some(local)) =
                (get(8), get(10), get32(16), get32(20), get32(24), get(28), get(30), get(32), get32(42))
            else {
                return Err(BAD_ZIP.into());
            };
            let (nl, xl, cl) = (usize::from(nl), usize::from(xl), usize::from(cl));
            let name_bytes = data.get(off + 46..off + 46 + nl).ok_or_else(|| BAD_ZIP.to_string())?;
            let name = String::from_utf8_lossy(name_bytes).into_owned();
            entries.push(ZipEntry { name, method, flags, crc, csize: cs as usize, usize_: us as usize, local: local as usize });
            off = off.checked_add(46 + nl + xl + cl).ok_or_else(|| BAD_ZIP.to_string())?;
        }
        Ok(Zip { data, entries })
    }

    /// The entry with this name (ignoring case and a leading slash).
    pub fn find(&self, name: &str) -> Option<&ZipEntry> {
        let want = name.trim_start_matches('/');
        self.entries.iter().find(|e| e.name.trim_start_matches('/').eq_ignore_ascii_case(want))
    }

    /// Unpack one file; it may not be bigger than `max` bytes.
    pub fn read(&self, e: &ZipEntry, max: usize) -> Result<Vec<u8>, String> {
        if e.flags & 1 != 0 {
            return Err("パスワードつきの ZIP には対応していません".into());
        }
        if e.usize_ > max {
            return Err(InflateError::TooBig.to_string());
        }
        let d = self.data;
        if u32at(d, e.local) != Some(0x0403_4b50) {
            return Err(BAD_ZIP.into());
        }
        let nl = usize::from(u16at(d, e.local + 26).ok_or_else(|| BAD_ZIP.to_string())?);
        let xl = usize::from(u16at(d, e.local + 28).ok_or_else(|| BAD_ZIP.to_string())?);
        let start = e.local.checked_add(30 + nl + xl).ok_or_else(|| BAD_ZIP.to_string())?;
        let end = start.checked_add(e.csize).ok_or_else(|| BAD_ZIP.to_string())?;
        let body = d.get(start..end).ok_or_else(|| InflateError::Truncated.to_string())?;
        let out = match e.method {
            0 => {
                if e.csize != e.usize_ {
                    return Err(BAD_ZIP.into());
                }
                body.to_vec()
            }
            8 => inflate(body, e.usize_).map_err(|x| x.to_string())?,
            _ => return Err("この圧縮方法の ZIP には対応していません".into()),
        };
        if out.len() != e.usize_ {
            return Err(BAD_ZIP.into());
        }
        if crc32(&out) != e.crc {
            return Err(format!("{BAD_ZIP}（チェックサム）"));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export_deflate::deflate;

    fn noise(n: usize, seed: u32) -> Vec<u8> {
        let mut x = seed;
        (0..n)
            .map(|_| {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (x >> 24) as u8
            })
            .collect()
    }

    #[test]
    fn inflate_reads_what_our_deflate_writes() {
        let text: Vec<u8> = (0..30000).flat_map(|i| format!("<vertex x=\"{}.5\" y=\"2\" z=\"{}\"/>\n", i % 97, i % 13).into_bytes()).collect();
        assert_eq!(inflate(&deflate(&text), 10_000_000).unwrap(), text);
        let n = noise(7000, 5);
        assert_eq!(inflate(&deflate(&n), 10_000).unwrap(), n);
        assert_eq!(inflate(&deflate(&[]), 10).unwrap(), Vec::<u8>::new());
        assert_eq!(inflate(&deflate(&[9]), 10).unwrap(), vec![9]);
        let zeros = vec![0u8; 300_000];
        assert_eq!(inflate(&deflate(&zeros), 300_000).unwrap(), zeros);
    }

    #[test]
    fn stored_blocks_work_and_are_checked() {
        // a stored block "abc" (final), then a broken length
        let ok = [1u8, 3, 0, 0xFC, 0xFF, b'a', b'b', b'c'];
        assert_eq!(inflate(&ok, 10).unwrap(), b"abc");
        let bad = [1u8, 3, 0, 0xFD, 0xFF, b'a', b'b', b'c'];
        assert!(matches!(inflate(&bad, 10), Err(InflateError::Corrupt(_))));
        let short = [1u8, 3, 0, 0xFC, 0xFF, b'a'];
        assert_eq!(inflate(&short, 10), Err(InflateError::Truncated));
        // two stored blocks
        let two = [0u8, 2, 0, 0xFD, 0xFF, b'a', b'b', 1, 1, 0, 0xFE, 0xFF, b'c'];
        assert_eq!(inflate(&two, 10).unwrap(), b"abc");
        assert_eq!(inflate(&two, 2), Err(InflateError::TooBig));
    }

    /// `zlib.compress(data, 9)` from python3 (a dynamic Huffman block) for [`py_text`].
    const PY_ZLIB: &[u8] = &[
        0x78, 0xda, 0x3d, 0xd5, 0xdb, 0x8d, 0x23, 0x31, 0x0c, 0x44, 0xd1, 0xff, 0x8d, 0x82, 0x21, 0x50, 0x0f, 0x52, 0x52, 0x62, 0x13, 0xff, 0xb6, 0xa8,
        0x5b, 0xc6, 0x7c, 0xf4, 0xc0, 0x36, 0x59, 0x50, 0xe3, 0xa0, 0xf4, 0x67, 0x7e, 0xff, 0xfe, 0xfd, 0x59, 0xb3, 0x61, 0xf1, 0x3d, 0xbb, 0xa5, 0xb5,
        0xfb, 0xc1, 0xb0, 0x63, 0xfd, 0x7b, 0xce, 0xef, 0xab, 0xf5, 0x3d, 0xe3, 0xfe, 0x77, 0x3f, 0x48, 0x5b, 0x36, 0xbf, 0xa7, 0x7f, 0xbf, 0xb3, 0x53,
        0xa3, 0xdd, 0x5a, 0x8d, 0x86, 0x65, 0x4d, 0x6e, 0x6b, 0xad, 0x46, 0xdd, 0x46, 0x8d, 0x7e, 0x1f, 0xd5, 0x64, 0x56, 0xd6, 0x37, 0x56, 0x59, 0xed,
        0xfe, 0x79, 0x4d, 0xce, 0xca, 0x1a, 0xdf, 0xea, 0xf5, 0x32, 0xfd, 0x85, 0xc5, 0xfd, 0xae, 0x46, 0xa3, 0xb2, 0xfc, 0xee, 0xae, 0x51, 0xaf, 0xac,
        0xfe, 0xcd, 0x54, 0xd6, 0xf8, 0x7e, 0x32, 0x6a, 0xf4, 0x54, 0x56, 0xdc, 0x9f, 0xd4, 0xe0, 0xac, 0x2c, 0xff, 0x56, 0xb7, 0x77, 0xd0, 0x6f, 0x77,
        0xaf, 0xd1, 0x5e, 0x61, 0xe3, 0xfe, 0xf6, 0x9d, 0x74, 0x57, 0x56, 0xd8, 0x3b, 0x57, 0xde, 0xdd, 0x35, 0x9a, 0x95, 0xd5, 0xbe, 0xd5, 0xed, 0x1d,
        0xb4, 0x55, 0xd6, 0xb8, 0x23, 0x35, 0xb8, 0x2a, 0x2b, 0xee, 0xea, 0xa8, 0xc9, 0xfe, 0xc2, 0xfc, 0x1e, 0xa0, 0x46, 0x77, 0x65, 0x75, 0xe3, 0x5c,
        0xe3, 0x0e, 0xd7, 0x68, 0x56, 0x56, 0xdc, 0xdd, 0x35, 0xd9, 0x2a, 0xcb, 0xef, 0x3b, 0x78, 0x07, 0x5d, 0x95, 0x75, 0x17, 0x56, 0xd8, 0xb8, 0x4b,
        0x6a, 0x32, 0x2a, 0x2b, 0xee, 0x1b, 0x79, 0x27, 0x7d, 0xe7, 0xf2, 0xfb, 0x1a, 0x6b, 0x32, 0x5f, 0x56, 0xff, 0x76, 0xcf, 0x9a, 0x6c, 0x95, 0x35,
        0xef, 0xee, 0x9a, 0x5c, 0x95, 0x95, 0xf5, 0xb6, 0xdf, 0x49, 0x7b, 0x85, 0xb5, 0xbb, 0xb5, 0x26, 0xb7, 0x3d, 0x0b, 0xef, 0x5c, 0xf3, 0xee, 0x78,
        0x27, 0x4d, 0x7b, 0x16, 0x4e, 0x65, 0xf9, 0x7d, 0xad, 0xef, 0xa0, 0xb3, 0xb2, 0xbe, 0x57, 0x5b, 0x59, 0xa3, 0x76, 0xd7, 0x68, 0xaf, 0xb0, 0xb8,
        0x6f, 0xe9, 0x9d, 0x74, 0x57, 0xd6, 0x05, 0xb8, 0x01, 0xe8, 0x00, 0x0c, 0xfc, 0x35, 0x07, 0x60, 0x07, 0xe0, 0xc2, 0x5f, 0xeb, 0x02, 0x38, 0x01,
        0x78, 0x00, 0xd8, 0x00, 0x98, 0xf8, 0x6b, 0x0d, 0x80, 0x03, 0x80, 0x1b, 0x80, 0x58, 0x00, 0xfd, 0x14, 0xfa, 0x25, 0xf4, 0x8e, 0xfa, 0x2e, 0xf5,
        0x81, 0xfa, 0x0d, 0x7a, 0x07, 0xfd, 0x00, 0x7d, 0x0a, 0xfd, 0x01, 0x7d, 0x03, 0xfd, 0x04, 0xfd, 0x12, 0x7a, 0x97, 0xfa, 0x8e, 0xfa, 0x40, 0xfd,
        0x16, 0x7a, 0x07, 0xfd, 0x00, 0x7d, 0x82, 0xfe, 0x80, 0xbe, 0x09, 0xfd, 0x04, 0xfd, 0x12, 0x7a, 0x47, 0x7d, 0x47, 0x7d, 0x48, 0xfd, 0x06, 0xbd,
        0x83, 0x7e, 0x08, 0x7d, 0x82, 0xfe, 0x80, 0xbe, 0x81, 0x7e, 0x82, 0x7e, 0xfd, 0xd0, 0x3b, 0xea, 0x3b, 0xea, 0x03, 0xf5, 0x1b, 0xf4, 0x2e, 0xf4,
        0x03, 0xf4, 0x09, 0xfa, 0x23, 0xf4, 0x0d, 0xf4, 0x13, 0xf4, 0x4b, 0xe8, 0x1d, 0xf5, 0x5d, 0xea, 0x03, 0xf5, 0x1b, 0xf4, 0x0e, 0xfa, 0x01, 0xfa,
        0x14, 0xfa, 0x03, 0xfa, 0x06, 0xfa, 0x29, 0xf4, 0x4b, 0xe8, 0x1d, 0xf5, 0x1d, 0xf5, 0x81, 0xfa, 0x2d, 0xf4, 0x0e, 0xfa, 0x01, 0xfa, 0x34, 0x15,
        0x60, 0xe0, 0x0f, 0xf4, 0xd3, 0x54, 0x80, 0x4b, 0xfe, 0x50, 0xdf, 0x51, 0x1f, 0xa8, 0xdf, 0xa6, 0x02, 0x4c, 0xfc, 0x81, 0x3e, 0x41, 0x7f, 0x40,
        0xdf, 0x4c, 0x05, 0x18, 0xf8, 0x13, 0x7a, 0x37, 0x15, 0xe0, 0xc2, 0x1f, 0xea, 0x37, 0xe8, 0x1d, 0xf4, 0xc3, 0x54, 0x80, 0x89, 0x3f, 0xd0, 0x37,
        0xd0, 0x4f, 0xd0, 0x2f, 0xfb, 0x15, 0x60, 0xe0, 0x0f, 0xf5, 0x61, 0x2a, 0xc0, 0x85, 0x3f, 0xd0, 0x0f, 0xd0, 0x27, 0xe8, 0x8f, 0xa9, 0x00, 0x13,
        0x7f, 0xa0, 0x5f, 0x42, 0xef, 0xa8, 0xef, 0xa6, 0x02, 0x0c, 0xfc, 0x81, 0xde, 0x4d, 0x05, 0xb8, 0xf0, 0x07, 0xfa, 0x03, 0xfa, 0x06, 0xfa, 0x69,
        0x2a, 0xc0, 0x94, 0x3f, 0xd4, 0x77, 0xd4, 0x07, 0xea, 0xb7, 0xa9, 0x00, 0x03, 0x7f, 0xa0, 0x4f, 0x53, 0x01, 0x2e, 0xfc, 0x81, 0x7e, 0x82, 0x7e,
        0x09, 0xbd, 0x9b, 0x0a, 0x30, 0xf1, 0x87, 0xfa, 0x0d, 0x7a, 0x07, 0xfd, 0x30, 0x15, 0x60, 0xe0, 0x0f, 0xf4, 0xcd, 0x54, 0x80, 0x0b, 0x7f, 0x42,
        0xef, 0xa8, 0xef, 0xa8, 0x0f, 0x53, 0x01, 0x26, 0xfe, 0x40, 0x3f, 0x40, 0x9f, 0xa0, 0x3f, 0xa6, 0x02, 0x0c, 0xdd, 0xc0, 0x0e, 0xc0, 0x5f, 0x01,
        0x2e, 0xdd, 0xc0, 0x1d, 0x80, 0x13, 0x80, 0x47, 0x0d, 0x08, 0xc0, 0xc4, 0x1f, 0xe8, 0x0f, 0xe8, 0x1b, 0xe8, 0xa7, 0xa9, 0x00, 0xe3, 0x77, 0x03,
        0x3b, 0x00, 0x55, 0x80, 0x4b, 0x37, 0x70, 0x07, 0xe0, 0x04, 0xe0, 0x51, 0x03, 0x02, 0x30, 0xf1, 0x07, 0xfa, 0x09, 0xfa, 0x25, 0xf4, 0x6e, 0x2a,
        0xc0, 0xd0, 0x0d, 0xec, 0x00, 0x54, 0x01, 0x2e, 0xdd, 0xc0, 0x1d, 0x80, 0x13, 0x80, 0x47, 0x0d, 0x08, 0xc0, 0xc4, 0x9f, 0xd0, 0x3b, 0xea, 0x3b,
        0xea, 0xc3, 0x54, 0x80, 0xa1, 0x1b, 0xd8, 0x01, 0xa8, 0x02, 0x5c, 0xba, 0x81, 0x3b, 0x00, 0x27, 0x00, 0x8f, 0x1a, 0x50, 0x00, 0x13, 0x7f, 0xa8,
        0x0f, 0xd4, 0x6f, 0xd0, 0xbb, 0xa9, 0x00, 0x43, 0x37, 0xb0, 0x03, 0x50, 0x05, 0xb8, 0x74, 0x03, 0x77, 0x00, 0x4e, 0x01, 0x3c, 0x6a, 0x40, 0x00,
        0x26, 0xfe, 0x40, 0xef, 0xa0, 0x1f, 0xa0, 0xbf, 0x2d, 0xf5, 0x1f, 0x40, 0xcd, 0xce, 0x49,
    ];

    fn py_text() -> Vec<u8> {
        (0..300).flat_map(|i: u32| format!("v {} {} {}\n", i % 7, (i * 3) % 11, (i * 5) % 13).into_bytes()).collect()
    }

    #[test]
    fn dynamic_huffman_stream_from_python_zlib() {
        assert_eq!(PY_ZLIB.get(2).map(|b| (b >> 1) & 3), Some(2), "a dynamic block");
        let want = py_text();
        assert_eq!(inflate_zlib(PY_ZLIB, 1 << 20).unwrap(), want);
        // the raw stream without the 2-byte header and the checksum
        assert_eq!(inflate(&PY_ZLIB[2..PY_ZLIB.len() - 4], 1 << 20).unwrap(), want);
        assert_eq!(inflate_counted(&PY_ZLIB[2..], 1 << 20).unwrap().1, PY_ZLIB.len() - 6);
        assert_eq!(inflate(&PY_ZLIB[2..PY_ZLIB.len() - 4], 100), Err(InflateError::TooBig));
    }

    /// Asks python3 (if it is installed) for streams of several levels and shapes, and reads them.
    #[test]
    #[ignore = "needs python3"]
    fn python_zlib_streams_of_every_kind() {
        let script = "import sys,zlib,struct\n\
            data=b''.join(b'v %d %d %d\\n'%(i%97,(i*7)%113,(i*31)%1009) for i in range(20000))\n\
            import random\nr=random.Random(5)\nnoise=bytes(r.randrange(256) for _ in range(30000))\n\
            for lvl in (0,1,6,9):\n\
            \tfor d in (data,noise,b'',b'a'*100000):\n\
            \t\tz=zlib.compress(d,lvl)\n\
            \t\tsys.stdout.buffer.write(struct.pack('<II',len(d),len(z))+zlib.crc32(d).to_bytes(4,'little')+z)\n";
        let out = std::process::Command::new("python3").args(["-I", "-c", script]).output().expect("python3");
        assert!(out.status.success());
        let o = out.stdout;
        let mut pos = 0;
        let mut n = 0;
        while pos < o.len() {
            let ul = u32::from_le_bytes([o[pos], o[pos + 1], o[pos + 2], o[pos + 3]]) as usize;
            let zl = u32::from_le_bytes([o[pos + 4], o[pos + 5], o[pos + 6], o[pos + 7]]) as usize;
            let crc = u32::from_le_bytes([o[pos + 8], o[pos + 9], o[pos + 10], o[pos + 11]]);
            let z = &o[pos + 12..pos + 12 + zl];
            let d = inflate_zlib(z, ul).unwrap();
            assert_eq!(d.len(), ul);
            assert_eq!(crc32(&d), crc);
            pos += 12 + zl;
            n += 1;
        }
        assert_eq!(n, 16);
    }

    #[test]
    fn truncated_and_garbage_streams_never_panic() {
        let text: Vec<u8> = (0..4000).flat_map(|i| format!("v {} {} {}\n", i % 7, i % 11, i % 13).into_bytes()).collect();
        let z = deflate(&text);
        for cut in 0..z.len().min(300) {
            let _ = inflate(&z[..cut], 1 << 20);
        }
        assert_eq!(inflate(&z[..z.len() - 1], 1 << 20), Err(InflateError::Truncated));
        let mut x = 99u32;
        for _ in 0..2000 {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let len = (x >> 24) as usize;
            let junk = noise(len, x);
            let _ = inflate(&junk, 1 << 16);
            let _ = inflate_zlib(&junk, 1 << 16);
        }
        // flip every byte of a good stream
        for i in 0..z.len().min(400) {
            let mut c = z.clone();
            c[i] ^= 0xA5;
            let _ = inflate(&c, 1 << 20);
        }
    }

    #[test]
    fn a_bomb_stops_at_the_limit() {
        let zeros = vec![0u8; 5_000_000];
        let z = deflate(&zeros);
        assert!(z.len() < 40_000);
        assert_eq!(inflate(&z, 1_000_000), Err(InflateError::TooBig));
        assert_eq!(inflate(&z, 5_000_000).unwrap().len(), 5_000_000);
    }

    #[test]
    fn zlib_wrapper_checks_header_and_checksum() {
        // zlib.compress(b"hello hello hello hello")
        let z = [0x78u8, 0x9c, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x27, 0x01, 0x68, 0x03, 0x08, 0xb1];
        assert_eq!(inflate_zlib(&z, 100).unwrap(), b"hello hello hello hello");
        let mut bad = z;
        bad[z.len() - 1] ^= 1;
        assert!(inflate_zlib(&bad, 100).is_err());
        assert!(inflate_zlib(&[0x78], 100).is_err());
        assert!(inflate_zlib(&[0x00, 0x00, 1], 100).is_err());
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn zip_reads_what_the_3mf_writer_writes_and_rejects_junk() {
        let big: Vec<u8> = (0..20000).flat_map(|i| format!("<vertex x=\"{}\"/>\n", i % 50).into_bytes()).collect();
        let files: Vec<(&str, &[u8])> = vec![("a.txt", b"hello"), ("dir/big.xml", &big)];
        let z = testutil::zip(&files, true);
        let zip = Zip::open(&z).unwrap();
        assert_eq!(zip.entries.len(), 2);
        let a = zip.find("A.TXT").unwrap();
        assert_eq!(zip.read(a, 100).unwrap(), b"hello");
        let b = zip.find("/dir/big.xml").unwrap();
        assert_eq!(zip.read(b, 1 << 20).unwrap(), big);
        assert!(zip.read(b, 1000).is_err());
        assert!(zip.find("nope").is_none());
        // truncations and bit flips never panic
        for cut in 0..z.len() {
            if let Ok(zp) = Zip::open(&z[..cut]) {
                for e in &zp.entries {
                    let _ = zp.read(e, 1 << 20);
                }
            }
        }
        for i in (0..z.len()).step_by(7) {
            let mut c = z.clone();
            c[i] ^= 0x5A;
            if let Ok(zp) = Zip::open(&c) {
                for e in &zp.entries {
                    let _ = zp.read(e, 1 << 20);
                }
            }
        }
    }

    #[test]
    fn zip_with_huge_declared_sizes_is_refused() {
        let z = testutil::zip(&[("a", b"hello")], false);
        let zip = Zip::open(&z).unwrap();
        let mut e = zip.entries[0].clone();
        e.usize_ = usize::MAX / 2;
        assert!(zip.read(&e, 1 << 20).is_err());
        e.usize_ = 5;
        e.local = usize::MAX - 3;
        assert!(zip.read(&e, 1 << 20).is_err());
    }
}

/// Helpers for the tests of this module and of the readers that sit on top of it.
#[cfg(test)]
pub mod testutil {
    use crate::export_formats::crc32;

    /// A ZIP archive with the given files; `deflated` packs every file with our own compressor.
    pub fn zip(files: &[(&str, &[u8])], deflated: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data) in files {
            let offset = out.len() as u32;
            let packed = crate::export_deflate::deflate(data);
            let (method, body): (u16, &[u8]) = if deflated { (8, &packed) } else { (0, data) };
            let mut head = Vec::new();
            head.extend_from_slice(&[20, 0, 0, 0]);
            head.extend_from_slice(&method.to_le_bytes());
            head.extend_from_slice(&[0, 0, 0x21, 0]);
            head.extend_from_slice(&crc32(data).to_le_bytes());
            head.extend_from_slice(&(body.len() as u32).to_le_bytes());
            head.extend_from_slice(&(data.len() as u32).to_le_bytes());
            head.extend_from_slice(&(name.len() as u16).to_le_bytes());
            head.extend_from_slice(&[0, 0]);
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&head);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(body);
            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&[20, 0]);
            central.extend_from_slice(&head);
            central.extend_from_slice(&[0; 10]);
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        let cd_size = central.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }
}
