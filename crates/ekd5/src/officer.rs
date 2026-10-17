//! 장수 능력 (원작 장수 기록 0x48바이트 @0x4ac2e0, 초기화 FUN_00406f?? / 레벨업 FUN_00407758 / 장비 보정 FUN_00407ffe).
//! DATA.E5[0] 32바이트: 0..12 이름, 13-14 얼굴, 15 그림, 18..22 능력 5 (공격·방어·정신·순발·사기),
//! 23-24 최대 HP, 25 최대 MP, 26 직업, 27 레벨, 28 경험치, 29 무기, 30 방어구, 31 보조 (0xFF 없음).
//! DATA.E5[1] 아이템 25바이트: 0..16 이름, +17 종류, +18 특수 종류, +19 가격, +20 그림, +21 기본치, +22 특수치, +23 성장치, +24 구분.
//! DATA.E5[3] 직업 27바이트: +0 이동력, +1 공격 범위 종류, +2..6 능력 성장 등급, +7 HP 등급, +8 MP 등급, +9.. 장비 가능.

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Officer {
    pub person: usize,
    pub job: u8,
    pub level: u8,
    pub exp: u8,
    /// 공격·방어·정신·순발·사기
    pub stats: [u16; 5],
    /// 처음 능력 (성장 등급 계산용, 원작 +0x21..)
    pub base: [u8; 5],
    pub max_hp: u16,
    pub max_mp: u16,
    /// 무기, 방어구 (아이템, 레벨), 보조
    pub weapon: Option<(u8, u8)>,
    pub armor: Option<(u8, u8)>,
    pub acc: Option<u8>,
    /// 무기·방어구 경험치 (원작 +0x30, +0x33)
    pub gear_exp: [u8; 2],
}

/// 아이템 능력 종류 (원작 FUN_00406bba): 0 공격 1 방어 2 정신 3 순발 4 사기 5 HP 6 MP 7 이동 8 기타
pub fn item_stat(items: &[u8], id: u8) -> Option<u8> {
    let t = *items.get(id as usize * 25 + 17)?;
    Some(match t {
        0..=9 | 0x18 => 0,
        0xA..=0xD | 0x19 => 2,
        0xE..=0x11 | 0x1A => 1,
        0x1B => 3,
        0x1C => 4,
        0x1D => 5,
        0x1E => 6,
        0x20 => 7,
        0x1F => 8,
        _ => return None,
    })
}

pub fn item_name(items: &[u8], id: u8) -> String {
    let o = id as usize * 25;
    let Some(r) = items.get(o..o + 17) else { return String::new() };
    let e = r.iter().position(|&b| b == 0).unwrap_or(17);
    encoding_rs::EUC_KR.decode(&r[..e]).0.to_string()
}

impl Officer {
    pub fn from_data(person: usize, r: &[u8]) -> Self {
        let g = |i: usize| r.get(i).copied().unwrap_or(0);
        let item = |v: u8| if v == 0xFF { None } else { Some((v, 1)) };
        let base = [g(18), g(19), g(20), g(21), g(22)];
        Officer {
            person,
            job: g(26),
            level: g(27).max(1),
            exp: g(28).min(99),
            stats: base.map(|v| v as u16),
            base,
            max_hp: (g(23) as u16 | (g(24) as u16) << 8).min(500),
            max_mp: (g(25) as u16).min(250),
            weapon: item(g(29)),
            armor: item(g(30)),
            acc: if g(31) == 0xFF { None } else { Some(g(31)) },
            gear_exp: [0; 2],
        }
    }

    /// 처음 능력 등급 (원작 FUN_0040769c): <25 1, <35 2, <45 3, 그 외 4
    fn base_grade(v: u8) -> u16 {
        match v {
            0..=24 => 1,
            25..=34 => 2,
            35..=44 => 3,
            _ => 4,
        }
    }

    /// n 레벨 올리기 (원작 FUN_00407758, 최대 50)
    pub fn level_up(&mut self, jobs: &[u8], n: u8) {
        let n = n.min(50u8.saturating_sub(self.level));
        let jt = |i: usize| jobs.get(self.job as usize * 27 + 2 + i).copied().unwrap_or(0) as u16;
        for _ in 0..n {
            for i in 0..5 {
                let g = (jt(i) + Self::base_grade(self.base[i])) >> 1;
                self.stats[i] = (self.stats[i] + g).min(255);
            }
            self.max_hp = (self.max_hp + jt(5)).min(500);
            self.max_mp = (self.max_mp + jt(6)).min(250);
        }
        self.level += n;
    }

    /// 레벨을 target 으로 (내려가지는 않음)
    pub fn set_level(&mut self, jobs: &[u8], target: u8) {
        if target > self.level {
            self.level_up(jobs, target - self.level);
        }
    }

    /// 장비 보정 (원작 FUN_00407ffe): 무기·방어구는 기본치 + 성장치×(레벨−1), 보조는 기본치
    pub fn equip_bonus(&self, items: &[u8], stat: u8) -> u16 {
        let rec = |id: u8, i: usize| items.get(id as usize * 25 + i).copied().unwrap_or(0) as u16;
        let mut b = 0;
        for (id, lv) in [self.weapon, self.armor].into_iter().flatten() {
            if item_stat(items, id) == Some(stat) {
                b += rec(id, 21) + rec(id, 23) * (lv.max(1) as u16 - 1);
            }
        }
        if let Some(id) = self.acc {
            if item_stat(items, id) == Some(stat) {
                b += rec(id, 21);
            }
        }
        b
    }

    /// 실제 능력치 (상한 400, 사기·순발 300)
    pub fn value(&self, items: &[u8], stat: u8) -> u16 {
        let cap = if stat >= 3 { 300 } else { 400 };
        (self.stats[stat as usize] + self.equip_bonus(items, stat)).min(cap)
    }

    /// 경험치 더하기 (원작 FUN_00408d42: 100 마다 레벨업, 주요 장수(0..25)만). 반환: 오른 레벨 수
    pub fn gain_exp(&mut self, jobs: &[u8], e: u32) -> u8 {
        if self.person >= 0x1A || self.level >= 50 {
            return 0;
        }
        let mut total = self.exp as u32 + e;
        let mut ups = 0;
        while total >= 100 && self.level < 50 {
            total -= 100;
            self.level_up(jobs, 1);
            ups += 1;
        }
        self.exp = if self.level >= 50 { 0 } else { total as u8 };
        ups
    }

    /// 장비 경험치 기본치 (원작 FUN_0040765d): 보조 장비 '기타' 값, 최소 10
    pub fn gear_base(&self, items: &[u8]) -> u32 {
        (self.equip_bonus(items, 8) as u32).max(10)
    }

    /// 장비 최대 레벨 (원작 FUN_004086c6 / FUN_0040cbf1): 무기·방어구 중 종류가 홀수면 9, 아니면 3
    pub fn gear_max(items: &[u8], id: u8) -> u8 {
        let k = items.get(id as usize * 25 + 17).copied().unwrap_or(0);
        if k < 0x12 && k & 1 == 1 { 9 } else { 3 }
    }

    /// 무기(0)·방어구(1) 경험치 더하기 (원작 FUN_004086c6: 100 마다 레벨업). 반환: 레벨이 올랐나
    pub fn gain_gear_exp(&mut self, items: &[u8], slot: usize, e: u32) -> bool {
        let g = if slot == 0 { &mut self.weapon } else { &mut self.armor };
        let Some((id, lv)) = g.as_mut() else { return false };
        let cap = Self::gear_max(items, *id);
        if *lv >= cap || e == 0 {
            return false;
        }
        let mut total = self.gear_exp[slot] as u32 + e;
        let mut up = false;
        while total >= 100 && *lv < cap {
            total -= 100;
            *lv += 1;
            up = true;
        }
        self.gear_exp[slot] = if *lv >= cap { 0 } else { total as u8 };
        up
    }

    /// 레벨에 맞는 기본 장비 (원작 FUN_004083d7): 직업이 쓸 수 있는 첫 무기 계열(직업 +9..+22) / 방어구 계열(+23..+26) 에서
    /// 등급 = min(레벨/12, 2), 아이템 레벨 = 레벨 < 36 이면 (레벨%12)/4+1, 아니면 3. 무기 = 계열/2×3+등급, 방어구 = 0x26 + 같은 식
    pub fn auto_equip(&mut self, jobs: &[u8], slot: u8) {
        let tier = (self.level / 12).min(2);
        let lv = if self.level < 36 { (self.level % 12) / 4 + 1 } else { 3 };
        let can = |k: usize| jobs.get(self.job as usize * 27 + 9 + k).copied().unwrap_or(0) != 0;
        let (range, base) = if slot == 0 { (0..14, 0u8) } else { (14..18, 0x26u8) };
        let start = range.start;
        if let Some(k) = range.clone().find(|&k| can(k)) {
            let id = base + ((k - start) / 2) as u8 * 3 + tier;
            let g = Some((id, lv));
            if slot == 0 {
                self.weapon = g;
            } else {
                self.armor = g;
            }
            self.gear_exp[slot as usize] = 0;
        }
    }

    /// 장비 특수 효과 (원작 FUN_00407df9 / FUN_00407ee2): 무기·방어구는 특수 무기(종류 홀수, 0x12 미만)일 때
    /// +18 특수 종류가 code 면 +22 특수치, 보조는 +17 종류가 code 면 +21 기본치. 없으면 None
    pub fn special(&self, items: &[u8], code: u8) -> Option<u8> {
        let rec = |id: u8| items.get(id as usize * 25..id as usize * 25 + 25);
        let mut v = None;
        for (id, _) in [self.weapon, self.armor].into_iter().flatten() {
            if let Some(r) = rec(id) {
                if r[17] < 0x12 && r[17] & 1 == 1 && r[18] == code {
                    v = Some(r[22]);
                }
            }
        }
        if let Some(r) = self.acc.and_then(rec) {
            if r[17] == code {
                v = Some(r[21]);
            }
        }
        v
    }
}
