//! EEX 실행기 (원작 FUN_00412471 블록 선택 / FUN_00412978 본문 실행 / FUN_0041a951 인자 읽기).
//! 명령 실행 자체는 엔진이 하고, 여기서는 위치·중첩·인자 해석만 맡는다.

use crate::eex::Script;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub enum Arg {
    U16(u16),
    U32(u32),
    Str(Vec<u8>),
}

impl Arg {
    pub fn i(&self) -> i32 {
        match self {
            Arg::U16(v) => *v as i32,
            Arg::U32(v) => *v as i32,
            Arg::Str(_) => 0,
        }
    }
    /// 0xFFFF(생략) → None
    pub fn opt(&self) -> Option<i32> {
        match self {
            Arg::U16(0xFFFF) => None,
            Arg::U32(0xFFFF_FFFF) => None,
            a => Some(a.i()),
        }
    }
    pub fn bytes(&self) -> &[u8] {
        match self {
            Arg::Str(s) => s,
            _ => &[],
        }
    }
}

#[derive(Debug)]
pub struct VmError(pub String);

pub struct Vm {
    pub script: Rc<Script>,
    pub pc: usize,
    pub section: usize,
    /// 중첩 본문 끝 위치들
    pub ends: Vec<usize>,
    pub running: bool,
}

/// 명령별 인자 표지 (원작 각 처리 함수의 FUN_0041a951 호출 순서). 1 = 표지 없는 u16.
/// 특수: 5(개수+목록 두 번), 0x46/0x47(부대 목록)은 직접 처리.
pub fn signature(op: u16) -> Option<&'static [u16]> {
    Some(match op {
        0 | 1 | 3 | 7 | 0xA | 0xC | 0xD | 0xE | 0x10 | 0x1C | 0x1D | 0x1E | 0x2F | 0x42 | 0x43 | 0x44 | 0x49 | 0x51 | 0x54 | 0x5A | 0x5F | 0x61 | 0x6A => &[],
        2 | 0x14 | 0x16 | 0x17 | 0x18 | 0x19 | 0x1A | 0x69 => &[5],
        4 => &[0x26],
        6 => &[0x2E, 4, 0x38, 0x38, 0x38, 0x38, 0x38, 0x39, 0x39, 0x39, 0x39, 0x39],
        8 => &[0x2E],
        9 | 0x13 => &[4],
        0xB => &[4, 0x27],
        0xF => &[0x12],
        0x11 => &[0x37],
        0x12 => &[5, 2],
        0x15 => &[2, 2, 5],
        0x1B => &[2, 0x27],
        0x1F => &[4, 4],
        0x20 => &[0x4A],
        0x21 => &[4, 4, 0x10, 0x26, 0x26],
        0x22 => &[0x1B],
        0x23 => &[0x1E, 4],
        0x24 => &[9],
        0x25 | 0x29 | 0x2A => &[2, 4, 4],
        0x26 => &[2, 4, 4, 4, 4],
        0x27 => &[0x2D, 0xC, 0x1A, 0x1C, 0x15],
        0x28 => &[0x36],
        0x2B | 0x2D | 0x5C => &[2],
        0x2C => &[5, 0x26, 0x26, 0x26],
        0x2E => &[2, 2, 0x26],
        0x30 => &[2, 4, 4, 0x2B, 0xD],
        0x31 => &[0x2C, 2, 4, 4, 4, 4, 3],
        0x32 | 0x55 => &[0x40, 2, 4, 4, 4, 0x2B],
        0x33 => &[2, 0xD, 0x2B],
        0x34 => &[2, 0xD],
        0x35 => &[2, 0x13],
        0x36 => &[2, 0x23, 4, 0x24],
        0x37 => &[0x28, 4, 0x24],
        0x38 => &[2, 0x23, 0x34, 4],
        0x39 => &[2, 4],
        0x3A => &[0x28, 0x34, 4],
        0x3B => &[2, 0xE, 0x3E],
        0x3C => &[2, 0xE, 0x3A],
        0x3D => &[0x17, 0x49, 0x26, 2],
        0x3E | 0x48 => &[2, 0x3B, 0x49, 0x3C, 0x49, 0x3D],
        0x3F => &[4, 0x24],
        0x40 => &[0x48],
        0x41 => &[3, 4, 0x24, 0x3F, 4, 4, 4, 4],
        0x45 => &[0x26, 0x26, 4, 0x3E, 0x32, 2, 0x32, 2, 0x47, 0x22],
        0x4A => &[4, 0x38, 0x38, 0x38, 0x38, 0x38, 0x39, 0x39, 0x39, 0x39, 0x39],
        0x4B => &[4, 4, 4, 0x2B, 0x26],
        0x4C => &[0x40, 2, 4],
        0x4D => &[0x41, 2, 4, 4, 4, 4, 4, 3, 0x2F, 0x18, 0x30, 4, 4],
        0x4E => &[0x2C, 2, 4, 4, 4, 4, 3, 7, 2, 4, 4],
        0x4F => &[2, 2, 0x2B, 0x26, 0x26, 0x26],
        0x50 => &[2, 0x46, 0x26, 0x26],
        0x52 => &[2, 3],
        0x53 => &[0x2C, 2, 4, 4, 4, 4, 3, 0x26],
        0x56 => &[0x47],
        0x57 => &[0x22],
        0x58 => &[0x42, 0x43, 0x44, 4, 4, 0x26, 0x26],
        0x59 => &[4, 0x17, 0x49, 0x17, 0x49, 0x17, 0x49, 0x26],
        0x5B => &[4, 4, 4, 4, 0x26],
        0x5D => &[0x34, 4],
        0x5E | 0x68 => &[2, 2],
        0x60 => &[0x26, 5, 0x4C],
        0x62 => &[0x26],
        0x63 => &[0x26, 5, 0x26],
        0x64 => &[0x26, 0x4C],
        0x65 => &[0x26, 0x4D, 0x26],
        0x66 => &[0x26, 0x4E, 0x26],
        0x67 => &[4, 5],
        0x6B => &[4, 4, 0x4B, 0x26],
        _ => return None,
    })
}

/// 부대 목록 (op46 / op47) 한 줄의 표지
pub const UNITS46: [u16; 11] = [2, 0x26, 4, 4, 0x2B, 0x3E, 0x45, 7, 2, 4, 4];
pub const UNITS47: [u16; 12] = [2, 0x26, 0x26, 4, 4, 0x2B, 0x3E, 0x45, 7, 2, 4, 4];

impl Vm {
    pub fn new(script: Rc<Script>) -> Self {
        Self { script, pc: 0, section: 0, ends: Vec::new(), running: false }
    }

    fn d(&self) -> &[u8] {
        &self.script.data
    }

    pub fn u16_at(&self, p: usize) -> Result<u16, VmError> {
        self.d().get(p..p + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(|| VmError(format!("범위 밖 {p:#x}")))
    }

    pub fn read_u16(&mut self) -> Result<u16, VmError> {
        let v = self.u16_at(self.pc)?;
        self.pc += 2;
        Ok(v)
    }

    pub fn arg(&mut self, tag: u16) -> Result<Arg, VmError> {
        if tag == 1 {
            return Ok(Arg::U16(self.read_u16()?));
        }
        let at = self.pc;
        let t = self.read_u16()?;
        if t != tag {
            return Err(VmError(format!("인자 표지 {t:#x} ≠ {tag:#x} @{at:#x}")));
        }
        Ok(match tag {
            4 => {
                let lo = self.read_u16()? as u32;
                let hi = self.read_u16()? as u32;
                Arg::U32(lo | hi << 16)
            }
            5 => {
                let d = self.d();
                let e = d[self.pc..].iter().position(|&b| b == 0).ok_or_else(|| VmError("문자열 끝 없음".into()))?;
                let s = d[self.pc..self.pc + e].to_vec();
                self.pc += e + 1;
                Arg::Str(s)
            }
            _ => Arg::U16(self.read_u16()?),
        })
    }

    /// 명령 하나의 번호와 인자 (op5, op46/47 포함)
    pub fn fetch(&mut self) -> Result<(u16, Vec<Arg>), VmError> {
        let at = self.pc;
        let op = self.read_u16()?;
        let mut args = Vec::new();
        match op {
            5 => {
                for _ in 0..2 {
                    let n = self.arg(0x35)?;
                    let k = n.i();
                    args.push(n);
                    for _ in 0..k {
                        args.push(self.arg(1)?);
                    }
                }
            }
            0x46 | 0x47 => {
                let rec: &[u16] = if op == 0x46 { &UNITS46 } else { &UNITS47 };
                while self.u16_at(self.pc)? == 2 {
                    for &t in rec {
                        args.push(self.arg(t)?);
                    }
                }
            }
            _ => {
                let sig = signature(op).ok_or_else(|| VmError(format!("모르는 명령 {op:#x} @{at:#x}")))?;
                for &t in sig {
                    args.push(self.arg(t)?);
                }
            }
        }
        Ok((op, args))
    }

    /// 다음 명령 번호 (읽지 않음)
    pub fn peek_op(&self) -> u16 {
        self.u16_at(self.pc).unwrap_or(0)
    }

    pub fn sections(&self) -> usize {
        self.script.sections.len()
    }

    /// 절 시작: 블록 목록 (블록마다 (머리 시작, 본문 길이 위치, 블록 끝))
    pub fn blocks(&self, section: usize) -> Result<Vec<(usize, usize)>, VmError> {
        let mut p = *self.script.sections.get(section).ok_or_else(|| VmError("절 없음".into()))?;
        let n = self.u16_at(p)?;
        p += 2;
        let mut out = Vec::new();
        for _ in 0..n {
            let len = self.u16_at(p)? as usize;
            out.push((p + 2, p + 2 + len));
            p += 2 + len;
        }
        Ok(out)
    }

    /// 조건부 본문 건너뛰기/들어가기: pc는 u16 길이 위치
    pub fn enter_body(&mut self, run: bool) -> Result<(), VmError> {
        let len = self.read_u16()? as usize;
        if run {
            self.ends.push(self.pc + len);
        } else {
            self.pc += len;
        }
        Ok(())
    }
}
