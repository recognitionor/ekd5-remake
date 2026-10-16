//! 게임 엔진: 스크립트 + 화면 + 입력. 매 틱(30Hz) `set_input` → `tick` → `render`.
//! 진행: 단계 번호 n (원작 0x4baee8) — 짝수 = R_(n/20)((n%20)/2).EEX 이야기, 홀수 = S_ 전투 (원작 FUN_0041a35e).

pub mod assets;
pub mod screen;
pub mod story;

use crate::eex;
use crate::font::Font;
use crate::vm::{Arg, Vm, VmError};
use crate::{ls12, palette};
use assets::{Asset, Assets};
use screen::{Pic, Screen, H};
use std::collections::HashMap;
use std::rc::Rc;
use story::Story;

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
    /// CD 음악 트랙 번호 (원작 op24: 인자+2), None = 정지
    Music(Option<i32>),
}

/// Ls12 묶음 캐시
#[derive(Default)]
pub struct Packs {
    files: HashMap<String, (Rc<Vec<u8>>, Vec<ls12::Entry>)>,
}

/// 받는 중
#[derive(Debug)]
pub struct Pending;

impl Packs {
    /// 묶음 `name`의 i번째 항목. Ok(None) = 없음
    pub fn get<A: Assets>(&mut self, assets: &mut A, name: &str, i: usize) -> Result<Option<Vec<u8>>, Pending> {
        if !self.files.contains_key(name) {
            match assets.get(name) {
                Asset::Ready(d) => {
                    let e = ls12::entries(&d);
                    self.files.insert(name.to_string(), (d, e));
                }
                Asset::Pending => return Err(Pending),
                Asset::Missing => return Ok(None),
            }
        }
        let (d, e) = &self.files[name];
        Ok(e.get(i).and_then(|&e| ls12::unpack(d, e)))
    }
}

/// 인물 (DATA.E5 0번, 32바이트: 이름, +13 얼굴, +15 이야기 스프라이트)
#[derive(Clone, Debug, Default)]
pub struct Person {
    pub name: String,
    pub face: u16,
    pub sprite: u16,
}

pub enum Scene {
    Boot,
    Title { pic: Pic, t: u32 },
    Story(Box<Story>),
    /// 전투 (아직)
    Battle { t: u32 },
}

/// 스크립트 대기 상태
#[derive(Debug, Clone, PartialEq)]
pub enum Wait {
    None,
    Ticks(u32),
    Moves,
    Dialog,
    Choice,
    /// 화면 밝기 목표 (0 / 16)
    Fade(u8),
    /// 자막 넘기기
    Key,
}

pub struct Engine<A: Assets> {
    pub assets: A,
    pub packs: Packs,
    pub screen: Screen,
    pub input: Input,
    pub tick: u32,
    pub scene: Scene,
    pub sounds: Vec<Sound>,
    pub log: Vec<String>,
    pub font: Option<Font>,
    pub persons: Vec<Person>,
    pub stage: u32,
    pub vm: Option<Vm>,
    pub wait: Wait,
    pub flags: Vec<bool>,
    pub choice: i32,
    pub error: Option<String>,
    enter_after_choice: bool,
    unknown: std::collections::BTreeSet<u16>,
}

impl<A: Assets> Engine<A> {
    pub fn new(assets: A) -> Self {
        Self {
            assets,
            packs: Packs::default(),
            screen: Screen::default(),
            input: Input::default(),
            tick: 0,
            scene: Scene::Boot,
            sounds: Vec::new(),
            log: Vec::new(),
            font: None,
            persons: Vec::new(),
            stage: 0,
            vm: None,
            wait: Wait::None,
            flags: vec![false; 0x1000],
            choice: 0,
            error: None,
            enter_after_choice: false,
            unknown: Default::default(),
        }
    }

    pub fn set_input(&mut self, held: u32, mouse: Option<(i32, i32)>, click: bool) {
        self.input.pressed = held & !self.input.held;
        self.input.held = held;
        self.input.mouse = mouse;
        self.input.click = click;
    }

    /// 기본 자료 (글꼴, 인물) 적재. 받는 중이면 false
    fn load_base(&mut self) -> bool {
        if self.font.is_none() {
            let (a, k) = (self.assets.get("ALL_FONT.16P"), self.assets.get("ANKFONT.DAT"));
            match (a, k) {
                (Asset::Ready(a), Asset::Ready(k)) => self.font = Some(Font { all: a.to_vec(), ank: k.to_vec() }),
                (Asset::Pending, _) | (_, Asset::Pending) => return false,
                _ => self.font = Some(Font { all: Vec::new(), ank: Vec::new() }),
            }
        }
        if self.persons.is_empty() {
            match self.packs.get(&mut self.assets, "DATA.E5", 0) {
                Err(Pending) => return false,
                Ok(d) => {
                    let d = d.unwrap_or_default();
                    self.persons = d
                        .chunks(32)
                        .filter(|r| r.len() == 32)
                        .map(|r| {
                            let e = r[..13].iter().position(|&b| b == 0).unwrap_or(13);
                            let (name, _, _) = encoding_rs::EUC_KR.decode(&r[..e]);
                            Person { name: name.to_string(), face: u16::from_le_bytes([r[13], r[14]]), sprite: u16::from_le_bytes([r[15], r[16]]) }
                        })
                        .collect();
                }
            }
        }
        true
    }

    pub fn tick(&mut self) {
        self.tick += 1;
        if self.error.is_some() {
            return;
        }
        match &mut self.scene {
            Scene::Boot => {
                let pic = self.packs.get(&mut self.assets, "LOGO.E5", 0);
                let pal = self.packs.get(&mut self.assets, "LOGO_P.E5", 0);
                if let (Ok(Some(pic)), Ok(Some(pal))) = (pic, pal) {
                    self.screen.pal = palette::from_bytes(&pal);
                    self.scene = Scene::Title { pic: Pic::new(640, 400, pic), t: 0 };
                }
            }
            Scene::Title { t, .. } => {
                *t += 1;
                if *t > 10 && self.input.pressed & key::DECIDE != 0 {
                    self.scene = Scene::Story(Box::default());
                    self.vm = None;
                }
            }
            Scene::Story(_) | Scene::Battle { .. } => {
                if !self.load_base() {
                    return;
                }
                if self.vm.is_none() && !self.load_stage() {
                    return;
                }
                self.update_scene();
                self.run_script();
            }
        }
    }

    /// 현재 단계의 EEX 적재. 받는 중이면 false
    fn load_stage(&mut self) -> bool {
        let n = self.stage;
        let name = format!("{}_{}{}.EEX", if n % 2 == 0 { "R" } else { "S" }, n / 20, (n % 20) / 2);
        match self.assets.get(&name) {
            Asset::Pending => false,
            Asset::Missing => {
                self.error = Some(format!("{name} 없음"));
                false
            }
            Asset::Ready(d) => match eex::parse(d.to_vec()) {
                None => {
                    self.error = Some(format!("{name} 형식 오류"));
                    false
                }
                Some(s) => {
                    self.log.push(format!("단계 {n}: {name}"));
                    let mut vm = Vm::new(Rc::new(s));
                    if let Err(e) = self.start_section(&mut vm, 0) {
                        self.error = Some(e.0);
                    }
                    self.vm = Some(vm);
                    if n % 2 == 1 {
                        self.scene = Scene::Battle { t: 0 };
                    } else if !matches!(self.scene, Scene::Story(_)) {
                        self.scene = Scene::Story(Box::default());
                    }
                    true
                }
            },
        }
    }

    /// 절 시작: 머리 조건이 모두 참인 마지막 블록의 본문으로 (원작 FUN_00412471)
    fn start_section(&mut self, vm: &mut Vm, sec: usize) -> Result<(), VmError> {
        vm.section = sec;
        vm.ends.clear();
        let blocks = vm.blocks(sec)?;
        let mut chosen = None;
        for (start, _end) in blocks {
            vm.pc = start;
            let mut ok = true;
            loop {
                let (op, args) = vm.fetch()?;
                if op == 0 {
                    break;
                }
                ok &= self.cond(op, &args);
            }
            if ok {
                chosen = Some(vm.pc);
            }
        }
        match chosen {
            Some(body) => {
                vm.pc = body;
                vm.enter_body(true)?;
                vm.running = true;
            }
            None => vm.running = false,
        }
        Ok(())
    }

    /// 조건 명령 (참/거짓)
    fn cond(&mut self, op: u16, args: &[Arg]) -> bool {
        match op {
            2 => true,
            // 플래그 목록: 앞 목록은 모두 켜짐, 뒤 목록은 모두 꺼짐 (원작 FUN_00413b71)
            5 => {
                let n1 = args.first().map_or(0, |a| a.i()) as usize;
                let n2 = args.get(1 + n1).map_or(0, |a| a.i()) as usize;
                let on: Vec<i32> = args[1..1 + n1].iter().map(|a| a.i()).collect();
                let off: Vec<i32> = args[2 + n1..2 + n1 + n2].iter().map(|a| a.i()).collect();
                on.iter().all(|&f| self.flag(f)) && off.iter().all(|&f| !self.flag(f))
            }
            0x13 => self.choice == args[0].i(),
            _ => {
                self.note(op);
                true
            }
        }
    }

    fn flag(&self, i: i32) -> bool {
        self.flags.get(i as usize).copied().unwrap_or(false)
    }

    fn note(&mut self, op: u16) {
        if self.unknown.insert(op) {
            self.log.push(format!("미구현 명령 {op:#04x}"));
        }
    }

    /// 대기 조건 갱신
    fn update_scene(&mut self) {
        let inp = self.input.clone();
        let decided = inp.pressed & key::DECIDE != 0;
        let Scene::Story(st) = &mut self.scene else {
            if let Scene::Battle { t } = &mut self.scene {
                *t += 1;
                // 전투는 아직 — 선택으로 건너뛰기
                if *t > 30 && decided {
                    self.stage += 1;
                    self.vm = None;
                    self.scene = Scene::Story(Box::default());
                }
            }
            return;
        };
        st.update();
        match self.wait.clone() {
            Wait::Ticks(n) => {
                self.wait = if n <= 1 { Wait::None } else { Wait::Ticks(n - 1) };
            }
            Wait::Moves => {
                if !st.moving() {
                    self.wait = Wait::None;
                }
            }
            Wait::Dialog => {
                if decided && st.next_page() {
                    self.wait = Wait::None;
                }
            }
            Wait::Choice => {
                if let Some(c) = st.choose(&inp) {
                    self.choice = c;
                    st.choices.clear();
                    self.wait = Wait::None;
                }
            }
            Wait::Fade(to) => {
                let l = &mut self.screen.level;
                if *l < to {
                    *l += 2;
                } else if *l > to {
                    *l = l.saturating_sub(2);
                }
                if *l == to {
                    self.wait = Wait::None;
                }
            }
            Wait::Key => {
                if decided {
                    if st.caption_clear_after {
                        st.caption.clear();
                    }
                    self.wait = Wait::None;
                }
            }
            Wait::None => {}
        }
    }

    fn run_script(&mut self) {
        if matches!(self.scene, Scene::Battle { .. }) {
            return;
        }
        let Some(mut vm) = self.vm.take() else { return };
        if self.wait == Wait::None && self.enter_after_choice {
            self.enter_after_choice = false;
            if let Err(e) = vm.enter_body(true) {
                self.error = Some(e.0);
            }
        }
        let mut budget = 300;
        while self.wait == Wait::None && budget > 0 && vm.running {
            budget -= 1;
            match self.step(&mut vm) {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => {
                    self.error = Some(format!("{} (단계 {} pc {:#x})", e.0, self.stage, vm.pc));
                    break;
                }
            }
        }
        if !vm.running && self.error.is_none() {
            // 이야기(R_)는 절을 차례로 (원작 FUN_00412978: 최상위 끝 → 다음 절)
            let next = vm.section + 1;
            if self.stage % 2 == 0 && next < vm.sections() {
                if let Err(e) = self.start_section(&mut vm, next) {
                    self.error = Some(e.0);
                }
                self.vm = Some(vm);
                return;
            }
            self.stage += 1;
            self.vm = None;
            return;
        }
        self.vm = Some(vm);
    }

    /// 명령 하나. Ok(false) = 이번 틱은 여기까지
    fn step(&mut self, vm: &mut Vm) -> Result<bool, VmError> {
        let at = vm.pc;
        let (op, args) = vm.fetch()?;
        let a = |i: usize| args.get(i).map_or(0, |a| a.i());
        match op {
            0 => {
                if let Some(end) = vm.ends.pop() {
                    if vm.pc != end {
                        return Err(VmError(format!("본문 끝 어긋남 {:#x} ≠ {end:#x}", vm.pc)));
                    }
                    if vm.ends.is_empty() {
                        vm.running = false;
                    }
                } else {
                    vm.running = false;
                }
            }
            1 => {
                let (cop, cargs) = vm.fetch()?;
                if cop == 0x12 {
                    // 선택지: 고른 뒤 본문으로 (본문 안에서 op13 으로 결과 비교)
                    self.start_choice(&cargs);
                    self.enter_after_choice = true;
                    return Ok(false);
                }
                let r = self.cond(cop, &cargs);
                vm.enter_body(r)?;
            }
            0xD => {
                // 다음 절로 (원작 op0d → 7)
                let next = vm.section + 1;
                if next < vm.sections() {
                    self.start_section(vm, next)?;
                } else {
                    vm.running = false;
                }
                return Ok(false);
            }
            0xA => self.flags[..0x100].fill(false),
            0xB => {
                let i = a(0) as usize;
                if i < self.flags.len() {
                    self.flags[i] = a(1) != 0;
                }
            }
            9 => {
                if a(0) > 0 {
                    self.wait = Wait::Ticks(a(0) as u32);
                }
            }
            0x24 => {
                let n = a(0);
                self.sounds.push(Sound::Music(if n == 0xFF { None } else { Some(n + 2) }));
            }
            0x23 => self.sounds.push(Sound::Effect(format!("SE{:02}.WAV", a(0)))),
            0x27 => {
                // 장면 전환: 0 = 정지 그림 MMAP[b], 1 = MMAP[c+0x72], 2 = 이야기 맵 d (원작 FUN_00416112 → FUN_0040c017)
                let mode = a(0);
                let v = args.get(1 + mode.clamp(0, 3) as usize).and_then(|x| x.opt()).unwrap_or(0);
                if let Scene::Story(st) = &mut self.scene {
                    match st.load_map(&mut self.packs, &mut self.assets, mode, v) {
                        Err(Pending) => {
                            vm.pc = at;
                            return Ok(false);
                        }
                        Ok(pal) => {
                            if let Some(p) = pal {
                                self.screen.pal = p;
                            }
                        }
                    }
                }
                return Ok(false);
            }
            0x30 => {
                // 인물 놓기 (인물, x, y, 방향, 자세)
                let pid = a(0) as usize;
                let sprite = self.persons.get(pid).map_or(0, |p| p.sprite);
                if let Scene::Story(st) = &mut self.scene {
                    if st.place(&mut self.packs, &mut self.assets, pid, sprite, a(1), a(2), a(3), a(4)).is_err() {
                        vm.pc = at;
                        return Ok(false);
                    }
                }
            }
            0x32 => {
                // 이동 (종류, 인물, ?, x, y, 방향). 다음 명령도 이동이면 함께, 아니면 다 걸을 때까지 대기 (원작 FUN_004156d3)
                if let Scene::Story(st) = &mut self.scene {
                    st.walk(a(1) as usize, a(3), a(4), a(5));
                }
                if vm.peek_op() != 0x32 {
                    self.wait = Wait::Moves;
                }
            }
            0x33 => {
                if let Scene::Story(st) = &mut self.scene {
                    st.turn(a(0) as usize, a(2));
                }
            }
            0x34 => {
                if let Scene::Story(st) = &mut self.scene {
                    st.pose(a(0) as usize, a(1));
                }
            }
            0x14 => {
                let pages = self.parse_dialog(args[0].bytes());
                if let Scene::Story(st) = &mut self.scene {
                    if st.show_dialog(&mut self.packs, &mut self.assets, pages).is_err() {
                        vm.pc = at;
                        return Ok(false);
                    }
                    self.wait = Wait::Dialog;
                }
            }
            0x12 => self.start_choice(&args),
            0x1C => self.wait = Wait::Fade(16),
            0x1D => self.wait = Wait::Fade(0),
            0x16 => {
                // 해설 창 (원작 FUN_0042fa65(글, 0xffff): 가운데 352×144)
                let (t, _, _) = encoding_rs::EUC_KR.decode(args[0].bytes());
                if let Scene::Story(st) = &mut self.scene {
                    let lines: Vec<String> = t.split('\n').filter(|l| !l.is_empty()).map(|l| l.to_string()).collect();
                    st.pages = lines.chunks(6).map(|c| story::Page { person: None, face: None, name: String::new(), lines: c.to_vec() }).collect();
                    st.page = 0;
                    st.narration = true;
                    self.wait = Wait::Dialog;
                }
            }
            0x2C => {
                // 지도 자막 (글, 이어쓰기, 키 대기, 대기 후 지우기)
                let (t, _, _) = encoding_rs::EUC_KR.decode(args[0].bytes());
                if let Scene::Story(st) = &mut self.scene {
                    if a(1) == 0 {
                        st.caption.clear();
                    }
                    let mut parts = t.split('\n');
                    if let Some(first) = parts.next() {
                        match st.caption.last_mut() {
                            Some(l) if a(1) != 0 => l.push_str(first),
                            _ => st.caption.push(first.to_string()),
                        }
                    }
                    for p in parts {
                        st.caption.push(p.to_string());
                    }
                    st.caption_clear_after = a(3) != 0;
                    if a(2) != 0 {
                        self.wait = Wait::Key;
                    }
                }
            }
            0x29 | 0x2A => {
                // 지도 표식 (인물, x, y): 29 놓기, 2a 옮기기
                let pid = a(0) as usize;
                let face = if pid < self.persons.len() { self.face_of(pid) } else { 0 };
                let name = self.persons.get(pid).map(|p| p.name.clone()).unwrap_or_default();
                if let Scene::Story(st) = &mut self.scene {
                    match self.packs.get(&mut self.assets, "FACE.E5", face as usize) {
                        Err(Pending) => {
                            vm.pc = at;
                            return Ok(false);
                        }
                        Ok(d) => {
                            st.markers.retain(|m| m.0 != pid);
                            st.markers.push((pid, a(1), a(2), d.map(|d| Pic::new(64, 80, d)), name));
                        }
                    }
                }
            }
            0x2B => {
                if let Scene::Story(st) = &mut self.scene {
                    st.markers.retain(|m| m.0 != a(0) as usize);
                }
            }
            0x17 | 0x18 => {
                let (t, _, _) = encoding_rs::EUC_KR.decode(args[0].bytes());
                if let Scene::Story(st) = &mut self.scene {
                    if op == 0x17 { st.place_name = t.to_string() } else { st.chapter_name = t.to_string() }
                    st.title_timer = 90;
                }
            }
            0x67 => {
                // 장 제목 카드
                let (t, _, _) = encoding_rs::EUC_KR.decode(args[1].bytes());
                if let Scene::Story(st) = &mut self.scene {
                    st.card = Some(format!("제{}장  {}", a(0), t));
                    self.wait = Wait::Ticks(75);
                }
            }
            _ => self.note(op),
        }
        Ok(true)
    }

    fn start_choice(&mut self, args: &[Arg]) {
        let (s, _, _) = encoding_rs::EUC_KR.decode(args[0].bytes());
        if let Scene::Story(st) = &mut self.scene {
            st.choices = s.split('\n').filter(|l| !l.is_empty()).map(|l| l.to_string()).collect();
            st.choice_sel = 0;
        }
        self.wait = Wait::Choice;
    }

    /// 대사 "&이름\n글…&이름\n글…" → 쪽들 (원작 FUN_00415a60: '&' 마다 한 창)
    fn parse_dialog(&self, text: &[u8]) -> Vec<story::Page> {
        let (s, _, _) = encoding_rs::EUC_KR.decode(text);
        let mut pages = Vec::new();
        for seg in s.split('&').filter(|x| !x.is_empty()) {
            let mut lines = seg.split('\n');
            let name = lines.next().unwrap_or("").trim().to_string();
            let body: Vec<String> = lines.filter(|l| !l.is_empty()).map(|l| l.to_string()).collect();
            let pid = self.persons.iter().position(|p| p.name == name);
            for chunk in body.chunks(4) {
                pages.push(story::Page { person: pid, face: pid.map(|i| self.face_of(i)), name: name.clone(), lines: chunk.to_vec() });
            }
        }
        pages
    }

    /// 대화 얼굴 번호 (원작 FUN_00407997: 대개 얼굴+7, 조조는 0..7 상태별, 일부 인물은 진행 상태에 따라 고정 얼굴)
    fn face_of(&self, pid: usize) -> u16 {
        match pid {
            0 => 0,
            1 => 0xDF,
            _ => self.persons[pid].face + 7,
        }
    }

    pub fn render(&mut self) -> &[u8] {
        let scr = &mut self.screen;
        match &mut self.scene {
            Scene::Boot => scr.clear(0),
            Scene::Title { pic, t } => {
                scr.clear(0);
                scr.blit(pic, 0, ((H - pic.h) / 2) as i32, None);
                scr.level = (*t / 2).min(16) as u8;
            }
            Scene::Story(st) => {
                scr.level = 16;
                st.render(scr, self.font.as_ref(), self.tick);
            }
            Scene::Battle { .. } => {
                scr.level = 16;
                scr.clear(0);
                if let Some(f) = &self.font {
                    story::draw_text(scr, f, 180, 220, "전투는 아직 만드는 중입니다", 255);
                    story::draw_text(scr, f, 210, 250, "(화면을 누르면 넘어갑니다)", 255);
                }
            }
        }
        if let Some(e) = &self.error {
            if let Some(f) = &self.font {
                scr.fill(0, 0, 640, 20, 0);
                story::draw_text(scr, f, 4, 2, &format!("오류: {e}"), 251);
            }
        }
        scr.rgba()
    }

    pub fn take_sounds(&mut self) -> Vec<Sound> {
        std::mem::take(&mut self.sounds)
    }
}
