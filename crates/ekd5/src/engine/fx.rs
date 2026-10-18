//! 책략·맵 연출 애니메이션 (원작 MEFF.E5 묶음 / MCALLnn.E5 단독 파일, FUN_00401360 / FUN_004021bc / FUN_004025df).
//! 머리 20바이트: +0 단계 수, +1 그림 수, +2 겹 그림 수, +3 팔레트 수, +4 지연(×50), +8 가로, +12 세로, +16 효과음(SE_M_nn).
//! 단계 20바이트: +0 그림, +1 팔레트(0xFF 없음), +2 혼합 방식, +3 강도, +4 겹 그림 사용, +8 dx, +12 dy,
//! +16 표지 (0단계가 2가 아니면 0단계에서 소리, 3 = 그 단계에서 소리, 1 = 대상이 맞는 순간).
//! 그 뒤 그림(가로×세로) × (그림 수 + 겹 그림 수), 팔레트 768바이트(R,G,B) × 팔레트 수.

use super::screen::Screen;
use std::rc::Rc;

/// 책략 번호 → 연출 (원작 표 0x48f718: 100 미만 = MEFF 항목, 100 이상 = MCALL(n−100), 255 = 없음)
pub const STRATEGY_FX: [u8; 68] = [
    0, 1, 0, 100, 57, 2, 3, 2, 101, 3, 4, 5, 4, 102, 4, 6, 7, 6, 103, 8, 31, 24, 32, 20, 20, 21, 21, 18, 18, 19, 19, 25, 25, 23, 23, 26, 26, 27, 104, 9, 10, 9, 10, 10, 11, 12, 28,
    28, 30, 15, 15, 16, 16, 13, 13, 14, 14, 29, 255, 255, 255, 13, 31, 17, 106, 105, 107, 108,
];

pub struct Fx {
    data: Rc<Vec<u8>>,
    steps: usize,
    imgs: usize,
    w: usize,
    h: usize,
    delay: u32,
    pub sound: u8,
    /// 자체 팔레트를 화면 팔레트로 옮긴 표
    map: Option<Vec<u8>>,
    /// 그릴 중심 (맵 좌표 픽셀)
    pub centers: Vec<(i32, i32)>,
    pub step: usize,
    t: u32,
}

impl Fx {
    pub fn parse(data: Rc<Vec<u8>>, scr_pal: &crate::palette::Palette, centers: Vec<(i32, i32)>) -> Option<Fx> {
        let d = &data;
        let u32at = |o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
        let (steps, imgs, masks, pals) = (*d.first()? as usize, *d.get(1)? as usize, *d.get(2)? as usize, *d.get(3)? as usize);
        let (w, h) = (u32at(8)?, u32at(12)?);
        if w == 0 || h == 0 || w > 640 || h > 480 {
            return None;
        }
        let delay = (u32at(4)? / 50).max(1) as u32;
        let pal_at = 20 + steps * 20 + (imgs + masks) * w * h;
        let map = (pals > 0).then(|| {
            (0..256)
                .map(|i| {
                    let c = d.get(pal_at + i * 3..pal_at + i * 3 + 3).unwrap_or(&[0, 0, 0]);
                    // 원작 팔레트 순서 B,R,G. 4비트(0..15) 또는 위 4비트
                    let v = |x: u8| if x < 16 { x * 17 } else { x & 0xF0 };
                    crate::palette::nearest(scr_pal, [v(c[1]), v(c[2]), v(c[0])])
                })
                .collect()
        });
        Some(Fx { sound: *d.get(16)?, data, steps, imgs, w, h, delay, map, centers, step: 0, t: 0 })
    }

    fn rec(&self, i: usize, k: usize) -> u8 {
        self.data.get(20 + i * 20 + k).copied().unwrap_or(0)
    }

    fn rec_i32(&self, i: usize, k: usize) -> i32 {
        let o = 20 + i * 20 + k;
        self.data.get(o..o + 4).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// 이 단계에서 소리
    pub fn sound_now(&self) -> bool {
        self.t == 0 && ((self.step == 0 && self.rec(0, 16) != 2) || self.rec(self.step, 16) == 3)
    }

    /// 이 단계가 맞는 순간
    pub fn hit_now(&self) -> bool {
        self.t == 0 && self.rec(self.step, 16) == 1
    }

    pub fn has_hit(&self) -> bool {
        (0..self.steps).any(|i| self.rec(i, 16) == 1)
    }

    /// 한 틱. 끝나면 true
    pub fn advance(&mut self) -> bool {
        self.t += 1;
        if self.t >= (self.delay * 3).div_ceil(2) {
            self.t = 0;
            self.step += 1;
        }
        self.step >= self.steps
    }

    pub fn render(&self, scr: &mut Screen, cam: (i32, i32)) {
        if self.step >= self.steps {
            return;
        }
        let img = self.rec(self.step, 0) as usize;
        if img >= self.imgs {
            return;
        }
        let (mode, level) = (self.rec(self.step, 2), self.rec(self.step, 3));
        let (dx, dy) = (self.rec_i32(self.step, 8), self.rec_i32(self.step, 12));
        let base = 20 + self.steps * 20 + img * self.w * self.h;
        // 반투명(강도 < 16) 은 8비트 화면이라 점무늬로
        let dither = mode != 0 && level < 12;
        for &(cx, cy) in &self.centers {
            let (x0, y0) = (cx - cam.0 - self.w as i32 / 2 + dx, cy - cam.1 - self.h as i32 / 2 + dy);
            for y in 0..self.h {
                for x in 0..self.w {
                    let Some(&c) = self.data.get(base + y * self.w + x) else { return };
                    if c == 0 || dither && (x + y) % 2 == 1 {
                        continue;
                    }
                    let c = self.map.as_ref().map_or(c, |m| m[c as usize]);
                    scr.fill(x0 + x as i32, y0 + y as i32, 1, 1, c);
                }
            }
        }
    }
}
