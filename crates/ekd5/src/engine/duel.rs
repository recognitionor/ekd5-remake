//! 일기토 (원작 op68 시작 / op60 등장+대사 / op63 대사 / op64 동작 / op65·op66 공방 / op5f 끝, FUN_0044a6a4 등).
//! 원작은 두 장수를 2배 크기(96×96)로 가운데 띠에 세우고 동작 16가지를 그림:
//! UNIT_MOV(서기 8, 걷기 4-5, 지침 9-10), UNIT_ATK(옆 공격 8-11, 64×64), UNIT_SPC(막기 2, 맞음 3, 승리 4).
//! 여기서는 동작을 그 그림들로 간략히 재현한다.

use super::screen::{Pic, Screen};
use std::rc::Rc;

/// 한 장수의 그림 (이동 48×48×11, 공격 64×64×12, 특수 48×48×5)
#[derive(Clone)]
pub struct Sprites {
    pub mov: Rc<Pic>,
    pub atk: Rc<Pic>,
    pub spc: Rc<Pic>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pose {
    Stand,
    Walk,
    Attack,
    Guard,
    Hurt,
    Win,
    Tired,
    Down,
}

pub struct Fighter {
    pub person: usize,
    pub sprite_idx: usize,
    pub pics: Option<Sprites>,
    pub shown: bool,
    /// 띠 안 x (왼쪽 위), 기본 위치
    pub x: f32,
    pub home: f32,
    pub pose: Pose,
    /// 동작 진행 틱
    pub t: u32,
    /// 이동 목표 x
    pub to: f32,
    /// 맞아서 깜빡임
    pub flash: u32,
    pub jump: u32,
}

pub struct Duel {
    pub f: [Fighter; 2],
}

pub const BAND_Y: i32 = 168;
pub const BAND_H: i32 = 144;

impl Duel {
    pub fn new(p0: (usize, usize), p1: (usize, usize)) -> Self {
        let mk = |(person, sprite_idx): (usize, usize), home: f32| Fighter { person, sprite_idx, pics: None, shown: false, x: home, home, pose: Pose::Stand, t: 0, to: home, flash: 0, jump: 0 };
        Duel { f: [mk(p0, 150.0), mk(p1, 394.0)] }
    }

    /// 동작 (원작 FUN_0044a6a4 의 0..15). 반환: 걸리는 틱
    pub fn motion(&mut self, side: usize, m: u8) -> u32 {
        let s = side.min(1);
        let dir = if s == 0 { 1.0 } else { -1.0 };
        let f = &mut self.f[s];
        f.shown = true;
        f.t = 0;
        match m {
            0 | 1 | 13 | 15 => {
                f.pose = Pose::Stand;
                if m == 13 {
                    f.to = f.home;
                }
                8
            }
            2 => {
                f.pose = Pose::Walk;
                f.to = f.home + dir * 56.0;
                16
            }
            3 => {
                f.pose = Pose::Walk;
                f.to = f.home;
                16
            }
            4 => {
                f.pose = Pose::Stand;
                f.jump = 16;
                16
            }
            5 => {
                f.pose = Pose::Guard;
                12
            }
            6 => {
                f.pose = Pose::Hurt;
                f.flash = 12;
                12
            }
            7 => {
                f.pose = Pose::Win;
                16
            }
            8..=10 | 14 => {
                f.pose = Pose::Attack;
                16
            }
            11 => {
                f.pose = Pose::Tired;
                12
            }
            12 => {
                f.pose = Pose::Down;
                12
            }
            _ => 8,
        }
    }

    /// 공방 (op65·op66): side 가 공격, 상대는 맞거나 막음. 반환: 걸리는 틱
    pub fn exchange(&mut self, side: usize, rush: bool, hit: bool) -> u32 {
        let s = side.min(1);
        let o = 1 - s;
        let dir = if s == 0 { 1.0 } else { -1.0 };
        let target = self.f[o].x - dir * 72.0;
        let a = &mut self.f[s];
        a.shown = true;
        a.pose = Pose::Attack;
        a.t = 0;
        if rush {
            a.to = target;
        }
        let d = &mut self.f[o];
        d.shown = true;
        d.t = 0;
        if hit {
            d.pose = Pose::Hurt;
            d.flash = 14;
        } else {
            d.pose = Pose::Guard;
        }
        24
    }

    pub fn update(&mut self) {
        for f in &mut self.f {
            f.t += 1;
            if (f.x - f.to).abs() > 0.5 {
                let step = (f.to - f.x).clamp(-5.0, 5.0);
                f.x += step;
            } else {
                f.x = f.to;
                if f.pose == Pose::Walk {
                    f.pose = Pose::Stand;
                }
            }
            if f.pose == Pose::Attack && f.t > 18 {
                f.pose = Pose::Stand;
            }
            f.flash = f.flash.saturating_sub(1);
            f.jump = f.jump.saturating_sub(1);
        }
    }

    pub fn render(&self, scr: &mut Screen, band: u8, line: u8, tick: u32) {
        scr.fill(0, BAND_Y, 640, BAND_H, band);
        scr.fill(0, BAND_Y, 640, 2, line);
        scr.fill(0, BAND_Y + BAND_H - 2, 640, 2, line);
        for (i, f) in self.f.iter().enumerate() {
            let Some(p) = &f.pics else { continue };
            if !f.shown || (f.flash > 0 && (tick / 2) % 2 == 0) {
                continue;
            }
            // 그림은 왼쪽을 봄 → 왼쪽 장수(0)는 뒤집음
            let flip = i == 0;
            let lift = if f.jump > 0 { ((16 - f.jump) as f32 * std::f32::consts::PI / 16.0).sin() * 40.0 } else { 0.0 };
            let (x, y) = (f.x as i32, BAND_Y + 24 - lift as i32);
            match f.pose {
                Pose::Attack => {
                    let k = 8 + (f.t / 4).min(3) as usize;
                    blit2(scr, &p.atk, 64, k, x - 16, y - 32, flip);
                }
                Pose::Walk => blit2(scr, &p.mov, 48, 4 + (f.t / 4 % 2) as usize, x, y, flip),
                Pose::Stand => blit2(scr, &p.mov, 48, 8, x, y, flip),
                Pose::Tired => blit2(scr, &p.mov, 48, 9, x, y, flip),
                Pose::Down => blit2(scr, &p.mov, 48, 10, x, y, flip),
                Pose::Guard => blit2(scr, &p.spc, 48, 2, x, y, flip),
                Pose::Hurt => blit2(scr, &p.spc, 48, 3, x, y, flip),
                Pose::Win => blit2(scr, &p.spc, 48, 4, x, y, flip),
            }
        }
    }
}

/// 세로로 쌓인 그림 묶음의 k 번째 장 (size×size) 을 2배로 그림 (0 = 투명)
fn blit2(scr: &mut Screen, p: &Pic, size: usize, k: usize, dx: i32, dy: i32, flip: bool) {
    let base = k * size * size;
    for y in 0..size {
        for x in 0..size {
            let sx = if flip { size - 1 - x } else { x };
            let Some(&c) = p.px.get(base + y * size + sx) else { return };
            if c == 0 {
                continue;
            }
            scr.fill(dx + x as i32 * 2, dy + y as i32 * 2, 2, 2, c);
        }
    }
}
