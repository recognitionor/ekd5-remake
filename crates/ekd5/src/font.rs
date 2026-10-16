//! 글꼴 (원작 FUN_00411782 / FUN_00411730).
//! 한글·전각(EUC-KR a1a1..aefe, b0a1..c8fe): ALL_FONT.16P, 30바이트 항목 (u16 코드 + 14행 × 16비트 BE).
//!   색인 = (상위 − (상위 < b0 ? a1 : a4)) × 94 + (하위 − a1)
//! 반각(0x20..0x7f): ANKFONT.DAT, 글자당 16바이트 (8×16).

pub struct Font {
    pub all: Vec<u8>,
    pub ank: Vec<u8>,
}

pub const ROW_H: i32 = 16;

impl Font {
    /// (폭, 높이, 비트 함수) — 전각 16×16 칸(글자 14행), 반각 8×16
    pub fn glyph(&self, code: u16) -> Option<(i32, Vec<u16>)> {
        let (hi, lo) = ((code >> 8) as u32, (code & 0xFF) as u32);
        if code >= 0x100 {
            let ok = (0xA1A1..=0xAEFE).contains(&(code as u32)) || (0xB0A1..=0xC8FE).contains(&(code as u32));
            if !ok || lo < 0xA1 {
                return None;
            }
            let idx = (hi - if hi < 0xB0 { 0xA1 } else { 0xA4 }) * 94 + (lo - 0xA1);
            let o = idx as usize * 30 + 2;
            let g = self.all.get(o..o + 28)?;
            Some((16, (0..14).map(|r| u16::from_be_bytes([g[r * 2], g[r * 2 + 1]])).collect()))
        } else {
            let i = if (0x20..0x80).contains(&code) { code as usize - 0x20 } else { 0 };
            let g = self.ank.get(i * 16..i * 16 + 16)?;
            Some((8, g.iter().map(|&b| (b as u16) << 8).collect()))
        }
    }
}

/// EUC-KR 바이트열 → 글자 코드들
pub fn codes(b: &[u8]) -> Vec<u16> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] >= 0x80 && i + 1 < b.len() {
            out.push(((b[i] as u16) << 8) | b[i + 1] as u16);
            i += 2;
        } else {
            out.push(b[i] as u16);
            i += 1;
        }
    }
    out
}
