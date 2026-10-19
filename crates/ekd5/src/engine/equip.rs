//! 장비 (원작 대화상자 328 "장비", FUN_0046e0d3 / FUN_0046beb8 / FUN_0046aeab) 와 아이템 정보 (대화상자 167, FUN_0040ccc6).
//! 왼쪽: 창고 일람 (열: 이름·속성·Lv·Exp·효과·장비 ○×, FUN_0046dc4e / FUN_00468749). 마우스를 올린 아이템을
//!   장비했을 때의 능력 변화를 오른쪽에 ±로 (FUN_0046a3bb → FUN_0046aeab), 누르면 장비 (FUN_0046dcfb → FUN_0046ad6b: 원래 장비는 창고로).
//! 오른쪽: 장수 고르기 (콤보 + 앞의/다음 장수로), 얼굴, HP·MP·공격력·정신력·방어력·순발력·사기·이동력 (값, 변화),
//!   무기·방어구·보조 칸 (그림, Lv, Exp, 효과, 추가 효과). 칸에 올리면 벗었을 때의 변화 (FUN_0046dea9), 누르면 벗기 (FUN_0046df69,
//!   창고 200 이 차면 안 됨). 오른쪽 클릭 = 아이템 정보 (목록 FUN_004696ed, 칸 FUN_0046c88b).
//! 원작은 끌어다 놓기도 됨 (목록 → 칸). 터치는 올려 두기가 없어서: 한 번 누르면 고르기(변화 보기), 고른 것을 다시 누르면 장비·벗기,
//! 길게 누르면 정보.

use super::screen::{Pic, Screen};
use super::sortie::{etched, rect};
use super::story::{draw_text, frame_box, NAME, TEXT};
use super::ui::{ListBox, Pick};
use super::{key, Assets, Engine};
use crate::officer::{item_bonus, item_cat, item_name, item_power, item_stat, item_unique, Officer};
use crate::palette::nearest;
use std::collections::HashMap;
use std::rc::Rc;

/// 창고 최대 (원작 FUN_0040db4e < 200)
pub const STORE_MAX: usize = 200;

const LIST: (i32, i32, i32, i32) = (8, 30, 356, 404);
const ROW_Y: i32 = 60;
const ROW_H: i32 = 22;
const ROWS: usize = 16;
const RX: i32 = 372;
const COMBO: (i32, i32, i32, i32) = (RX, 30, 256, 24);
const SLOTS: [(i32, i32, i32, i32); 3] = [(RX, 214, 256, 74), (RX, 292, 256, 74), (RX, 370, 256, 60)];
const PREV_BTN: (i32, i32, i32, i32) = (RX, 444, 92, 26);
const NEXT_BTN: (i32, i32, i32, i32) = (RX + 96, 444, 92, 26);
const END_BTN: (i32, i32, i32, i32) = (RX + 192, 444, 64, 26);
const UP_BTN: (i32, i32, i32, i32) = (LIST.0 + LIST.2 - 20, ROW_Y, 16, 20);
const DOWN_BTN: (i32, i32, i32, i32) = (LIST.0 + LIST.2 - 20, ROW_Y + ROWS as i32 * ROW_H - 20, 16, 20);
const INFO: (i32, i32, i32, i32) = (110, 40, 420, 400);
const INFO_OK: (i32, i32, i32, i32) = (INFO.0 + INFO.2 - 100, INFO.1 + INFO.3 - 36, 88, 26);

/// 아이템 속성 이름 (원작 0x495f78, FUN_0040cabd: 무기·방어구 종류/2, 보조 9, 도구 10)
const TYPE_NAMES: [&str; 11] = ["검", "창", "활", "곤봉", "포차", "부채", "보검", "갑옷", "옷", "보조", "도구"];
/// 특수 효과 이름 (원작 0x495fa8, 종류 − 0x12)
const EFFECT_NAMES: [&str; 72] = [
    "매턴 HP회복", "매턴 MP회복", "매턴 상태 회복", "매턴 Exp획득", "매턴 무기Exp획득", "매턴 방어구Exp획득", "공격력 보조", "정신력 보조",
    "방어력 보조", "순발력 보조", "사기 보조", "HP보조", "MP보조", "획득 Exp보조", "이동력 보조", "돌격 이동", "험로 이동", "혼란 이동",
    "독 공격", "부동 공격", "금책 공격", "공격명중 보조", "반격 반격", "회심 공격", "간접 공격", "찔러 공격", "무반격 공격", "기마 공격",
    "인도 공격", "화계 책략 보조", "바람계 책략 보조", "MP절약", "금주 가능", "책략 모방", "책략 명중 보조", "공격 방어 보조", "책략 방어 보조",
    "전 방어 보조", "간접 방어", "회심 방어", "2번 방어", "책략 손해 경감", "MP방어", "간접 손해 경감", "자동 콩 사용", "HP회복", "MP회복",
    "혼란 회복", "독 회복", "부동 회복", "금책 회복", "상태 회복", "무력 상승", "지력 상승", "통솔력 상승", "민첩성 상승", "운 상승",
    "레벨 상승", "클래스 상승", "", "공격 (불)", "공격 (물)", "공격 (땅)", "공격 (바람)", "공격", "HP흡수", "MP흡수", "능력 하강", "방어 하강",
    "민첩 하강", "사기 하강", "능력 상승",
];
/// 장비 가능 부대 (원작 대화상자 167 0x5c0..)
const GROUP_NAMES: [&str; 13] = ["군주계", "보병계", "궁병계", "기병계", "궁기병계", "포차계", "무도가계", "적병계", "책사계", "풍수사계", "도사계", "기마책사계", "무희계"];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Sel {
    Row(usize),
    Slot(u8),
}

#[derive(Clone)]
pub struct EquipUi {
    pub pids: Vec<usize>,
    pub cur: usize,
    pub sel: Option<Sel>,
    pub top: usize,
    /// 고른 것을 다시 누름 / 누르기 시작한 목록 줄 (손을 떼면 장비·벗기·끌어다 놓기)
    pending: Option<Sel>,
    drag: Option<usize>,
    /// 아이템 정보 창 (아이템, 레벨, Exp)
    pub info: Option<(u8, u8, Option<u8>)>,
    /// 장수 콤보를 펼친 목록
    pick: Option<ListBox>,
    faces: HashMap<usize, Pic>,
}

/// 글 너비 (영숫자 8, 한글 16)
fn text_w(s: &str) -> i32 {
    s.chars().map(|c| if c.is_ascii() { 8 } else { 16 }).sum()
}

fn inside((mx, my): (i32, i32), (x, y, w, h): (i32, i32, i32, i32)) -> bool {
    mx >= x && mx < x + w && my >= y && my < y + h
}

/// 특수치 글 (원작 FUN_0046419b): 반환 (값, 글). 값 0 이면 이름만
fn effect_value(items: &[u8], id: u8) -> (u32, String) {
    let r = |i: usize| items.get(id as usize * 25 + i).copied().unwrap_or(0);
    let unique = item_unique(items, id);
    let v = if unique { r(22) } else { r(21) } as u32;
    let kind = if item_cat(r(17)) < 2 { r(18) } else { r(17) };
    match kind.wrapping_sub(0x12) {
        6..=0xE | 0x2D | 0x2E | 0x34..=0x38 => match v {
            0xFF => (0xFF, "ALL".into()),
            0 => (0, "--".into()),
            _ => (v, format!("+{v}")),
        },
        0x15 | 0x22..=0x25 => (v, format!("+{v}%")),
        0x29 | 0x2B => (v, format!("-{v}%")),
        _ => (0, "--".into()),
    }
}

/// 특수 효과 글 (원작 FUN_00463ed3): 일반 무기·방어구는 없음
fn special_text(items: &[u8], id: u8) -> String {
    let r = |i: usize| items.get(id as usize * 25 + i).copied().unwrap_or(0);
    if !item_unique(items, id) && r(17) < 0x12 {
        return String::new();
    }
    let kind = if item_cat(r(17)) < 2 { r(18) } else { r(17) };
    let name = EFFECT_NAMES.get(kind.wrapping_sub(0x12) as usize).copied().unwrap_or("");
    match effect_value(items, id) {
        (0, _) => name.into(),
        (0xFF, s) => format!("{name} ({s})"),
        (_, s) => format!("{name} {s}"),
    }
}

/// 효과 글 (원작 FUN_00463fe5): 공격력·방어력·정신력 + 값, 그 외는 특수 효과
pub fn effect_text(items: &[u8], id: u8, lv: u8) -> String {
    match item_stat(items, id) {
        Some(0) => format!("공격력 +{}", item_power(items, id, lv)),
        Some(1) => format!("방어력 +{}", item_power(items, id, lv)),
        Some(2) => format!("정신력 +{}", item_power(items, id, lv)),
        _ => special_text(items, id),
    }
}

/// 추가 효과 글 (원작 FUN_00464154): 고유 무기·방어구만
fn extra_text(items: &[u8], id: u8) -> String {
    if item_unique(items, id) { special_text(items, id) } else { String::new() }
}

/// 목록 효과 열 (원작 FUN_004640df)
fn effect_col(items: &[u8], id: u8, lv: u8) -> String {
    let t = items.get(id as usize * 25 + 17).copied().unwrap_or(0xFF);
    if item_cat(t) < 2 { format!("+{}", item_power(items, id, lv)) } else { effect_value(items, id).1 }
}

impl<A: Assets> Engine<A> {
    /// 장비 열기 (본진 장비 단추·메뉴)
    pub fn open_equip(&mut self) {
        let pids = if self.roster.is_empty() { vec![0] } else { self.roster.clone() };
        self.equip_ui = Some(EquipUi { pids, cur: 0, sel: None, top: 0, pending: None, drag: None, info: None, pick: None, faces: HashMap::new() });
    }

    /// 장비 칸 아이템 (아이템, 레벨)
    fn slot_item(o: &Officer, s: u8) -> Option<(u8, u8)> {
        match s {
            0 => o.weapon,
            1 => o.armor,
            _ => o.acc.map(|i| (i, 1)),
        }
    }

    /// 창고 아이템 장비 (원작 FUN_0046dcfb / FUN_0046ad6b): 원래 장비는 그 자리에 들어감
    fn equip_from_store(&mut self, p: usize, i: usize) -> bool {
        let Some(&(id, lv)) = self.inventory.get(i) else { return false };
        let mut o = self.officer(p);
        if !o.can_equip(&self.items, &self.jobs, id) {
            return false;
        }
        let t = self.items.get(id as usize * 25 + 17).copied().unwrap_or(0xFF);
        let s = item_cat(t);
        let old = Self::slot_item(&o, s);
        match s {
            0 => o.weapon = Some((id, lv)),
            1 => o.armor = Some((id, lv)),
            _ => o.acc = Some(id),
        }
        if s < 2 {
            o.gear_exp[s as usize] = 0;
        }
        match old {
            Some(it) => self.inventory[i] = it,
            None => {
                self.inventory.remove(i);
            }
        }
        self.officers.insert(p, o);
        self.sounds.push(super::Sound::Effect("SE09.WAV".into()));
        true
    }

    /// 벗기 (원작 FUN_0046df69 / FUN_0046acc9): 창고가 차 있으면 안 됨
    fn unequip_slot(&mut self, p: usize, s: u8) {
        let mut o = self.officer(p);
        let Some(it) = Self::slot_item(&o, s) else { return };
        if self.inventory.len() >= STORE_MAX {
            self.sounds.push(super::Sound::Effect("SE00.WAV".into()));
            return;
        }
        match s {
            0 => o.weapon = None,
            1 => o.armor = None,
            _ => o.acc = None,
        }
        self.inventory.push(it);
        self.officers.insert(p, o);
        self.sounds.push(super::Sound::Effect("SE01.WAV".into()));
    }

    fn equip_slot_of(&self, k: usize) -> u8 {
        self.inventory.get(k).map_or(3, |&(id, _)| item_cat(self.items.get(id as usize * 25 + 17).copied().unwrap_or(0xFF)))
    }

    /// 장비하면 그 칸을 고름 (목록 줄은 다른 아이템으로 바뀌므로 — 두 번 눌러 잘못 장비하지 않게)
    fn after_equip(ui: &mut EquipUi, s: u8, ok: bool) {
        if ok && s < 3 {
            ui.sel = Some(Sel::Slot(s));
        }
    }

    fn equip_hit(ui: &EquipUi, m: (i32, i32), n: usize) -> Option<Sel> {
        for (s, &r) in SLOTS.iter().enumerate() {
            if inside(m, r) {
                return Some(Sel::Slot(s as u8));
            }
        }
        if inside(m, (LIST.0 + 4, ROW_Y, LIST.2 - 28, ROWS as i32 * ROW_H)) {
            let k = ui.top + ((m.1 - ROW_Y) / ROW_H) as usize;
            if k < n {
                return Some(Sel::Row(k));
            }
        }
        None
    }

    fn equip_scroll(ui: &mut EquipUi, n: usize) {
        if let Some(Sel::Row(k)) = ui.sel {
            if k < ui.top {
                ui.top = k;
            }
            if k >= ui.top + ROWS {
                ui.top = k + 1 - ROWS;
            }
        }
        ui.top = ui.top.min(n.saturating_sub(ROWS));
    }

    /// 장비 화면 입력. 닫으면 equip_ui = None
    pub fn update_equip(&mut self) {
        let Some(mut ui) = self.equip_ui.take() else { return };
        if self.imsg.is_none() {
            if let super::Asset::Ready(d) = self.assets.get("IMSG.E5") {
                self.imsg = Some(d);
            }
        }
        let p = ui.pids[ui.cur.min(ui.pids.len() - 1)];
        if !ui.faces.contains_key(&p) {
            let face = self.face_of(p);
            if let Ok(d) = self.packs.get(&mut self.assets, "FACE.E5", face as usize) {
                ui.faces.insert(p, Pic::new(64, 80, d.unwrap_or_default()));
            }
        }
        let inp = self.input.clone();
        let n = self.inventory.len();
        let released = inp.held & key::DECIDE == 0;
        // 아이템 정보 창: 아무 데나 누르거나 취소하면 닫음
        if ui.info.is_some() {
            if inp.click || inp.alt || inp.pressed & (key::CANCEL | key::DECIDE) != 0 {
                ui.info = None;
                ui.pending = None;
                ui.drag = None;
            }
            self.equip_ui = Some(ui);
            return;
        }
        // 장수 콤보 목록
        if let Some(mut l) = ui.pick.take() {
            match l.update(&inp, inp.pressed & key::DECIDE != 0) {
                Pick::Row(k) => {
                    ui.cur = k;
                    ui.sel = None;
                    self.sounds.push(super::Sound::Effect("SE00.WAV".into()));
                }
                Pick::Cancel => {}
                Pick::None => ui.pick = Some(l),
            }
            self.equip_ui = Some(ui);
            return;
        }
        // 정보 보기 (오른쪽 클릭·길게 누르기)
        if inp.alt {
            if let Some(m) = inp.mouse {
                let o = self.officer(p);
                match Self::equip_hit(&ui, m, n) {
                    Some(Sel::Row(k)) => {
                        let (id, lv) = self.inventory[k];
                        ui.sel = Some(Sel::Row(k));
                        ui.info = Some((id, lv, if item_cat(self.items.get(id as usize * 25 + 17).copied().unwrap_or(0xFF)) < 2 { Some(0) } else { None }));
                    }
                    Some(Sel::Slot(s)) => {
                        if let Some((id, lv)) = Self::slot_item(&o, s) {
                            ui.sel = Some(Sel::Slot(s));
                            ui.info = Some((id, lv, if s < 2 { Some(o.gear_exp[s as usize]) } else { None }));
                        }
                    }
                    None => {}
                }
            }
            ui.pending = None;
            ui.drag = None;
            self.equip_ui = Some(ui);
            return;
        }
        // 마우스는 올리기만 해도 고름 (원작)
        if inp.hover && !inp.click && ui.drag.is_none() {
            if let Some(m) = inp.mouse {
                if let Some(h) = Self::equip_hit(&ui, m, n) {
                    ui.sel = Some(h);
                }
            }
        }
        let mut close = false;
        if inp.click {
            if let Some(m) = inp.mouse {
                if let Some(h) = Self::equip_hit(&ui, m, n) {
                    if ui.sel == Some(h) || inp.hover {
                        ui.pending = Some(h);
                    }
                    ui.sel = Some(h);
                    if let Sel::Row(k) = h {
                        ui.drag = Some(k);
                    }
                } else if inside(m, UP_BTN) {
                    ui.top = ui.top.saturating_sub(ROWS - 1);
                } else if inside(m, DOWN_BTN) {
                    ui.top = (ui.top + ROWS - 1).min(n.saturating_sub(ROWS));
                } else if inside(m, COMBO) {
                    let mut l = ListBox::new("장수", 260);
                    for &q in &ui.pids {
                        let o = self.officer(q);
                        let name = self.persons.get(q).map(|x| x.name.clone()).unwrap_or_default();
                        l.push(name, format!("Lv{}", o.level), true);
                    }
                    l.sel = ui.cur;
                    l.top = ui.cur.saturating_sub(5);
                    ui.pick = Some(l);
                    self.sounds.push(super::Sound::Effect("SE00.WAV".into()));
                } else if inside(m, PREV_BTN) && ui.cur > 0 {
                    ui.cur -= 1;
                    ui.sel = None;
                } else if inside(m, NEXT_BTN) && ui.cur + 1 < ui.pids.len() {
                    ui.cur += 1;
                    ui.sel = None;
                } else if inside(m, END_BTN) {
                    close = true;
                }
            }
        } else if released && (ui.pending.is_some() || ui.drag.is_some()) {
            // 손을 뗌: 끌어다 칸에 놓기 / 고른 것 다시 누르기
            let over = inp.mouse.and_then(|m| Self::equip_hit(&ui, m, n));
            let mut done = false;
            if let (Some(k), Some(Sel::Slot(s))) = (ui.drag, over) {
                if let Some(&(id, _)) = self.inventory.get(k) {
                    if item_cat(self.items.get(id as usize * 25 + 17).copied().unwrap_or(0xFF)) == s && self.equip_from_store(p, k) {
                        ui.sel = Some(Sel::Slot(s));
                    }
                }
                done = true;
            }
            if !done && over.is_some() && over == ui.pending {
                match over {
                    Some(Sel::Row(k)) => Self::after_equip(&mut ui, self.equip_slot_of(k), self.equip_from_store(p, k)),
                    Some(Sel::Slot(s)) => self.unequip_slot(p, s),
                    None => {}
                }
            }
            ui.pending = None;
            ui.drag = None;
        } else if !inp.click {
            // 키보드
            let pr = inp.pressed;
            if pr & (key::DOWN | key::UP) != 0 && n > 0 {
                let k = match ui.sel {
                    Some(Sel::Row(k)) if pr & key::DOWN != 0 => (k + 1).min(n - 1),
                    Some(Sel::Row(k)) => k.saturating_sub(1),
                    _ => ui.top,
                };
                ui.sel = Some(Sel::Row(k));
            }
            if pr & key::LEFT != 0 && ui.cur > 0 {
                ui.cur -= 1;
                ui.sel = None;
            }
            if pr & key::RIGHT != 0 && ui.cur + 1 < ui.pids.len() {
                ui.cur += 1;
                ui.sel = None;
            }
            if pr & key::DECIDE != 0 {
                match ui.sel {
                    Some(Sel::Row(k)) => Self::after_equip(&mut ui, self.equip_slot_of(k), self.equip_from_store(p, k)),
                    Some(Sel::Slot(s)) => self.unequip_slot(p, s),
                    None => {}
                }
            }
            if pr & key::CANCEL != 0 {
                close = true;
            }
        }
        let n = self.inventory.len();
        if let Some(Sel::Row(k)) = ui.sel {
            if k >= n {
                ui.sel = if n > 0 { Some(Sel::Row(n - 1)) } else { None };
            }
        }
        if !inp.hover || inp.click || inp.pressed != 0 {
            Self::equip_scroll(&mut ui, n);
        }
        if close {
            self.sounds.push(super::Sound::Effect("SE01.WAV".into()));
            return;
        }
        self.equip_ui = Some(ui);
    }

    pub fn render_equip(&mut self) {
        let Some(ui) = self.equip_ui.as_ref() else { return };
        let Some(f) = self.font.as_ref() else { return };
        let items = self.items.clone();
        let jobs = self.jobs.clone();
        let pal = self.screen.pal;
        let white = nearest(&pal, [255, 255, 255]);
        let gray = nearest(&pal, [192, 192, 192]);
        let dark = nearest(&pal, [128, 128, 128]);
        let navy = nearest(&pal, [0, 0, 128]);
        let blue = nearest(&pal, [0, 0, 255]);
        let red = nearest(&pal, [192, 0, 0]);
        let p = ui.pids[ui.cur.min(ui.pids.len() - 1)];
        let o = self.officers.get(&p).cloned().unwrap_or_else(|| {
            let r = self.person_data.get(p * 32..p * 32 + 32).map(|r| r.to_vec()).unwrap_or_else(|| vec![0; 32]);
            Officer::from_data(p, &r)
        });
        let small: Vec<Option<Rc<Pic>>> = self.inventory.iter().map(|&(id, _)| self.icons.item(&items, id, &pal)).collect();
        let big: Vec<Option<Rc<Pic>>> = (0..3u8).map(|s| Self::slot_item(&o, s).and_then(|(id, _)| self.icons.item_big(&items, id, &pal))).collect();
        let name = self.persons.get(p).map(|x| x.name.clone()).unwrap_or_default();
        let inventory = self.inventory.clone();
        let scr = &mut self.screen;
        frame_box(scr, 4, 2, 632, 476);
        scr.fill(6, 4, 628, 20, navy);
        draw_text(scr, f, 14, 6, "장비", white);

        // 창고 일람
        etched(scr, LIST.0, LIST.1, LIST.2, LIST.3, white, dark);
        scr.fill(LIST.0 + 8, LIST.1 - 8, 72, 16, gray);
        draw_text(scr, f, LIST.0 + 10, LIST.1 - 8, "창고 일람", TEXT);
        let hy = ROW_Y - 22;
        scr.fill(LIST.0 + 4, hy, LIST.2 - 8, 20, gray);
        for (label, x, right) in [("이름", 30, false), ("속성", 150, false), ("Lv", 206, true), ("Exp", 240, true), ("효과", 294, true), ("장비", 300, false)] {
            let x = LIST.0 + if right { x - text_w(label) } else { x };
            draw_text(scr, f, x, hy + 2, label, TEXT);
        }
        for (i, &(id, lv)) in inventory.iter().enumerate().skip(ui.top).take(ROWS) {
            let y = ROW_Y + (i - ui.top) as i32 * ROW_H;
            let selected = ui.sel == Some(Sel::Row(i));
            let ok = o.can_equip(&items, &jobs, id);
            if selected {
                scr.fill(LIST.0 + 4, y, LIST.2 - 28, ROW_H, navy);
            }
            let col = if selected { white } else if ok { TEXT } else { dark };
            if let Some(Some(ic)) = small.get(i) {
                scr.blit(ic, LIST.0 + 8, y + (ROW_H - ic.h as i32) / 2, Some(0));
            }
            let t = items.get(id as usize * 25 + 17).copied().unwrap_or(0xFF);
            let gear = item_cat(t) < 2;
            draw_text(scr, f, LIST.0 + 30, y + 3, &item_name(&items, id), col);
            let tn = TYPE_NAMES[if t < 0x12 { t as usize >> 1 } else if t < 0x3F { 9 } else { 10 }];
            draw_text(scr, f, LIST.0 + 150, y + 3, tn, col);
            let lv_s = if gear { format!("{lv}") } else { "--".into() };
            draw_text(scr, f, LIST.0 + 206 - text_w(&lv_s), y + 3, &lv_s, col);
            let ex = if gear { "0".to_string() } else { "--".into() };
            draw_text(scr, f, LIST.0 + 240 - text_w(&ex), y + 3, &ex, col);
            let ef = effect_col(&items, id, lv);
            draw_text(scr, f, LIST.0 + 294 - text_w(&ef), y + 3, &ef, col);
            draw_text(scr, f, LIST.0 + 308, y + 3, if ok { "○" } else { "×" }, if selected { white } else if ok { TEXT } else { red });
        }
        if inventory.is_empty() {
            draw_text(scr, f, LIST.0 + 30, ROW_Y + 3, "(창고가 비어 있다)", dark);
        }
        // 넘기기
        for (b, label, on) in [(UP_BTN, "▲", ui.top > 0), (DOWN_BTN, "▼", ui.top + ROWS < inventory.len())] {
            scr.fill(b.0, b.1, b.2, b.3, gray);
            draw_text(scr, f, b.0, b.1 + 2, label, if on { TEXT } else { dark });
        }
        draw_text(scr, f, LIST.0 + 8, LIST.1 + LIST.3 - 22, &format!("{} / {STORE_MAX}", inventory.len()), TEXT);

        // 장수 콤보
        scr.fill(COMBO.0, COMBO.1, COMBO.2, COMBO.3, white);
        rect(scr, COMBO.0, COMBO.1, COMBO.2, COMBO.3, dark);
        draw_text(scr, f, COMBO.0 + 6, COMBO.1 + 4, &name, TEXT);
        draw_text(scr, f, COMBO.0 + 164, COMBO.1 + 4, &format!("Lv{:2}", o.level), TEXT);
        scr.fill(COMBO.0 + COMBO.2 - 22, COMBO.1 + 2, 20, COMBO.3 - 4, gray);
        draw_text(scr, f, COMBO.0 + COMBO.2 - 20, COMBO.1 + 4, "▼", TEXT);
        // 얼굴
        if let Some(face) = ui.faces.get(&p) {
            scr.blit(face, RX + 2, 62, None);
        }
        rect(scr, RX + 1, 61, 66, 82, dark);
        // 능력 (원작 FUN_0046aeab: 고른 아이템을 장비했을 때 − 지금)
        let (new, cur): (Option<(u8, u8)>, Option<(u8, u8)>) = match ui.sel {
            Some(Sel::Row(k)) => match inventory.get(k) {
                Some(&(id, lv)) if o.can_equip(&items, &jobs, id) => {
                    let s = item_cat(items.get(id as usize * 25 + 17).copied().unwrap_or(0xFF));
                    (Some((id, lv)), Self::slot_item(&o, s))
                }
                _ => (None, None),
            },
            Some(Sel::Slot(s)) => (None, Self::slot_item(&o, s)),
            None => (None, None),
        };
        let bonus = |it: Option<(u8, u8)>, st: u8| it.map_or(0, |(id, lv)| item_bonus(&items, id, lv, st) as i32);
        let d = |st: u8| bonus(new, st) - bonus(cur, st);
        let mv = (jobs.get(o.job as usize * 27).copied().unwrap_or(0) as i32 + o.equip_bonus(&items, 7) as i32).min(255);
        let rows: [(&str, i32, i32); 8] = [
            ("HP", o.hp_max(&items) as i32, d(5)),
            ("MP", o.mp_max(&items) as i32, d(6)),
            ("공격력", o.value(&items, 0) as i32, d(0)),
            ("정신력", o.value(&items, 2) as i32, d(2)),
            ("방어력", o.value(&items, 1) as i32, d(1)),
            ("순발력", o.value(&items, 3) as i32, d(3)),
            ("사기", o.value(&items, 4) as i32, d(4)),
            ("이동력", mv, d(7)),
        ];
        for (i, (label, v, dv)) in rows.iter().enumerate() {
            let y = 58 + i as i32 * 19;
            draw_text(scr, f, RX + 76, y, label, TEXT);
            let vs = format!("{}", v + dv);
            draw_text(scr, f, RX + 176 - text_w(&vs), y, &vs, TEXT);
            if *dv != 0 {
                let ds = if *dv > 0 { format!("+{dv}") } else { format!("{dv}") };
                draw_text(scr, f, RX + 222 - text_w(&ds), y, &ds, if *dv > 0 { blue } else { red });
            }
        }
        // 장비 칸
        for s in 0..3u8 {
            let (x, y, w, h) = SLOTS[s as usize];
            etched(scr, x, y, w, h, white, dark);
            if ui.sel == Some(Sel::Slot(s)) {
                rect(scr, x + 2, y + 2, w - 4, h - 4, navy);
                rect(scr, x + 3, y + 3, w - 6, h - 6, navy);
            }
            let label = ["무기", "방어구", "보조"][s as usize];
            let it = Self::slot_item(&o, s);
            let iname = it.map_or("없음".to_string(), |(id, _)| item_name(&items, id));
            draw_text(scr, f, x + 6, y + 4, &format!("{label}: {iname}"), TEXT);
            if let Some(Some(ic)) = big.get(s as usize) {
                scr.blit(ic, x + 6, y + 24, Some(0));
            }
            if let Some((id, lv)) = it {
                let tx = x + 46;
                if s < 2 {
                    draw_text(scr, f, tx, y + 22, &format!("Lv{lv}"), TEXT);
                    draw_text(scr, f, tx + 40, y + 22, "Exp", TEXT);
                    let (gx, gy, gw) = (tx + 70, y + 26, 100);
                    scr.fill(gx, gy, gw, 8, gray);
                    rect(scr, gx - 1, gy - 1, gw + 2, 10, dark);
                    let ex = o.gear_exp[s as usize].min(100) as i32;
                    scr.fill(gx, gy, gw * ex / 100, 8, blue);
                    draw_text(scr, f, tx, y + 38, &effect_text(&items, id, lv), TEXT);
                    draw_text(scr, f, tx, y + 54, &extra_text(&items, id), TEXT);
                } else {
                    draw_text(scr, f, tx, y + 30, &effect_text(&items, id, lv), TEXT);
                }
            }
        }
        // 단추
        let can_prev = ui.cur > 0;
        let can_next = ui.cur + 1 < ui.pids.len();
        for (b, label, on) in [(PREV_BTN, "앞의 장수로", can_prev), (NEXT_BTN, "다음 장수로", can_next), (END_BTN, "종료", true)] {
            frame_box(scr, b.0, b.1, b.2, b.3);
            draw_text(scr, f, b.0 + (b.2 - text_w(label)) / 2, b.1 + 5, label, if on { TEXT } else { dark });
        }
        draw_text(scr, f, LIST.0 + 96, LIST.1 + LIST.3 - 22, "길게 누르기·오른쪽 클릭 = 정보", dark);
        if let Some(l) = &ui.pick {
            let hi = nearest(&pal, [208, 176, 112]);
            l.render(scr, f, hi);
        }
        if let Some((id, lv, exp)) = ui.info {
            let icon = self.icons.item_big(&items, id, &pal);
            let desc = self.imsg.as_ref().and_then(|m| m.get(id as usize * 216..id as usize * 216 + 216)).map(|r| {
                let e = r.iter().position(|&b| b == 0).unwrap_or(r.len());
                encoding_rs::EUC_KR.decode(&r[..e]).0.to_string()
            });
            let scr = &mut self.screen;
            render_info(scr, f, &items, &jobs, id, lv, exp, icon.as_deref(), desc.as_deref().unwrap_or(""), [white, gray, dark, navy, blue]);
        }
    }
}

/// 아이템 정보 (원작 대화상자 167 / FUN_0040ccc6)
#[allow(clippy::too_many_arguments)]
fn render_info(scr: &mut Screen, f: &crate::font::Font, items: &[u8], jobs: &[u8], id: u8, lv: u8, exp: Option<u8>, icon: Option<&Pic>, desc: &str, [white, gray, dark, navy, blue]: [u8; 5]) {
    let (x, y, w, h) = INFO;
    frame_box(scr, x, y, w, h);
    scr.fill(x + 2, y + 2, w - 4, 20, navy);
    draw_text(scr, f, x + 10, y + 4, "아이템 정보", white);
    if let Some(ic) = icon {
        scr.blit(ic, x + 12, y + 30, Some(0));
    }
    draw_text(scr, f, x + 52, y + 30, &item_name(items, id), NAME);
    let r = |i: usize| items.get(id as usize * 25 + i).copied().unwrap_or(0);
    let t = r(17);
    if let Some(ex) = exp {
        draw_text(scr, f, x + 52, y + 48, &format!("Lv{lv}"), TEXT);
        draw_text(scr, f, x + 100, y + 48, "Exp", TEXT);
        let (gx, gy, gw) = (x + 132, y + 52, 100);
        scr.fill(gx, gy, gw, 8, gray);
        rect(scr, gx - 1, gy - 1, gw + 2, 10, dark);
        scr.fill(gx, gy, gw * ex.min(100) as i32 / 100, 8, blue);
    }
    // 속성·가격 / 효과·추가 효과
    let by = y + 76;
    etched(scr, x + 8, by, 132, 48, white, dark);
    let tn = TYPE_NAMES[if t < 0x12 { t as usize >> 1 } else if t < 0x3F { 9 } else { 10 }];
    draw_text(scr, f, x + 14, by + 6, &format!("속성: {tn}"), TEXT);
    let price = if r(19) == 0xFF { "---".to_string() } else { format!("{}", r(19) as u32 * 100) };
    draw_text(scr, f, x + 14, by + 26, &format!("가격: {price}"), TEXT);
    etched(scr, x + 146, by, w - 154, 48, white, dark);
    scr.fill(x + 154, by - 8, 36, 16, gray);
    draw_text(scr, f, x + 156, by - 8, "효과", TEXT);
    draw_text(scr, f, x + 152, by + 8, &effect_text(items, id, lv), TEXT);
    draw_text(scr, f, x + 152, by + 26, &extra_text(items, id), TEXT);
    // 설명 (IMSG.E5 기록 = 아이템 번호)
    let dy = by + 56;
    scr.fill(x + 8, dy, w - 16, 108, white);
    rect(scr, x + 8, dy, w - 16, 108, dark);
    let mut line = String::new();
    let mut ly = dy + 6;
    for c in desc.chars() {
        if text_w(&line) + text_w(&c.to_string()) > w - 28 {
            draw_text(scr, f, x + 14, ly, &line, TEXT);
            line.clear();
            ly += 18;
        }
        line.push(c);
    }
    if !line.is_empty() {
        draw_text(scr, f, x + 14, ly, &line, TEXT);
    }
    // 장비 가능 부대 (원작: 무기·방어구는 병종 첫 직업의 직업 표, 보조는 +23 제한 병종(0xFF 모두), 도구는 없음)
    let gy = dy + 124;
    etched(scr, x + 8, gy, w - 16, 92, white, dark);
    scr.fill(x + 16, gy - 8, 104, 16, gray);
    draw_text(scr, f, x + 18, gy - 8, "장비 가능 부대", TEXT);
    for (g, label) in GROUP_NAMES.iter().enumerate() {
        let on = match item_cat(t) {
            0 | 1 => jobs.get(g * 3 * 27 + 9 + t as usize).copied().unwrap_or(0) != 0,
            2 => r(23) == 0xFF || r(23) as usize == g,
            _ => false,
        };
        let (cx, cy) = (x + 16 + (g % 4) as i32 * 100, gy + 10 + (g / 4) as i32 * 19);
        draw_text(scr, f, cx, cy, label, if on { TEXT } else { gray });
    }
    frame_box(scr, INFO_OK.0, INFO_OK.1, INFO_OK.2, INFO_OK.3);
    draw_text(scr, f, INFO_OK.0 + 28, INFO_OK.1 + 5, "확인", TEXT);
}
