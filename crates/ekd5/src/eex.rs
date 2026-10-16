//! EEX 스크립트 (원작 FUN_00412643 / FUN_00412471 / FUN_00412978 / FUN_00412dbf).
//! 머리: "EEX\0" 판(1) 하위(2) + 덩어리(u16 종류, u16 길이, 자료)… 종류 0 = 끝.
//! 이어서 u32 절 오프셋 표 (개수 = 첫 오프셋−머리끝 / 4). 절: u16 블록 수, 블록 = u16 길이 + [머리 조건 명령…op0] + u16 본문 길이 + 본문.
//! 본문: 명령…op0. op1(IF) 뒤에 조건 명령 하나, 이어서 u16 길이 + 중첩 본문.
//! 명령 = u16 번호 + 인자들. 인자 = u16 표지 + 값 (표지 4 = u32, 5 = C 문자열, 그 외 u16). 표지 1 = 표지 없는 u16.

#[derive(Debug, Clone)]
pub struct Script {
    pub data: Vec<u8>,
    pub sections: Vec<usize>,
}

pub fn parse(d: Vec<u8>) -> Option<Script> {
    if d.get(..4)? != b"EEX\0" {
        return None;
    }
    let mut p = 6;
    loop {
        let t = u16::from_le_bytes(d.get(p..p + 2)?.try_into().ok()?);
        let l = u16::from_le_bytes(d.get(p + 2..p + 4)?.try_into().ok()?) as usize;
        p += 4 + l;
        if t == 0 {
            break;
        }
    }
    let first = u32::from_le_bytes(d.get(p..p + 4)?.try_into().ok()?) as usize;
    let n = first.checked_sub(p)? / 4;
    let sections = (0..n).map(|i| u32::from_le_bytes(d[p + i * 4..p + i * 4 + 4].try_into().unwrap()) as usize).collect();
    Some(Script { data: d, sections })
}
