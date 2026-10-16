//! KOEI Ls12 묶음 (원작 FUN_004221f9 / FUN_00422147 / FUN_0042202a).
//! "Ls12" + 공백 12바이트, 0x10: 바이트 치환 사전 256, 0x110: 항목(BE u32 압축 크기, 원래 크기, 오프셋)… 압축 크기 0 = 끝.
//! 압축: MSB부터 읽는 비트열. 부호 = (1의 개수 n + 0)과 같은 길이 n+1 비트 → 값 = 앞부분 + 뒷부분.
//! 값 < 256 → 사전[값] 한 바이트, 아니면 거리 = 값−256, 길이 = 다음 부호 + 3 복사. 압축 크기 = 원래 크기면 무압축.

pub fn is_ls12(d: &[u8]) -> bool {
    d.len() >= 0x110 && &d[..4] == b"Ls12"
}

#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub packed: usize,
    pub size: usize,
    pub offset: usize,
}

fn be32(d: &[u8], p: usize) -> Option<usize> {
    Some(u32::from_be_bytes(d.get(p..p + 4)?.try_into().ok()?) as usize)
}

pub fn entries(d: &[u8]) -> Vec<Entry> {
    let mut out = Vec::new();
    if !is_ls12(d) {
        return out;
    }
    let mut p = 0x110;
    while let (Some(packed), Some(size), Some(offset)) = (be32(d, p), be32(d, p + 4), be32(d, p + 8)) {
        if packed == 0 {
            break;
        }
        out.push(Entry { packed, size, offset });
        p += 12;
    }
    out
}

struct Bits<'a> {
    d: &'a [u8],
    p: usize,
    b: u8,
}

impl Bits<'_> {
    fn bit(&mut self) -> u32 {
        let v = self.d.get(self.p).map_or(0, |&x| (x >> (7 - self.b)) & 1) as u32;
        self.b += 1;
        if self.b == 8 {
            self.b = 0;
            self.p += 1;
        }
        v
    }
    fn code(&mut self) -> u32 {
        let (mut c, mut n) = (0u32, 0);
        loop {
            let b = self.bit();
            c = (c << 1) | b;
            n += 1;
            if b == 0 || n > 24 {
                break;
            }
        }
        let mut c2 = 0;
        for _ in 0..n {
            c2 = (c2 << 1) | self.bit();
        }
        c + c2
    }
}

/// 항목 하나 풀기
pub fn unpack(d: &[u8], e: Entry) -> Option<Vec<u8>> {
    if e.packed == e.size {
        return d.get(e.offset..e.offset + e.size).map(|s| s.to_vec());
    }
    let dic = d.get(0x10..0x110)?;
    let mut bs = Bits { d, p: e.offset, b: 0 };
    let mut o = Vec::with_capacity(e.size);
    while o.len() < e.size {
        if bs.p > d.len() {
            return None;
        }
        let c = bs.code();
        if c < 256 {
            o.push(dic[c as usize]);
        } else {
            let dist = (c - 256) as usize;
            let len = bs.code() as usize + 3;
            if dist == 0 || dist > o.len() {
                return None;
            }
            for _ in 0..len {
                let v = o[o.len() - dist];
                o.push(v);
            }
        }
    }
    o.truncate(e.size);
    Some(o)
}

/// 묶음의 i번째 항목
pub fn get(d: &[u8], i: usize) -> Option<Vec<u8>> {
    unpack(d, *entries(d).get(i)?)
}
