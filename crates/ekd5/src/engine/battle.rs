//! 전투 (S_xx.EEX). 맵: HMnn.E5 (48×48 칸 그림을 칸 순서로 이어 붙인 8비트), 지형 HEXZMAP[n] (머리 2바이트 = 가로/세로 픽셀÷16, 이어서 칸마다 지형 번호),
//! 팔레트 SPALET[n]. 부대 그림 UNIT_MOV[i] (48×48 × 11프레임: 0-1 아래, 2-3 위, 4-5 옆, 9-10 지침), i = 그룹×3 + 진영 (원작 FUN_00441da9 / FUN_00406ce7 / FUN_00406e15).
//! 직업 표 DATA.E5[3] (27바이트: 이동력, 이동 종류, 능력 성장 5, HP 등급, MP 등급, 장비 가능 18),
//! 지형 표 DATA.E5[4] (이동 종류마다 60바이트: 지형 효과 30 (10 = 100%), 이동 비용 30 (0xFF = 못 감)).
//! 공격/피해 공식과 적 행동은 원작 분석 전이라 단순화한 것이다.

use super::screen::{Pic, Screen};
use super::story::{self, draw_text, frame_box};
use super::{key, Input};
use crate::font::Font;
use crate::officer::Officer;
use crate::strategy::{self, status, Effect, Strategy};
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
    pub job: u8,
    /// 능력 (원작 장수 기록)
    pub off: Officer,
    pub mp: i32,
    /// 능력 상태 (3 = 보통, <3 ×0.7, ≥4 ×1.2) — 원작 부대 +0x18..
    pub state: [u8; 5],
    /// 상태이상 비트 (독 2, 혼란 4, 마비 8, 봉책 0x10)
    pub status: u8,
    pub acted: bool,
    pub visible: bool,
    /// 스크립트 AI (원작 표 0x498bf8): 0 적극 공격, 1 범위 안만 공격, 2 제자리 고수, 3 장수 공격, 4 지점으로, 5 장수 따라가기, 6 지점으로 도주
    pub ai: u8,
    /// AI 3·5 의 대상 장수
    pub ai_person: Option<usize>,
    pub ai_target: Option<(i32, i32)>,
    pub sprite: Option<Rc<Pic>>,
    /// 공격 그림 (UNIT_ATK 64×64×12: 아래 0-3, 위 4-7, 옆 8-11), 특수 (UNIT_SPC 48×48×5: 막기 0-2, 맞음 3, 승리 4)
    pub atk: Option<Rc<Pic>>,
    pub spc: Option<Rc<Pic>>,
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
    /// 책략 고르기
    StratList { unit: usize, from: (i32, i32), list: Vec<u8> },
    StratTarget { unit: usize, from: (i32, i32), sid: u8, cells: Vec<(i32, i32)> },
    /// 책략 시전 연출
    Cast { unit: usize, sid: u8, cell: (i32, i32), t: u32 },
    /// 도구 고르기 (소지품 번호들)
    ToolList { unit: usize, from: (i32, i32), list: Vec<usize> },
    ToolTarget { unit: usize, from: (i32, i32), inv: usize, cells: Vec<(i32, i32)> },
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
    /// 아이템 표 (DATA.E5[1])
    pub items: Rc<Vec<u8>>,
    /// 진행 중인 공격 (공격자, 대상, 피해(None=빗나감), 치명타)
    pub strikes: VecDeque<(usize, usize, Option<i32>, bool)>,
    /// 책략 표
    pub strategies: Rc<Vec<Strategy>>,
    /// 소지품 (아이템, 레벨) — 전투 끝나면 엔진에 되돌림
    pub inventory: Vec<(u8, u8)>,
    pub rng: u32,
    pub attack_target: Option<(usize, u32)>,
    /// 표시 색 (이동 범위, 공격 범위, 아군 막대, 적 막대, 흰색, 노랑, 검정)
    pub col: [u8; 7],
    /// 시험용: 아군 강화
    pub cheat: bool,
    strike_t: u32,
    killed_by: Option<usize>,
    cast_fx: Vec<(i32, i32)>,
    /// 맵 물체 (op58): (x, y, 종류, 원래 지형). 종류 0 불, 1 짚더미, 2 타는 짚더미, 4.. 성문 등 (GATE.E5)
    pub objs: Vec<(i32, i32, u8, u8)>,
    /// 칸별 겹침 그림 종류 (0xFF 없음, 원작 지도 +8000)
    pub overlay: Vec<u8>,
    /// 겹침 그림 (MARK.E5: 불 4장, 짚더미 2장, 타는 짚더미 2장)
    pub mark_tiles: Vec<Pic>,
    /// 덮어 그릴 GATE.E5 그림 (번호, x, y) — 엔진이 읽어 붙임
    pub gate_queue: Vec<(usize, i32, i32)>,
    /// 목표 구역 표시 (op5b: x1, y1, x2, y2)
    pub goal_zone: Option<(i32, i32, i32, i32)>,
    /// op47 적 목록 순서 (op55 번호 지정용)
    pub list47: Vec<usize>,
    /// 일기토 화면
    pub duel: Option<super::duel::Duel>,
    duel_tick: u32,
    /// 전투 시작 연출 진행 틱 (원작 op5a FUN_0044db19: 가장자리부터 열고 "작전개시")
    pub intro: Option<u32>,
    /// 책략 연출 (원작 MEFF/MCALL)
    pub fx: Option<super::fx::Fx>,
    /// 연출 읽기 요청 (연출 번호, 중심들) — 엔진이 읽어 fx 로
    pub fx_req: Option<(u8, Vec<(i32, i32)>)>,
    cast_applied: bool,
    cast_end: Option<u32>,
    /// 낼 효과음 (엔진이 가져감)
    pub sounds: Vec<String>,
    ai_plan: Option<AiPlan>,
    cast_t: u32,
    /// 시험 기록용
    pub cast_log: Vec<String>,
}

/// 컴퓨터 행동 계획
#[derive(Clone, Debug)]
pub enum AiPlan {
    Attack(usize),
    Strat(u8, (i32, i32)),
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
/// 세로로 쌓인 그림의 k 번째 장 (size×size), 0 = 투명
fn draw_frame(scr: &mut Screen, p: &Pic, size: usize, k: usize, dx: i32, dy: i32, flip: bool) {
    let base = k * size * size;
    for y in 0..size {
        for x in 0..size {
            let sx = if flip { size - 1 - x } else { x };
            let Some(&c) = p.px.get(base + y * size + sx) else { return };
            if c != 0 {
                scr.fill(dx + x as i32, dy + y as i32, 1, 1, c);
            }
        }
    }
}

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
            items: Rc::new(Vec::new()),
            strikes: VecDeque::new(),
            strategies: Rc::new(Vec::new()),
            inventory: Vec::new(),
            rng: 12345,
            attack_target: None,
            col: [85, 58, 85, 58, 18, 251, 31],
            cheat: false,
            strike_t: 0,
            killed_by: None,
            cast_fx: Vec::new(),
            objs: Vec::new(),
            overlay: Vec::new(),
            mark_tiles: Vec::new(),
            gate_queue: Vec::new(),
            goal_zone: None,
            list47: Vec::new(),
            duel: None,
            duel_tick: 0,
            intro: None,
            fx: None,
            fx_req: None,
            cast_applied: false,
            cast_end: None,
            sounds: Vec::new(),
            ai_plan: None,
            cast_t: 0,
            cast_log: Vec::new(),
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
        self.overlay = vec![0xFF; (self.cols * self.rows) as usize];
        self.map = Some(untile_map(img, self.cols as usize, self.rows as usize));
    }

    /// 물체 놓기 (원작 FUN_00458120): 지형을 바꾸고 원래 지형을 기억. 종류 0 → 지형 0x1A, 1·2 → 0x1B.
    /// 반환: 바로 옆에 이미 성문이 있는지 (두 칸짜리 성문의 둘째 칸)
    pub fn place_obj(&mut self, kind: u8, terr: u8, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x >= self.cols || y >= self.rows || self.objs.iter().any(|o| (o.0, o.1, o.2) == (x, y, kind)) {
            return false;
        }
        let terr = match kind {
            0 => 0x1A,
            1 | 2 => 0x1B,
            _ => terr,
        };
        let i = (y * self.cols + x) as usize;
        let old = self.terrain.get(i).copied().unwrap_or(0);
        self.objs.push((x, y, kind, old));
        let near = terr == 0x11 && [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| self.terr_at(x + dx, y + dy) == Some(0x11));
        if let Some(t) = self.terrain.get_mut(i) {
            *t = terr;
        }
        if kind < 4 {
            self.overlay[i] = kind;
        } else {
            self.gate_queue.push((kind as usize * 2 - 8, x, y));
        }
        near
    }

    /// 물체 치우기 (원작 FUN_004587a1): 원래 지형으로. 반환: 치운 것이 성문이었나
    pub fn remove_obj(&mut self, kind: u8, x: i32, y: i32) -> bool {
        let Some(k) = self.objs.iter().position(|o| (o.0, o.1, o.2) == (x, y, kind)) else { return false };
        let (_, _, _, old) = self.objs.remove(k);
        let i = (y * self.cols + x) as usize;
        let was_gate = self.terrain.get(i) == Some(&0x11);
        if let Some(t) = self.terrain.get_mut(i) {
            *t = old;
        }
        if kind < 4 {
            self.overlay[i] = 0xFF;
        } else {
            self.gate_queue.push((kind as usize * 2 - 7, x, y));
        }
        was_gate
    }

    fn terr_at(&self, x: i32, y: i32) -> Option<u8> {
        if x < 0 || y < 0 || x >= self.cols || y >= self.rows {
            return None;
        }
        self.terrain.get((y * self.cols + x) as usize).copied()
    }

    /// GATE.E5 그림 (3×3 칸, 칸마다 48×48) 을 (x, y) 둘레에 덮어 그림
    pub fn patch_gate(&mut self, d: &[u8], x: i32, y: i32) {
        let (cols, rows) = (self.cols, self.rows);
        let Some(m) = self.map.as_mut() else { return };
        let mut k = 0;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (tx, ty) = (x + dx, y + dy);
                if tx >= 0 && ty >= 0 && tx < cols && ty < rows {
                    for yy in 0..48 {
                        let s = k * 2304 + yy * 48;
                        let o = (ty as usize * 48 + yy) * m.w + tx as usize * 48;
                        if let Some(src) = d.get(s..s + 48) {
                            m.px[o..o + 48].copy_from_slice(src);
                        }
                    }
                }
                k += 1;
            }
        }
    }

    fn job_byte(&self, job: u8, i: usize) -> u8 {
        self.jobs.get(job as usize * 27 + i).copied().unwrap_or(0)
    }

    pub fn make_unit(&self, off: Officer, side: Side, x: i32, y: i32) -> Unit {
        let person = off.person;
        Unit {
            person,
            side,
            x,
            y,
            px: (x * CELL) as f32,
            py: (y * CELL) as f32,
            path: VecDeque::new(),
            dir: 0,
            hp: off.max_hp as i32,
            max_hp: off.max_hp as i32,
            level: off.level as i32,
            job: off.job,
            mp: off.max_mp as i32,
            state: [3; 5],
            status: 0,
            acted: false,
            visible: true,
            ai: 0,
            ai_target: None,
            ai_person: None,
            sprite: None,
            atk: None,
            spc: None,
            sprite_idx: sprite_index(person, off.job, side),
            pose: None,
            fade: 16,
            off,
        }
    }

    /// (x, y) 에서 가장 가까운 빈 칸 (지나갈 수 있는 곳)
    pub fn free_near(&self, job: u8, x: i32, y: i32) -> (i32, i32) {
        for r in 0i32..8 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    let (cx, cy) = (x + dx, y + dy);
                    if cx >= 0 && cy >= 0 && cx < self.cols && cy < self.rows && self.at(cx, cy).is_none() && self.move_cost(job, cx, cy).is_some() {
                        return (cx, cy);
                    }
                }
            }
        }
        (x, y)
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

    /// 지형 표 행 (원작 FUN_00407bc0: 직업 < 39 → 직업/3, < 53 → 직업−26)
    fn move_row(job: u8) -> usize {
        if job < 0x27 { job as usize / 3 } else if job < 0x35 { job as usize - 0x1A } else { 0x1A }
    }

    fn move_cost(&self, job: u8, x: i32, y: i32) -> Option<i32> {
        let mt = Self::move_row(job);
        let t = self.terr(x, y) as usize;
        match self.terrain_tab.get(mt * 60 + 30 + t.min(29)) {
            Some(&0xFF) | None => None,
            Some(&c) => Some(c as i32),
        }
    }

    /// 지형 효과 (100 = 보통)
    fn terrain_bonus(&self, job: u8, x: i32, y: i32) -> i32 {
        let mt = Self::move_row(job);
        self.terrain_tab.get(mt * 60 + (self.terr(x, y) as usize).min(29)).map_or(100, |&v| v as i32 * 10)
    }

    /// 이동 가능한 칸들 (적이 막음)
    pub fn reachable(&self, ui: usize) -> Vec<(i32, i32)> {
        let u = &self.units[ui];
        if u.x < 0 || u.y < 0 || u.x >= self.cols || u.y >= self.rows {
            return Vec::new();
        }
        let mp = if u.status & status::PARALYZE != 0 { 0 } else { self.job_byte(u.job, 0) as i32 };
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

    /// 목표 칸까지 지형 이동 비용 거리 (부대 무시)
    fn dist_field(&self, job: u8, goal: (i32, i32)) -> Vec<i32> {
        let n = (self.cols * self.rows) as usize;
        let mut d = vec![i32::MAX / 2; n];
        let mut q = VecDeque::new();
        if goal.0 >= 0 && goal.1 >= 0 && goal.0 < self.cols && goal.1 < self.rows {
            d[(goal.1 * self.cols + goal.0) as usize] = 0;
            q.push_back(goal);
        }
        while let Some((x, y)) = q.pop_front() {
            let c = d[(y * self.cols + x) as usize];
            for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= self.cols || ny >= self.rows {
                    continue;
                }
                let Some(cost) = self.move_cost(job, nx, ny) else { continue };
                let i = (ny * self.cols + nx) as usize;
                if c + cost < d[i] {
                    d[i] = c + cost;
                    q.push_back((nx, ny));
                }
            }
        }
        d
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

    /// 공격 범위 (원작 FUN_00454878, 종류 = 직업표 +1): 0 상하좌우 1 주위 8칸 2 대각+2칸 십자 3 대각+거리2 테두리+3칸 십자
    /// 4 3에 거리3 테두리·(±2,±2) 5 상하좌우+2칸 십자 6 주위8+2칸 십자 7 맨해튼 4 8 맨해튼 4·5 9 2칸 십자 10 전체 11 없음
    pub fn range_offsets(kind: u8) -> Vec<(i32, i32)> {
        let ortho = [(0, -1), (0, 1), (-1, 0), (1, 0)];
        let diag = [(1, -1), (1, 1), (-1, 1), (-1, -1)];
        let cross2 = [(-2, 0), (2, 0), (0, -2), (0, 2)];
        let ring = |r: i32| -> Vec<(i32, i32)> {
            let mut v = Vec::new();
            for k in -1..=1 {
                v.push((k, -r));
                v.push((k, r));
                v.push((r, k));
                v.push((-r, k));
            }
            v
        };
        let manhattan = |d: i32| -> Vec<(i32, i32)> {
            let mut v = Vec::new();
            for y in -d..=d {
                for x in -d..=d {
                    if x.abs() + y.abs() == d {
                        v.push((x, y));
                    }
                }
            }
            v
        };
        let mut o: Vec<(i32, i32)> = Vec::new();
        match kind {
            0 => o.extend(ortho),
            1 => {
                o.extend(ortho);
                o.extend(diag);
            }
            2 => {
                o.extend(diag);
                o.extend(cross2);
            }
            3 | 0xC => {
                if kind == 0xC {
                    o.extend(ortho);
                }
                o.extend(diag);
                o.extend(ring(2));
                o.extend([(-3, 0), (3, 0), (0, -3), (0, 3)]);
            }
            4 | 0xD => {
                if kind == 0xD {
                    o.extend(ortho);
                    o.extend([(-4, 0), (4, 0), (0, -4), (0, 4)]);
                }
                o.extend(diag);
                o.extend(ring(2));
                o.extend(ring(3));
                o.extend([(-2, -2), (2, 2), (2, -2), (-2, 2)]);
            }
            5 => {
                o.extend(ortho);
                o.extend(cross2);
            }
            6 => {
                o.extend(ortho);
                o.extend(diag);
                o.extend(cross2);
            }
            7 => o.extend(manhattan(4)),
            8 => {
                o.extend(manhattan(4));
                o.extend(manhattan(5));
            }
            9 => o.extend(cross2),
            _ => {}
        }
        o.sort();
        o.dedup();
        o
    }

    fn range_kind(&self, ui: usize) -> u8 {
        self.job_byte(self.units[ui].job, 1)
    }

    fn attack_cells(&self, ui: usize, from: (i32, i32)) -> Vec<(i32, i32)> {
        let k = self.range_kind(ui);
        if k == 10 {
            return (0..self.rows).flat_map(|y| (0..self.cols).map(move |x| (x, y))).collect();
        }
        Self::range_offsets(k)
            .into_iter()
            .map(|(dx, dy)| (from.0 + dx, from.1 + dy))
            .filter(|&(x, y)| x >= 0 && y >= 0 && x < self.cols && y < self.rows)
            .collect()
    }

    fn in_range(&self, ui: usize, from: (i32, i32), cell: (i32, i32)) -> bool {
        let k = self.range_kind(ui);
        k == 10 || Self::range_offsets(k).contains(&(cell.0 - from.0, cell.1 - from.1))
    }

    fn targets_from(&self, ui: usize, from: (i32, i32)) -> Vec<usize> {
        let side = self.units[ui].side;
        self.attack_cells(ui, from).into_iter().filter_map(|(x, y)| self.at(x, y)).filter(|&t| self.units[t].side.hostile(side)).collect()
    }

    /// 능력치 (0 공격 1 방어 2 정신 3 순발 4 사기). 원작은 부대 상태(3 = 보통)에 따라 ×0.7 / ×1.2
    fn val(&self, u: usize, stat: u8) -> u32 {
        let v = self.units[u].off.value(&self.items, stat) as u32;
        match self.units[u].state[stat as usize] {
            0..=2 => v * 7 / 10,
            3 => v,
            _ => (v * 12 / 10).min(400),
        }
    }

    /// 쓸 수 있는 책략 (직업·레벨, MP, 봉책 아님)
    pub fn known_strategies(&self, u: usize) -> Vec<u8> {
        let un = &self.units[u];
        if un.status & status::SEAL != 0 {
            return Vec::new();
        }
        self.strategies.iter().filter(|s| s.known(un.job, un.level as u8)).map(|s| s.id).collect()
    }

    fn strat(&self, sid: u8) -> Option<&Strategy> {
        self.strategies.get(sid as usize)
    }

    /// 책략 사거리 칸
    pub fn strat_cells(&self, u: usize, sid: u8) -> Vec<(i32, i32)> {
        let Some(st) = self.strat(sid) else { return Vec::new() };
        let from = (self.units[u].x, self.units[u].y);
        let mut v: Vec<(i32, i32)> = if st.range == 10 {
            (0..self.rows).flat_map(|y| (0..self.cols).map(move |x| (x, y))).collect()
        } else {
            Self::range_offsets(st.range).into_iter().map(|(dx, dy)| (from.0 + dx, from.1 + dy)).collect()
        };
        if st.target == 1 || st.range == 11 {
            v.push(from);
        }
        v.retain(|&(x, y)| x >= 0 && y >= 0 && x < self.cols && y < self.rows);
        v.sort();
        v.dedup();
        v
    }

    /// 그 칸에 책략을 쓰면 영향받는 부대들
    fn strat_victims(&self, u: usize, sid: u8, cell: (i32, i32)) -> Vec<usize> {
        let Some(st) = self.strat(sid) else { return Vec::new() };
        let side = self.units[u].side;
        st.area_offsets()
            .into_iter()
            .filter_map(|(dx, dy)| self.at(cell.0 + dx, cell.1 + dy))
            .filter(|&t| if st.target == 0 { self.units[t].side.hostile(side) } else { !self.units[t].side.hostile(side) })
            .collect()
    }

    /// 책략 성공률 (원작 FUN_0043d84f: (정신+사기) 비교 곡선 × 책략별 배율, 포박된 대상은 100)
    fn strat_rate_for(&self, a: usize, d: usize, sid: u8) -> u32 {
        if self.units[d].status & status::PARALYZE != 0 && sid != 9 && sid != 21 {
            return 100;
        }
        let base = self.strat_rate(a, d);
        match sid {
            9 | 21 => {
                if self.units[d].person < 0x1A || self.targets.contains(&self.units[d].person) { 0 } else { base / if sid == 9 { 3 } else { 2 } }
            }
            _ => base * strategy::rate_scale(sid) / 100,
        }
    }

    fn strat_rate(&self, a: usize, d: usize) -> u32 {
        let (x, y) = (self.val(a, 2) + self.val(a, 4), (self.val(d, 2) + self.val(d, 4)).max(1));
        if x >= y * 2 {
            100
        } else if x >= y {
            (x - y) * 10 / y + 90
        } else if x >= y / 2 {
            (x - y / 2) * 30 / (y / 2).max(1) + 60
        } else {
            x.saturating_sub(y / 3) * 30 / (y / 3).max(1) + 30
        }
    }

    /// 책략 피해 (원작 FUN_0043e0ba)
    fn strat_damage(&mut self, a: usize, d: usize, pw: u32) -> i32 {
        let (ia, id) = (self.val(a, 2), self.val(d, 2));
        let mut v = self.units[a].level.max(1) as u32 + 25;
        if ia < id {
            v = v.saturating_sub((id - ia) / 3);
        } else {
            v += (ia - id) / 3;
        }
        v = v * pw / 10;
        match self.units[d].job {
            0x27 | 0x2A | 0x2B => v = v * 11 / 10,
            0x31 | 0x32 => v = v * 13 / 10,
            _ => {}
        }
        let r = self.rand(7) as u32;
        v = if r < 3 { v.saturating_sub(r) } else { v - 2 + r };
        if self.cheat {
            v = if self.units[a].side == Side::Enemy { 1 } else { v * 4 };
        }
        (v.max(1) as i32).min(self.units[d].hp.max(0))
    }

    /// 장비 경험치 (원작 FUN_00403cdd / FUN_00403de5 / FUN_00423923 / FUN_00423a3b): 기본치 × k / 10
    fn gear_exp(&mut self, who: usize, slot: usize, k: u32) {
        if self.units[who].side != Side::Player || k == 0 {
            return;
        }
        let items = self.items.clone();
        let e = self.units[who].off.gear_base(&items) * k / 10;
        if self.units[who].off.gain_gear_exp(&items, slot, e) {
            let lv = if slot == 0 { self.units[who].off.weapon } else { self.units[who].off.armor }.map_or(0, |w| w.1);
            let m = format!("{} 레벨 업! Lv{lv}", if slot == 0 { "무기" } else { "방어구" });
            self.cast_log.push(format!("@{m}"));
            self.message = Some((m, 60));
            self.sounds.push("SE13.WAV".into());
        }
    }

    fn weapon_stat(&self, u: usize) -> Option<u8> {
        self.units[u].off.weapon.and_then(|(id, _)| crate::officer::item_stat(&self.items, id))
    }

    /// 책략 효과 적용. 반환: 표시 글 (칸, 글)
    fn apply_strategy(&mut self, u: usize, sid: u8, cell: (i32, i32)) -> Vec<((i32, i32), String)> {
        let mut out = Vec::new();
        let Some(st) = self.strat(sid).cloned() else { return out };
        self.units[u].mp = (self.units[u].mp - st.mp as i32).max(0);
        let jobs = self.jobs.clone();
        let victims = self.strat_victims(u, sid, cell);
        // 시전자 무기 경험치 (정신 무기만): 대상 레벨이 낮으면 2, 아니면 3
        if self.weapon_stat(u) == Some(2) {
            let lu = self.units[u].level;
            let k = if st.target < 2 && !victims.is_empty() && victims.iter().all(|&t| self.units[t].level < lu) { 2 } else { 3 };
            self.gear_exp(u, 0, k);
        }
        for t in victims {
            let pos = (self.units[t].x, self.units[t].y);
            let hostile = st.target == 0;
            let terr = self.terr(pos.0, pos.1);
            let already = matches!(strategy::effect(sid), Effect::Status(bit, _) if self.units[t].status & bit != 0);
            if !strategy::terrain_ok(sid, terr) || already {
                out.push((pos, "효과 없음".to_string()));
                continue;
            }
            if hostile && (self.rand(100) as u32) >= self.strat_rate_for(u, t, sid) && !(self.cheat && self.units[u].side != Side::Enemy) {
                out.push((pos, "실패".to_string()));
                continue;
            }
            let mut dmg = 0;
            let text = match strategy::effect(sid) {
                Effect::Damage(pw) => {
                    dmg = self.strat_damage(u, t, pw);
                    format!("{dmg}")
                }
                Effect::Instant => {
                    dmg = self.units[t].hp.max(0);
                    format!("{dmg}")
                }
                Effect::MpDamage(pw) => {
                    let m = (self.strat_damage(u, t, pw)).min(self.units[t].mp);
                    self.units[t].mp -= m;
                    format!("MP-{m}")
                }
                Effect::Status(bit, pw) => {
                    if pw > 0 {
                        dmg = self.strat_damage(u, t, pw);
                    }
                    self.units[t].status |= bit;
                    let name = match bit {
                        status::POISON => "독",
                        status::CONFUSE => "혼란",
                        status::PARALYZE => "마비",
                        _ => "봉책",
                    };
                    if dmg > 0 { format!("{dmg} {name}") } else { name.to_string() }
                }
                Effect::Heal(big) => {
                    let i = self.val(u, 2) as i32;
                    let h = if big { i / 2 + 70 } else { i / 10 + 40 };
                    let un = &mut self.units[t];
                    let h = h.min(un.max_hp - un.hp).max(0);
                    un.hp += h;
                    format!("+{h}")
                }
                Effect::Mp(n) => {
                    let un = &mut self.units[t];
                    let m = (n as i32).min(un.off.max_mp as i32 - un.mp).max(0);
                    un.mp += m;
                    format!("MP+{m}")
                }
                Effect::Buff(k, up) => {
                    let s = &mut self.units[t].state[k as usize];
                    *s = if up { (*s + 1).clamp(4, 5) } else { s.saturating_sub(1).min(2) };
                    let names = ["공격", "방어", "정신", "순발", "사기"];
                    format!("{}{}", names[k as usize], if up { "↑" } else { "↓" })
                }
                Effect::Cure => {
                    self.units[t].status = 0;
                    "회복".to_string()
                }
                Effect::None => String::new(),
            };
            if dmg > 0 {
                self.units[t].hp -= dmg;
                if self.units[t].hp <= 0 {
                    self.units[t].fade = 15;
                }
                if hostile {
                    // 맞은 쪽 방어구 경험치: 시전자 레벨이 낮으면 3, 아니면 4
                    let k = if self.units[u].level < self.units[t].level { 3 } else { 4 };
                    self.gear_exp(t, 1, k);
                }
                let e = Self::exp_for(self.units[u].level, self.units[t].level, self.units[t].hp <= 0);
                self.units[u].off.gain_exp(&jobs, e);
            } else if !hostile {
                // 아군 지원도 경험치 조금
                self.units[u].off.gain_exp(&jobs, 8);
            }
            out.push((pos, text));
        }
        let un = &mut self.units[u];
        if un.off.level as i32 > un.level {
            let old = un.max_hp;
            un.max_hp = un.off.max_hp as i32;
            un.hp += un.max_hp - old;
            un.level = un.off.level as i32;
            self.message = Some((format!("레벨 업! Lv{}", un.level), 60));
            self.sounds.push("SE11.WAV".into());
        }
        out
    }

    /// 책략 연출. 끝나면 true (원작 FUN_004021bc: 연출을 단계마다 그리고, 표지 1 단계에서 효과)
    pub fn cast_step(&mut self, u: usize, sid: u8, cell: (i32, i32), t: u32) -> bool {
        if t == 0 {
            self.units[u].pose = Some(6);
            self.cast_fx = self.strat(sid).map(|s| s.area_offsets().into_iter().map(|(dx, dy)| (cell.0 + dx, cell.1 + dy)).collect()).unwrap_or_default();
            let name = self.strat(sid).map(|s| s.name.clone()).unwrap_or_default();
            self.cast_log.push(name.clone());
            self.message = Some((name, 40));
            self.cast_applied = false;
            self.cast_end = None;
            if cell.0 * CELL < self.cam.0 + 48 || cell.0 * CELL > self.cam.0 + 544 || cell.1 * CELL < self.cam.1 + 48 || cell.1 * CELL > self.cam.1 + 384 {
                self.focus(cell.0, cell.1);
            }
            let id = super::fx::STRATEGY_FX.get(sid as usize).copied().unwrap_or(255);
            if id != 255 {
                let mid = |(x, y): (i32, i32)| (x * CELL + CELL / 2, y * CELL + CELL / 2);
                let centers: Vec<(i32, i32)> = if id >= 100 {
                    vec![mid(cell)]
                } else {
                    let v = self.strat_victims(u, sid, cell);
                    if v.is_empty() { vec![mid(cell)] } else { v.iter().map(|&i| mid((self.units[i].x, self.units[i].y))).collect() }
                };
                self.fx_req = Some((id, centers));
            }
        }
        if self.fx_req.is_some() {
            return false;
        }
        if self.cast_end.is_none() {
            if let Some(mut f) = self.fx.take() {
                if f.sound_now() {
                    self.sounds.push(format!("SE_M_{:02}.WAV", f.sound));
                }
                let hit = f.hit_now() || !f.has_hit() && f.step * 2 >= 1 && f.step == 1;
                let done = f.advance();
                if hit && !self.cast_applied {
                    self.cast_apply(u, sid, cell);
                }
                if done {
                    if !self.cast_applied {
                        self.cast_apply(u, sid, cell);
                    }
                    self.cast_end = Some(t);
                } else {
                    self.fx = Some(f);
                    return false;
                }
            } else {
                // 연출 없음: 예전 시간표
                if t >= 14 && !self.cast_applied {
                    self.cast_apply(u, sid, cell);
                }
                if t < 30 {
                    return false;
                }
                self.cast_end = Some(t);
            }
        }
        self.units[u].pose = None;
        self.cast_fx.clear();
        let mut fading = false;
        for un in &mut self.units {
            if un.visible && un.hp <= 0 {
                fading = true;
                un.fade = un.fade.saturating_sub(1);
                if un.fade == 0 {
                    un.visible = false;
                }
            }
        }
        !fading
    }

    fn cast_apply(&mut self, u: usize, sid: u8, cell: (i32, i32)) {
        self.cast_applied = true;
        let res = self.apply_strategy(u, sid, cell);
        if let Some(((x, y), txt)) = res.first() {
            self.popup = Some((x * CELL + 4 - self.cam.0, y * CELL - 8 - self.cam.1, txt.clone(), 30));
        }
    }

    /// 예상 물리 피해 (난수·명중 빼고)
    fn est_damage(&self, a: usize, d: usize) -> i32 {
        let atk = self.val(a, 0);
        let def = self.val(d, 1);
        let mut dmg = self.units[a].level.max(1) as u32 + 25;
        if atk < def {
            dmg = dmg.saturating_sub((def - atk) >> 1).max(1);
        } else {
            dmg += (atk - def) >> 1;
        }
        let (ga, gd) = (self.group(a), self.group(d));
        if ga == 1 && gd == 3 {
            dmg = dmg * 3 / 5;
        } else if (ga == 1 && (gd == 2 || gd == 5)) || (matches!(ga, 2 | 4 | 5) && matches!(gd, 3 | 0x10 | 0x11)) {
            dmg = dmg * 3 / 2;
        }
        (dmg as i32).min(self.units[d].hp) * self.hit_rate(a, d) as i32 / 100
    }

    /// 책략 예상 점수 (from 칸에서 cell 에 쓸 때)
    fn est_strategy(&self, u: usize, sid: u8, from: (i32, i32), cell: (i32, i32)) -> i32 {
        let Some(st) = self.strat(sid) else { return 0 };
        if cell.0 < 0 || cell.1 < 0 || cell.0 >= self.cols || cell.1 >= self.rows {
            return 0;
        }
        let in_range = cell == from && (st.target == 1 || st.range == 11) || Self::range_offsets(st.range).contains(&(cell.0 - from.0, cell.1 - from.1));
        if !in_range {
            return 0;
        }
        let side = self.units[u].side;
        let mut score = 0;
        for (dx, dy) in st.area_offsets() {
            let Some(t) = self.at(cell.0 + dx, cell.1 + dy) else { continue };
            if t == u && st.target == 0 {
                continue;
            }
            let hostile = self.units[t].side.hostile(side);
            match strategy::effect(sid) {
                Effect::Damage(pw) | Effect::Status(_, pw) if hostile && st.target == 0 && strategy::terrain_ok(sid, self.terr(cell.0 + dx, cell.1 + dy)) => {
                    let (ia, id) = (self.val(u, 2), self.val(t, 2));
                    let mut v = self.units[u].level.max(1) + 25;
                    v += (ia as i32 - id as i32) / 3;
                    let d = (v.max(1) * pw as i32 / 10).min(self.units[t].hp);
                    let already = matches!(strategy::effect(sid), Effect::Status(bit, _) if self.units[t].status & bit != 0);
                    if !already {
                        score += (d * self.strat_rate_for(u, t, sid) as i32 / 100) * 10 + if d >= self.units[t].hp { 500 } else { 0 } + 20;
                    }
                }
                Effect::Heal(big) if !hostile && st.target == 1 => {
                    let lost = self.units[t].max_hp - self.units[t].hp;
                    if lost * 10 > self.units[t].max_hp * 3 {
                        let i = self.val(u, 2) as i32;
                        score += (if big { i / 2 + 70 } else { i / 10 + 40 }).min(lost) * 12;
                    }
                }
                Effect::Cure if !hostile && self.units[t].status != 0 => score += 200,
                _ => {}
            }
        }
        score
    }

    /// 차례 시작: 상태이상 (독 피해, 회복 확률 — 원작 FUN_0043e8e6)
    pub fn start_side(&mut self, side: Side) {
        let idx: Vec<usize> = self.alive(side).collect();
        for i in idx {
            let luck = self.val(i, 4);
            let cure = (luck / 3).clamp(10, 60);
            let un = &mut self.units[i];
            if un.status & status::POISON != 0 {
                let d = (un.max_hp / 10).max(1).min(un.hp - 1).max(0);
                un.hp -= d;
            }
            for bit in [status::POISON, status::CONFUSE, status::PARALYZE, status::SEAL] {
                if self.units[i].status & bit != 0 && (self.rand(100) as u32) < cure {
                    self.units[i].status &= !bit;
                }
            }
            // 능력 상태는 차례마다 보통 쪽으로 한 칸
            for k in 0..5 {
                let r = self.rand(3);
                let st = &mut self.units[i].state[k];
                if *st > 3 && r == 0 {
                    *st -= 1;
                } else if *st < 3 && r == 0 {
                    *st += 1;
                }
            }
        }
    }

    /// 지형 표 행 = 병종 (원작 FUN_00407bc0)
    fn group(&self, u: usize) -> usize {
        Self::move_row(self.units[u].job)
    }

    /// 물리 피해 (원작 FUN_0043dd5b). counter = 반격 (×3/4)
    fn damage(&mut self, a: usize, d: usize, counter: bool) -> i32 {
        let ea = self.terrain_bonus(self.units[a].job, self.units[a].x, self.units[a].y) as u32 / 10;
        let ed = self.terrain_bonus(self.units[d].job, self.units[d].x, self.units[d].y) as u32 / 10;
        let atk = self.val(a, 0) * ea / 10;
        let def = self.val(d, 1) * ed / 10;
        let mut dmg = self.units[a].level.max(1) as u32 + 25;
        if atk < def {
            dmg = dmg.saturating_sub((def - atk) >> 1).max(1);
        } else {
            dmg += (atk - def) >> 1;
        }
        let (ga, gd) = (self.group(a), self.group(d));
        match ga {
            1 => {
                if gd == 3 {
                    dmg = (dmg * 3 / 5).max(1);
                } else if gd == 2 || gd == 5 {
                    dmg = dmg * 3 >> 1;
                }
            }
            2 | 4 | 5 => {
                if gd == 3 || gd == 0x11 || gd == 0x10 {
                    dmg = dmg * 3 >> 1;
                }
            }
            _ => {}
        }
        if counter {
            dmg = (dmg * 3 >> 2).max(1);
        }
        let r = self.rand(7) as u32;
        dmg = if r < 3 { dmg.saturating_sub(r).max(1) } else { dmg - 2 + r };
        if self.cheat {
            dmg = if self.units[a].side == Side::Enemy { 1 } else { dmg * 4 };
        }
        (dmg as i32).min(self.units[d].hp.max(0))
    }

    /// 비율 곡선 (원작 FUN_00406046 / FUN_004060c9): a ≥ 3d 100, ≥ 2d 20+80(a−2d)/d, ≥ d 2+18(a−d)/d, 그 외 1
    fn ratio_curve(a: u32, d: u32) -> u32 {
        let d = d.max(1);
        if a >= d * 3 {
            100
        } else if a >= d * 2 {
            (a - d * 2) * 80 / d + 20
        } else if a >= d {
            (a - d) * 18 / d + 2
        } else {
            1
        }
    }

    /// 명중률 (원작 FUN_0043d61d, 순발력 비교)
    fn hit_rate(&self, a: usize, d: usize) -> u32 {
        let (x, y) = (self.val(a, 3), self.val(d, 3).max(1));
        if x >= y * 2 {
            100
        } else if x >= y {
            (x - y) * 10 / y + 90
        } else if x >= y / 2 {
            (x - y / 2) * 30 / (y / 2).max(1) + 60
        } else {
            x.saturating_sub(y / 3) * 30 / (y / 3).max(1) + 30
        }
    }

    /// 공격 한 번 결과 (명중, 치명타 ×1.5, 연속 공격, 반격)
    fn resolve(&mut self, a: usize, d: usize) {
        self.strikes.clear();
        let mut hp_d = self.units[d].hp;
        let mut hp_a = self.units[a].hp;
        let strike = |b: &mut Self, x: usize, y: usize, counter: bool, crit_ok: bool, hp: &mut i32| {
            let hit = (b.rand(100) as u32) < b.hit_rate(x, y) || b.cheat && b.units[x].side != Side::Enemy;
            if !hit {
                b.strikes.push_back((x, y, None, false));
                return;
            }
            let crit = crit_ok && (b.rand(100) as u32) < Self::ratio_curve(b.val(x, 4), b.val(y, 4));
            let mut dmg = b.damage(x, y, counter);
            if crit {
                dmg = dmg * 3 / 2;
            }
            let dmg = dmg.min(*hp).max(0);
            *hp -= dmg;
            b.strikes.push_back((x, y, Some(dmg), crit));
        };
        strike(self, a, d, false, true, &mut hp_d);
        if hp_d > 0 && (self.rand(100) as u32) < Self::ratio_curve(self.val(a, 3), self.val(d, 3)) {
            strike(self, a, d, false, false, &mut hp_d);
        }
        let (ax, ay, dx, dy) = (self.units[a].x, self.units[a].y, self.units[d].x, self.units[d].y);
        if hp_d > 0 && self.in_range(d, (dx, dy), (ax, ay)) {
            strike(self, d, a, true, false, &mut hp_a);
        }
    }

    /// 경험치 (원작 FUN_00403afb): 격파 32+8×레벨차(최대 200) / 아니면 8+2×레벨차(최대 100)
    fn exp_for(la: i32, lt: i32, killed: bool) -> u32 {
        let (la, lt) = (la.max(1) as u32, lt.max(1) as u32);
        if killed {
            if la < lt { ((lt - la) * 8 + 32).min(200) } else { 32u32.saturating_sub((la - lt) * 4).max(10) }
        } else if la < lt {
            ((lt - la) * 2 + 8).min(100)
        } else {
            8u32.saturating_sub(la - lt).max(1)
        }
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
                let u = unit;
                let items = self.menu_items(u);
                if cancel {
                    let un = &mut self.units[u];
                    un.x = from.0;
                    un.y = from.1;
                    un.px = (from.0 * CELL) as f32;
                    un.py = (from.1 * CELL) as f32;
                    self.phase = Phase::PlayerIdle;
                } else if tapped.is_some() {
                    let pick = match inp.mouse {
                        Some((mx, my)) => {
                            let (bx, by) = self.menu_pos(u);
                            if mx >= bx && mx < bx + 120 && my >= by && my < by + items.len() as i32 * 30 {
                                Some(((my - by) / 30) as usize)
                            } else {
                                None
                            }
                        }
                        None => Some(0),
                    };
                    if let Some(k) = pick {
                        match items.get(k).map(|s| s.0) {
                            Some("공격") => {
                                let cells = self.attack_cells(u, (self.units[u].x, self.units[u].y));
                                self.phase = Phase::Targeting { unit: u, from, cells };
                            }
                            Some("책략") => {
                                let list = self.known_strategies(u);
                                self.phase = Phase::StratList { unit: u, from, list };
                            }
                            Some("도구") => {
                                let list = self.tool_list();
                                self.phase = Phase::ToolList { unit: u, from, list };
                            }
                            Some("대기") => {
                                self.units[u].acted = true;
                                self.phase = Phase::PlayerIdle;
                                self.after_player_action();
                                return true;
                            }
                            _ => {}
                        }
                    }
                }
            }
            Phase::StratList { unit, from, list } => {
                if cancel {
                    self.phase = Phase::Menu { unit, from };
                } else if let (Some(_), Some((mx, my))) = (tapped, inp.mouse) {
                    let (bx, by, w, _) = self.strat_box(list.len());
                    if mx >= bx && mx < bx + w && my >= by + 8 {
                        let k = ((my - by - 8) / 22) as usize;
                        if let Some(&sid) = list.get(k) {
                            let need = self.strat(sid).map_or(0, |s| s.mp as i32);
                            if self.units[unit].mp >= need {
                                let cells = self.strat_cells(unit, sid);
                                self.phase = Phase::StratTarget { unit, from, sid, cells };
                            } else {
                                self.message = Some(("MP가 모자랍니다".into(), 40));
                            }
                        }
                    } else {
                        self.phase = Phase::Menu { unit, from };
                    }
                }
            }
            Phase::ToolList { unit, from, list } => {
                if cancel {
                    self.phase = Phase::Menu { unit, from };
                } else if let (Some(_), Some((mx, my))) = (tapped, inp.mouse) {
                    let (bx, by, w, _) = self.strat_box(list.len());
                    if mx >= bx && mx < bx + w && my >= by + 8 {
                        let k = ((my - by - 8) / 22) as usize;
                        if let Some(&inv) = list.get(k) {
                            let (x, y) = (self.units[unit].x, self.units[unit].y);
                            let cells: Vec<(i32, i32)> = [(0, 0), (0, -1), (0, 1), (-1, 0), (1, 0)].iter().map(|(dx, dy)| (x + dx, y + dy)).collect();
                            self.phase = Phase::ToolTarget { unit, from, inv, cells };
                        }
                    } else {
                        self.phase = Phase::Menu { unit, from };
                    }
                }
            }
            Phase::ToolTarget { unit, from, inv, cells } => {
                if cancel {
                    let list = self.tool_list();
                    self.phase = Phase::ToolList { unit, from, list };
                } else if let Some(c) = tapped {
                    match self.at(c.0, c.1).filter(|&t| cells.contains(&c) && !self.units[t].side.hostile(Side::Player)) {
                        Some(t) => {
                            let msg = self.use_tool(inv, t);
                            let (tx, ty) = (self.units[t].x, self.units[t].y);
                            self.popup = Some((tx * CELL + 4 - self.cam.0, ty * CELL - 8 - self.cam.1, msg, 30));
                            self.units[unit].acted = true;
                            self.phase = Phase::PlayerIdle;
                            self.after_player_action();
                            return true;
                        }
                        None => self.message = Some(("대상이 없습니다".into(), 40)),
                    }
                }
            }
            Phase::StratTarget { unit, from, sid, cells } => {
                if cancel {
                    let list = self.known_strategies(unit);
                    self.phase = Phase::StratList { unit, from, list };
                } else if let Some(c) = tapped {
                    if cells.contains(&c) && !self.strat_victims(unit, sid, c).is_empty() {
                        self.phase = Phase::Cast { unit, sid, cell: c, t: 0 };
                    } else {
                        self.message = Some(("대상이 없습니다".into(), 40));
                    }
                }
            }
            Phase::Cast { unit, sid, cell, t } => {
                if self.cast_step(unit, sid, cell, t) {
                    self.units[unit].acted = true;
                    self.phase = Phase::PlayerIdle;
                    self.after_player_action();
                    return true;
                }
                self.phase = Phase::Cast { unit, sid, cell, t: t + 1 };
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

    /// 쓸 수 있는 소모품 (소지품 번호, 같은 종류는 하나만)
    pub fn tool_list(&self) -> Vec<usize> {
        let mut seen = Vec::new();
        let mut out = Vec::new();
        for (i, &(it, _)) in self.inventory.iter().enumerate() {
            let t = self.items.get(it as usize * 25 + 17).copied().unwrap_or(0);
            if t >= 0x3F && !seen.contains(&it) {
                seen.push(it);
                out.push(i);
            }
        }
        out
    }

    /// 소모품 사용 (원작 아이템 종류 0x3f HP, 0x40 MP, 0x41~0x45 상태 치료, 0x46~0x4a 능력 열매, 0x4b 경험, 0x4c 인삼)
    fn use_tool(&mut self, inv: usize, t: usize) -> String {
        let Some(&(it, _)) = self.inventory.get(inv) else { return String::new() };
        self.inventory.remove(inv);
        let rec = |i: usize| self.items.get(it as usize * 25 + i).copied().unwrap_or(0);
        let (ty, v) = (rec(17), rec(21) as i32);
        let jobs = self.jobs.clone();
        let un = &mut self.units[t];
        match ty {
            0x3F => {
                let h = if v == 0xFF { un.max_hp } else { v }.min(un.max_hp - un.hp);
                un.hp += h;
                format!("+{h}")
            }
            0x40 => {
                let m = v.min(un.off.max_mp as i32 - un.mp);
                un.mp += m;
                format!("MP+{m}")
            }
            0x41 => {
                un.status &= !status::CONFUSE;
                "혼란 회복".into()
            }
            0x42 => {
                un.status &= !status::POISON;
                "해독".into()
            }
            0x43 => {
                un.status &= !status::PARALYZE;
                "포박 회복".into()
            }
            0x44 => {
                un.status &= !status::SEAL;
                "봉책 회복".into()
            }
            0x45 => {
                un.status = 0;
                "모두 회복".into()
            }
            0x46..=0x4A => {
                // 무력·지력·통솔·민첩·운 → 공격·정신·방어·순발·사기
                let k = [0usize, 2, 1, 3, 4][(ty - 0x46) as usize];
                un.off.stats[k] = (un.off.stats[k] + v as u16).min(255);
                format!("{}+{v}", ["공격", "방어", "정신", "순발", "사기"][k])
            }
            0x4B => {
                un.off.gain_exp(&jobs, v as u32);
                un.level = un.off.level as i32;
                un.max_hp = un.off.max_hp as i32;
                format!("경험+{v}")
            }
            _ => {
                un.hp = un.max_hp;
                un.mp = un.off.max_mp as i32;
                "완전 회복".into()
            }
        }
    }

    /// 행동 메뉴 항목 (이름, 쓸 수 있나)
    fn menu_items(&self, u: usize) -> Vec<(&'static str, bool)> {
        let (x, y) = (self.units[u].x, self.units[u].y);
        let mut v = vec![("공격", !self.targets_from(u, (x, y)).is_empty())];
        let strats = self.known_strategies(u);
        if !strats.is_empty() {
            v.push(("책략", true));
        }
        if self.units[u].side == Side::Player && !self.tool_list().is_empty() {
            v.push(("도구", true));
        }
        v.push(("대기", true));
        v
    }

    /// 책략 목록 창 (x, y, 폭, 높이)
    fn strat_box(&self, n: usize) -> (i32, i32, i32, i32) {
        let h = n as i32 * 22 + 16;
        (180, (240 - h / 2).max(30), 280, h)
    }

    fn menu_pos(&self, u: usize) -> (i32, i32) {
        let (sx, sy) = (self.units[u].x * CELL - self.cam.0, self.units[u].y * CELL - self.cam.1);
        ((sx + 56).min(640 - 128), (sy - 8).clamp(8, 480 - 72))
    }

    fn unit_info(&self, u: usize) -> String {
        let un = &self.units[u];
        format!("Lv{} HP {}/{}", un.level, un.hp.max(0), un.max_hp)
    }

    /// 공격 연출 (한 번에 한 타격씩, 18틱). 끝나면 true
    pub fn attack_step(&mut self, a: usize, d: usize, t: u32) -> bool {
        if t == 0 {
            self.resolve(a, d);
            self.strike_t = 0;
            self.killed_by = None;
        }
        if let Phase::Attack { t: tt, .. } = &mut self.phase {
            *tt += 1;
        }
        if let Some(&(x, y, dmg, crit)) = self.strikes.front() {
            let st = self.strike_t;
            self.strike_t += 1;
            if st == 0 {
                let (ux, uy, vx, vy) = (self.units[x].x, self.units[x].y, self.units[y].x, self.units[y].y);
                self.units[x].dir = if (vx - ux).abs() > (vy - uy).abs() { if vx < ux { 2 } else { 3 } } else if vy < uy { 1 } else { 0 };
                self.units[x].pose = Some(6);
                let m = format!("@공격 {}→{}", self.units[x].person, self.units[y].person);
                self.cast_log.push(m);
                // 휘두르는 소리 (원작 FUN_00404849: 그룹 0,1,7,9,10,14,15,18 → 34, 2·4 → 37, 그 외 32)
                let g = self.group(x);
                let se = if matches!(g, 0 | 1 | 7 | 9 | 10 | 14 | 15 | 18) { 0x22 } else if matches!(g, 2 | 4) { 0x25 } else { 0x20 };
                self.sounds.push(format!("SE{se:02}.WAV"));
            }
            if st == 8 {
                let (vx, vy) = (self.units[y].x, self.units[y].y);
                // 장비 경험치: 공격자 무기 (공격 무기만, 빗나감 1 / 상대 레벨이 낮으면 2 / 3), 맞은 쪽 방어구 (빗나감 1 / 공격자 레벨이 낮으면 3 / 4)
                let (lx, ly) = (self.units[x].level, self.units[y].level);
                if self.weapon_stat(x) == Some(0) {
                    self.gear_exp(x, 0, if dmg.is_none() { 1 } else if ly < lx { 2 } else { 3 });
                }
                self.gear_exp(y, 1, if dmg.is_none() { 1 } else if lx < ly { 3 } else { 4 });
                // 맞는 소리 (원작 FUN_004044e6: 피해 없음 30, 맞음 35, 치명이면 +1)
                self.sounds.push(format!("SE{:02}.WAV", if dmg.is_some() { 35 } else { 30 } + crit as u32));
                let text = match dmg {
                    None => "빗나감".to_string(),
                    Some(v) => {
                        self.units[y].hp -= v;
                        self.units[y].pose = Some(9);
                        if self.units[y].hp <= 0 {
                            self.killed_by = Some(x);
                            // 퇴각 소리 (원작 FUN_004438a5)
                            self.sounds.push("SE16.WAV".into());
                        }
                        // 경험치
                        let killed = self.units[y].hp <= 0;
                        let e = Self::exp_for(self.units[x].level, self.units[y].level, killed);
                        let ups = self.units[x].off.gain_exp(&self.jobs.clone(), e);
                        if ups > 0 {
                            let un = &mut self.units[x];
                            let old_max = un.max_hp;
                            un.max_hp = un.off.max_hp as i32;
                            un.hp += un.max_hp - old_max;
                            un.level = un.off.level as i32;
                            self.message = Some((format!("레벨 업! Lv{}", un.level), 60));
            self.sounds.push("SE11.WAV".into());
                        }
                        if crit { format!("치명타 {v}") } else { format!("{v}") }
                    }
                };
                self.popup = Some((vx * CELL + 8 - self.cam.0, vy * CELL - 8 - self.cam.1, text, 24));
            }
            if st >= 18 {
                self.units[x].pose = None;
                self.units[y].pose = None;
                self.strikes.pop_front();
                self.strike_t = 0;
                if self.units[y].hp <= 0 {
                    self.units[y].fade = 15;
                    self.strikes.clear();
                }
            }
            return false;
        }
        // 퇴각 사라짐
        let mut fading = false;
        for u in &mut self.units {
            if u.visible && u.hp <= 0 {
                fading = true;
                u.fade = u.fade.saturating_sub(1);
                if u.fade == 0 {
                    u.visible = false;
                }
            }
        }
        !fading
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
                // 목표 정하기: 갈 수 있는 칸마다 공격/책략의 예상 효과로 점수
                let cells = self.reachable(u);
                let ai = self.units[u].ai;
                let hold = ai == 2;
                // 따라가기·도주 (원작 내부 방식 4) 는 공격하지 않음
                let no_attack = matches!(ai, 5 | 6);
                // 시험 자동 진행: 목표 구역이 있으면 조조군은 그쪽으로, 조조는 싸우지 않고 감
                let zone_goal = if self.units[u].side == Side::Player {
                    self.goal_zone.map(|(x1, y1, x2, y2)| {
                        let (ux, uy) = (self.units[u].x, self.units[u].y);
                        (ux.clamp(x1.min(x2), x1.max(x2)), uy.clamp(y1.min(y2), y1.max(y2)))
                    })
                } else {
                    None
                };
                // 시험 자동 진행: 조조가 다치면 적에게서 멀어짐
                let cao_retreat = self.units[u].side == Side::Player && self.units[u].person == 0 && self.units[u].hp * 2 < self.units[u].max_hp;
                let no_attack = no_attack || cao_retreat || (zone_goal.is_some() && self.units[u].person == 0);
                let person_goal = zone_goal.or(self.units[u].ai_person.and_then(|p| self.units.iter().find(|o| o.person == p && o.visible && o.hp > 0)).map(|o| (o.x, o.y)));
                let confused = self.units[u].status & status::CONFUSE != 0;
                let mut best: Option<((i32, i32), AiPlan, i32)> = None;
                if !confused && !no_attack {
                    let strats: Vec<u8> = self.known_strategies(u).into_iter().filter(|&sid| self.strat(sid).is_some_and(|st| st.mp as i32 <= self.units[u].mp)).collect();
                    for &c in &cells {
                        if hold && c != (self.units[u].x, self.units[u].y) {
                            continue;
                        }
                        for t in self.targets_from(u, c) {
                            let d = self.est_damage(u, t);
                            let focus = ai == 3 && self.units[u].ai_person == Some(self.units[t].person);
                            let score = d * 10 + if d >= self.units[t].hp { 500 } else { 0 } + if focus { 1000 } else { 0 };
                            if best.as_ref().is_none_or(|b| score > b.2) {
                                best = Some((c, AiPlan::Attack(t), score));
                            }
                        }
                        for &sid in &strats {
                            let Some(st) = self.strat(sid).cloned() else { continue };
                            for (dx, dy) in Self::range_offsets(st.range).into_iter().chain(std::iter::once((0, 0))) {
                                let cell = (c.0 + dx, c.1 + dy);
                                let score = self.est_strategy(u, sid, c, cell);
                                if score > 0 && best.as_ref().is_none_or(|b| score > b.2) {
                                    best = Some((c, AiPlan::Strat(sid, cell), score));
                                }
                            }
                        }
                    }
                }
                self.ai_plan = best.as_ref().map(|b| b.1.clone());
                let dest = match best {
                    Some((c, _, _)) => c,
                    None if cao_retreat && zone_goal.is_none() => {
                        let s = self.units[u].side;
                        let foes: Vec<(i32, i32)> = self.units.iter().filter(|o| o.visible && o.hp > 0 && o.side.hostile(s)).map(|o| (o.x, o.y)).collect();
                        *cells.iter().max_by_key(|c| foes.iter().map(|f| (f.0 - c.0).abs() + (f.1 - c.1).abs()).min().unwrap_or(0)).unwrap_or(&(self.units[u].x, self.units[u].y))
                    }
                    // 범위 안만 공격 (1) 은 칠 상대가 없으면 기다림
                    None if hold || confused || ai == 1 => (self.units[u].x, self.units[u].y),
                    None => {
                        // 가장 가까운 적에게 다가감 (또는 지정 장수·지점)
                        let goal = person_goal.or(self.units[u].ai_target).or_else(|| {
                            let (ux, uy) = (self.units[u].x, self.units[u].y);
                            let s = self.units[u].side;
                            self.units.iter().filter(|o| o.visible && o.hp > 0 && o.side.hostile(s)).map(|o| (o.x, o.y)).min_by_key(|&(x, y)| (x - ux).abs() + (y - uy).abs())
                        });
                        match goal {
                            Some(g) => {
                                // 지형을 따라 잰 거리 (벽은 돌아감)
                                let df = self.dist_field(self.units[u].job, g);
                                let idx = |c: &(i32, i32)| (c.1 * self.cols + c.0) as usize;
                                *cells.iter().min_by_key(|c| df[idx(c)]).unwrap_or(&(self.units[u].x, self.units[u].y))
                            }
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
                    match self.ai_plan.take() {
                        Some(AiPlan::Strat(sid, cell)) if !self.strat_victims(u, sid, cell).is_empty() => {
                            self.cast_t = 0;
                            self.phase = Phase::Ai { side, idx, stage: 3 };
                            self.ai_plan = Some(AiPlan::Strat(sid, cell));
                        }
                        other => {
                            let t = match other {
                                Some(AiPlan::Attack(t)) if self.targets_from(u, (x, y)).contains(&t) => Some(t),
                                _ => self.targets_from(u, (x, y)).into_iter().min_by_key(|&t| self.units[t].hp),
                            };
                            match t {
                                Some(t) if self.units[u].status & status::CONFUSE == 0 => {
                                    self.phase = Phase::Ai { side, idx, stage: 2 };
                                    self.attack_target = Some((t, 0));
                                }
                                _ => {
                                    self.phase = Phase::Ai { side, idx: idx + 1, stage: 0 };
                                    return (true, false);
                                }
                            }
                        }
                    }
                }
            }
            3 => {
                let Some(AiPlan::Strat(sid, cell)) = self.ai_plan.clone() else {
                    self.phase = Phase::Ai { side, idx: idx + 1, stage: 0 };
                    return (true, false);
                };
                let t = self.cast_t;
                self.cast_t += 1;
                if self.cast_step(u, sid, cell, t) {
                    self.ai_plan = None;
                    self.phase = Phase::Ai { side, idx: idx + 1, stage: 0 };
                    return (true, false);
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
        // 겹침 그림 (원작 FUN_00454724: 불 4장, 짚더미·타는 짚더미 2장씩)
        if self.mark_tiles.len() >= 8 {
            for (i, &k) in self.overlay.iter().enumerate() {
                let idx = match k {
                    0 => (tick / 4 % 4) as usize,
                    1 => 4 + (tick / 8 % 2) as usize,
                    2 => 6 + (tick / 4 % 2) as usize,
                    _ => continue,
                };
                let (x, y) = (i as i32 % self.cols, i as i32 / self.cols);
                scr.blit(&self.mark_tiles[idx], x * CELL - self.cam.0, y * CELL - self.cam.1, Some(0));
            }
        }
        // 목표 구역 (깜빡이는 노란 테두리)
        if let Some((x1, y1, x2, y2)) = self.goal_zone {
            if (tick / 15) % 2 == 0 {
                let c = self.col[5];
                for y in y1..=y2 {
                    for x in x1..=x2 {
                        let (sx, sy) = (x * CELL - self.cam.0, y * CELL - self.cam.1);
                        scr.fill(sx + 2, sy + 2, CELL - 4, 2, c);
                        scr.fill(sx + 2, sy + CELL - 4, CELL - 4, 2, c);
                        scr.fill(sx + 2, sy + 2, 2, CELL - 4, c);
                        scr.fill(sx + CELL - 4, sy + 2, 2, CELL - 4, c);
                    }
                }
            }
        }
        // 범위 표시 (점무늬)
        let (cells, col): (Vec<(i32, i32)>, u8) = match &self.phase {
            Phase::Selected { cells, .. } => (cells.clone(), self.col[0]),
            Phase::Targeting { cells, .. } | Phase::StratTarget { cells, .. } => (cells.clone(), self.col[1]),
            Phase::ToolTarget { cells, .. } => (cells.clone(), self.col[0]),
            Phase::Cast { .. } if self.fx.is_none() => (self.cast_fx.clone(), self.col[5]),
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
            // 공격 동작 (UNIT_ATK) · 맞음 (UNIT_SPC 3)
            let striking = self.strikes.front().is_some_and(|s| s.0 == i);
            if let (Some(6), true, Some(atk)) = (u.pose, striking, &u.atk) {
                let base = match u.dir {
                    0 => 0,
                    1 => 4,
                    _ => 8,
                };
                let k = base + (self.strike_t / 3).min(3) as usize;
                draw_frame(scr, atk, 64, k, sx - 8, sy - 8, u.dir == 3);
            } else if let (Some(9), Some(spc)) = (u.pose, &u.spc) {
                draw_frame(scr, spc, 48, 3, sx, sy, u.dir == 3);
            } else if flip {
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
        draw_text(scr, f, 8, 4, &format!("{}턴/{}  {}", self.turn, self.turn_limit, side_name), story::TEXT);
        if !self.objective.is_empty() {
            draw_text(scr, f, 200, 4, &self.objective, story::NAME);
        }
        if matches!(self.phase, Phase::PlayerIdle) {
            draw_text(scr, f, 540, 4, "[턴 종료]", story::NAME);
        }
        // 선택된 부대 정보
        let sel = match &self.phase {
            Phase::Selected { unit, .. } | Phase::Menu { unit, .. } | Phase::Targeting { unit, .. } | Phase::Moving { unit, .. } | Phase::StratList { unit, .. } | Phase::StratTarget { unit, .. } | Phase::ToolList { unit, .. } | Phase::ToolTarget { unit, .. } => Some(*unit),
            _ => None,
        };
        if let Some(u) = sel {
            self.draw_unit_box(scr, f, u);
        }
        if let Phase::Menu { unit, .. } = &self.phase {
            let (bx, by) = self.menu_pos(*unit);
            let items = self.menu_items(*unit);
            frame_box(scr, bx, by, 120, items.len() as i32 * 30 + 4);
            for (k, (name, ok)) in items.iter().enumerate() {
                draw_text(scr, f, bx + 16, by + 8 + k as i32 * 30, name, if *ok { story::TEXT } else { 24 });
            }
        }
        if let Phase::ToolList { list, .. } = &self.phase {
            let (bx, by, w, h) = self.strat_box(list.len());
            frame_box(scr, bx, by, w, h);
            for (k, &inv) in list.iter().enumerate() {
                let it = self.inventory[inv].0;
                let cnt = self.inventory.iter().filter(|x| x.0 == it).count();
                draw_text(scr, f, bx + 12, by + 8 + k as i32 * 22, &crate::officer::item_name(&self.items, it), story::TEXT);
                draw_text(scr, f, bx + w - 60, by + 8 + k as i32 * 22, &format!("×{cnt}"), story::TEXT);
            }
        }
        if let Phase::StratList { unit, list, .. } = &self.phase {
            let (bx, by, w, h) = self.strat_box(list.len());
            frame_box(scr, bx, by, w, h);
            let mp = self.units[*unit].mp;
            for (k, sid) in list.iter().enumerate() {
                if let Some(st) = self.strat(*sid) {
                    let col = if mp >= st.mp as i32 { story::TEXT } else { 24 };
                    draw_text(scr, f, bx + 12, by + 8 + k as i32 * 22, &st.name, col);
                    draw_text(scr, f, bx + w - 80, by + 8 + k as i32 * 22, &format!("MP {:>2}", st.mp), col);
                }
            }
        }
        if let Some((m, _)) = &self.message {
            frame_box(scr, 180, 440, 280, 32);
            draw_text(scr, f, 196, 448, m, story::TEXT);
        }
        match &self.phase {
            Phase::TurnBanner { side, .. } => {
                let s = match side {
                    Side::Player => format!("제 {}턴  아군 차례", self.turn),
                    Side::Ally => "우군 차례".to_string(),
                    Side::Enemy => "적군 차례".to_string(),
                };
                frame_box(scr, 200, 210, 240, 44);
                draw_text(scr, f, 320 - s.chars().count() as i32 * 7, 224, &s, story::TEXT);
            }
            Phase::Over { won, .. } => {
                let s = if *won { "승리!" } else { "패배……" };
                frame_box(scr, 240, 200, 160, 60);
                draw_text(scr, f, 296, 222, s, story::NAME);
            }
            _ => {}
        }
        if let Some(f) = &self.fx {
            f.render(scr, self.cam);
        }
        if let Some(t) = self.intro {
            // 가장자리에서 가운데로 열림 (16틱), 그 뒤 "작전개시" (원작 문자열 0x498a04)
            let open = (t as i32 * 20).min(320);
            scr.fill(open, 0, 640 - open * 2, 480, self.col[6]);
            if t >= 14 {
                let txt = "작전개시";
                let k = 3;
                let w = txt.chars().count() as i32 * 16 * k;
                story::draw_text_big(scr, f, 320 - w / 2, 240 - 8 * k, txt, k, self.col[4], self.col[6]);
            }
        }
        if let Some(d) = self.duel.as_mut() {
            if tick != self.duel_tick {
                self.duel_tick = tick;
                d.update();
            }
            d.render(scr, self.col[6], self.col[5], tick);
        }
        self.ui.render_overlay(scr, Some(f), tick);
    }

    fn draw_unit_box(&self, scr: &mut Screen, f: &Font, u: usize) {
        let un = &self.units[u];
        frame_box(scr, 0, 432, 300, 48);
        draw_text(scr, f, 8, 438, &format!("Lv{} HP {}/{}", un.level, un.hp.max(0), un.max_hp), story::TEXT);
        let v = |k: u8| un.off.value(&self.items, k);
        draw_text(scr, f, 8, 458, &format!("공{} 방{} 정{} 순{} 사{}", v(0), v(1), v(2), v(3), v(4)), story::TEXT);
    }
}
