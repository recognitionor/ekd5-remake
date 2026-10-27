//! 장수 정보 창 (원작 대화상자 166 '장수 정보' / FUN_0047b36a): 왼쪽 얼굴·이름·소속, 부대 속성(병과·Lv·Exp), 상태(HP·MP·상태이상),
//! 오른쪽 탭 5개 — 능력 / 장비 / 책략 / 장수열전 (IMSG 450+인물) / 부대특성 (IMSG 350+병과), 아래 앞의 장수로·다음 장수로·닫음.
//! 전투에서는 부대를 길게 누르기(원작 오른쪽 클릭), 본진에서는 메뉴 "장수 정보". 장비 탭에서 아이템을 누르면 아이템 정보 창 (대화상자 167).

use super::assets::Assets;
use super::battle::Side;
use super::equip::effect_text;
use super::screen::Pic;
use super::sortie::{etched, rect};
use super::story::{draw_text, frame_box, NAME, TEXT};
use super::{key, Engine, Scene};
use crate::officer::{item_name, Officer};
use crate::palette::nearest;
use crate::strategy::status;
use std::collections::HashMap;
use std::rc::Rc;

/// 누구의 정보인가: 전투 부대 번호 / 인물 번호 (본진)
#[derive(Clone, Copy, PartialEq)]
pub enum Who {
    Unit(usize),
    Person(usize),
}

pub struct InfoUi {
    pub list: Vec<Who>,
    pub cur: usize,
    /// 0 능력 1 장비 2 책략 3 장수열전 4 부대특성
    pub tab: u8,
    /// 책략 탭에서 고른 줄
    pub strat_sel: Option<usize>,
    pub strat_top: usize,
    /// 아이템 정보 창 (아이템, 레벨, Exp)
    pub item: Option<(u8, u8, Option<u8>)>,
    /// 처음 열 때 안내
    pub hint: bool,
    faces: HashMap<usize, Pic>,
}

const WIN: (i32, i32, i32, i32) = (40, 16, 560, 452);
const LEFT: i32 = WIN.0 + 10;
const RIGHT: (i32, i32, i32, i32) = (WIN.0 + 246, WIN.1 + 92, 304, 312);
/// 탭: 윗줄 장수열전·부대특성, 아랫줄 능력·장비·책략 (원작 배치)
const TABS: [(u8, &str, i32, i32, i32); 5] = [
    (3, "장수열전", RIGHT.0 + 4, WIN.1 + 30, 100),
    (4, "부대특성", RIGHT.0 + 108, WIN.1 + 30, 100),
    (0, "능력", RIGHT.0 + 4, WIN.1 + 58, 80),
    (1, "장비", RIGHT.0 + 88, WIN.1 + 58, 80),
    (2, "책략", RIGHT.0 + 172, WIN.1 + 58, 80),
];
const TAB_H: i32 = 26;
const PREV_BTN: (i32, i32, i32, i32) = (WIN.0 + 10, WIN.1 + WIN.3 - 36, 110, 26);
const NEXT_BTN: (i32, i32, i32, i32) = (WIN.0 + 126, WIN.1 + WIN.3 - 36, 110, 26);
const CLOSE_BTN: (i32, i32, i32, i32) = (WIN.0 + WIN.2 - 100, WIN.1 + WIN.3 - 36, 90, 26);
/// 장비 탭 칸 (무기·방어구·보조)
const GEAR: [(i32, i32, i32, i32); 3] = [(RIGHT.0 + 4, RIGHT.1 + 6, RIGHT.2 - 8, 100), (RIGHT.0 + 4, RIGHT.1 + 110, RIGHT.2 - 8, 100), (RIGHT.0 + 4, RIGHT.1 + 214, RIGHT.2 - 8, 72)];
const STRAT_ROWS: usize = 8;
const STRAT_ROW_H: i32 = 22;
pub const HINT_EQUIP: u32 = 1;
pub const HINT_INFO: u32 = 2;
pub const HINT_BATTLE: u32 = 4;

fn inside((x, y): (i32, i32), (rx, ry, rw, rh): (i32, i32, i32, i32)) -> bool {
    x >= rx && x < rx + rw && y >= ry && y < ry + rh
}

/// 글 너비 (영숫자 8, 한글 16)
fn text_w(s: &str) -> i32 {
    s.chars().map(|c| if c.is_ascii() { 8 } else { 16 }).sum()
}

/// 너비에 맞춰 줄 나누기 ("\n" 은 줄바꿈)
pub fn wrap(s: &str, w: i32) -> Vec<String> {
    let mut out = Vec::new();
    for para in s.split("\\n").flat_map(|p| p.split('\n')) {
        let mut line = String::new();
        for c in para.chars() {
            if text_w(&line) + text_w(&c.to_string()) > w {
                out.push(std::mem::take(&mut line));
            }
            line.push(c);
        }
        out.push(line);
    }
    out
}

/// 그릴 때 쓰는 값
struct View {
    person: usize,
    name: String,
    belong: &'static str,
    off: Officer,
    hp: (i32, i32),
    mp: (i32, i32),
    status: u8,
    /// 능력 상태 (공격·방어·정신·순발·사기·이동, 3 보통)
    state: [u8; 6],
    strats: Vec<(u8, String, i32)>,
}

impl<A: Assets> Engine<A> {
    /// 장수 정보 창 열기
    pub fn open_info(&mut self, list: Vec<Who>, cur: usize) {
        if list.is_empty() {
            return;
        }
        let hint = self.hints & HINT_INFO == 0;
        self.info_ui = Some(InfoUi { list, cur, tab: 0, strat_sel: None, strat_top: 0, item: None, hint, faces: HashMap::new() });
        self.sounds.push(super::Sound::Effect("SE00.WAV".into()));
    }

    /// 처음 안내를 봤다고 기록 (회차 공통 기록에 함께 저장)
    pub(super) fn hint_seen(&mut self, bit: u32) {
        if self.hints & bit == 0 {
            self.hints |= bit;
            self.common_out = Some(super::tutor::common_string(self.endings, &self.book, self.hints));
        }
    }

    fn info_view(&mut self, who: Who) -> View {
        let items = self.items.clone();
        match who {
            Who::Unit(u) => {
                let b = match &self.scene {
                    Scene::Battle(b) => b,
                    _ => unreachable!(),
                };
                let un = &b.units[u];
                let mut state = [3; 6];
                state[..5].copy_from_slice(&un.state);
                state[5] = un.mv_state;
                let strats = b.known_strategies(u).into_iter().filter_map(|sid| b.strategies.get(sid as usize).map(|s| (sid, s.name.clone(), b.strat_mp(u, sid)))).collect();
                View {
                    person: un.person,
                    name: self.persons.get(un.person).map(|p| p.name.clone()).unwrap_or_default(),
                    belong: match un.side {
                        Side::Player => "조조군",
                        Side::Ally => "우군",
                        Side::Enemy => "적군",
                    },
                    off: un.off.clone(),
                    hp: (un.hp.max(0), un.max_hp),
                    mp: (un.mp, un.off.mp_max(&items) as i32),
                    status: un.status,
                    state,
                    strats,
                }
            }
            Who::Person(p) => {
                let o = self.officer(p);
                let strats = self.strategies.iter().filter(|s| s.known(o.job, o.level)).map(|s| (s.id, s.name.clone(), s.mp as i32)).collect();
                View {
                    person: p,
                    name: self.persons.get(p).map(|x| x.name.clone()).unwrap_or_default(),
                    belong: "조조군",
                    hp: (o.hp_max(&items) as i32, o.hp_max(&items) as i32),
                    mp: (o.mp_max(&items) as i32, o.mp_max(&items) as i32),
                    off: o,
                    status: 0,
                    state: [3; 6],
                    strats,
                }
            }
        }
    }

    fn imsg_text(&self, k: usize) -> String {
        self.imsg
            .as_ref()
            .and_then(|m| m.get(k * 216..k * 216 + 216))
            .map(|r| {
                let e = r.iter().position(|&b| b == 0).unwrap_or(r.len());
                encoding_rs::EUC_KR.decode(&r[..e]).0.to_string()
            })
            .unwrap_or_default()
    }

    pub fn update_info(&mut self) {
        let Some(mut ui) = self.info_ui.take() else { return };
        if self.imsg.is_none() {
            if let super::Asset::Ready(d) = self.assets.get("IMSG.E5") {
                self.imsg = Some(d);
            }
        }
        let who = ui.list[ui.cur.min(ui.list.len() - 1)];
        let v = self.info_view(who);
        if !ui.faces.contains_key(&v.person) {
            let face = self.face_of(v.person);
            if let Ok(d) = self.packs.get(&mut self.assets, "FACE.E5", face as usize) {
                ui.faces.insert(v.person, Pic::new(64, 80, d.unwrap_or_default()));
            }
        }
        let inp = self.input.clone();
        let tap = inp.click || inp.pressed & key::DECIDE != 0;
        // 안내·아이템 정보 창은 아무 데나 누르면 닫힘
        if ui.hint {
            if tap || inp.alt || inp.pressed & key::CANCEL != 0 {
                ui.hint = false;
                self.hint_seen(HINT_INFO);
            }
            self.info_ui = Some(ui);
            return;
        }
        if ui.item.is_some() {
            if tap || inp.alt || inp.pressed & key::CANCEL != 0 {
                ui.item = None;
            }
            self.info_ui = Some(ui);
            return;
        }
        let n = ui.list.len();
        let mut close = inp.pressed & key::CANCEL != 0;
        let gear_of = |o: &Officer, s: usize| -> Option<(u8, u8)> {
            match s {
                0 => o.weapon,
                1 => o.armor,
                _ => o.acc.map(|a| (a, 1)),
            }
        };
        if inp.click || inp.alt {
            if let Some(m) = inp.mouse {
                if let Some(t) = TABS.iter().find(|t| inside(m, (t.2, t.3, t.4, TAB_H))) {
                    if ui.tab != t.0 {
                        ui.tab = t.0;
                        self.sounds.push(super::Sound::Effect("SE00.WAV".into()));
                    }
                } else if inside(m, PREV_BTN) {
                    ui.cur = (ui.cur + n - 1) % n;
                    ui.strat_sel = None;
                    ui.strat_top = 0;
                } else if inside(m, NEXT_BTN) {
                    ui.cur = (ui.cur + 1) % n;
                    ui.strat_sel = None;
                    ui.strat_top = 0;
                } else if inside(m, CLOSE_BTN) || !inside(m, WIN) {
                    close = true;
                } else if ui.tab == 1 {
                    // 장비 탭: 칸을 누르면 아이템 정보
                    if let Some(s) = GEAR.iter().position(|&r| inside(m, r)) {
                        if let Some((id, lv)) = gear_of(&v.off, s) {
                            let cap = Officer::gear_max(&self.items, id);
                            ui.item = Some((id, lv, (s < 2).then(|| if lv >= cap { 100 } else { v.off.gear_exp[s] })));
                            self.sounds.push(super::Sound::Effect("SE00.WAV".into()));
                        }
                    }
                } else if ui.tab == 2 && inside(m, RIGHT) {
                    // 책략 탭: 줄을 누르면 설명, 위·아래 끝 줄은 넘기기
                    let k = ((m.1 - RIGHT.1 - 8) / STRAT_ROW_H).max(0) as usize;
                    if k < STRAT_ROWS {
                        let i = ui.strat_top + k;
                        if i < v.strats.len() {
                            ui.strat_sel = Some(i);
                        }
                    } else if v.strats.len() > STRAT_ROWS {
                        ui.strat_top = (ui.strat_top + STRAT_ROWS).min(v.strats.len() - 1) % v.strats.len().max(1);
                    }
                }
            }
        }
        let pr = inp.pressed;
        if pr & key::LEFT != 0 {
            ui.tab = ui.tab.saturating_sub(1);
        }
        if pr & key::RIGHT != 0 {
            ui.tab = (ui.tab + 1).min(4);
        }
        if pr & key::UP != 0 {
            ui.cur = (ui.cur + n - 1) % n;
        }
        if pr & key::DOWN != 0 {
            ui.cur = (ui.cur + 1) % n;
        }
        if close {
            self.sounds.push(super::Sound::Effect("SE01.WAV".into()));
            return;
        }
        self.info_ui = Some(ui);
    }

    pub fn render_info(&mut self) {
        let Some(ui) = self.info_ui.as_ref() else { return };
        let who = ui.list[ui.cur.min(ui.list.len() - 1)];
        let (tab, strat_sel, strat_top, item, hint) = (ui.tab, ui.strat_sel, ui.strat_top, ui.item, ui.hint);
        let face = ui.faces.get(&match who {
            Who::Unit(u) => match &self.scene {
                Scene::Battle(b) => b.units[u].person,
                _ => 0,
            },
            Who::Person(p) => p,
        });
        let face = face.cloned();
        let v = self.info_view(who);
        let items = self.items.clone();
        let jobs = self.jobs.clone();
        let pal = self.screen.pal;
        let [white, gray, dark, navy, green, yellow, purple, red] =
            [[255, 255, 255], [192, 192, 192], [128, 128, 128], [0, 0, 128], [48, 192, 64], [224, 208, 48], [208, 96, 224], [192, 0, 0]].map(|c| nearest(&pal, c));
        let job_name = self.job_names.get(v.off.job as usize).cloned().unwrap_or_default();
        let bio = wrap(&self.imsg_text(450 + v.person), RIGHT.2 - 20);
        let unit_note = wrap(&self.imsg_text(350 + v.off.job as usize), RIGHT.2 - 20);
        let strat_desc = strat_sel.and_then(|i| v.strats.get(i)).map(|s| wrap(&self.imsg_text(150 + s.0 as usize), RIGHT.2 - 20)).unwrap_or_default();
        let icons: Vec<Option<Rc<Pic>>> = [v.off.weapon.map(|w| w.0), v.off.armor.map(|a| a.0), v.off.acc].iter().map(|id| id.and_then(|i| self.icons.item_big(&items, i, &pal))).collect();
        let ps = self.persons.get(v.person).map(|x| x.stats).unwrap_or_default();
        let mv = jobs.get(v.off.job as usize * 27).copied().unwrap_or(0) as i32 + v.off.equip_bonus(&items, 7) as i32;
        let Some(f) = self.font.as_ref() else { return };
        let scr = &mut self.screen;
        let (x0, y0, w, h) = WIN;
        frame_box(scr, x0, y0, w, h);
        scr.fill(x0 + 2, y0 + 2, w - 4, 20, navy);
        draw_text(scr, f, x0 + 10, y0 + 4, "장수 정보", white);
        // 얼굴·이름·소속
        if let Some(fp) = &face {
            scr.blit(fp, LEFT, y0 + 30, None);
        }
        draw_text(scr, f, LEFT + 72, y0 + 34, &v.name, NAME);
        draw_text(scr, f, LEFT + 72, y0 + 56, v.belong, TEXT);
        let bar = |scr: &mut super::screen::Screen, x: i32, y: i32, w: i32, cur: i32, max: i32, col: u8, label: &str| {
            scr.fill(x, y, w, 12, gray);
            rect(scr, x - 1, y - 1, w + 2, 14, dark);
            scr.fill(x, y, (w * cur.clamp(0, max.max(1)) / max.max(1)).min(w), 12, col);
            draw_text(scr, f, x + w - text_w(label) - 2, y - 2, label, TEXT);
        };
        // 부대 속성
        let by = y0 + 120;
        etched(scr, LEFT, by, 226, 64, white, dark);
        scr.fill(LEFT + 8, by - 8, 72, 16, gray);
        draw_text(scr, f, LEFT + 10, by - 8, "부대 속성", TEXT);
        draw_text(scr, f, LEFT + 10, by + 12, &job_name, TEXT);
        draw_text(scr, f, LEFT + 10, by + 36, &format!("Lv{:2}", v.off.level), TEXT);
        draw_text(scr, f, LEFT + 62, by + 36, "Exp", TEXT);
        let exp_max = v.off.level >= 50 || v.off.exp == 0xFF;
        bar(scr, LEFT + 94, by + 38, 120, if exp_max { 100 } else { v.off.exp as i32 }, 100, purple, if exp_max { "MAX" } else { "" });
        // 상태
        let sy = by + 80;
        etched(scr, LEFT, sy, 226, 196, white, dark);
        scr.fill(LEFT + 8, sy - 8, 40, 16, gray);
        draw_text(scr, f, LEFT + 10, sy - 8, "상태", TEXT);
        draw_text(scr, f, LEFT + 10, sy + 12, "HP", TEXT);
        bar(scr, LEFT + 40, sy + 14, 174, v.hp.0, v.hp.1, green, &format!("{}/{}", v.hp.0, v.hp.1));
        draw_text(scr, f, LEFT + 10, sy + 36, "MP", TEXT);
        bar(scr, LEFT + 40, sy + 38, 174, v.mp.0, v.mp.1, yellow, &format!("{}/{}", v.mp.0, v.mp.1));
        let mut st: Vec<String> = [(status::POISON, "독"), (status::CONFUSE, "혼란"), (status::PARALYZE, "포박"), (status::SEAL, "봉책")]
            .iter()
            .filter(|(b, _)| v.status & b != 0)
            .map(|(_, n)| n.to_string())
            .collect();
        for (k, n) in ["공격", "방어", "정신", "순발", "사기", "이동"].iter().enumerate() {
            match v.state[k] {
                0..=2 => st.push(format!("{n}↓")),
                3 => {}
                _ => st.push(format!("{n}↑")),
            }
        }
        if st.is_empty() {
            st.push("정상".into());
        }
        for (i, l) in wrap(&st.join(" "), 200).iter().enumerate().take(6) {
            draw_text(scr, f, LEFT + 10, sy + 64 + i as i32 * 20, l, if l == "정상" { TEXT } else { red });
        }
        // 탭
        for &(t, label, tx, ty, tw) in &TABS {
            let on = t == tab;
            scr.fill(tx, ty, tw, TAB_H, if on { white } else { gray });
            rect(scr, tx, ty, tw, TAB_H, dark);
            draw_text(scr, f, tx + (tw - text_w(label)) / 2, ty + 5, label, if on { NAME } else { TEXT });
        }
        let (rx, ry, rw, rh) = RIGHT;
        etched(scr, rx, ry, rw, rh, white, dark);
        match tab {
            // 능력: 무력·지력·통솔·민첩·운 / 공격력·정신력·방어력·순발력·사기 / 이동력
            0 => {
                for (i, (label, val)) in [("무력", ps[0]), ("지력", ps[2]), ("통솔", ps[1]), ("민첩", ps[3]), ("운", ps[4])].into_iter().enumerate() {
                    let (x, y) = (rx + 12 + (i % 3) as i32 * 96, ry + 12 + (i / 3) as i32 * 22);
                    draw_text(scr, f, x, y, label, TEXT);
                    draw_text(scr, f, x + 40, y, &format!("{val:3}"), TEXT);
                }
                for (i, (label, k)) in [("공격력", 0u8), ("정신력", 2), ("방어력", 1), ("순발력", 3), ("사기", 4)].into_iter().enumerate() {
                    let y = ry + 70 + i as i32 * 30;
                    let val = v.off.value(&items, k) as i32;
                    let mark = match v.state[k as usize] {
                        0..=2 => "↓",
                        3 => "",
                        _ => "↑",
                    };
                    draw_text(scr, f, rx + 12, y, &format!("{label}{mark}"), TEXT);
                    bar(scr, rx + 92, y + 2, 196, val, 400, purple, &format!("{val}"));
                }
                let mark = match v.state[5] {
                    0..=2 => " (−1)",
                    3 => "",
                    _ => " (+2)",
                };
                draw_text(scr, f, rx + 12, ry + 228, &format!("이동력  {mv}{mark}"), TEXT);
            }
            // 장비: 무기·방어구 (Lv·Exp·효과), 보조 (효과). 누르면 아이템 정보
            1 => {
                for (s, &(gx, gy, gw, gh)) in GEAR.iter().enumerate() {
                    rect(scr, gx, gy, gw, gh, gray);
                    let label = ["무기", "방어구", "보조"][s];
                    let g = match s {
                        0 => v.off.weapon,
                        1 => v.off.armor,
                        _ => v.off.acc.map(|a| (a, 1)),
                    };
                    let Some((id, lv)) = g else {
                        draw_text(scr, f, gx + 8, gy + 6, &format!("{label}: 없음"), TEXT);
                        continue;
                    };
                    draw_text(scr, f, gx + 8, gy + 6, &format!("{label}: {}", item_name(&items, id)), TEXT);
                    if let Some(ic) = &icons[s] {
                        scr.blit(ic, gx + 8, gy + 28, Some(0));
                    }
                    if s < 2 {
                        let cap = Officer::gear_max(&items, id);
                        draw_text(scr, f, gx + 52, gy + 28, &format!("Lv {lv}"), TEXT);
                        draw_text(scr, f, gx + 52, gy + 48, "Exp", TEXT);
                        let full = lv >= cap;
                        bar(scr, gx + 88, gy + 50, 150, if full { 100 } else { v.off.gear_exp[s] as i32 }, 100, purple, if full { "MAX" } else { "" });
                        draw_text(scr, f, gx + 8, gy + 74, &effect_text(&items, id, lv), TEXT);
                    } else {
                        draw_text(scr, f, gx + 52, gy + 34, &effect_text(&items, id, lv), TEXT);
                    }
                }
                draw_text(scr, f, rx + 8, ry + rh - 22, "아이템을 누르면 아이템 정보", dark);
            }
            // 책략: 이름·MP, 누르면 설명
            2 => {
                if v.strats.is_empty() {
                    draw_text(scr, f, rx + 12, ry + 12, "쓸 수 있는 책략이 없습니다", TEXT);
                }
                for (k, (_, name, mp)) in v.strats.iter().enumerate().skip(strat_top).take(STRAT_ROWS) {
                    let y = ry + 8 + (k - strat_top) as i32 * STRAT_ROW_H;
                    if strat_sel == Some(k) {
                        scr.fill(rx + 4, y - 2, rw - 8, STRAT_ROW_H, white);
                    }
                    draw_text(scr, f, rx + 12, y, name, TEXT);
                    let t = format!("MP {mp:2}");
                    draw_text(scr, f, rx + rw - 16 - text_w(&t), y, &t, TEXT);
                }
                let dy = ry + 8 + STRAT_ROWS as i32 * STRAT_ROW_H + 4;
                if v.strats.len() > STRAT_ROWS {
                    draw_text(scr, f, rx + rw - 120, dy - 2, "▼ 다음 쪽", dark);
                }
                rect(scr, rx + 4, dy + 16, rw - 8, rh - (dy - ry) - 20, gray);
                if strat_desc.is_empty() {
                    draw_text(scr, f, rx + 12, dy + 22, "책략을 누르면 설명", dark);
                }
                for (i, l) in strat_desc.iter().enumerate().take(5) {
                    draw_text(scr, f, rx + 10, dy + 22 + i as i32 * 18, l, TEXT);
                }
            }
            // 장수열전 / 부대특성 (IMSG)
            _ => {
                let lines = if tab == 3 { &bio } else { &unit_note };
                if tab == 4 {
                    draw_text(scr, f, rx + 10, ry + 10, &job_name, NAME);
                }
                let top = if tab == 4 { ry + 34 } else { ry + 10 };
                for (i, l) in lines.iter().enumerate().take(14) {
                    draw_text(scr, f, rx + 10, top + i as i32 * 20, l, TEXT);
                }
            }
        }
        // 단추
        for (b, label) in [(PREV_BTN, "앞의 장수로"), (NEXT_BTN, "다음 장수로"), (CLOSE_BTN, "닫음")] {
            frame_box(scr, b.0, b.1, b.2, b.3);
            draw_text(scr, f, b.0 + (b.2 - text_w(label)) / 2, b.1 + 5, label, TEXT);
        }
        if let Some((id, lv, exp)) = item {
            self.draw_item_info(id, lv, exp);
        }
        if hint {
            let scr = &mut self.screen;
            let Some(f) = self.font.as_ref() else { return };
            let (hx, hy, hw, hh) = (100, 160, 440, 140);
            frame_box(scr, hx, hy, hw, hh);
            scr.fill(hx + 2, hy + 2, hw - 4, 20, navy);
            draw_text(scr, f, hx + 10, hy + 4, "안내", white);
            for (i, l) in ["위의 탭으로 능력·장비·책략·장수열전·", "부대특성을 볼 수 있습니다.", "장비 탭에서 아이템을 누르면 아이템 정보,", "책략 탭에서 책략을 누르면 설명이 나옵니다."].iter().enumerate() {
                draw_text(scr, f, hx + 14, hy + 32 + i as i32 * 22, l, TEXT);
            }
        }
    }
}
