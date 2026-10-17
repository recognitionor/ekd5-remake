//! 출진 장수 선택 (원작 대화상자 236 "장수 선택", FUN_004606b3 / FUN_0045e119 / FUN_0046087d).
//! 왼쪽 위: 후보 판 (U_SELECT.E5[3] 400×296) — 7열, 칸 56×72, 부대 그림 UNIT_MOV 6번 장, 레벨·이름.
//!   고른 장수는 반투명 (FUN_0043230a 11/6), 반드시 출진은 어둡게 (FUN_00432a30 6).
//!   오른쪽 아래 정렬 단추 둘 (300,244)/(300,266) 88×16: "부대속성순" / "<레벨순>"→공격력→정신력→방어력 (누를 때마다 바뀜).
//! 가운데: "출진장수  - n/max". 왼쪽 아래: 출진 판 (U_SELECT.E5[4] 400×128) — 8열 50×64, 자리 표시 U_SELECT[0..2]
//!   (반드시 출진 자리 2, 최소 인원(최대의 2/3)까지 1, 나머지 0).
//! 오른쪽: 장수 정보 (얼굴, 이름, 부대속성, Lv, 무력·지력·통솔력·민첩성·운, HP/MP/Exp/공격력/정신력/방어력/순발력/사기 막대, 이동력).
//! 결정: 고른 수가 최대의 2/3 이상일 때만 (FUN_0045e0ee). 취소: 고른 것 비움 (반드시 출진만 남김).
//! 원작은 9pt 윈도 글꼴이라, 16px 글꼴에 맞게 오른쪽 정보 칸 배치는 조금 다르다.

use super::camp::SortieRule;
use super::screen::{Pic, Screen};
use super::story::{draw_text, frame_box};
use super::{key, Assets, Engine, Pending};
use crate::palette::nearest;
use std::collections::HashMap;

/// 화면 위치 (대화상자 바탕 안)
const LIST: (i32, i32) = (12, 26);
const PICK: (i32, i32) = (12, 344);
const COUNT_Y: i32 = 326;
const TABS: [(i32, i32); 2] = [(300, 244), (300, 266)];
const OK_BTN: (i32, i32, i32, i32) = (436, 446, 88, 24);
const CANCEL_BTN: (i32, i32, i32, i32) = (532, 446, 88, 24);
const INFO_X: i32 = 420;

#[derive(Clone)]
pub struct SortieUi {
    /// 후보 (원작 순서: 반드시 출진 먼저)
    pub pids: Vec<usize>,
    /// 표시 순서 (후보 번호)
    order: Vec<usize>,
    pub chosen: Vec<usize>,
    pub rule: SortieRule,
    /// 반드시 출진 수 (후보 앞쪽)
    forced: usize,
    /// 정보 칸에 보이는 후보 / 키보드 커서 (표시 칸, 후보 수 = 결정, +1 = 취소)
    info: usize,
    cursor: usize,
    /// 정렬 (1 부대속성, 2 레벨, 3 공격력, 4 정신력, 5 방어력), 둘째 단추가 가리키는 정렬
    sort: u8,
    sort2: u8,
    bg: Option<Vec<Pic>>,
    sprites: HashMap<usize, Pic>,
    faces: HashMap<usize, Pic>,
    /// 본진에서 열었으면 출진 블록 (본문, 절)
    pub depart: Option<(usize, usize)>,
}

impl<A: Assets> Engine<A> {
    /// 출진 후보와 반드시 출진 수 (원작 FUN_0041aac4): 조조는 불가 목록에 없으면 반드시 출진,
    /// 반드시 출진 목록 중 합류한 장수, 그다음 불가가 아닌 합류 장수. 최대 = max(규칙 최대, 반드시 수)
    fn sortie_candidates(&self, rule: &SortieRule) -> (Vec<usize>, usize, usize) {
        let roster = if self.roster.is_empty() { vec![0] } else { self.roster.clone() };
        let mut pids = Vec::new();
        if !rule.excluded.contains(&0) && roster.contains(&0) {
            pids.push(0);
        }
        for &p in &rule.forced {
            if roster.contains(&p) && !pids.contains(&p) {
                pids.push(p);
            }
        }
        let forced = pids.len();
        for &p in &roster {
            if !pids.contains(&p) && !rule.excluded.contains(&p) {
                pids.push(p);
            }
        }
        (pids, forced, rule.max.max(forced))
    }

    /// 고르지 않고 정하기 (시험 자동 진행·화면 없이): 반드시 출진 + 앞에서부터 최대까지
    pub fn auto_sortie(&self, rule: &SortieRule) -> Vec<usize> {
        let (pids, _, max) = self.sortie_candidates(rule);
        pids.into_iter().take(max).collect()
    }

    /// 출진 고르기 열기. 고를 것이 없으면 (반드시 출진 = 최대) 바로 Some(출진 장수)
    /// depart = 본진 출진 블록 (본문, 절): 결정하면 실행, 취소하면 본진으로 (원작 FUN_0041aac4 가 거짓 → 본진 그대로)
    pub fn open_sortie(&mut self, rule: SortieRule, depart: Option<(usize, usize)>) -> Option<Vec<usize>> {
        let (pids, forced, max) = self.sortie_candidates(&rule);
        if forced >= max {
            return Some(pids[..forced].to_vec());
        }
        let chosen: Vec<usize> = pids[..forced].to_vec();
        let mut rule = rule;
        // 후보가 최대보다 적으면 후보 수가 최대 (FUN_0045e119)
        rule.max = max.min(pids.len()).min(15);
        let mut ui = SortieUi {
            order: (0..pids.len()).collect(),
            pids,
            chosen,
            rule,
            forced,
            info: 0,
            cursor: 0,
            sort: self.sortie_sort.0,
            sort2: self.sortie_sort.1,
            bg: None,
            sprites: HashMap::new(),
            faces: HashMap::new(),
            depart,
        };
        self.sort_sortie(&mut ui);
        self.sortie_ui = Some(ui);
        None
    }

    /// 정렬 (원작 FUN_0045fe9c → FUN_0045ddc3 삽입 정렬, 큰 값 먼저)
    fn sort_sortie(&mut self, ui: &mut SortieUi) {
        let keys: Vec<i32> = ui
            .pids
            .iter()
            .map(|&p| {
                let o = self.officer(p);
                match ui.sort {
                    // FUN_00407c62: 병과 표 (0x48F430) 앞쪽일수록 큼, 같은 병과는 직업 단계
                    1 => {
                        const ORDER: [u8; 27] = [25, 0, 18, 13, 3, 4, 1, 2, 7, 15, 14, 6, 12, 16, 17, 5, 11, 8, 10, 9, 19, 20, 23, 24, 21, 22, 26];
                        let g = if o.job < 0x27 { o.job / 3 } else if o.job < 0x35 { o.job - 0x1a } else { 0x1b };
                        let pos = ORDER.iter().position(|&x| x == g).unwrap_or(27) as i32;
                        (27 - pos) * 3 + if o.job < 0x27 { (o.job % 3) as i32 } else { 0 }
                    }
                    2 => o.level as i32,
                    3 => o.value(&self.items, 0) as i32,
                    4 => o.value(&self.items, 2) as i32,
                    5 => o.value(&self.items, 1) as i32,
                    _ => -(p as i32),
                }
            })
            .collect();
        let mut order: Vec<usize> = (0..ui.pids.len()).collect();
        for i in 1..order.len() {
            let mut j = i;
            while j > 0 && keys[order[j]] > keys[order[j - 1]] {
                order.swap(j, j - 1);
                j -= 1;
            }
        }
        ui.order = order;
    }

    /// 그림 읽기 (U_SELECT, 부대 그림, 얼굴). 아직이면 Err
    fn load_sortie(&mut self, ui: &mut SortieUi) -> Result<(), Pending> {
        if ui.bg.is_none() {
            let sizes = [(50, 50), (50, 50), (50, 50), (400, 296), (400, 128)];
            let mut v = Vec::new();
            for (i, (w, h)) in sizes.into_iter().enumerate() {
                v.push(Pic::new(w, h, self.packs.get(&mut self.assets, "U_SELECT.E5", i)?.unwrap_or_default()));
            }
            ui.bg = Some(v);
        }
        for &p in &ui.pids.clone() {
            if !ui.sprites.contains_key(&p) {
                let o = self.officer(p);
                let idx = super::battle::sprite_index(p, o.job, super::battle::Side::Player);
                let d = self.packs.get(&mut self.assets, "UNIT_MOV.E5", idx)?.unwrap_or_default();
                let frame = d.get(0x3600..0x3600 + 0x900).map(|f| f.to_vec()).unwrap_or_default();
                ui.sprites.insert(p, Pic::new(48, 48, frame));
            }
        }
        let p = ui.pids.get(ui.info).copied().unwrap_or(0);
        if !ui.faces.contains_key(&p) {
            let face = self.face_of(p);
            let d = self.packs.get(&mut self.assets, "FACE.E5", face as usize)?.unwrap_or_default();
            ui.faces.insert(p, Pic::new(64, 80, d));
        }
        Ok(())
    }

    fn can_depart(ui: &SortieUi) -> bool {
        ui.chosen.len() >= ui.rule.max * 2 / 3
    }

    fn toggle_sortie(&mut self, ui: &mut SortieUi, k: usize) {
        let p = ui.pids[k];
        ui.info = k;
        if k < ui.forced {
            return;
        }
        if ui.chosen.contains(&p) {
            ui.chosen.retain(|&x| x != p);
            self.sounds.push(super::Sound::Effect("SE01.WAV".into()));
        } else if ui.chosen.len() < ui.rule.max {
            ui.chosen.push(p);
            self.sounds.push(super::Sound::Effect("SE02.WAV".into()));
        }
    }

    /// 출진 고르기 입력. 결정 Some(Some(고른 장수들)), 취소 Some(None) (본진에서 연 것만)
    pub fn update_sortie(&mut self) -> Option<Option<Vec<usize>>> {
        let mut ui = self.sortie_ui.take()?;
        let _ = self.load_sortie(&mut ui);
        let inp = self.input.clone();
        let n = ui.pids.len();
        let cell = |slot: usize| (LIST.0 + (slot % 7) as i32 * 56 + 8, LIST.1 + (slot / 7) as i32 * 72 + 8);
        let inside = |(mx, my): (i32, i32), (x, y, w, h): (i32, i32, i32, i32)| mx >= x && mx < x + w && my >= y && my < y + h;
        // 마우스를 올린 장수를 정보 칸에
        if let Some(m) = inp.mouse {
            for slot in 0..n {
                let (x, y) = cell(slot);
                if inside(m, (x, y, 56, 64)) {
                    ui.info = ui.order[slot];
                    ui.cursor = slot;
                }
            }
        }
        let mut done = None;
        let mut cancelled = false;
        let mut act = |eng: &mut Self, ui: &mut SortieUi, target: usize| {
            if target < n {
                eng.toggle_sortie(ui, ui.order[target]);
            } else if target == n {
                if Self::can_depart(ui) {
                    eng.sounds.push(super::Sound::Effect("SE02.WAV".into()));
                    done = Some(ui.chosen.clone());
                }
            } else {
                // 취소 (원작 FUN_0045f7f4: 고른 것 비우고 닫음 → 본진). 본진 밖에서 연 것은 고른 것만 비움
                ui.chosen.truncate(ui.forced);
                eng.sounds.push(super::Sound::Effect("SE01.WAV".into()));
                if ui.depart.is_some() {
                    cancelled = true;
                }
            }
        };
        if inp.click {
            if let Some(m) = inp.mouse {
                for slot in 0..n {
                    let (x, y) = cell(slot);
                    if inside(m, (x, y, 56, 64)) {
                        act(self, &mut ui, slot);
                    }
                }
                // 출진 판: 누르면 빼기
                for i in 0..ui.chosen.len() {
                    let (x, y) = (PICK.0 + (i % 8) as i32 * 50, PICK.1 + (i / 8) as i32 * 64);
                    if inside(m, (x, y, 50, 64)) {
                        if let Some(k) = ui.pids.iter().position(|&p| p == ui.chosen[i]) {
                            self.toggle_sortie(&mut ui, k);
                        }
                        break;
                    }
                }
                // 정렬 단추 (원작 FUN_00460063: 둘째 단추는 이미 골라져 있으면 다음 정렬로)
                for (t, &(tx, ty)) in TABS.iter().enumerate() {
                    if inside(m, (LIST.0 + tx, LIST.1 + ty, 88, 16)) {
                        if t == 1 && (2..6).contains(&ui.sort) {
                            ui.sort2 = if ui.sort2 >= 5 { 2 } else { ui.sort2 + 1 };
                        }
                        ui.sort = if t == 0 { 1 } else { ui.sort2 };
                        self.sortie_sort = (ui.sort, ui.sort2);
                        self.sort_sortie(&mut ui);
                        self.sounds.push(super::Sound::Effect("SE02.WAV".into()));
                    }
                }
                if inside(m, OK_BTN) {
                    act(self, &mut ui, n);
                } else if inside(m, CANCEL_BTN) {
                    act(self, &mut ui, n + 1);
                }
            }
        } else {
            // 키보드: 칸 이동 (마지막 줄 아래 = 결정·취소)
            let total = n + 2;
            let p = inp.pressed;
            if p & key::RIGHT != 0 {
                ui.cursor = (ui.cursor + 1) % total;
            }
            if p & key::LEFT != 0 {
                ui.cursor = (ui.cursor + total - 1) % total;
            }
            if p & key::DOWN != 0 {
                ui.cursor = if ui.cursor + 7 < n { ui.cursor + 7 } else if ui.cursor < n { n } else { ui.cursor };
            }
            if p & key::UP != 0 {
                ui.cursor = if ui.cursor >= n { n.saturating_sub(1) } else { ui.cursor.saturating_sub(7) };
            }
            if ui.cursor < n {
                ui.info = ui.order[ui.cursor];
            }
            if p & key::DECIDE != 0 {
                let c = ui.cursor;
                act(self, &mut ui, c);
            }
            if p & key::CANCEL != 0 {
                act(self, &mut ui, n + 1);
            }
        }
        if done.is_some() {
            return Some(done);
        }
        if cancelled {
            return Some(None);
        }
        self.sortie_ui = Some(ui);
        None
    }

    pub fn render_sortie(&mut self) {
        let Some(ui) = self.sortie_ui.as_ref() else { return };
        let Some(f) = self.font.as_ref() else { return };
        let Some(bg) = &ui.bg else { return };
        let mark = self.mark_data.clone();
        let items = self.items.clone();
        let jobs = self.jobs.clone();
        let scr = &mut self.screen;
        let pal = scr.pal;
        let white = nearest(&pal, [255, 255, 255]);
        let black = nearest(&pal, [0, 0, 0]);
        let gray = nearest(&pal, [192, 192, 192]);
        let dark = nearest(&pal, [96, 96, 96]);
        let title = nearest(&pal, [0, 0, 128]);
        // 바탕 창과 제목 띠
        frame_box(scr, 4, 2, 632, 476);
        scr.fill(6, 4, 628, 20, title);
        draw_text(scr, f, 14, 6, "장수 선택", white);
        // 후보 판
        scr.blit(&bg[3], LIST.0, LIST.1, None);
        // 어둡게 (원작 FUN_00432a30 6: 각 채널 4비트 −6)
        let darken: Vec<u8> = (0..256usize).map(|c| if c == 0 { 0 } else { nearest(&pal, [pal[c][0].saturating_sub(96), pal[c][1].saturating_sub(96), pal[c][2].saturating_sub(96)]) }).collect();
        for (slot, &k) in ui.order.iter().enumerate() {
            let p = ui.pids[k];
            let (x, y) = (LIST.0 + (slot % 7) as i32 * 56 + 8, LIST.1 + (slot / 7) as i32 * 72 + 8);
            let chosen = ui.chosen.contains(&p);
            if let Some(sp) = ui.sprites.get(&p) {
                for yy in 0..48 {
                    for xx in 0..48 {
                        let mut c = sp.px[yy * 48 + xx];
                        if c == 0 {
                            continue;
                        }
                        if k < ui.forced {
                            c = darken[c as usize];
                        }
                        let (dx, dy) = (x + xx as i32, y + yy as i32);
                        if chosen {
                            // 반투명 (원작 FUN_0043230a: 밝기 l, u = 11·(256 − 6l)/16, 바탕·u + 그림·(256 − 10u/16) / 256, 4비트)
                            let bgc = scr.px[(dy as usize) * 640 + dx as usize];
                            let (s, b) = (pal[c as usize], pal[bgc as usize]);
                            let l = (s[0] as u32 / 16 + s[1] as u32 / 16 + s[2] as u32 / 16) / 3;
                            let u = 11 * (256 - l * 6) >> 4;
                            let mix = |i: usize| (((b[i] as u32 / 16) * u + (s[i] as u32 / 16) * (256 - (u * 10 >> 4))) >> 8).min(15) as u8 * 17;
                            c = nearest(&pal, [mix(0), mix(1), mix(2)]);
                        }
                        scr.fill(dx, dy, 1, 1, c);
                    }
                }
            }
            // 레벨 (테두리 색: 안 고름 0x3A, 반드시 0x28, 고름 0x1F) 과 이름 (가운데)
            let o = self.officers.get(&p).cloned();
            let lv = o.as_ref().map_or(1, |o| o.level);
            let edge = if !chosen { 0x3A } else if k < ui.forced { 0x28 } else { 0x1F };
            outlined(scr, f, x, y + 30, &format!("{lv}"), 0x12, edge);
            let name = self.persons.get(p).map(|x| x.name.clone()).unwrap_or_default();
            let w = name.chars().count() as i32 * 16;
            outlined(scr, f, x + (56 - w) / 2, y + 48, &name, 0x12, black);
        }
        // 정렬 단추
        let labels = ["부대속성순", "<레벨순>", "<공격력순>", "<정신력순>", "<방어력순>"];
        for (t, &(tx, ty)) in TABS.iter().enumerate() {
            let (x, y) = (LIST.0 + tx, LIST.1 + ty);
            for yy in 0..16 {
                for xx in 0..88 {
                    let i = ((y + yy) as usize) * 640 + (x + xx) as usize;
                    scr.px[i] = darken[scr.px[i] as usize];
                }
            }
            let (label, mode) = if t == 0 { (labels[0], 1) } else { (labels[(ui.sort2 as usize).clamp(2, 5) - 1], ui.sort2) };
            let col = if ui.sort == mode { 0x12 } else { 0x18 };
            draw_text(scr, f, x + 4, y, label, col);
        }
        // 출진 수
        draw_text(scr, f, PICK.0, COUNT_Y, &format!("출진장수  - {:2}/{:2}", ui.chosen.len(), ui.rule.max), super::story::TEXT);
        // 출진 판과 자리 표시
        scr.blit(&bg[4], PICK.0, PICK.1, None);
        for i in 0..ui.rule.max {
            let img = if i < ui.forced { 2 } else if i < ui.rule.max * 2 / 3 { 1 } else { 0 };
            scr.blit(&bg[img], PICK.0 + (i % 8) as i32 * 50, PICK.1 + (i / 8) as i32 * 64 + 13, Some(0));
        }
        for (i, p) in ui.chosen.iter().enumerate() {
            if let Some(sp) = ui.sprites.get(p) {
                scr.blit(sp, PICK.0 + (i % 8) as i32 * 50 + 1, PICK.1 + (i / 8) as i32 * 64 + 1, Some(0));
            }
        }
        // 키보드 커서
        let n = ui.pids.len();
        if ui.cursor < n {
            let (x, y) = (LIST.0 + (ui.cursor % 7) as i32 * 56 + 8, LIST.1 + (ui.cursor / 7) as i32 * 72 + 8);
            rect(scr, x - 2, y - 2, 56, 68, white);
        }
        // 장수 정보
        let p = ui.pids.get(ui.info).copied().unwrap_or(0);
        let o = self.officers.get(&p).cloned().unwrap_or_else(|| crate::officer::Officer::from_data(p, &[0; 32]));
        let x0 = INFO_X;
        etched(scr, x0, 30, 212, 408, white, dark);
        scr.fill(x0 + 8, 22, 80, 16, gray);
        draw_text(scr, f, x0 + 10, 22, "장수 정보", super::story::TEXT);
        if let Some(face) = ui.faces.get(&p) {
            scr.blit(face, x0 + 8, 44, None);
        }
        let name = self.persons.get(p).map(|x| x.name.clone()).unwrap_or_default();
        draw_text(scr, f, x0 + 80, 46, &name, super::story::NAME);
        draw_text(scr, f, x0 + 80, 68, self.job_names.get(o.job as usize).map_or("", |s| s.as_str()), super::story::TEXT);
        draw_text(scr, f, x0 + 80, 90, &format!("Lv {}", o.level), super::story::TEXT);
        let ps = self.persons.get(p).map(|x| x.stats).unwrap_or_default();
        // 무력·지력·통솔력 / 민첩성·운 (원작 +18 무력, +19 통솔, +20 지력, +21 민첩, +22 운)
        for (i, (label, v)) in [("무력", ps[0]), ("지력", ps[2]), ("통솔", ps[1]), ("민첩", ps[3]), ("운", ps[4])].into_iter().enumerate() {
            let (x, y) = (x0 + 8 + (i / 3) as i32 * 104, 130 + (i % 3) as i32 * 18);
            draw_text(scr, f, x, y, label, super::story::TEXT);
            draw_text(scr, f, x + 40, y, &format!("{v:3}"), super::story::TEXT);
        }
        // 막대 (원작 FUN_0046342f: HP 무늬 0x80 /500, MP 0x40 /250, Exp 0x140 /100, 나머지 0xC0 /400)
        let mv = jobs.get(o.job as usize * 27).copied().unwrap_or(0) as u16 + o.equip_bonus(&items, 7);
        let rows: [(&str, Option<usize>, i32, i32, String); 9] = [
            ("HP", Some(0x80), o.max_hp as i32, 500, format!("{}", o.max_hp)),
            ("MP", Some(0x40), o.max_mp as i32, 250, format!("{}", o.max_mp)),
            ("Exp", Some(0x140), o.exp as i32, 100, if o.exp == 0xFF { "MAX".into() } else { format!("{}", o.exp) }),
            ("공격력", Some(0xC0), o.value(&items, 0) as i32, 400, format!("{}", o.value(&items, 0))),
            ("정신력", Some(0xC0), o.value(&items, 2) as i32, 400, format!("{}", o.value(&items, 2))),
            ("방어력", Some(0xC0), o.value(&items, 1) as i32, 400, format!("{}", o.value(&items, 1))),
            ("순발력", Some(0xC0), o.value(&items, 3) as i32, 400, format!("{}", o.value(&items, 3))),
            ("사기", Some(0xC0), o.value(&items, 4) as i32, 400, format!("{}", o.value(&items, 4))),
            ("이동력", None, 0, 0, format!("{mv}")),
        ];
        for (i, (label, pat, v, max, text)) in rows.iter().enumerate() {
            let y = 188 + i as i32 * 26;
            draw_text(scr, f, x0 + 8, y, label, super::story::TEXT);
            if let Some(pat) = pat {
                let (bx, by, bw) = (x0 + 64, y + 4, 100);
                scr.fill(bx, by, bw, 9, gray);
                rect(scr, bx - 1, by - 1, bw + 2, 11, dark);
                let w = (bw * (*v).clamp(0, *max) / (*max).max(1)).min(bw);
                for yy in 0..9 {
                    for xx in 0..w {
                        let c = mark.get(pat + (yy as usize % 8) * 8 + (xx as usize % 8)).copied().unwrap_or(0x12);
                        scr.fill(bx + xx, by + yy, 1, 1, if c == 0 { gray } else { c });
                    }
                }
            }
            let tw = text.chars().count() as i32 * 8;
            draw_text(scr, f, x0 + 204 - tw, y, text, super::story::TEXT);
        }
        // 결정 (모자라면 흐리게) · 취소
        let ok = Self::can_depart(ui);
        for (b, label, on, focus) in [(OK_BTN, "결정", ok, ui.cursor == n), (CANCEL_BTN, "취소", true, ui.cursor == n + 1)] {
            frame_box(scr, b.0, b.1, b.2, b.3);
            if focus {
                rect(scr, b.0 + 2, b.1 + 2, b.2 - 4, b.3 - 4, white);
            }
            draw_text(scr, f, b.0 + (b.2 - 32) / 2, b.1 + 4, label, if on { super::story::TEXT } else { dark });
        }
    }
}

fn rect(scr: &mut Screen, x: i32, y: i32, w: i32, h: i32, c: u8) {
    scr.fill(x, y, w, 1, c);
    scr.fill(x, y + h - 1, w, 1, c);
    scr.fill(x, y, 1, h, c);
    scr.fill(x + w - 1, y, 1, h, c);
}

/// 윈도 묶음 상자 테두리 (오목한 두 줄)
fn etched(scr: &mut Screen, x: i32, y: i32, w: i32, h: i32, light: u8, dark: u8) {
    rect(scr, x + 1, y + 1, w - 1, h - 1, light);
    rect(scr, x, y, w - 1, h - 1, dark);
}

/// 테두리 글자 (원작 글꼴 테두리 색)
fn outlined(scr: &mut Screen, f: &crate::font::Font, x: i32, y: i32, s: &str, col: u8, edge: u8) {
    for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
        draw_text(scr, f, x + dx, y + dy, s, edge);
    }
    draw_text(scr, f, x, y, s, col);
}
