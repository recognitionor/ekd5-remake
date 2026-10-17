//! 이야기 장면 (원작 장면 종류 2 = FUN_0042af27 / FUN_0040c017 / FUN_0042c994 / FUN_0042fa65).
//! 배경: MMAP[번호+40] (8×8 타일 80×50, FUN_00429dc3), 팔레트 PMPALET[번호+40], 화면 y=40 에 640×400.
//! 인물: PMAPOBJ[2s] 앞모습 / [2s+1] 뒷모습 (48×64 × 20프레임), 좌우 반전본 포함 4방향 (FUN_0042933a).
//! 좌표 (원작 FUN_00429995): sx = (x−y)·8 − 8 + 폭·4 − 스크롤x·8, sy = (x+y)·4 − 8z − (스크롤y·8 − 8), 화면 y += 40.

use super::assets::Assets;
use super::screen::{Pic, Screen};
use super::{Input, Packs, Pending};
use crate::font::{self, Font};
use crate::palette::{self, Palette};
use std::collections::HashMap;
use std::rc::Rc;

pub struct Page {
    pub person: Option<usize>,
    pub face: Option<u16>,
    pub name: String,
    pub lines: Vec<String>,
}

pub struct Actor {
    pub id: usize,
    /// 그림 번호 (PMAPOBJ)
    pub sprite: u16,
    pub x: f32,
    pub y: f32,
    pub dir: i32,
    pub pose: i32,
    pub target: Option<(f32, f32)>,
    /// 다 걸은 뒤 바라볼 방향 (스크립트 지정)
    pub end_dir: i32,
    pub step: u32,
    pub images: Rc<[Pic; 4]>,
}

pub struct Story {
    pub bg: Option<Pic>,
    pub map_w: i32,
    pub scroll: (i32, i32),
    pub actors: Vec<Actor>,
    pub sprites: HashMap<u16, Rc<[Pic; 4]>>,
    pub pages: Vec<Page>,
    pub page: usize,
    pub face: Option<Pic>,
    pub left: bool,
    pub last_speaker: Option<usize>,
    pub choices: Vec<String>,
    pub choice_sel: usize,
    /// 해설 창 (얼굴/이름 없음, 가운데)
    pub narration: bool,
    /// 지도 자막 줄들
    pub caption: Vec<String>,
    pub caption_clear_after: bool,
    /// 지도 표식 (인물, x, y, 얼굴, 이름)
    pub markers: Vec<(usize, i32, i32, Option<Pic>, String)>,
    pub place_name: String,
    pub chapter_name: String,
    pub title_timer: u32,
    pub card: Option<String>,
    /// 전투 중 말하는 부대의 화면 y (창 위치 결정)
    pub speaker_y: Option<i32>,
}

impl Default for Story {
    fn default() -> Self {
        Self {
            bg: None,
            map_w: 200,
            scroll: (0x3C, 0x19),
            actors: Vec::new(),
            sprites: HashMap::new(),
            pages: Vec::new(),
            page: 0,
            face: None,
            left: false,
            last_speaker: None,
            choices: Vec::new(),
            choice_sel: 0,
            narration: false,
            caption: Vec::new(),
            caption_clear_after: false,
            markers: Vec::new(),
            place_name: String::new(),
            chapter_name: String::new(),
            title_timer: 0,
            card: None,
            speaker_y: None,
        }
    }
}

/// 8×8 타일 80×50 → 640×400
fn untile(d: &[u8]) -> Pic {
    let mut px = vec![0u8; 640 * 400];
    for t in 0..4000 {
        let (tx, ty) = (t % 80, t / 80);
        for y in 0..8 {
            let s = t * 64 + y * 8;
            let o = (ty * 8 + y) * 640 + tx * 8;
            if let Some(src) = d.get(s..s + 8) {
                px[o..o + 8].copy_from_slice(src);
            }
        }
    }
    Pic::new(640, 400, px)
}

fn flip(p: &Pic) -> Pic {
    let mut px = vec![0u8; p.w * p.h];
    for y in 0..p.h {
        for x in 0..p.w {
            px[y * p.w + x] = p.px[y * p.w + (p.w - 1 - x)];
        }
    }
    Pic::new(p.w, p.h, px)
}

/// 프레임 f 를 48×1280 세로 띠에서
fn frame_strip(d: Vec<u8>) -> Pic {
    Pic::new(48, 1280, d)
}

impl Story {
    /// 장면 그림 적재. 반환: 새 팔레트
    pub fn load_map<A: Assets>(&mut self, packs: &mut Packs, assets: &mut A, mode: i32, v: i32) -> Result<Option<Palette>, Pending> {
        let idx = match mode {
            0 => v,
            1 => v + 0x72,
            _ => v + 0x28,
        } as usize;
        let img = packs.get(assets, "MMAP.E5", idx)?;
        let pal = packs.get(assets, "PMPALET.E5", idx)?;
        if mode == 2 {
            if let Some(pm) = packs.get(assets, "PMAP.E5", v as usize)? {
                self.map_w = u16::from_le_bytes([pm[8], pm[9]]) as i32;
            }
        }
        self.bg = img.map(|d| if mode == 2 { untile(&d) } else { Pic::new(640, 400, d) });
        self.actors.clear();
        self.markers.clear();
        self.caption.clear();
        self.card = None;
        Ok(pal.map(|p| palette::from_bytes(&p)))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn place<A: Assets>(&mut self, packs: &mut Packs, assets: &mut A, id: usize, sprite: u16, x: i32, y: i32, dir: i32, pose: i32) -> Result<(), Pending> {
        let images = match self.sprites.get(&sprite) {
            Some(i) => i.clone(),
            None => {
                let front = packs.get(assets, "PMAPOBJ.E5", sprite as usize * 2)?.unwrap_or_default();
                let back = packs.get(assets, "PMAPOBJ.E5", sprite as usize * 2 + 1)?.unwrap_or_default();
                let (f, b) = (frame_strip(front), frame_strip(back));
                let imgs = Rc::new([b.clone(), flip(&f), f, flip(&b)]);
                self.sprites.insert(sprite, imgs.clone());
                imgs
            }
        };
        self.actors.retain(|a| a.id != id);
        self.actors.push(Actor { id, x: x as f32, y: y as f32, dir: dir & 3, pose: pose.clamp(0, 19), target: None, end_dir: dir & 3, step: 0, sprite, images });
        Ok(())
    }

    pub fn walk(&mut self, id: usize, x: i32, y: i32, dir: i32) {
        if let Some(a) = self.actors.iter_mut().find(|a| a.id == id) {
            a.target = Some((x as f32, y as f32));
            a.end_dir = dir & 3;
            a.pose = 0;
        }
    }

    pub fn turn(&mut self, id: usize, dir: i32) {
        if let Some(a) = self.actors.iter_mut().find(|a| a.id == id) {
            a.dir = dir & 3;
        }
    }

    pub fn pose(&mut self, id: usize, pose: i32) {
        if let Some(a) = self.actors.iter_mut().find(|a| a.id == id) {
            a.pose = pose.clamp(0, 19);
        }
    }

    pub fn moving(&self) -> bool {
        self.actors.iter().any(|a| a.target.is_some())
    }

    pub fn update(&mut self) {
        self.title_timer = self.title_timer.saturating_sub(1);
        const SPEED: f32 = 0.34;
        for a in &mut self.actors {
            if let Some((tx, ty)) = a.target {
                // 원작은 칸 단위로 가로·세로 축을 따라 걷고, 걷는 쪽을 바라봄 (스크립트 통계: x+ 1, x− 3, y+ 2, y− 0).
                // 다 걸으면 스크립트가 정한 방향으로 돌아섬
                let (dx, dy) = (tx - a.x, ty - a.y);
                a.step += 1;
                if dx.abs() > 0.001 {
                    a.dir = if dx > 0.0 { 1 } else { 3 };
                    a.x += dx.signum() * SPEED.min(dx.abs());
                } else if dy.abs() > 0.001 {
                    a.dir = if dy > 0.0 { 2 } else { 0 };
                    a.y += dy.signum() * SPEED.min(dy.abs());
                } else {
                    a.x = tx;
                    a.y = ty;
                    a.target = None;
                    a.step = 0;
                    a.dir = a.end_dir;
                }
            }
        }
    }

    pub fn show_dialog<A: Assets>(&mut self, packs: &mut Packs, assets: &mut A, pages: Vec<Page>) -> Result<(), Pending> {
        // 얼굴 미리 받기
        for p in &pages {
            if let Some(f) = p.face {
                packs.get(assets, "FACE.E5", f as usize)?;
            }
        }
        self.pages = pages;
        self.page = 0;
        self.open_page(packs, assets);
        Ok(())
    }

    fn open_page<A: Assets>(&mut self, packs: &mut Packs, assets: &mut A) {
        let Some(p) = self.pages.get(self.page) else { return };
        self.face = p.face.and_then(|f| packs.get(assets, "FACE.E5", f as usize).ok().flatten()).map(|d| Pic::new(64, 80, d));
        // 말하는 사람이 바뀌면 창이 좌우를 오간다 (조조는 항상 왼쪽)
        if p.person != self.last_speaker {
            self.left = !self.left;
            self.last_speaker = p.person;
        }
        if p.person == Some(0) {
            self.left = true;
        }
    }

    /// 다음 쪽. 끝이면 true
    pub fn next_page(&mut self) -> bool {
        self.page += 1;
        if self.page >= self.pages.len() {
            self.pages.clear();
            self.face = None;
            self.narration = false;
            return true;
        }
        let p = &self.pages[self.page];
        if p.person != self.last_speaker {
            self.left = !self.left;
            self.last_speaker = p.person;
        }
        if p.person == Some(0) {
            self.left = true;
        }
        false
    }

    /// 선택지 고르기 → 1부터
    pub fn choose(&mut self, inp: &Input) -> Option<i32> {
        let n = self.choices.len().max(1);
        if inp.pressed & super::key::DOWN != 0 {
            self.choice_sel = (self.choice_sel + 1) % n;
        }
        if inp.pressed & super::key::UP != 0 {
            self.choice_sel = (self.choice_sel + n - 1) % n;
        }
        let (x0, y0) = Self::choice_box(n);
        if let Some((mx, my)) = inp.mouse {
            if mx >= x0 && mx < x0 + 360 && my >= y0 + 12 && my < y0 + 12 + n as i32 * 24 {
                self.choice_sel = ((my - y0 - 12) / 24) as usize;
            }
        }
        if inp.pressed & super::key::DECIDE != 0 {
            return Some(self.choice_sel as i32 + 1);
        }
        None
    }

    fn choice_box(n: usize) -> (i32, i32) {
        (140, 200 - n as i32 * 12)
    }

    pub fn screen_pos(&self, x: f32, y: f32) -> (i32, i32) {
        let sx = (x - y) * 8.0 - 8.0 + (self.map_w * 4) as f32 - self.scroll.0 as f32 * 8.0;
        let sy = (x + y) * 4.0 - (self.scroll.1 * 8 - 8) as f32;
        (sx as i32, sy as i32 + 40)
    }

    pub fn render(&mut self, scr: &mut Screen, font: Option<&Font>, tick: u32) {
        scr.clear(0);
        if let Some(bg) = &self.bg {
            scr.blit(bg, 0, 40, None);
        }
        let mut order: Vec<usize> = (0..self.actors.len()).collect();
        order.sort_by(|&a, &b| {
            let (pa, pb) = (&self.actors[a], &self.actors[b]);
            (pa.x + pa.y).partial_cmp(&(pb.x + pb.y)).unwrap_or(std::cmp::Ordering::Equal)
        });
        for i in order {
            let a = &self.actors[i];
            let (sx, sy) = self.screen_pos(a.x, a.y);
            let frame = if a.target.is_some() { [1, 0, 2, 0][((a.step / 4) % 4) as usize] } else { a.pose };
            let img = &a.images[a.dir as usize & 3];
            scr.blit_rect(img, 0, frame * 64, 48, 64, sx, sy - 64, Some(0));
        }
        // 위아래 띠
        scr.fill(0, 0, 640, 40, 0);
        scr.fill(0, 440, 640, 40, 0);
        self.render_overlay(scr, font, tick);
    }

    /// 창들만 (대화/해설/선택지/자막/표식)
    pub fn render_overlay(&mut self, scr: &mut Screen, font: Option<&Font>, tick: u32) {
        if let Some(f) = font {
            for (_, x, y, face, name) in &self.markers {
                let (x, y) = (*x, *y + 40);
                frame_box(scr, x - 36, y - 44, 72, 104);
                if let Some(face) = face {
                    scr.blit(face, x - 32, y - 40, None);
                }
                draw_text(scr, f, x - name.chars().count() as i32 * 8, y + 42, name, TEXT);
            }
            if !self.caption.is_empty() {
                let h = self.caption.len() as i32 * 18 + 12;
                frame_box(scr, 40, 436 - h, 560, h);
                for (i, l) in self.caption.iter().enumerate() {
                    draw_text(scr, f, 56, 442 - h + i as i32 * 18, l, TEXT);
                }
            }
            if self.title_timer > 0 {
                if !self.chapter_name.is_empty() {
                    draw_text(scr, f, 16, 12, &self.chapter_name, 251);
                }
                if !self.place_name.is_empty() {
                    draw_text(scr, f, 624 - self.place_name.chars().count() as i32 * 16, 12, &self.place_name, 18);
                }
            }
            if let Some(c) = &self.card {
                frame_box(scr, 160, 200, 320, 64);
                draw_text(scr, f, 320 - c.chars().count() as i32 * 7, 224, c, NAME);
            }
            if let Some(p) = self.pages.get(self.page) {
                if self.narration {
                    let h = 144;
                    frame_box(scr, 144, 240 - h / 2, 352, h);
                    for (i, l) in p.lines.iter().enumerate() {
                        draw_text(scr, f, 168, 240 - h / 2 + 16 + i as i32 * 20, l, TEXT);
                    }
                } else {
                    self.draw_window(scr, f, p, tick);
                }
            }
            if !self.choices.is_empty() {
                let n = self.choices.len();
                let (x0, y0) = Self::choice_box(n);
                frame_box(scr, x0, y0, 360, n as i32 * 24 + 24);
                for (i, c) in self.choices.iter().enumerate() {
                    let y = y0 + 12 + i as i32 * 24;
                    if i == self.choice_sel {
                        scr.fill(x0 + 8, y, 344, 22, 24);
                    }
                    draw_text(scr, f, x0 + 16, y + 3, &format!("{}. {c}", i + 1), TEXT);
                }
            }
        }
    }

    fn draw_window(&self, scr: &mut Screen, f: &Font, p: &Page, tick: u32) {
        // 원작: 448×96, 왼쪽 x=16 (얼굴 왼쪽) / 오른쪽 x=176 (얼굴 오른쪽). 말하는 인물이 위쪽이면 창은 아래 (y=318), 아니면 위 (y=56)
        let speaker_low = if let Some(y) = self.speaker_y { y > 240 } else { p.person.and_then(|id| self.actors.iter().find(|a| a.id == id)).map(|a| self.screen_pos(a.x, a.y).1 > 240).unwrap_or(false) };
        let y = if speaker_low { 56 } else { 318 };
        let x = if self.left { 16 } else { 176 };
        frame_box(scr, x, y, 448, 96);
        let (fx, tx) = if self.left { (x + 8, x + 0x70) } else { (x + 0x178, x + 0x20) };
        if let Some(face) = &self.face {
            scr.blit(face, fx, y + 8, None);
        }
        draw_text(scr, f, tx - 0x10, y + 6, &p.name, NAME);
        for (i, l) in p.lines.iter().enumerate() {
            draw_text(scr, f, tx, y + 0x1A + i as i32 * 16, l, TEXT);
        }
        // 다음 표시
        if (tick / 8) % 2 == 0 {
            scr.fill(x + 448 - 20, y + 96 - 14, 8, 6, 251);
        }
    }
}

/// 창 (원작 FUN_004752f2): 32×48 무늬를 깔고, 오른쪽·아래 3px 그림자(0x83,0x85,0x87), 위·왼쪽 1px 선(0x80)
pub fn frame_box(scr: &mut Screen, x: i32, y: i32, w: i32, h: i32) {
    match scr.window_tex.take() {
        Some(t) => {
            let mut ty = y;
            while ty < y + h {
                let mut tx = x;
                while tx < x + w {
                    scr.blit_rect(&t, 0, 0, (x + w - tx).min(32), (y + h - ty).min(48), tx, ty, None);
                    tx += 32;
                }
                ty += 48;
            }
            scr.window_tex = Some(t);
        }
        None => scr.fill(x, y, w, h, 30),
    }
    for k in 0..3 {
        let c = 0x83 + k as u8 * 2;
        scr.fill(x + k, y + h + k, w, 1, c);
        scr.fill(x + w + k, y + k, 1, h + 2, c);
    }
    scr.fill(x, y - 1, w, 1, 0x80);
    scr.fill(x - 1, y - 1, 1, h + 1, 0x80);
}

/// 창 글자 색 (원작 0x1f = 검정), 이름 색 (진한 빨강)
pub const TEXT: u8 = 0x1F;
pub const NAME: u8 = 62;

/// 글자 그리기 (EUC-KR). col = 팔레트 번호 (창 글자색이 아니면 그림자)
/// k 배 크게 쓴 글 (테두리 col_edge)
pub fn draw_text_big(scr: &mut Screen, f: &Font, x: i32, y: i32, s: &str, k: i32, col: u8, col_edge: u8) {
    let (b, _, _) = encoding_rs::EUC_KR.encode(s);
    for (dx, dy, c) in [(-2, 0, col_edge), (2, 0, col_edge), (0, -2, col_edge), (0, 2, col_edge), (0, 0, col)] {
        let mut cx = x;
        for code in font::codes(&b) {
            if let Some((w, rows)) = f.glyph(code) {
                for (r, bits) in rows.iter().enumerate() {
                    for bx in 0..w {
                        if bits & (0x8000 >> bx) != 0 {
                            scr.fill(cx + bx * k + dx, y + r as i32 * k + dy, k, k, c);
                        }
                    }
                }
                cx += w * k;
            } else {
                cx += if code >= 0x100 { 16 * k } else { 8 * k };
            }
        }
    }
}

pub fn draw_text(scr: &mut Screen, f: &Font, x: i32, y: i32, s: &str, col: u8) {
    let (b, _, _) = encoding_rs::EUC_KR.encode(s);
    let mut cx = x;
    for c in font::codes(&b) {
        if let Some((w, rows)) = f.glyph(c) {
            for (r, bits) in rows.iter().enumerate() {
                for bx in 0..w {
                    if bits & (0x8000 >> bx) != 0 {
                        let (px, py) = (cx + bx, y + r as i32);
                        if col != TEXT && col != NAME {
                            scr.fill(px + 1, py + 1, 1, 1, 31);
                        }
                        scr.fill(px, py, 1, 1, col);
                    }
                }
            }
            cx += w;
        } else {
            cx += if c >= 0x100 { 16 } else { 8 };
        }
    }
}
