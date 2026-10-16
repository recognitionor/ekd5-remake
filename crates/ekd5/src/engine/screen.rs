//! 8비트 인덱스 화면 640x480 + 256색 팔레트 (원작 DIB 섹션과 같은 구조)

use crate::palette::Palette;

pub const W: usize = 640;
pub const H: usize = 480;

/// 8비트 그림
#[derive(Clone)]
pub struct Pic {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Pic {
    pub fn new(w: usize, h: usize, px: Vec<u8>) -> Self {
        let mut px = px;
        px.resize(w * h, 0);
        Self { w, h, px }
    }
}

pub struct Screen {
    pub px: Vec<u8>,
    pub pal: Palette,
    /// 밝기 0..=16 (16 = 원래). 원작 팔레트 페이드
    pub level: u8,
    rgba: Vec<u8>,
    /// 창 바탕 무늬 (MARK.E5 0x18dc0, 32×48)
    pub window_tex: Option<Pic>,
}

impl Default for Screen {
    fn default() -> Self {
        Self { px: vec![0; W * H], pal: [[0; 3]; 256], level: 16, rgba: vec![0; W * H * 4], window_tex: None }
    }
}

impl Screen {
    pub fn clear(&mut self, c: u8) {
        self.px.fill(c);
    }

    /// 그림 복사. key = 투명 색 번호
    pub fn blit(&mut self, p: &Pic, dx: i32, dy: i32, key: Option<u8>) {
        self.blit_rect(p, 0, 0, p.w as i32, p.h as i32, dx, dy, key);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn blit_rect(&mut self, p: &Pic, sx: i32, sy: i32, w: i32, h: i32, dx: i32, dy: i32, key: Option<u8>) {
        for y in 0..h {
            let (ty, yy) = (dy + y, sy + y);
            if ty < 0 || ty >= H as i32 || yy < 0 || yy >= p.h as i32 {
                continue;
            }
            for x in 0..w {
                let (tx, xx) = (dx + x, sx + x);
                if tx < 0 || tx >= W as i32 || xx < 0 || xx >= p.w as i32 {
                    continue;
                }
                let c = p.px[yy as usize * p.w + xx as usize];
                if Some(c) == key {
                    continue;
                }
                self.px[ty as usize * W + tx as usize] = c;
            }
        }
    }

    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, c: u8) {
        for ty in y.max(0)..(y + h).min(H as i32) {
            for tx in x.max(0)..(x + w).min(W as i32) {
                self.px[ty as usize * W + tx as usize] = c;
            }
        }
    }

    /// RGBA로 변환
    pub fn rgba(&mut self) -> &[u8] {
        let l = self.level.min(16) as u32;
        for (i, &c) in self.px.iter().enumerate() {
            let p = self.pal[c as usize];
            let o = i * 4;
            self.rgba[o] = (p[0] as u32 * l / 16) as u8;
            self.rgba[o + 1] = (p[1] as u32 * l / 16) as u8;
            self.rgba[o + 2] = (p[2] as u32 * l / 16) as u8;
            self.rgba[o + 3] = 255;
        }
        &self.rgba
    }
}
