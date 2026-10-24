//! 책략·맵 연출 애니메이션 (원작 MEFF.E5 묶음 / MCALLnn.E5 단독 파일, FUN_00401360 / FUN_004021bc / FUN_004025df).
//! 머리 20바이트: +0 단계 수, +1 그림 수, +2 겹 그림 수, +3 팔레트 수, +4 지연(×50), +8 가로, +12 세로, +16 효과음(SE_M_nn).
//! 단계 20바이트: +0 그림, +1 팔레트(0x28 이상 = 화면 팔레트), +2 혼합 방식 (겹 그림 사용이면 겹 그림 번호), +3 강도,
//! +4 겹 그림 사용, +8 dx, +12 dy,
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
    masks: usize,
    w: usize,
    h: usize,
    delay: u32,
    pub sound: u8,
    /// 자체 팔레트들 (4비트 RGB)
    pals: Vec<[[u8; 3]; 256]>,
    /// 화면 팔레트 (4비트 RGB)
    scr: [[u8; 3]; 256],
    /// 4비트 RGB → 화면 색 번호 (원작 0x4bc2a0 색 입방체)
    cube: Vec<u8>,
    /// 그릴 중심 (맵 좌표 픽셀)
    pub centers: Vec<(i32, i32)>,
    pub step: usize,
    /// 시작부터 지난 틱
    t: u32,
    /// 이번 틱에 새 단계로 들어섬
    fresh: bool,
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
        // 원작 팔레트는 4비트 (0..15). 위 4비트로 든 파일도 있어 둘 다 받음. 순서 B,R,G
        let v = |x: u8| if x < 16 { x } else { x >> 4 };
        let pals = (0..pals)
            .map(|k| {
                let mut p = [[0u8; 3]; 256];
                for (i, c) in p.iter_mut().enumerate() {
                    let o = pal_at + k * 0x300 + i * 3;
                    let s = d.get(o..o + 3).unwrap_or(&[0, 0, 0]);
                    *c = [v(s[1]), v(s[2]), v(s[0])];
                }
                p
            })
            .collect();
        let scr = std::array::from_fn(|i| scr_pal[i].map(|x| x >> 4));
        let cube = (0..4096u32).map(|i| crate::palette::nearest(scr_pal, [((i >> 8) as u8) << 4, (((i >> 4) & 15) as u8) << 4, ((i & 15) as u8) << 4])).collect();
        Some(Fx { sound: *d.get(16)?, data, steps, imgs, masks, w, h, delay, pals, scr, cube, centers, step: 0, t: 0, fresh: true })
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
        self.fresh && ((self.step == 0 && self.rec(0, 16) != 2) || self.rec(self.step, 16) == 3)
    }

    /// 이 단계가 맞는 순간
    pub fn hit_now(&self) -> bool {
        self.fresh && self.rec(self.step, 16) == 1
    }

    pub fn has_hit(&self) -> bool {
        (0..self.steps).any(|i| self.rec(i, 16) == 1)
    }

    /// 한 틱. 끝나면 true. 단계마다 지연 × 1/20 초 (= 1.5틱) 를 누적으로 환산
    pub fn advance(&mut self) -> bool {
        self.t += 1;
        let step = (self.t * 2 / (self.delay * 3)) as usize;
        self.fresh = step != self.step;
        self.step = step;
        self.step >= self.steps
    }

    /// 그리기 (원작 FUN_004025df → FUN_0043230a / 겹 그림이면 FUN_00432678).
    /// 원작 색은 채널마다 4비트. 그림 밝기 L = (R+G+B)/3 으로 바탕 비침 k 를 정해 섞는다:
    /// k = 방식 × (256 − L × 강도) / 16 (겹 그림: (256 − 겹) × (256 − L × 강도) / 256),
    /// 결과 = (바탕 × k + 그림 × (256 − k × (16 − 강도) / 16)) / 256, 15 넘으면 15.
    /// 방식 16·강도 16 이면 밝은 곳만 진하고 어두운 테두리는 바탕이 비치는 더하기 합성이 된다.
    pub fn render(&self, scr: &mut Screen, cam: (i32, i32)) {
        if self.step >= self.steps {
            return;
        }
        let img = self.rec(self.step, 0) as usize;
        if img >= self.imgs {
            return;
        }
        let (mode, level) = (self.rec(self.step, 2) as u32, self.rec(self.step, 3) as u32);
        let masked = self.rec_i32(self.step, 4) != 0;
        let pal_no = self.rec(self.step, 1) as usize;
        let pal = if pal_no < 0x28 { self.pals.get(pal_no) } else { None };
        let (dx, dy) = (self.rec_i32(self.step, 8), self.rec_i32(self.step, 12));
        let base = 20 + self.steps * 20 + img * self.w * self.h;
        let mask_base = (masked && (mode as usize) < self.masks).then(|| 20 + self.steps * 20 + (self.imgs + mode as usize) * self.w * self.h);
        if masked && mask_base.is_none() {
            return;
        }
        // 그대로 덮기 (원작 FUN_004025df: 방식 0 이고 자체 팔레트 없으면 투명색만 빼고 복사)
        let plain = !masked && mode == 0 && pal.is_none();
        let (sw, sh) = (super::screen::W as i32, super::screen::H as i32);
        for &(cx, cy) in &self.centers {
            let (x0, y0) = (cx - cam.0 - self.w as i32 / 2 + dx, cy - cam.1 - self.h as i32 / 2 + dy);
            for y in 0..self.h {
                let ty = y0 + y as i32;
                if ty < 0 || ty >= sh {
                    continue;
                }
                for x in 0..self.w {
                    let tx = x0 + x as i32;
                    if tx < 0 || tx >= sw {
                        continue;
                    }
                    let Some(&c) = self.data.get(base + y * self.w + x) else { return };
                    if c == 0 {
                        continue;
                    }
                    let at = ty as usize * super::screen::W + tx as usize;
                    if plain {
                        scr.px[at] = c;
                        continue;
                    }
                    let s = pal.map_or(self.scr[c as usize], |p| p[c as usize]);
                    let lum = (s[0] as u32 + s[1] as u32 + s[2] as u32) / 3;
                    let k = if let Some(mb) = mask_base {
                        let m = self.data.get(mb + y * self.w + x).copied().unwrap_or(0) as u32;
                        if m == 0 && level == 0 {
                            continue;
                        }
                        if m == 0xFF {
                            0
                        } else {
                            (0x100 - m) * (0x100 - lum * level) >> 8
                        }
                    } else {
                        if mode == 0x10 && level == 0 {
                            continue;
                        }
                        mode * (0x100 - lum * level) >> 4
                    };
                    let out = if k == 0 {
                        s
                    } else {
                        let b = self.scr[scr.px[at] as usize];
                        let ws = 0x100u32.wrapping_sub(k * (0x10 - level) >> 4);
                        std::array::from_fn(|i| ((b[i] as u32 * k).wrapping_add(s[i] as u32 * ws) >> 8).min(15) as u8)
                    };
                    scr.px[at] = self.cube[(out[0] as usize) << 8 | (out[1] as usize) << 4 | out[2] as usize];
                }
            }
        }
    }
}
