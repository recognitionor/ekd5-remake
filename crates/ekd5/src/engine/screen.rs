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
    /// 반투명 칠 (화소 번호, 칠할 때 화소 색, 섞을 색): RGBA 로 바꿀 때 반반 섞음. 그 뒤 덮어 그려진 화소는 그대로
    pub glaze: Vec<(u32, u8, u8)>,
    /// 선명한 글자 (웹): draw_text 가 글자마다 기록 → rgba() 때 가려지지 않은 글자는 비트맵을 지우고 text_out 으로 넘김 (웹이 고해상도 글꼴로 그림)
    pub hires: bool,
    chars: Vec<TextChar>,
    /// 화소마다 마지막으로 그린 글자 번호 +1 (0 = 없음) / 그 글자 기록 안 화소 순번
    tmark: Vec<u32>,
    tidx: Vec<u16>,
    /// 넘길 글자: 16바이트씩 (x i16, y i16, 칸 폭 u8, 글자색 rgb, 그림자 있음 u8, 그림자 rgb, 유니코드 u32)
    pub text_out: Vec<u8>,
}

struct TextChar {
    x: i32,
    y: i32,
    w: i32,
    ch: char,
    col: u8,
    shadow: Option<u8>,
    /// (화소, 지금 있어야 할 색, 그리기 전 색)
    pix: Vec<(u32, u8, u8)>,
}

impl Default for Screen {
    fn default() -> Self {
        Self { px: vec![0; W * H], pal: [[0; 3]; 256], level: 16, rgba: vec![0; W * H * 4], window_tex: None, glaze: Vec::new(), hires: false, chars: Vec::new(), tmark: vec![0; W * H], tidx: vec![0; W * H], text_out: Vec::new() }
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
    /// 글자 하나 시작 (draw_text). 기록 번호
    pub fn text_begin(&mut self, x: i32, y: i32, w: i32, ch: char, col: u8, shadow: Option<u8>) -> Option<usize> {
        if !self.hires {
            return None;
        }
        self.chars.push(TextChar { x, y, w, ch, col, shadow, pix: Vec::new() });
        Some(self.chars.len() - 1)
    }

    /// 글자 화소 찍기 (기록하면서)
    pub fn text_px(&mut self, id: Option<usize>, x: i32, y: i32, c: u8) {
        if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 {
            return;
        }
        let i = y as usize * W + x as usize;
        if let Some(id) = id {
            let tc = &mut self.chars[id];
            if self.tmark[i] == id as u32 + 1 {
                if let Some(e) = tc.pix.get_mut(self.tidx[i] as usize) {
                    e.1 = c;
                }
            } else {
                self.tmark[i] = id as u32 + 1;
                self.tidx[i] = tc.pix.len().min(u16::MAX as usize) as u16;
                tc.pix.push((i as u32, c, self.px[i]));
            }
        }
        self.px[i] = c;
    }

    /// 가려지지 않은 글자를 비트맵에서 지우고 text_out 에 (나중 글자부터 보고, 넘길 때는 그린 차례로)
    fn take_text(&mut self) {
        self.text_out.clear();
        let chars = std::mem::take(&mut self.chars);
        if chars.is_empty() {
            return;
        }
        let mut glazed = vec![false; 0];
        if !self.glaze.is_empty() {
            glazed = vec![false; W * H];
            for &(i, _, _) in &self.glaze {
                glazed[i as usize] = true;
            }
        }
        let mut vis = vec![false; chars.len()];
        for id in (0..chars.len()).rev() {
            let tc = &chars[id];
            let ok = !tc.pix.is_empty()
                && tc.pix.iter().all(|&(i, want, _)| {
                    let i = i as usize;
                    if glazed.get(i).copied().unwrap_or(false) {
                        return false;
                    }
                    if self.px[i] == want && self.tmark[i] == id as u32 + 1 {
                        return true;
                    }
                    // 뒤 글자가 덮었으면 그 글자가 보일 때만 (그 글자가 이미 지워 놓았으면 내 색이 돌아와 있음)
                    let m = self.tmark[i] as usize;
                    m > id + 1 && vis[m - 1] && self.px[i] == want
                });
            if ok {
                vis[id] = true;
                for &(i, _, old) in &tc.pix {
                    self.px[i as usize] = old;
                }
            }
        }
        for (id, tc) in chars.iter().enumerate() {
            for &(i, _, _) in &tc.pix {
                self.tmark[i as usize] = 0;
            }
            if !vis[id] {
                continue;
            }
            let l = self.level.min(16) as u32;
            let rgb = |c: u8| {
                let p = self.pal[c as usize];
                [(p[0] as u32 * l / 16) as u8, (p[1] as u32 * l / 16) as u8, (p[2] as u32 * l / 16) as u8]
            };
            let o = &mut self.text_out;
            o.extend_from_slice(&(tc.x as i16).to_le_bytes());
            o.extend_from_slice(&(tc.y as i16).to_le_bytes());
            o.push(tc.w as u8);
            o.extend_from_slice(&rgb(tc.col));
            o.push(tc.shadow.is_some() as u8);
            o.extend_from_slice(&rgb(tc.shadow.unwrap_or(0)));
            o.extend_from_slice(&(tc.ch as u32).to_le_bytes());
        }
    }

    pub fn rgba(&mut self) -> &[u8] {
        self.take_text();
        let l = self.level.min(16) as u32;
        for (i, &c) in self.px.iter().enumerate() {
            let p = self.pal[c as usize];
            let o = i * 4;
            self.rgba[o] = (p[0] as u32 * l / 16) as u8;
            self.rgba[o + 1] = (p[1] as u32 * l / 16) as u8;
            self.rgba[o + 2] = (p[2] as u32 * l / 16) as u8;
            self.rgba[o + 3] = 255;
        }
        for (i, at, col) in std::mem::take(&mut self.glaze) {
            if self.px[i as usize] != at {
                continue;
            }
            let k = self.pal[col as usize];
            let o = i as usize * 4;
            for j in 0..3 {
                self.rgba[o + j] = ((self.rgba[o + j] as u32 + k[j] as u32 * l / 16) / 2) as u8;
            }
        }
        &self.rgba
    }
}

impl std::fmt::Debug for Pic {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Pic({}×{})", self.w, self.h)
    }
}
