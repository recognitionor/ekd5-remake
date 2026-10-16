//! 전투 (S_xx.EEX). 맵: HMnn.E5 (48×48 칸 그림을 칸 순서로 이어 붙인 8비트), 지형 HEXZMAP[n] (머리 2바이트 = 가로/세로 픽셀÷16, 이어서 칸마다 지형 번호),
//! 팔레트 SPALET[n]. 부대 그림 UNIT_MOV[i] (48×48 × 11프레임: 0-1 아래, 2-3 위, 4-5 옆, 9-10 지침), i = 그룹×3 + 진영 (원작 FUN_00441da9 / FUN_00406ce7 / FUN_00406e15).
//! 직업 표 DATA.E5[3] (27바이트: 이동력, 이동 종류, 능력 성장 5, HP 등급, MP 등급, 장비 가능 18),
//! 지형 표 DATA.E5[4] (이동 종류마다 60바이트: 지형 효과 30 (10 = 100%), 이동 비용 30 (0xFF = 못 감)).
//! 공격/피해 공식과 적 행동은 원작 분석 전이라 단순화한 것이다.

use super::screen::{Pic, Screen};
use super::story::{self, draw_text, frame_box};
use super::{key, Input, Person};
use crate::font::Font;
use std::collections::VecDeque;
use std::rc::Rc;

pub const CELL: i32 = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Player,
    Ally,
    Enemy,
}

impl Side {
    /// 부대 그림 진영 번호 (0 적 빨강, 1 우군 주황, 2 아군 파랑)
    fn color(self) -> usize {
        match self {
            Side::Enemy => 0,
            Side::Ally => 1,
            Side::Player => 2,
        }
    }
    pub fn hostile(self, o: Side) -> bool {
        (self == Side::Enemy) != (o == Side::Enemy)
    }
}

#[derive(Clone)]
pub struct Unit {
    pub person: usize,
    pub side: Side,
    pub x: i32,
    pub y: i32,
    /// 그리기 위치 (픽셀, 이동 중 보간)
    pub px: f32,
    pub py: f32,
    pub path: VecDeque<(i32, i32)>,
    /// 0 아래, 1 위, 2 왼쪽, 3 오른쪽
    pub dir: u8,
    pub hp: i32,
    pub max_hp: i32,
    pub level: i32,
    pub exp: i32,
    pub job: u8,
    pub stats: [u8; 5],
    pub acted: bool,
    pub visible: bool,
    pub ai: u8,
    pub ai_target: Option<(i32, i32)>,
    pub sprite: Option<Rc<Pic>>,
    pub sprite_idx: usize,
    pub pose: Option<u8>,
    pub fade: u8,
}

/// 전투 단계
#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    /// 스크립트 진행 중 (입력 없음)
    Script,
    PlayerIdle,
    Selected { unit: usize, cells: Vec<(i32, i32)> },
    Moving { unit: usize, from: (i32, i32) },
    Menu { unit: usize, from: (i32, i32) },
    Targeting { unit: usize, from: (i32, i32), cells: Vec<(i32, i32)> },
    Attack { unit: usize, target: usize, t: u32 },
    Ai { side: Side, idx: usize, stage: u8 },
    TurnBanner { t: u32, side: Side },
    Over { won: bool, t: u32 },
}

pub struct Battle {
    pub map_no: usize,
    pub map: Option<Pic>,
    pub cols: i32,
    pub rows: i32,
    pub terrain: Vec<u8>,
    pub cam: (i32, i32),
    pub units: Vec<Unit>,
    pub phase: Phase,
    pub turn: i32,
    pub turn_limit: i32,
    pub cursor: (i32, i32),
    /// 대사/해설 겹쳐 그리기용
    pub ui: story::Story,
    pub slots: Vec<(i32, i32)>,
    pub targets: Vec<usize>,
    pub message: Option<(String, u32)>,
    pub popup: Option<(i32, i32, String, u32)>,
    pub objective: String,
    pub started: bool,
    drag: Option<(i32, i32, i32, i32, bool)>,
    pub jobs: Rc<Vec<u8>>,
    pub terrain_tab: Rc<Vec<u8>>,
    pub rng: u32,
    pub attack_target: Option<(usize, u32)>,
    /// 표시 색 (이동 범위, 공격 범위, 아군 막대, 적 막대, 흰색, 노랑, 검정)
    pub col: [u8; 7],
    /// 시험용: 아군 강화
    pub cheat: bool,
}

/// 직업 → 그림 그룹 (원작 FUN_00406ce7 표)
const JOB_GROUP: [u8; 53] = [
    5, 5, 5, 0, 1, 2, 6, 7, 8, 3, 4, 5, 9, 10, 11, 0x12, 0x13, 0x14, 0xC, 0xD, 0xE, 0xF, 0x10, 0x11, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x17, 0x17,
    0x17, 0xE, 0xE, 0xE, 0x21, 0x22, 0x20, 0x1F, 0x1E, 0x17, 0x17, 0x27, 0x23, 0x24, 0x25, 0x26, 0x44, 0x45,
];

/// 고유 그림이 있는 인물 (원작 FUN_00406e15)
fn unique_group(person: usize, job: u8) -> Option<u8> {
    let k = job % 3;
    Some(match person {
        0 => b'(' + k,
        1 => b'+' + k,
        2 => b'.' + k,
        3 => if k != 0 { 0x4D } else { 0x39 },
        0xF => b'I' + k,
        0x10 => b'F' + k,
        0x11 => b'1' + k,
        0x13 => b'4' + k,
        0x20 => if k != 0 { 0x37 } else { 0x4C },
        0x21 => if k != 0 { 0x4E } else { 0x3A },
        0x22 => b';',
        0x23 | 0xAA..=0xAD => b'8',
        0x48 => b'O',
        0x4C | 99 | 100 => b'<',
        0x4D => b'?',
        0x51 => b'A',
        0x65 => b'C',
        0x77 => b'B',
        0x95 => b'D',
        0xA2 | 0xA9 => b'R',
        0xA3 => b'P',
        0xA4 => b'Q',
        0xA5 => b'@',
        0xA6..=0xA8 => b'=',
        _ => return None,
    })
}

pub fn sprite_index(person: usize, job: u8, side: Side) -> usize {
    let g = unique_group(person, job).unwrap_or_else(|| JOB_GROUP.get(job as usize).copied().unwrap_or(0));
    if g < 0x22 {
        g as usize * 3 + side.color()
    } else {
        g as usize + 0x44
    }
}

/// 48×48 칸들을 이어 붙인 맵 → 한 장
pub fn untile_map(d: &[u8], cols: usize, rows: usize) -> Pic {
    let w = cols * 48;
    let mut px = vec![0u8; w * rows * 48];
    for t in 0..cols * rows {
        let (tx, ty) = (t % cols, t / cols);
        for y in 0..48 {
            let s = t * 2304 + y * 48;
            let o = (ty * 48 + y) * w + tx * 48;
            if let Some(src) = d.get(s..s + 48) {
                px[o..o + 48].copy_from_slice(src);
            }
        }
    }
    Pic::new(w, rows * 48, px)
}

impl Battle {
    pub fn new(map_no: usize, jobs: Rc<Vec<u8>>, terrain_tab: Rc<Vec<u8>>) -> Self {
        Self {
            map_no,
            map: None,
            cols: 20,
            rows: 20,
            terrain: Vec::new(),
            cam: (0, 0),
            units: Vec::new(),
            phase: Phase::Script,
            turn: 1,
            turn_limit: 99,
            cursor: (0, 0),
            ui: story::Story::default(),
            slots: vec![(0, 0); 16],
            targets: Vec::new(),
            message: None,
            popup: None,
            objective: String::new(),
            started: false,
            drag: None,
            jobs,
            terrain_tab,
            rng: 12345,
            attack_target: None,
            col: [85, 58, 85, 58, 18, 251, 31],
            cheat: false,
        }
    }

    fn rand(&mut self, n: i32) -> i32 {
        self.rng = self.rng.wrapping_mul(1103515245).wrapping_add(12345);
        ((self.rng >> 16) as i32 & 0x7FFF) % n.max(1)
    }

    pub fn set_map(&mut self, img: &[u8], hex: &[u8]) {
        self.cols = hex[0] as i32 * 16 / CELL;
        self.rows = hex[1] as i32 * 16 / CELL;
        self.terrain = hex[2..].to_vec();
        self.map = Some(untile_map(img, self.cols as usize, self.rows as usize));
    }

    fn job_byte(&self, job: u8, i: usize) -> u8 {
        self.jobs.get(job as usize * 27 + i).copied().unwrap_or(0)
    }

    pub fn make_unit(&self, person: usize, p: &Person, side: Side, x: i32, y: i32, level: i32) -> Unit {
        let level = level.clamp(1, 99);
        let hp = 40 + level * 5 + p.stats[1] as i32 / 2 + self.job_byte(p.job, 7) as i32 * 6;
        Unit {
            person,
            side,
            x,
            y,
            px: (x * CELL) as f32,
            py: (y * CELL) as f32,
            path: VecDeque::new(),
            dir: 0,
            hp,
            max_hp: hp,
            level,
            exp: 0,
            job: p.job,
            stats: p.stats,
            acted: false,
            visible: true,
            ai: 0,
            ai_target: None,
            sprite: None,
            sprite_idx: sprite_index(person, p.job, side),
            pose: None,
            fade: 16,
        }
    }

    pub fn unit_of(&self, person: usize) -> Option<usize> {
        self.units.iter().position(|u| u.person == person)
    }

    pub fn at(&self, x: i32, y: i32) -> Option<usize> {
        self.units.iter().position(|u| u.visible && u.hp > 0 && u.x == x && u.y == y)
    }

    fn terr(&self, x: i32, y: i32) -> u8 {
        self.terrain.get((y * self.cols + x) as usize).copied().unwrap_or(0)
    }

    fn move_cost(&self, job: u8, x: i32, y: i32) -> Option<i32> {
        let mt = self.job_byte(job, 1) as usize;
        let t = self.terr(x, y) as usize;
        match self.terrain_tab.get(mt * 60 + 30 + t.min(29)) {
            Some(&0xFF) | None => None,
            Some(&c) => Some(c as i32),
        }
    }

    /// 지형 효과 (100 = 보통)
    fn terrain_bonus(&self, job: u8, x: i32, y: i32) -> i32 {
        let mt = self.job_byte(job, 1) as usize;
        self.terrain_tab.get(mt * 60 + (self.terr(x, y) as usize).min(29)).map_or(100, |&v| v as i32 * 10)
    }

    /// 이동 가능한 칸들 (적이 막음)
    pub fn reachable(&self, ui: usize) -> Vec<(i32, i32)> {
        let u = &self.units[ui];
        let mp = self.job_byte(u.job, 0) as i32;
        let mut best = vec![i32::MAX; (self.cols * self.rows) as usize];
        let mut q = VecDeque::new();
        best[(u.y * self.cols + u.x) as usize] = 0;
        q.push_back((u.x, u.y));
        while let Some((x, y)) = q.pop_front() {
            let c = best[(y * self.cols + x) as usize];
            for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= self.cols || ny >= self.rows {
                    continue;
                }
                let Some(cost) = self.move_cost(u.job, nx, ny) else { continue };
                if let Some(o) = self.at(nx, ny) {
                    if self.units[o].side.hostile(u.side) {
                        continue;
                    }
                }
                let nc = c + cost;
                if nc <= mp && nc < best[(ny * self.cols + nx) as usize] {
                    best[(ny * self.cols + nx) as usize] = nc;
                    q.push_back((nx, ny));
                }
            }
        }
        let mut out = Vec::new();
        for y in 0..self.rows {
            for x in 0..self.cols {
                if best[(y * self.cols + x) as usize] != i32::MAX && (self.at(x, y).is_none() || (x, y) == (u.x, u.y)) {
                    out.push((x, y));
                }
            }
        }
        out
    }

    /// 경로 (BFS, 이동 비용 무시한 칸 경로)
    fn path_to(&self, ui: usize, to: (i32, i32), allowed: &[(i32, i32)]) -> VecDeque<(i32, i32)> {
        let u = &self.units[ui];
        let mut prev = std::collections::HashMap::new();
        let mut q = VecDeque::new();
        q.push_back((u.x, u.y));
        prev.insert((u.x, u.y), (u.x, u.y));
        while let Some(p) = q.pop_front() {
            if p == to {
                break;
            }
            for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                let n = (p.0 + dx, p.1 + dy);
                if !prev.contains_key(&n) && (allowed.contains(&n) || n == to) {
                    prev.insert(n, p);
                    q.push_back(n);
                }
            }
        }
        let mut path = VecDeque::new();
        let mut c = to;
        while c != (u.x, u.y) {
            path.push_front(c);
            match prev.get(&c) {
                Some(&p) => c = p,
                None => return VecDeque::from(vec![to]),
            }
        }
        path
    }

    /// 공격 사거리 (원작은 무기별 범위 — 활·포차는 2칸, 나머지 붙어서)
    fn attack_cells(&self, ui: usize, from: (i32, i32)) -> Vec<(i32, i32)> {
        let job = self.units[ui].job;
        let bow = self.job_byte(job, 13) != 0 || self.job_byte(job, 17) != 0;
        let mut out = Vec::new();
        for dy in -3i32..=3 {
            for dx in -3i32..=3 {
                let d = dx.abs() + dy.abs();
                let ok = if bow { (2..=2).contains(&d) || (dx.abs() == 1 && dy.abs() == 1) } else { d == 1 };
                if ok {
                    let (x, y) = (from.0 + dx, from.1 + dy);
                    if x >= 0 && y >= 0 && x < self.cols && y < self.rows {
                        out.push((x, y));
                    }
                }
            }
        }
        out
    }

    fn targets_from(&self, ui: usize, from: (i32, i32)) -> Vec<usize> {
        let side = self.units[ui].side;
        self.attack_cells(ui, from).into_iter().filter_map(|(x, y)| self.at(x, y)).filter(|&t| self.units[t].side.hostile(side)).collect()
    }

    /// 피해량 (단순화: 무력·레벨 대 통솔·레벨, 지형 효과)
    fn damage(&mut self, a: usize, d: usize) -> i32 {
        let (ua, ud) = (&self.units[a], &self.units[d]);
        let atk = (ua.stats[0] as i32 + ua.level * 2 + 10) * 2;
        let def = ud.stats[1] as i32 + ud.level * 2;
        let bonus = self.terrain_bonus(ud.job, ud.x, ud.y);
        let base = (atk - def).max(4) * 100 / bonus.max(50);
        let r = self.rand(5) - 2;
        let mut dmg = (base / 2 + r).max(1);
        if self.cheat {
            dmg = if self.units[a].side == Side::Enemy { 1 } else { dmg * 4 };
        }
        dmg
    }

    pub fn alive(&self, side: Side) -> impl Iterator<Item = usize> + '_ {
        self.units.iter().enumerate().filter(move |(_, u)| u.visible && u.hp > 0 && u.side == side).map(|(i, _)| i)
    }

    pub fn focus(&mut self, x: i32, y: i32) {
        let (mw, mh) = (self.cols * CELL, self.rows * CELL);
        self.cam.0 = (x * CELL + 24 - 320).clamp(0, (mw - 640).max(0));
        self.cam.1 = (y * CELL + 24 - 240).clamp(0, (mh - 480).max(0));
    }

    /// 걷기 진행. 모두 섰으면 true
    pub fn animate(&mut self) -> bool {
        let mut idle = true;
        for u in &mut self.units {
            if let Some(&(nx, ny)) = u.path.front() {
                idle = false;
                let (tx, ty) = ((nx * CELL) as f32, (ny * CELL) as f32);
                let (dx, dy) = (tx - u.px, ty - u.py);
                u.dir = if dx.abs() > dy.abs() { if dx < 0.0 { 2 } else { 3 } } else if dy < 0.0 { 1 } else { 0 };
                let sp = 8.0;
                if dx.abs() <= sp && dy.abs() <= sp {
                    u.px = tx;
                    u.py = ty;
                    u.x = nx;
                    u.y = ny;
                    u.path.pop_front();
                } else {
                    u.px += dx.signum() * sp.min(dx.abs());
                    u.py += dy.signum() * sp.min(dy.abs());
                }
            }
        }
        if let Some((_, t)) = &mut self.message {
            *t = t.saturating_sub(1);
            if *t == 0 {
                self.message = None;
            }
        }
        if let Some(p) = &mut self.popup {
            p.3 = p.3.saturating_sub(1);
            if p.3 == 0 {
                self.popup = None;
            }
        }
        idle
    }

    pub fn walk_to(&mut self, ui: usize, x: i32, y: i32) {
        let all: Vec<(i32, i32)> = (0..self.rows).flat_map(|yy| (0..self.cols).map(move |xx| (xx, yy))).collect();
        let p = self.path_to(ui, (x, y), &all);
        self.units[ui].path = p;
    }

    fn cell_at_screen(&self, mx: i32, my: i32) -> (i32, i32) {
        ((mx + self.cam.0).div_euclid(CELL), (my + self.cam.1).div_euclid(CELL))
    }

    /// 사람 차례 입력/진행. 반환: 사건 검사가 필요하면 true
    pub fn update_player(&mut self, inp: &Input) -> bool {
        // 끌어서 화면 옮기기
        let mut tapped = None;
        if let Some((mx, my)) = inp.mouse {
            if inp.held & key::DECIDE != 0 {
                match &mut self.drag {
                    None => self.drag = Some((mx, my, self.cam.0, self.cam.1, false)),
                    Some((sx, sy, cx, cy, moved)) => {
                        if (mx - *sx).abs() + (my - *sy).abs() > 12 {
                            *moved = true;
                        }
                        if *moved {
                            let (mw, mh) = (self.cols * CELL, self.rows * CELL);
                            self.cam.0 = (*cx - (mx - *sx)).clamp(0, (mw - 640).max(0));
                            self.cam.1 = (*cy - (my - *sy)).clamp(0, (mh - 480).max(0));
                        }
                    }
                }
            } else if let Some((_, _, _, _, moved)) = self.drag.take() {
                if !moved {
                    tapped = Some(self.cell_at_screen(mx, my));
                }
            }
        }
        // 키보드
        let mut dc = (0, 0);
        if inp.pressed & key::UP != 0 { dc.1 -= 1; }
        if inp.pressed & key::DOWN != 0 { dc.1 += 1; }
        if inp.pressed & key::LEFT != 0 { dc.0 -= 1; }
        if inp.pressed & key::RIGHT != 0 { dc.0 += 1; }
        if dc != (0, 0) {
            self.cursor = ((self.cursor.0 + dc.0).clamp(0, self.cols - 1), (self.cursor.1 + dc.1).clamp(0, self.rows - 1));
            let c = self.cursor;
            if c.0 * CELL < self.cam.0 + 48 || c.0 * CELL > self.cam.0 + 560 || c.1 * CELL < self.cam.1 + 48 || c.1 * CELL > self.cam.1 + 400 {
                self.focus(c.0, c.1);
            }
        }
        if inp.mouse.is_none() && inp.pressed & key::DECIDE != 0 {
            tapped = Some(self.cursor);
        }
        let cancel = inp.pressed & key::CANCEL != 0;
        if let Some(c) = tapped {
            self.cursor = c;
        }
        match self.phase.clone() {
            Phase::PlayerIdle => {
                // 위쪽 띠 오른쪽 [턴 종료]
                let end_tap = tapped.is_some() && inp.mouse.is_some_and(|(mx, my)| my < 26 && mx > 500);
                if end_tap {
                    self.end_player_turn();
                } else if let Some(c) = tapped {
                    if let Some(u) = self.at(c.0, c.1) {
                        if self.units[u].side == Side::Player && !self.units[u].acted {
                            let cells = self.reachable(u);
                            self.phase = Phase::Selected { unit: u, cells };
                        } else {
                            let name = self.unit_info(u);
                            self.message = Some((name, 60));
                        }
                    }
                }
            }
            Phase::Selected { unit, cells } => {
                if cancel {
                    self.phase = Phase::PlayerIdle;
                } else if let Some(c) = tapped {
                    if cells.contains(&c) {
                        let from = (self.units[unit].x, self.units[unit].y);
                        let p = self.path_to(unit, c, &cells);
                        self.units[unit].path = p;
                        self.phase = Phase::Moving { unit, from };
                    } else {
                        self.phase = Phase::PlayerIdle;
                    }
                }
            }
            Phase::Moving { unit, from } => {
                if self.units[unit].path.is_empty() {
                    self.phase = Phase::Menu { unit, from };
                }
            }
            Phase::Menu { unit, from } => {
                let (u, ux, uy) = (unit, self.units[unit].x, self.units[unit].y);
                let has_target = !self.targets_from(u, (ux, uy)).is_empty();
                if cancel {
                    let un = &mut self.units[u];
                    un.x = from.0;
                    un.y = from.1;
                    un.px = (from.0 * CELL) as f32;
                    un.py = (from.1 * CELL) as f32;
                    self.phase = Phase::PlayerIdle;
                } else if let Some((mx, my)) = inp.mouse.filter(|_| tapped.is_some()) {
                    let (bx, by) = self.menu_pos(u);
                    let item = if mx >= bx && mx < bx + 120 && my >= by { (my - by) / 30 } else { -1 };
                    match item {
                        0 if has_target => {
                            let cells = self.attack_cells(u, (ux, uy));
                            self.phase = Phase::Targeting { unit: u, from, cells };
                        }
                        1 | 0 => {
                            self.units[u].acted = true;
                            self.phase = Phase::PlayerIdle;
                            self.after_player_action();
                            return true;
                        }
                        _ => {}
                    }
                } else if tapped.is_some() {
                    // 키보드: 공격 가능하면 공격, 아니면 대기
                    if has_target {
                        let cells = self.attack_cells(u, (ux, uy));
                        self.phase = Phase::Targeting { unit: u, from, cells };
                    } else {
                        self.units[u].acted = true;
                        self.phase = Phase::PlayerIdle;
                        self.after_player_action();
                        return true;
                    }
                }
            }
            Phase::Targeting { unit, from, cells } => {
                if cancel {
                    self.phase = Phase::Menu { unit, from };
                } else if let Some(c) = tapped {
                    if let Some(t) = self.at(c.0, c.1).filter(|&t| cells.contains(&c) && self.units[t].side.hostile(Side::Player)) {
                        self.phase = Phase::Attack { unit, target: t, t: 0 };
                    } else {
                        self.phase = Phase::Menu { unit, from };
                    }
                }
            }
            Phase::Attack { unit, target, t } => {
                if self.attack_step(unit, target, t) {
                    self.units[unit].acted = true;
                    self.phase = Phase::PlayerIdle;
                    self.after_player_action();
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    fn menu_pos(&self, u: usize) -> (i32, i32) {
        let (sx, sy) = (self.units[u].x * CELL - self.cam.0, self.units[u].y * CELL - self.cam.1);
        ((sx + 56).min(640 - 128), (sy - 8).clamp(8, 480 - 72))
    }

    fn unit_info(&self, u: usize) -> String {
        let un = &self.units[u];
        format!("Lv{} HP {}/{}", un.level, un.hp.max(0), un.max_hp)
    }

    /// 공격 연출. 끝나면 true
    pub fn attack_step(&mut self, a: usize, d: usize, t: u32) -> bool {
        if t == 0 {
            let (ax, ay, dx, dy) = (self.units[a].x, self.units[a].y, self.units[d].x, self.units[d].y);
            self.units[a].dir = if (dx - ax).abs() > (dy - ay).abs() { if dx < ax { 2 } else { 3 } } else if dy < ay { 1 } else { 0 };
            self.units[a].pose = Some(6);
        }
        if t == 8 {
            let dmg = self.damage(a, d);
            self.units[d].hp -= dmg;
            self.units[d].pose = Some(9);
            let (dx, dy) = (self.units[d].x, self.units[d].y);
            self.popup = Some((dx * CELL + 12 - self.cam.0, dy * CELL - 8 - self.cam.1, format!("{dmg}"), 24));
            // 경험치
            let gain = if self.units[d].hp <= 0 { 40 } else { 12 };
            let un = &mut self.units[a];
            un.exp += gain;
            if un.exp >= 100 {
                un.exp -= 100;
                un.level += 1;
                un.max_hp += 6;
                un.hp = (un.hp + 6).min(un.max_hp);
            }
        }
        if t == 24 {
            self.units[a].pose = None;
            self.units[d].pose = None;
            if self.units[d].hp <= 0 {
                self.units[d].fade = 15;
            }
        }
        // 퇴각 사라짐
        if self.units[d].hp <= 0 && t > 24 {
            let f = &mut self.units[d].fade;
            *f = f.saturating_sub(1);
            if *f == 0 {
                self.units[d].visible = false;
                return true;
            }
            return false;
        }
        if let Phase::Attack { t: tt, .. } = &mut self.phase {
            *tt += 1;
        }
        if let Phase::Ai { stage, .. } = &mut self.phase {
            let _ = stage;
        }
        t >= 30
    }

    fn after_player_action(&mut self) {
        if self.alive(Side::Player).all(|i| self.units[i].acted) {
            self.end_player_turn();
        }
    }

    pub fn end_player_turn(&mut self) {
        self.phase = Phase::TurnBanner { t: 0, side: Side::Ally };
    }

    /// 컴퓨터 차례 한 단계. 반환: (사건 검사 필요, 차례 끝)
    pub fn update_ai(&mut self) -> (bool, bool) {
        let Phase::Ai { side, idx, stage } = self.phase.clone() else { return (false, false) };
        let list: Vec<usize> = self.alive(side).collect();
        let Some(&u) = list.get(idx) else { return (false, true) };
        match stage {
            0 => {
                // 목표 정하기
                let cells = self.reachable(u);
                let mut best: Option<((i32, i32), usize, i32)> = None;
                let hold = self.units[u].ai == 1;
                for &c in &cells {
                    if hold && c != (self.units[u].x, self.units[u].y) {
                        continue;
                    }
                    for t in self.targets_from(u, c) {
                        let score = 1000 - self.units[t].hp;
                        if best.is_none_or(|b| score > b.2) {
                            best = Some((c, t, score));
                        }
                    }
                }
                let dest = match best {
                    Some((c, _, _)) => c,
                    None if hold => (self.units[u].x, self.units[u].y),
                    None => {
                        // 가장 가까운 적에게 다가감 (또는 지정 지점)
                        let goal = self.units[u].ai_target.or_else(|| {
                            let (ux, uy) = (self.units[u].x, self.units[u].y);
                            let s = self.units[u].side;
                            self.units.iter().filter(|o| o.visible && o.hp > 0 && o.side.hostile(s)).map(|o| (o.x, o.y)).min_by_key(|&(x, y)| (x - ux).abs() + (y - uy).abs())
                        });
                        match goal {
                            Some(g) => *cells.iter().min_by_key(|&&(x, y)| (x - g.0).abs() + (y - g.1).abs()).unwrap_or(&(self.units[u].x, self.units[u].y)),
                            None => (self.units[u].x, self.units[u].y),
                        }
                    }
                };
                let p = self.path_to(u, dest, &cells);
                self.units[u].path = p;
                if self.units[u].x * CELL < self.cam.0 || self.units[u].x * CELL > self.cam.0 + 592 || self.units[u].y * CELL < self.cam.1 || self.units[u].y * CELL > self.cam.1 + 432 {
                    let (x, y) = (self.units[u].x, self.units[u].y);
                    self.focus(x, y);
                }
                self.phase = Phase::Ai { side, idx, stage: 1 };
            }
            1 => {
                if self.units[u].path.is_empty() {
                    let (x, y) = (self.units[u].x, self.units[u].y);
                    let t = self.targets_from(u, (x, y)).into_iter().min_by_key(|&t| self.units[t].hp);
                    match t {
                        Some(t) => {
                            self.phase = Phase::Ai { side, idx, stage: 2 };
                            self.attack_target = Some((t, 0));
                        }
                        None => {
                            self.phase = Phase::Ai { side, idx: idx + 1, stage: 0 };
                            return (true, false);
                        }
                    }
                }
            }
            _ => {
                let Some((t, tt)) = self.attack_target else {
                    self.phase = Phase::Ai { side, idx: idx + 1, stage: 0 };
                    return (true, false);
                };
                let done = self.attack_step(u, t, tt);
                self.attack_target = Some((t, tt + 1));
                if done {
                    self.attack_target = None;
                    // 죽은 부대가 목록에서 빠지므로 같은 번호를 다시 (자기 자신이 죽지는 않음)
                    self.phase = Phase::Ai { side, idx: idx + 1, stage: 0 };
                    return (true, false);
                }
            }
        }
        (false, false)
    }

    pub fn render(&mut self, scr: &mut Screen, font: Option<&Font>, tick: u32) {
        scr.clear(0);
        if let Some(m) = &self.map {
            scr.blit_rect(m, self.cam.0, self.cam.1, 640, 480, 0, 0, None);
        }
        // 범위 표시 (점무늬)
        let (cells, col): (Vec<(i32, i32)>, u8) = match &self.phase {
            Phase::Selected { cells, .. } => (cells.clone(), self.col[0]),
            Phase::Targeting { cells, .. } => (cells.clone(), self.col[1]),
            _ => (Vec::new(), 0),
        };
        for (x, y) in cells {
            let (sx, sy) = (x * CELL - self.cam.0, y * CELL - self.cam.1);
            for yy in 0..CELL {
                for xx in 0..CELL {
                    if (xx + yy) % 2 == 0 {
                        scr.fill(sx + xx, sy + yy, 1, 1, col);
                    }
                }
            }
        }
        // 부대
        let mut order: Vec<usize> = (0..self.units.len()).filter(|&i| self.units[i].visible).collect();
        order.sort_by(|&a, &b| self.units[a].py.partial_cmp(&self.units[b].py).unwrap_or(std::cmp::Ordering::Equal));
        for i in order {
            let u = &self.units[i];
            let Some(sp) = &u.sprite else { continue };
            if u.fade < 16 && (tick / 2) % 2 == 0 {
                continue;
            }
            let walking = !u.path.is_empty();
            let step = if walking { (tick / 4) % 2 } else { (tick / 16) % 2 } as i32;
            let (frame, flip) = match u.pose {
                Some(9) => (9 + step, false),
                Some(6) => (6 + (u.dir.min(2)) as i32, u.dir == 3),
                _ => match u.dir {
                    0 => (step, false),
                    1 => (2 + step, false),
                    2 => (4 + step, false),
                    _ => (4 + step, true),
                },
            };
            let frame = if u.hp > 0 && u.hp * 4 < u.max_hp && u.pose.is_none() && !walking { 9 + step } else { frame };
            let (sx, sy) = (u.px as i32 - self.cam.0, u.py as i32 - self.cam.1);
            if flip {
                for yy in 0..48 {
                    for xx in 0..48 {
                        let c = sp.px[((frame * 48 + yy) as usize) * 48 + (47 - xx) as usize];
                        if c != 0 {
                            scr.fill(sx + xx, sy + yy, 1, 1, c);
                        }
                    }
                }
            } else {
                scr.blit_rect(sp, 0, frame * 48, 48, 48, sx, sy, Some(0));
            }
            if u.acted && u.side == Side::Player && matches!(self.phase, Phase::PlayerIdle | Phase::Selected { .. } | Phase::Menu { .. } | Phase::Targeting { .. } | Phase::Moving { .. }) {
                // 행동 끝 표시: 어두운 점
                for yy in (0..48).step_by(2) {
                    for xx in (0..48).step_by(2) {
                        scr.fill(sx + xx + (yy / 2) % 2, sy + yy, 1, 1, 31);
                    }
                }
            }
            // HP 막대
            let w = (u.hp.max(0) * 40 / u.max_hp.max(1)).clamp(0, 40);
            scr.fill(sx + 4, sy + 44, 40, 3, self.col[6]);
            scr.fill(sx + 4, sy + 44, w, 3, if u.side == Side::Enemy { self.col[3] } else { self.col[2] });
        }
        // 커서
        let (cx, cy) = (self.cursor.0 * CELL - self.cam.0, self.cursor.1 * CELL - self.cam.1);
        if (tick / 8) % 2 == 0 {
            scr.fill(cx, cy, 48, 2, 18);
            scr.fill(cx, cy + 46, 48, 2, 18);
            scr.fill(cx, cy, 2, 48, 18);
            scr.fill(cx + 46, cy, 2, 48, 18);
        }
        let Some(f) = font else { return };
        if let Some((x, y, s, _)) = &self.popup {
            draw_text(scr, f, *x, *y, s, 251);
        }
        // 위 정보 띠
        frame_box(scr, 0, 0, 640, 24);
        let side_name = match &self.phase {
            Phase::Ai { side: Side::Enemy, .. } | Phase::TurnBanner { side: Side::Enemy, .. } => "적군",
            Phase::Ai { side: Side::Ally, .. } | Phase::TurnBanner { side: Side::Ally, .. } => "우군",
            _ => "아군",
        };
        draw_text(scr, f, 8, 4, &format!("{}턴/{}  {}", self.turn, self.turn_limit, side_name), 18);
        if !self.objective.is_empty() {
            draw_text(scr, f, 200, 4, &self.objective, 251);
        }
        if matches!(self.phase, Phase::PlayerIdle) {
            draw_text(scr, f, 540, 4, "[턴 종료]", 18);
        }
        // 선택된 부대 정보
        let sel = match &self.phase {
            Phase::Selected { unit, .. } | Phase::Menu { unit, .. } | Phase::Targeting { unit, .. } | Phase::Moving { unit, .. } => Some(*unit),
            _ => None,
        };
        if let Some(u) = sel {
            self.draw_unit_box(scr, f, u);
        }
        if let Phase::Menu { unit, .. } = &self.phase {
            let (bx, by) = self.menu_pos(*unit);
            let has = !self.targets_from(*unit, (self.units[*unit].x, self.units[*unit].y)).is_empty();
            frame_box(scr, bx, by, 120, 64);
            draw_text(scr, f, bx + 16, by + 8, "공격", if has { 18 } else { 24 });
            draw_text(scr, f, bx + 16, by + 38, "대기", 18);
        }
        if let Some((m, _)) = &self.message {
            frame_box(scr, 180, 440, 280, 32);
            draw_text(scr, f, 196, 448, m, 18);
        }
        match &self.phase {
            Phase::TurnBanner { side, .. } => {
                let s = match side {
                    Side::Player => format!("제 {}턴  아군 차례", self.turn),
                    Side::Ally => "우군 차례".to_string(),
                    Side::Enemy => "적군 차례".to_string(),
                };
                frame_box(scr, 200, 210, 240, 44);
                draw_text(scr, f, 320 - s.chars().count() as i32 * 7, 224, &s, 18);
            }
            Phase::Over { won, .. } => {
                let s = if *won { "승리!" } else { "패배……" };
                frame_box(scr, 240, 200, 160, 60);
                draw_text(scr, f, 296, 222, s, 251);
            }
            _ => {}
        }
        self.ui.render_overlay(scr, Some(f), tick);
    }

    fn draw_unit_box(&self, scr: &mut Screen, f: &Font, u: usize) {
        let un = &self.units[u];
        frame_box(scr, 0, 432, 260, 48);
        draw_text(scr, f, 8, 438, &format!("Lv{} HP {}/{}", un.level, un.hp.max(0), un.max_hp), 18);
        draw_text(scr, f, 8, 458, &format!("무{} 통{} 지{} 민{}", un.stats[0], un.stats[1], un.stats[2], un.stats[3]), 18);
    }
}
