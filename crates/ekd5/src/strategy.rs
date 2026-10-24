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

/// 효과 종류 (원작 FUN_00427cbf 의 책략별 갈래 → FUN_00424b40 의 결과 코드)
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
    /// MP 주기: 그 책략의 MP 만큼 (원작 FUN_0043e658 0x2c/0x2d)
    MpGift,
    /// 능력 상태 (0 공격(책사 계열은 정신) 1 방어 3 순발 4 사기 5 이동, 올림?)
    Buff(u8, bool),
    /// 상태이상 (비트) + 피해 위력
    Status(u8, u32),
    /// 상태이상 치료
    Cure,
    /// 공격·방어·순발·사기 모두 (올림?, 이동도?) — 패기·팔진도·현무 (원작 0x6f / 0xaf)
    BuffAll(bool, bool),
    /// HP 다 채움 — 팔진도·현무 (원작 FUN_0043e658 0x26/0x42: 최대 HP − HP)
    HealFull,
    /// 한 번 더 행동 (원작 57 회귀, 0xb8 → FUN_00406a10(부대, 6))
    ReAct,
    /// 날씨 바꾸기 (원작 FUN_004261c7): 0 맑게 1 흐리게 2 비
    Weather(u8),
    /// HP 를 MP 로 (원작 62 명상, FUN_00426404)
    Meditate,
    /// 현무: 맵의 모든 부대에 무작위 효과 (FUN_00426ea8)
    Genbu,
    /// 백호: 모든 아군 큰 회복 + 치료 (FUN_00426bab)
    Byakko,
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
        1 | 2 | 6 | 7 | 0xB | 0xD | 0xE | 0x10 | 0x16 | 0x17 | 0x19 | 0x21 | 0x2F | 0x32 | 0x34 => 90,
        3 | 8 | 0x12 | 0x18 | 0x1A | 0x1B | 0x1D | 0x1F | 0x22 | 0x23 | 0x25 | 0x36 | 0x38 => 80,
        4 | 0x14 | 0x1C | 0x1E | 0x24 => 60,
        0x20 => 50,
        _ => 100,
    }
}

/// 성공률이 늘 100 인 책략 (원작 FUN_0043d84f: 팔진도·회복·조언·강행·분기·견고·회귀·백호·각성)
pub fn rate_always(id: u8) -> bool {
    matches!(id, 0x26..=0x2E | 0x30 | 0x35 | 0x37 | 0x39 | 0x43)
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
        // 내림 (FUN_0042334a): 둔병 순발, 허탈 사기, 압박 공격(책사는 정신), 욕설 방어
        23 | 24 => Effect::Buff(3, false),
        25 | 26 => Effect::Buff(4, false),
        27 | 28 => Effect::Buff(0, false),
        29 | 30 => Effect::Buff(1, false),
        39 | 41 => Effect::Heal(false),
        40 | 42 | 43 => Effect::Heal(true),
        44 | 45 => Effect::MpGift,
        46 | 47 => Effect::Cure,
        // 올림: 강행 이동, 연병 순발, 고양 사기, 분기·기합 공격(책사는 정신), 견고 방어
        48 => Effect::Buff(5, true),
        49 | 50 => Effect::Buff(3, true),
        51 | 52 => Effect::Buff(4, true),
        53 | 54 | 61 => Effect::Buff(0, true),
        55 | 56 => Effect::Buff(1, true),
        57 => Effect::ReAct,
        58 => Effect::Weather(2),
        59 => Effect::Weather(0),
        60 => Effect::Weather(1),
        62 => Effect::Meditate,
        63 => Effect::BuffAll(true, false),
        66 => Effect::Genbu,
        67 => Effect::Byakko,
        _ => Effect::None,
    }
}

/// 책략 결과 연출 (원작 FUN_00424b40: 결과 코드 → MEFF). 실패 45
pub fn result_fx(e: Effect) -> Option<u8> {
    match e {
        Effect::Buff(k, false) => Some([39, 40, 39, 41, 42, 0][k as usize]).filter(|&v| v != 0),
        Effect::Buff(k, true) => Some([34, 35, 34, 36, 37, 46][k as usize]),
        Effect::BuffAll(false, _) => Some(43),
        Effect::BuffAll(true, _) => Some(38),
        Effect::Status(b, _) => match b {
            status::CONFUSE => Some(48),
            status::POISON => Some(49),
            status::PARALYZE => Some(50),
            _ => Some(51),
        },
        Effect::Cure | Effect::ReAct => Some(44),
        _ => None,
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
