//! 책략 (DATA.E5[5], 70바이트 × 73 — 원작 0x4b9c38).
//! 0..11 이름, +12 대상 (0 적, 1 아군, 2 날씨), +13 사거리 종류 (공격 범위 표와 같음), +14 효과 범위 (0 한 칸, 1 십자, 2 3×3),
//! +15 MP, +16 계통, +17+직업 = 그 직업이 배우는 레벨 (0 = 못 배움).
//! 피해 (원작 FUN_0043e0ba): (레벨+25 ± 정신력 차/3) × 위력/10 ± 난수. 회복 (FUN_0043e658): 정신력/10+40, 대회복 정신력/2+70.

#[derive(Clone, Debug)]
pub struct Strategy {
    pub id: u8,
    pub name: String,
    pub target: u8,
    pub range: u8,
    pub area: u8,
    pub mp: u8,
    pub cat: u8,
    pub learn: Vec<u8>,
}

/// 효과 종류
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// 피해 (위력 ×10%)
    Damage(u32),
    /// 대상 HP 전부 (원작 9 해일 / 21 현혹 — 성공률이 낮음)
    Instant,
    /// MP 피해 (원작 22 첩보)
    MpDamage(u32),
    /// 회복: 작은(정신력/10+40) / 큰(정신력/2+70)
    Heal(bool),
    /// MP 회복
    Mp(u32),
    /// 능력 상태 바꾸기 (능력 번호, 올림?)
    Buff(u8, bool),
    /// 상태이상 (비트) + 피해 위력
    Status(u8, u32),
    /// 상태이상 치료
    Cure,
    /// 공격·방어·순발·사기 모두 (올림?) — 팔진도 결과 (원작 0x6f / 0xaf)
    BuffAll(bool),
    /// HP 다 채움 — 팔진도 결과 (원작 FUN_0043e658 0x26: 최대 HP − HP)
    HealFull,
    None,
}

/// 상태이상 비트 (원작 FUN_00422aba / FUN_0043e8e6)
pub mod status {
    /// 포박 (못 움직임)
    pub const PARALYZE: u8 = 2;
    /// 봉책 (책략 못 씀)
    pub const SEAL: u8 = 4;
    /// 혼란 (행동 못함)
    pub const CONFUSE: u8 = 8;
    /// 독 (차례마다 HP 감소)
    pub const POISON: u8 = 0x10;
}

/// 책략 성공률 배율 % (원작 FUN_0043d84f 의 switch)
pub fn rate_scale(id: u8) -> u32 {
    match id {
        1 | 2 | 6 | 7 | 0xB | 0xD | 0xE | 0x10 | 0x16 | 0x17 | 0x19 | 0x21 | 0x32 | 0x34 => 90,
        3 | 8 | 0x12 | 0x18 | 0x1A | 0x1B | 0x1D | 0x1F | 0x22 | 0x23 | 0x25 | 0x36 | 0x38 => 80,
        4 | 0x14 | 0x1C | 0x1E | 0x24 => 60,
        0x20 => 50,
        _ => 100,
    }
}

/// 계통별 지형 제한 (원작 FUN_00422aba: 불 0-4, 물 5-9, 바람 10-14, 땅 15-19)
pub fn terrain_ok(id: u8, t: u8) -> bool {
    match id {
        0..=4 => matches!(t, 0 | 1 | 2 | 8 | 0x10 | 0x12..=0x18),
        5..=9 => (7..=10).contains(&t) || t == 0xD,
        10..=14 => matches!(t, 0 | 1 | 3 | 4 | 7 | 8 | 9 | 10 | 0xD),
        15..=19 => (3..=4).contains(&t),
        _ => true,
    }
}

/// 피해 책략 위력 (원작 FUN_0043e0ba 의 switch)
pub fn power(id: u8) -> u32 {
    match id {
        0 | 3 | 5 | 8 | 0xB | 0xF | 0x11 | 0x14 | 0x21 | 0x40 => 7,
        1 | 4 | 6 | 0x10 | 0x12 => 9,
        2 | 7 | 0xA | 0xD | 0x16 | 0x23 => 5,
        0xC | 0x22 => 4,
        0xE | 0x24 => 2,
        0x13 | 0x26 => 12,
        0x41 => 10,
        _ => 0,
    }
}

pub fn effect(id: u8) -> Effect {
    use status::*;
    match id {
        9 | 21 => Effect::Instant,
        0..=20 | 38 | 64 | 65 => Effect::Damage(power(id)),
        22 => Effect::MpDamage(power(id)),
        31 | 32 => Effect::Status(CONFUSE, 0),
        33 | 34 => Effect::Status(POISON, power(id)),
        35 | 36 => Effect::Status(PARALYZE, power(id)),
        37 => Effect::Status(SEAL, 0),
        23 | 24 => Effect::Buff(3, false),
        25 | 26 => Effect::Buff(4, false),
        27 | 28 => Effect::Buff(0, false),
        29 | 30 => Effect::Buff(1, false),
        39 | 41 | 61 => Effect::Heal(false),
        40 | 42 | 43 | 67 => Effect::Heal(true),
        44 | 62 => Effect::Mp(20),
        45 => Effect::Mp(40),
        46 | 47 => Effect::Cure,
        48 => Effect::Buff(3, true),
        49 | 50 | 53 | 54 | 63 => Effect::Buff(0, true),
        51 | 52 => Effect::Buff(4, true),
        55 | 56 | 66 => Effect::Buff(1, true),
        _ => Effect::None,
    }
}

pub fn parse(d: &[u8]) -> Vec<Strategy> {
    d.chunks(70)
        .enumerate()
        .take(68)
        .map(|(i, r)| {
            let e = r[..12].iter().position(|&b| b == 0).unwrap_or(12);
            Strategy {
                id: i as u8,
                name: encoding_rs::EUC_KR.decode(&r[..e]).0.to_string(),
                target: r[12],
                range: r[13],
                area: r[14],
                mp: r[15],
                cat: r[16],
                learn: r[17..70].to_vec(),
            }
        })
        .collect()
}

impl Strategy {
    /// 이 직업·레벨에서 쓸 수 있나
    pub fn known(&self, job: u8, level: u8) -> bool {
        let l = self.learn.get(job as usize).copied().unwrap_or(0);
        l != 0 && l <= level && effect(self.id) != Effect::None
    }

    /// 효과 범위 칸 (대상 칸 기준)
    pub fn area_offsets(&self) -> Vec<(i32, i32)> {
        match self.area {
            0 => vec![(0, 0)],
            1 => vec![(0, 0), (0, -1), (0, 1), (-1, 0), (1, 0)],
            _ => (-1..=1).flat_map(|y| (-1..=1).map(move |x| (x, y))).collect(),
        }
    }
}
