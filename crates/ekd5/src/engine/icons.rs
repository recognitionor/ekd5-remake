//! 아이콘 (원작 ItemIcon/MgcIcon/CmdIcon/MapAtr.dll 비트맵, 추출 때 data/ICONS.BIN 으로 묶음).
//! 묶음: 0 아이템 (그림 번호 k → 작은 2k / 큰 2k+1), 1 책략, 2 명령, 3 지형(48×48), 4 본진 단추 (EKD5.EXE 0x155 출진·0x156 매각·0x157 구입·0x158 장비). 색 0 = 투명.

use super::screen::Pic;
use crate::palette::{nearest, Palette};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub const ITEM: u8 = 0;
pub const MAGIC: u8 = 1;
pub const COMMAND: u8 = 2;
pub const TERRAIN: u8 = 3;
pub const CAMP: u8 = 4;

struct Img {
    w: usize,
    h: usize,
    pal: Vec<u8>,
    px: Vec<u8>,
}

#[derive(Default)]
pub struct Icons {
    groups: Vec<Vec<Img>>,
    /// 화면 팔레트에 맞춘 그림 (팔레트가 바뀌면 비움)
    cache: RefCell<(u64, HashMap<(u8, usize), Rc<Pic>>)>,
}

impl Icons {
    pub fn parse(d: &[u8]) -> Icons {
        let mut groups = Vec::new();
        if d.get(..4) != Some(b"ICON") {
            return Icons::default();
        }
        let mut o = 4;
        let u16at = |o: usize| d.get(o..o + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]) as usize);
        for _ in 0..5 {
            if o + 2 > d.len() {
                break;
            }
            let n = u16at(o);
            o += 2;
            let mut g = Vec::new();
            for _ in 0..n {
                let (w, h) = (u16at(o), u16at(o + 2));
                o += 4;
                let pal = d.get(o..o + 768).map(|b| b.to_vec()).unwrap_or_default();
                o += 768;
                let px = d.get(o..o + w * h).map(|b| b.to_vec()).unwrap_or_default();
                o += w * h;
                g.push(Img { w, h, pal, px });
            }
            groups.push(g);
        }
        Icons { groups, cache: Default::default() }
    }

    /// 화면 팔레트로 옮긴 그림 (투명 = 0)
    pub fn pic(&self, group: u8, idx: usize, pal: &Palette) -> Option<Rc<Pic>> {
        let img = self.groups.get(group as usize)?.get(idx)?;
        let sum = pal.iter().enumerate().fold(0u64, |a, (i, c)| a.wrapping_mul(31).wrapping_add((c[0] as u64) << 16 | (c[1] as u64) << 8 | c[2] as u64).wrapping_add(i as u64));
        let mut cache = self.cache.borrow_mut();
        if cache.0 != sum {
            *cache = (sum, HashMap::new());
        }
        if let Some(p) = cache.1.get(&(group, idx)) {
            return Some(p.clone());
        }
        // 본진 단추는 불투명 (색 0 도 그림)
        let opaque = group == CAMP;
        let mut map = [0u8; 256];
        for (c, m) in map.iter_mut().enumerate().skip(if opaque { 0 } else { 1 }) {
            let q = img.pal.get(c * 3..c * 3 + 3).unwrap_or(&[0, 0, 0]);
            *m = nearest(pal, [q[0], q[1], q[2]]).max(1);
        }
        let px: Vec<u8> = img.px.iter().map(|&c| if c == 0 && !opaque { 0 } else { map[c as usize] }).collect();
        let p = Rc::new(Pic::new(img.w, img.h, px));
        cache.1.insert((group, idx), p.clone());
        Some(p)
    }

    /// 아이템 작은 아이콘 (아이템 +20 그림 번호)
    pub fn item(&self, items: &[u8], id: u8, pal: &Palette) -> Option<Rc<Pic>> {
        let k = *items.get(id as usize * 25 + 20)? as usize;
        self.pic(ITEM, k * 2, pal)
    }
}
