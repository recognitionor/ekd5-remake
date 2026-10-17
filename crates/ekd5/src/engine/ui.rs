//! 공용 목록 창 (본진 메뉴, 상점, 장비, 출진 선택). 탭/방향키/결정·취소.

use super::screen::{Pic, Screen};
use super::story::{draw_text, frame_box, NAME, TEXT};
use super::{key, Input};
use crate::font::Font;

#[derive(Clone, Debug)]
pub struct ListBox {
    pub title: String,
    /// (왼쪽 글, 오른쪽 글, 고를 수 있나, 체크 표시)
    pub rows: Vec<(String, String, bool, bool)>,
    pub sel: usize,
    pub top: usize,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub visible: usize,
    /// 아래에 붙는 도움말
    pub footer: String,
    /// 줄마다 아이콘 (원작 ImageList)
    pub icons: Vec<Option<std::rc::Rc<Pic>>>,
}

impl PartialEq for ListBox {
    fn eq(&self, o: &Self) -> bool {
        self.title == o.title && self.rows == o.rows && self.sel == o.sel && self.top == o.top
    }
}

pub const ROW: i32 = 24;

/// 결과
pub enum Pick {
    None,
    Row(usize),
    Cancel,
}

impl ListBox {
    pub fn new(title: &str, w: i32) -> Self {
        Self { title: title.into(), rows: Vec::new(), sel: 0, top: 0, x: 320 - w / 2, y: 60, w, visible: 12, footer: String::new(), icons: Vec::new() }
    }

    pub fn push(&mut self, l: impl Into<String>, r: impl Into<String>, ok: bool) {
        self.rows.push((l.into(), r.into(), ok, false));
    }

    fn height(&self) -> i32 {
        self.rows.len().min(self.visible) as i32 * ROW + 40 + if self.footer.is_empty() { 0 } else { 22 }
    }

    pub fn update(&mut self, inp: &Input, tapped: bool) -> Pick {
        let n = self.rows.len();
        if n == 0 {
            return if inp.pressed & key::CANCEL != 0 || tapped { Pick::Cancel } else { Pick::None };
        }
        if inp.pressed & key::DOWN != 0 {
            self.sel = (self.sel + 1) % n;
        }
        if inp.pressed & key::UP != 0 {
            self.sel = (self.sel + n - 1) % n;
        }
        if self.sel < self.top {
            self.top = self.sel;
        }
        if self.sel >= self.top + self.visible {
            self.top = self.sel + 1 - self.visible;
        }
        if inp.pressed & key::CANCEL != 0 {
            return Pick::Cancel;
        }
        if tapped {
            if let Some((mx, my)) = inp.mouse {
                let top_y = self.y + 32;
                // 위/아래 넘기기 화살표
                if mx >= self.x + self.w - 40 && mx < self.x + self.w && my >= self.y && my < self.y + 28 && self.top > 0 {
                    self.top = self.top.saturating_sub(self.visible);
                    self.sel = self.top;
                    return Pick::None;
                }
                let bottom = top_y + self.rows.len().min(self.visible) as i32 * ROW;
                if mx >= self.x + self.w - 40 && mx < self.x + self.w && my >= bottom && my < bottom + 24 && self.top + self.visible < n {
                    self.top = (self.top + self.visible).min(n - 1);
                    self.sel = self.top;
                    return Pick::None;
                }
                if mx >= self.x && mx < self.x + self.w && my >= top_y && my < bottom {
                    let k = self.top + ((my - top_y) / ROW) as usize;
                    if k < n {
                        self.sel = k;
                        return if self.rows[k].2 { Pick::Row(k) } else { Pick::None };
                    }
                }
                if mx < self.x || mx >= self.x + self.w || my < self.y || my >= self.y + self.height() {
                    return Pick::Cancel;
                }
                return Pick::None;
            }
            return if self.rows[self.sel].2 { Pick::Row(self.sel) } else { Pick::None };
        }
        Pick::None
    }

    pub fn render(&self, scr: &mut Screen, f: &Font, hi: u8) {
        let h = self.height();
        frame_box(scr, self.x, self.y, self.w, h);
        draw_text(scr, f, self.x + 12, self.y + 8, &self.title, NAME);
        let top_y = self.y + 32;
        for (i, (l, r, ok, chk)) in self.rows.iter().enumerate().skip(self.top).take(self.visible) {
            let y = top_y + (i - self.top) as i32 * ROW;
            if i == self.sel {
                scr.fill(self.x + 6, y - 2, self.w - 12, ROW - 2, hi);
            }
            let col = if *ok { TEXT } else { 24 };
            if let Some(Some(p)) = self.icons.get(i) {
                scr.blit(p, self.x + 8, y + (ROW - 2 - p.h as i32) / 2 - 1, Some(0));
            }
            if *chk {
                draw_text(scr, f, self.x + 10, y + 2, "●", NAME);
            }
            draw_text(scr, f, self.x + 28, y + 2, l, col);
            if !r.is_empty() {
                let rw = r.chars().map(|c| if c.is_ascii() { 8 } else { 16 }).sum::<i32>();
                draw_text(scr, f, self.x + self.w - 14 - rw, y + 2, r, col);
            }
        }
        if self.top > 0 {
            draw_text(scr, f, self.x + self.w - 34, self.y + 8, "▲", NAME);
        }
        let bottom = top_y + self.rows.len().min(self.visible) as i32 * ROW;
        if self.top + self.visible < self.rows.len() {
            draw_text(scr, f, self.x + self.w - 34, bottom + 2, "▼", NAME);
        }
        if !self.footer.is_empty() {
            draw_text(scr, f, self.x + 12, self.y + h - 24, &self.footer, TEXT);
        }
    }
}
