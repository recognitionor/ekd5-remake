//! 게임 엔진: 스크립트 + 화면 + 입력. 매 틱(30Hz) `set_input` → `tick` → `render`.

pub mod assets;
pub mod screen;

use crate::{ls12, palette};
use assets::{Asset, Assets};
use screen::{Pic, Screen, H, W};
use std::rc::Rc;

pub const TICK_HZ: f32 = 30.0;

/// 입력 비트
pub mod key {
    pub const DECIDE: u32 = 0x20;
    pub const CANCEL: u32 = 0x40;
    pub const UP: u32 = 0x1000;
    pub const RIGHT: u32 = 0x2000;
    pub const DOWN: u32 = 0x4000;
    pub const LEFT: u32 = 0x8000;
}

#[derive(Clone, Default)]
pub struct Input {
    pub held: u32,
    pub pressed: u32,
    pub mouse: Option<(i32, i32)>,
    pub click: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Sound {
    Effect(String),
}

/// Ls12 묶음 캐시
#[derive(Default)]
pub struct Packs {
    files: std::collections::HashMap<String, (Rc<Vec<u8>>, Vec<ls12::Entry>)>,
}

impl Packs {
    /// 묶음 `name`의 i번째 항목. 아직 받는 중이면 Err(true), 없으면 Err(false)
    pub fn get<A: Assets>(&mut self, assets: &mut A, name: &str, i: usize) -> Result<Vec<u8>, bool> {
        if !self.files.contains_key(name) {
            match assets.get(name) {
                Asset::Ready(d) => {
                    let e = ls12::entries(&d);
                    self.files.insert(name.to_string(), (d, e));
                }
                Asset::Pending => return Err(true),
                Asset::Missing => return Err(false),
            }
        }
        let (d, e) = &self.files[name];
        e.get(i).and_then(|&e| ls12::unpack(d, e)).ok_or(false)
    }
}

enum Scene {
    /// 시작 화면 대기 (그림 받는 중)
    Boot,
    /// 타이틀 (LOGO.E5 0 + LOGO_P 0)
    Title { pic: Pic, t: u32 },
}

pub struct Engine<A: Assets> {
    pub assets: A,
    pub packs: Packs,
    pub screen: Screen,
    pub input: Input,
    pub tick: u32,
    scene: Scene,
    pub sounds: Vec<Sound>,
    pub log: Vec<String>,
}

impl<A: Assets> Engine<A> {
    pub fn new(assets: A) -> Self {
        Self { assets, packs: Packs::default(), screen: Screen::default(), input: Input::default(), tick: 0, scene: Scene::Boot, sounds: Vec::new(), log: Vec::new() }
    }

    pub fn set_input(&mut self, held: u32, mouse: Option<(i32, i32)>, click: bool) {
        self.input.pressed = held & !self.input.held;
        self.input.held = held;
        self.input.mouse = mouse;
        self.input.click = click;
    }

    pub fn tick(&mut self) {
        self.tick += 1;
        match &mut self.scene {
            Scene::Boot => {
                let pic = self.packs.get(&mut self.assets, "LOGO.E5", 0);
                let pal = self.packs.get(&mut self.assets, "LOGO_P.E5", 0);
                if let (Ok(pic), Ok(pal)) = (pic, pal) {
                    self.screen.pal = palette::from_bytes(&pal);
                    self.scene = Scene::Title { pic: Pic::new(640, 400, pic), t: 0 };
                }
            }
            Scene::Title { t, .. } => {
                *t += 1;
            }
        }
    }

    pub fn render(&mut self) -> &[u8] {
        match &self.scene {
            Scene::Boot => self.screen.clear(0),
            Scene::Title { pic, t } => {
                self.screen.clear(0);
                self.screen.blit(pic, 0, ((H - pic.h) / 2) as i32, None);
                self.screen.level = (*t / 2).min(16) as u8;
            }
        }
        let _ = W;
        self.screen.rgba()
    }

    pub fn take_sounds(&mut self) -> Vec<Sound> {
        std::mem::take(&mut self.sounds)
    }
}
