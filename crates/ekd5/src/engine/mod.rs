//! 게임 엔진: 스크립트 + 화면 + 입력. 매 틱(30Hz) `set_input` → `tick` → `render`.
//! 진행: 단계 번호 n (원작 0x4baee8) — 짝수 = R_(n/20)((n%20)/2).EEX 이야기, 홀수 = S_ 전투 (원작 FUN_0041a35e).

pub mod assets;
pub mod battle;
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
    /// 능력치 (+18..22: 무력, 통솔, 지력, 민첩, 운)
    pub stats: [u8; 5],
    /// 직업 (+26, JOB 표 번호)
    pub job: u8,
}

pub enum Scene {
    Boot,
    Title { pic: Pic, t: u32 },
    Story(Box<Story>),
    Battle(Box<battle::Battle>),
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
    /// 직전 IF 결과 (op03 = ELSE)
    last_if: bool,
    /// 직업 표 / 지형 표 (DATA.E5 3, 4)
    pub jobs: Rc<Vec<u8>>,
    pub terrain_tab: Rc<Vec<u8>>,
    /// 조조군 장수 (op3b 로 합류한 순서)
    pub roster: Vec<usize>,
    /// 사건 처리 뒤 돌아갈 전투 단계
    resume: Option<battle::Phase>,
    /// 시험용: 아군도 컴퓨터가 둠
    pub autoplay: bool,
    /// 전투 시작 시점 저장 (패배 후 다시 하기용)
    saved: Option<(Vec<bool>, Vec<usize>)>,
    /// 단계 시작마다 만든 저장 문자열 (웹이 가져감)
    pub save_out: Option<String>,
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
            last_if: false,
            jobs: Rc::new(Vec::new()),
            terrain_tab: Rc::new(Vec::new()),
            roster: Vec::new(),
            resume: None,
            autoplay: false,
            saved: None,
            save_out: None,
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
                            Person {
                                name: name.to_string(),
                                face: u16::from_le_bytes([r[13], r[14]]),
                                sprite: u16::from_le_bytes([r[15], r[16]]),
                                stats: [r[18], r[19], r[20], r[21], r[22]],
                                job: r[26],
                            }
                        })
                        .collect();
                }
            }
        }
        if self.jobs.is_empty() {
            match (self.packs.get(&mut self.assets, "DATA.E5", 3), self.packs.get(&mut self.assets, "DATA.E5", 4)) {
                (Ok(j), Ok(t)) => {
                    self.jobs = Rc::new(j.unwrap_or_default());
                    self.terrain_tab = Rc::new(t.unwrap_or_default());
                }
                _ => return false,
            }
        }
        true
    }

    /// 대화/해설 창을 그리는 곳 (이야기 장면 또는 전투 겹침)
    fn ui(&mut self) -> Option<&mut Story> {
        match &mut self.scene {
            Scene::Story(s) => Some(s),
            Scene::Battle(b) => Some(&mut b.ui),
            _ => None,
        }
    }

    fn battle(&mut self) -> Option<&mut battle::Battle> {
        match &mut self.scene {
            Scene::Battle(b) => Some(b),
            _ => None,
        }
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
                    self.screen.level = 16;
                }
            }
            Scene::Story(_) | Scene::Battle(_) => {
                if !self.load_base() {
                    return;
                }
                if self.vm.is_none() && !self.load_stage() {
                    return;
                }
                if !self.load_battle_map() {
                    return;
                }
                self.update_scene();
                self.run_script();
                self.update_battle();
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
                    self.save_out = Some(self.snapshot());
                    let mut vm = Vm::new(Rc::new(s));
                    if n % 2 == 1 {
                        match &self.saved {
                            Some((f, r)) => {
                                self.flags = f.clone();
                                self.roster = r.clone();
                            }
                            None => self.saved = Some((self.flags.clone(), self.roster.clone())),
                        }
                        let mut b = battle::Battle::new((n / 2) as usize, self.jobs.clone(), self.terrain_tab.clone());
                        b.cheat = self.autoplay;
                        self.scene = Scene::Battle(Box::new(b));
                    } else if !matches!(self.scene, Scene::Story(_)) {
                        self.scene = Scene::Story(Box::default());
                    }
                    if let Err(e) = self.start_section(&mut vm, 0) {
                        self.error = Some(e.0);
                    }
                    self.vm = Some(vm);
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
            3 => !self.last_if,
            _ if self.battle().is_some() => self.battle_cond(op, args),
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
        let battle_idle = match &mut self.scene {
            Scene::Battle(b) => b.animate(),
            _ => true,
        };
        let level = self.screen.level;
        let st: &mut Story = match &mut self.scene {
            Scene::Story(s) => s,
            Scene::Battle(b) => &mut b.ui,
            _ => return,
        };
        st.update();
        match self.wait.clone() {
            Wait::Ticks(n) => {
                self.wait = if n <= 1 { Wait::None } else { Wait::Ticks(n - 1) };
            }
            Wait::Moves => {
                if !st.moving() && battle_idle {
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
                let l = if level < to { (level + 2).min(to) } else { level.saturating_sub(2).max(to) };
                self.screen.level = l;
                if l == to {
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
        if !vm.running && self.error.is_none() && self.stage % 2 == 1 && self.wait == Wait::None {
            // 전투: 절 0(배치·도입) 끝 → 턴 시작. 절 1 사건 본문 끝 → 하던 단계로. 절 2(끝) → 다음 단계
            if vm.section == 0 {
                let roster = if self.roster.is_empty() { vec![0] } else { self.roster.clone() };
                let persons = self.persons.clone();
                if let Some(b) = self.battle() {
                    if !b.started {
                        // 출진 (원작은 출진 화면에서 고름): 출진 칸이 둘 이상 정해져 있으면 합류 순서대로 자동 배치
                        if b.slots.get(1).is_some_and(|&s| s != (0, 0)) {
                            for (k, &pid) in roster.iter().enumerate() {
                                let Some(&(x, y)) = b.slots.get(k) else { break };
                                if k > 0 && (x, y) == (0, 0) {
                                    break;
                                }
                                if b.unit_of(pid).is_none() && pid < persons.len() {
                                    let u = b.make_unit(pid, &persons[pid], battle::Side::Player, x, y, 1);
                                    b.units.push(u);
                                }
                            }
                        }
                        b.started = true;
                        b.phase = battle::Phase::TurnBanner { t: 0, side: battle::Side::Player };
                        b.ui = Story::default();
                    }
                }
                self.vm = Some(vm);
                return;
            }
            if vm.section == 1 {
                if let (Some(p), Some(b)) = (self.resume.take(), self.battle()) {
                    b.phase = p;
                }
                self.vm = Some(vm);
                return;
            }
        }
        if !vm.running && self.error.is_none() && self.wait == Wait::None {
            // 이야기(R_)는 절을 차례로 (원작 FUN_00412978: 최상위 끝 → 다음 절)
            let next = vm.section + 1;
            if self.stage % 2 == 0 && next < vm.sections() {
                if let Err(e) = self.start_section(&mut vm, next) {
                    self.error = Some(e.0);
                }
                self.vm = Some(vm);
                return;
            }
            if self.stage % 2 == 1 && self.flag(1) {
                // 패배 처리 스크립트(플래그 1) → 같은 전투 다시
                self.log.push("전투 패배 — 다시".into());
            } else {
                self.stage += 1;
                self.saved = None;
            }
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
                if cop == 3 {
                    let r = !self.last_if;
                    vm.enter_body(r)?;
                    return Ok(true);
                }
                if cop == 0x12 {
                    // 선택지: 고른 뒤 본문으로 (본문 안에서 op13 으로 결과 비교)
                    self.start_choice(&cargs);
                    self.enter_after_choice = true;
                    return Ok(false);
                }
                let r = self.cond(cop, &cargs);
                self.last_if = r;
                vm.enter_body(r)?;
            }
            0xD => {
                // 다음 절로 (원작 op0d → 7)
                let next = vm.section + 1;
                self.log.push(format!("절 {next}로 (pc {at:#x})"));
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
                match &mut self.scene {
                    Scene::Story(st) => st.walk(a(1) as usize, a(3), a(4), a(5)),
                    Scene::Battle(b) => {
                        if let Some(u) = b.unit_of(a(1) as usize) {
                            b.walk_to(u, a(3), a(4));
                            let (x, y) = (a(3), a(4));
                            if x * battle::CELL < b.cam.0 || x * battle::CELL > b.cam.0 + 592 || y * battle::CELL < b.cam.1 || y * battle::CELL > b.cam.1 + 432 {
                                b.focus(x, y);
                            }
                        }
                    }
                    _ => {}
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
                // 전투 중이면 말하는 부대에 화면을 맞춤
                if let (Some(pid), Scene::Battle(b)) = (pages.first().and_then(|p| p.person), &mut self.scene) {
                    if let Some(u) = b.unit_of(pid).filter(|&u| b.units[u].visible) {
                        let (x, y) = (b.units[u].x, b.units[u].y);
                        if x * battle::CELL < b.cam.0 + 24 || x * battle::CELL > b.cam.0 + 568 || y * battle::CELL < b.cam.1 + 24 || y * battle::CELL > b.cam.1 + 408 {
                            b.focus(x, y);
                        }
                        b.ui.speaker_y = Some(y * battle::CELL - b.cam.1 + 24);
                    } else {
                        b.ui.speaker_y = None;
                    }
                }
                let (packs, assets) = (&mut self.packs, &mut self.assets);
                let st = match &mut self.scene {
                    Scene::Story(s) => Some(&mut **s),
                    Scene::Battle(b) => Some(&mut b.ui),
                    _ => None,
                };
                if let Some(st) = st {
                    if st.show_dialog(packs, assets, pages).is_err() {
                        vm.pc = at;
                        return Ok(false);
                    }
                    self.wait = Wait::Dialog;
                }
            }
            0x12 => self.start_choice(&args),
            0x1C => self.wait = Wait::Fade(16),
            0x1D => self.wait = Wait::Fade(0),
            0x16 | 0x19 => {
                // 해설 창 (원작 FUN_0042fa65(글, 0xffff): 가운데 352×144). 19 = 전투 승패 조건
                let (t, _, _) = encoding_rs::EUC_KR.decode(args[0].bytes());
                if let Some(st) = self.ui() {
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
            0x3B => {
                // 합류 (인물, 레벨?, ?)
                let p = a(0) as usize;
                if p < 0x200 && !self.roster.contains(&p) {
                    self.roster.push(p);
                }
            }
            0x1A => {
                let (t, _, _) = encoding_rs::EUC_KR.decode(args[0].bytes());
                if let Some(b) = self.battle() {
                    b.objective = t.to_string();
                    b.message = Some((t.to_string(), 90));
                }
            }
            0x5C => {
                if let Some(b) = self.battle() {
                    b.targets.push(a(0) as usize);
                }
            }
            0x44..=0x47 | 0x4B..=0x4F | 0x31 | 0x50 | 0x53 if self.battle().is_some() => self.battle_op(op, &args),
            _ => self.note(op),
        }
        Ok(true)
    }


    /// 전투 맵·팔레트·부대 그림 적재. 받는 중이면 false
    fn load_battle_map(&mut self) -> bool {
        let Scene::Battle(b) = &mut self.scene else { return true };
        if b.map.is_none() {
            let n = b.map_no;
            let img = match self.assets.get(&format!("HM{n:02}.E5")) {
                Asset::Pending => return false,
                Asset::Ready(d) => d,
                Asset::Missing => {
                    self.error = Some(format!("HM{n:02}.E5 없음"));
                    return false;
                }
            };
            let (hex, pal) = match (self.packs.get(&mut self.assets, "HEXZMAP.E5", n), self.packs.get(&mut self.assets, "SPALET.E5", n)) {
                (Ok(h), Ok(p)) => (h.unwrap_or_else(|| vec![60, 60]), p.unwrap_or_default()),
                _ => return false,
            };
            b.set_map(&img, &hex);
            self.screen.pal = palette::from_bytes(&pal);
            let p = &self.screen.pal;
            b.col = [
                palette::nearest(p, [40, 90, 255]),
                palette::nearest(p, [255, 40, 40]),
                palette::nearest(p, [80, 160, 255]),
                palette::nearest(p, [255, 60, 40]),
                palette::nearest(p, [255, 255, 255]),
                palette::nearest(p, [255, 230, 60]),
                palette::nearest(p, [0, 0, 0]),
            ];
        }
        // 부대 그림
        for i in 0..b.units.len() {
            if b.units[i].sprite.is_none() {
                match self.packs.get(&mut self.assets, "UNIT_MOV.E5", b.units[i].sprite_idx) {
                    Err(Pending) => return false,
                    Ok(d) => b.units[i].sprite = Some(Rc::new(Pic::new(48, 48 * 11, d.unwrap_or_default()))),
                }
            }
        }
        true
    }

    /// 전투 부대 명령
    fn battle_op(&mut self, op: u16, args: &[Arg]) {
        let a = |i: usize| args.get(i).map_or(0, |a| a.i());
        let persons = self.persons.clone();
        let roster = self.roster.clone();
        let Some(b) = self.battle() else { return };
        let level_of = |adj: i32| if adj == 0xFFFF { 1 } else if adj <= 50 { adj.max(1) } else { (adj - 50).max(1) };
        match op {
            0x44 => b.units.clear(),
            0x45 => b.turn_limit = a(2),
            0x46 | 0x47 => {
                // 부대 목록: 46 아군 (표지 0x26 이 1이면 처음에 숨김), 47 적군
                let n = if op == 0x46 { 11 } else { 12 };
                for r in args.chunks(n) {
                    let pid = r[0].i();
                    if pid == 0xFFFF || pid as usize >= persons.len() {
                        continue;
                    }
                    let (side, hidden, x, y, lv, ai, tx, ty) = if op == 0x46 {
                        (battle::Side::Ally, r[1].i() == 1, r[2].i(), r[3].i(), r[5].i(), r[7].i(), r[9].i(), r[10].i())
                    } else {
                        (battle::Side::Enemy, r[1].i() == 1, r[3].i(), r[4].i(), r[6].i(), r[8].i(), r[10].i(), r[11].i())
                    };
                    let mut u = b.make_unit(pid as usize, &persons[pid as usize], side, x, y, level_of(lv));
                    u.visible = !hidden;
                    u.ai = if ai == 0xFFFF { 0 } else { ai as u8 };
                    if matches!(u.ai, 4 | 6) {
                        u.ai_target = Some((tx, ty));
                    }
                    b.units.retain(|o| o.person != pid as usize);
                    b.units.push(u);
                }
            }
            0x4B => {
                // 출진 칸 (번호, x, y, ...)
                let i = a(0) as usize;
                if i < b.slots.len() {
                    b.slots[i] = (a(1), a(2));
                }
            }
            0x4C => {
                // 등장: 0 = 인물 하나, 1 = 조조군 장수들을 출진 칸에
                if a(0) == 1 {
                    let list: Vec<usize> = if roster.is_empty() { vec![0] } else { roster.clone() };
                    for (k, &pid) in list.iter().enumerate().take(b.slots.len()) {
                        let (x, y) = b.slots[k];
                        if k > 0 && (x, y) == (0, 0) {
                            break;
                        }
                        match b.unit_of(pid) {
                            Some(u) => {
                                let un = &mut b.units[u];
                                un.visible = true;
                                un.x = x;
                                un.y = y;
                                un.px = (x * battle::CELL) as f32;
                                un.py = (y * battle::CELL) as f32;
                            }
                            None => {
                                let u = b.make_unit(pid, &persons[pid], battle::Side::Player, x, y, 1);
                                b.units.push(u);
                            }
                        }
                    }
                } else if let Some(u) = b.unit_of(a(1) as usize) {
                    b.units[u].visible = true;
                    b.units[u].fade = 16;
                }
            }
            0x4D => {
                // 체력 지정 (…, 백분율)
                if let Some(u) = b.unit_of(a(1) as usize) {
                    let pct = a(11).clamp(1, 100);
                    let un = &mut b.units[u];
                    un.hp = un.max_hp * pct / 100;
                }
            }
            0x4E => {
                // 새 부대 등장 (종류, 인물, x, y, …)
                let pid = a(1) as usize;
                if pid < persons.len() {
                    match b.unit_of(pid) {
                        Some(u) => {
                            let un = &mut b.units[u];
                            un.visible = true;
                            if (a(2), a(3)) != (0, 0) {
                                un.x = a(2);
                                un.y = a(3);
                                un.px = (un.x * battle::CELL) as f32;
                                un.py = (un.y * battle::CELL) as f32;
                            }
                        }
                        None => {
                            let u = b.make_unit(pid, &persons[pid], battle::Side::Enemy, a(2), a(3), 1);
                            b.units.push(u);
                        }
                    }
                }
            }
            0x31 | 0x53 => {
                // 퇴장/퇴각
                if let Some(u) = b.unit_of(a(1) as usize) {
                    if op == 0x53 {
                        b.units[u].hp = 0;
                    }
                    b.units[u].visible = false;
                }
            }
            0x4F => {
                // 방향 바꾸기 (인물, 대상 인물, …)
                if let (Some(u), Some(t)) = (b.unit_of(a(0) as usize), b.unit_of(a(1) as usize)) {
                    let (dx, dy) = (b.units[t].x - b.units[u].x, b.units[t].y - b.units[u].y);
                    b.units[u].dir = if dx.abs() > dy.abs() { if dx < 0 { 2 } else { 3 } } else if dy < 0 { 1 } else { 0 };
                }
            }
            0x50 => {
                // 동작 (인물, 동작 번호): 0 보통, 6/7 공격, 3/4/9 맞음·지침
                if let Some(u) = b.unit_of(a(0) as usize) {
                    b.units[u].pose = match a(1) {
                        0 => None,
                        6 | 7 => Some(6),
                        _ => Some(9),
                    };
                }
            }
            _ => {}
        }
    }

    /// 전투 사건 조건 (원작 op3f/25/2e/36/40/42/43)
    fn battle_cond(&mut self, op: u16, args: &[Arg]) -> bool {
        let a = |i: usize| args.get(i).map_or(0, |a| a.i());
        let Some(b) = self.battle() else { return false };
        let phase_no = match &b.phase {
            battle::Phase::Ai { side: battle::Side::Ally, .. } | battle::Phase::TurnBanner { side: battle::Side::Ally, .. } => 2,
            battle::Phase::Ai { side: battle::Side::Enemy, .. } | battle::Phase::TurnBanner { side: battle::Side::Enemy, .. } => 3,
            _ => 1,
        };
        match op {
            // 턴 비교 (0: 이상, 1: 미만, 2: 같음)
            0x3F => match a(1) {
                0 => a(0) <= b.turn,
                1 => b.turn < a(0),
                _ => b.turn == a(0),
            },
            0x40 => phase_no == a(0),
            // 부대가 칸에 (0x401 = 아군 누구든, 0x402 = 적 누구든)
            0x25 => {
                let (w, x, y) = (a(0), a(1), a(2));
                b.units.iter().any(|u| {
                    u.visible && u.hp > 0 && u.x == x && u.y == y && match w {
                        0x400 => true,
                        0x401 => u.side != battle::Side::Enemy,
                        0x402 => u.side == battle::Side::Enemy,
                        p => u.person == p as usize,
                    }
                })
            }
            // 사각형 안 (부대, x1, y1, x2, y2)
            0x26 => {
                let (w, x1, y1, x2, y2) = (a(0), a(1), a(2), a(3), a(4));
                b.units.iter().any(|u| {
                    u.visible && u.hp > 0 && (x1..=x2).contains(&u.x) && (y1..=y2).contains(&u.y) && match w {
                        0x400 => true,
                        0x401 => u.side != battle::Side::Enemy,
                        0x402 => u.side == battle::Side::Enemy,
                        p => u.person == p as usize,
                    }
                })
            }
            // 이웃 (인물 a, 인물 b)
            0x2E => {
                let m = |u: &battle::Unit, w: i32| match w {
                    0x400 => true,
                    0x401 => u.side != battle::Side::Enemy,
                    0x402 => u.side == battle::Side::Enemy,
                    p => u.person == p as usize,
                };
                let live: Vec<&battle::Unit> = b.units.iter().filter(|u| u.visible && u.hp > 0).collect();
                live.iter().any(|u| m(u, a(0)) && live.iter().any(|v| m(v, a(1)) && (u.x - v.x).abs() + (u.y - v.y).abs() == 1))
            }
            // 능력 비교: 7 = HP (0 이면 퇴각)
            0x36 => {
                let Some(u) = b.unit_of(a(0) as usize) else { return false };
                let v = b.units[u].hp.max(0);
                match a(3) {
                    0 => v >= a(2),
                    1 => v < a(2),
                    _ => v == a(2),
                }
            }
            // 승리 조건: 목표(op5c)가 있으면 목표 전원 퇴각, 없으면 적 전멸 (원작 FUN_0044e6af)
            0x42 => {
                if b.cheat && b.turn > 3 {
                    return true;
                }
                if b.targets.is_empty() {
                    !b.units.iter().any(|u| u.side == battle::Side::Enemy && u.hp > 0)
                } else {
                    b.targets.iter().all(|&p| b.unit_of(p).is_none_or(|u| b.units[u].hp <= 0))
                }
            }
            0x43 => {
                let cao_dead = b.unit_of(0).is_some_and(|u| b.units[u].hp <= 0);
                cao_dead || b.turn > b.turn_limit
            }
            // 진영별 부대 수 비교 (종류 0 아군/1 우군/2 적/4 아군+우군/5 적/6 전부, 수, 비교, 영역?, x1, y1, x2, y2) — 원작 FUN_0044e93d
            0x41 => {
                let (kind, n, cmp, area) = (a(0), a(1), a(2), a(3) == 1);
                let (x1, y1, x2, y2) = (a(4), a(5), a(6), a(7));
                let cnt = b
                    .units
                    .iter()
                    .filter(|u| u.visible && u.hp > 0)
                    .filter(|u| !area || ((x1..=x2).contains(&u.x) && (y1..=y2).contains(&u.y)))
                    .filter(|u| {
                        let c = match u.side {
                            battle::Side::Player => 0,
                            battle::Side::Ally => 1,
                            battle::Side::Enemy => 2,
                        };
                        match kind {
                            0..=3 => c == kind,
                            4 => c <= 1,
                            5 => c >= 2,
                            _ => true,
                        }
                    })
                    .count() as i32;
                match cmp {
                    0 => cnt >= n,
                    1 => cnt < n,
                    _ => cnt == n,
                }
            }
            0x37 | 0x38 | 0x48 => false,
            _ => {
                self.note(op);
                false
            }
        }
    }

    /// 절 1 사건 검사: 머리 조건이 맞는 첫 블록을 실행. 실행했으면 true
    fn check_events(&mut self) -> bool {
        let Some(mut vm) = self.vm.take() else { return false };
        let mut fired = false;
        if vm.sections() > 1 && !vm.running {
            if let Ok(blocks) = vm.blocks(1) {
                for (start, _) in blocks {
                    vm.pc = start;
                    let mut ok = true;
                    loop {
                        match vm.fetch() {
                            Ok((0, _)) => break,
                            Ok((op, args)) => ok &= self.cond(op, &args),
                            Err(_) => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok {
                        vm.section = 1;
                        vm.ends.clear();
                        if vm.enter_body(true).is_ok() {
                            vm.running = true;
                            fired = true;
                        }
                        break;
                    }
                }
            }
        }
        self.vm = Some(vm);
        if fired {
            if let Some(b) = self.battle() {
                let p = std::mem::replace(&mut b.phase, battle::Phase::Script);
                if self.resume.is_none() {
                    self.resume = Some(p);
                }
            }
        }
        fired
    }

    /// 전투 진행 (스크립트가 쉬는 동안)
    fn update_battle(&mut self) {
        let running = self.vm.as_ref().is_some_and(|v| v.running);
        if running || self.wait != Wait::None {
            return;
        }
        let inp = self.input.clone();
        let Some(b) = self.battle() else { return };
        if !b.started {
            return;
        }
        let mut check = false;
        match b.phase.clone() {
            battle::Phase::Script => {}
            battle::Phase::TurnBanner { t, side } => {
                if t == 0 {
                    // 차례 시작: 그 진영 부대가 없으면 건너뜀
                    let has = b.alive(side).next().is_some();
                    if !has && side != battle::Side::Player {
                        b.phase = match side {
                            battle::Side::Ally => battle::Phase::TurnBanner { t: 0, side: battle::Side::Enemy },
                            _ => {
                                b.turn += 1;
                                battle::Phase::TurnBanner { t: 0, side: battle::Side::Player }
                            }
                        };
                        return;
                    }
                }
                if t == 1 {
                    check = true;
                }
                if t >= 36 {
                    if side == battle::Side::Player {
                        for u in &mut b.units {
                            u.acted = false;
                        }
                        b.phase = battle::Phase::PlayerIdle;
                        let t = b.turn;
                        if self.autoplay {
                            if let Some(b) = self.battle() {
                                b.phase = battle::Phase::Ai { side: battle::Side::Player, idx: 0, stage: 0 };
                            }
                        }
                        self.log.push(format!("아군 차례 {t}턴"));
                        let Some(b) = self.battle() else { return };
                        if let Some(u) = b.unit_of(0).filter(|&u| b.units[u].visible) {
                            let (x, y) = (b.units[u].x, b.units[u].y);
                            b.cursor = (x, y);
                            b.focus(x, y);
                        }
                    } else {
                        b.phase = battle::Phase::Ai { side, idx: 0, stage: 0 };
                    }
                } else if let battle::Phase::TurnBanner { t: tt, .. } = &mut b.phase {
                    *tt += 1;
                }
            }
            battle::Phase::Ai { side, .. } => {
                let (c, done) = b.update_ai();
                check = c;
                if done {
                    b.phase = match side {
                        battle::Side::Player => battle::Phase::TurnBanner { t: 0, side: battle::Side::Ally },
                        battle::Side::Ally => battle::Phase::TurnBanner { t: 0, side: battle::Side::Enemy },
                        _ => {
                            b.turn += 1;
                            battle::Phase::TurnBanner { t: 0, side: battle::Side::Player }
                        }
                    };
                }
            }
            battle::Phase::Over { won, t } => {
                if t > 60 {
                    self.log.push(format!("전투 {}", if won { "승리" } else { "패배 — 다시" }));
                    if won {
                        self.stage += 1;
                        self.saved = None;
                    }
                    self.vm = None;
                    self.resume = None;
                    return;
                }
                b.phase = battle::Phase::Over { won, t: t + 1 };
            }
            _ => check = b.update_player(&inp),
        }
        if check {
            // 사건이 연달아 생길 수 있으니 하나씩
            self.check_events();
        }
        // 스크립트가 끝을 내지 않았을 때의 기본 승패 (사건이 먼저)
        let ended = self.battle().is_some_and(|b| {
            b.started && !matches!(b.phase, battle::Phase::Script | battle::Phase::Over { .. }) && (!b.units.iter().any(|u| u.side == battle::Side::Enemy && u.hp > 0) || b.unit_of(0).is_some_and(|u| b.units[u].hp <= 0) || b.turn > b.turn_limit)
        });
        if ended && self.check_events() {
            return;
        }
        if let Some(b) = self.battle() {
            if b.started && !matches!(b.phase, battle::Phase::Script | battle::Phase::Over { .. }) {
                let enemies = b.units.iter().filter(|u| u.side == battle::Side::Enemy && u.hp > 0).count();
                let cao_dead = b.unit_of(0).is_some_and(|u| b.units[u].hp <= 0);
                if enemies == 0 && b.turn > 0 {
                    self.log.push("적 전멸".into());
                    let Some(b) = self.battle() else { return };
                    b.phase = battle::Phase::Over { won: true, t: 0 };
                } else if cao_dead || b.turn > b.turn_limit {
                    b.phase = battle::Phase::Over { won: false, t: 0 };
                }
            }
        }
    }

    fn start_choice(&mut self, args: &[Arg]) {
        let (s, _, _) = encoding_rs::EUC_KR.decode(args[0].bytes());
        if let Some(st) = self.ui() {
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
            Scene::Story(st) => st.render(scr, self.font.as_ref(), self.tick),
            Scene::Battle(b) => b.render(scr, self.font.as_ref(), self.tick),
        }
        if let Some(e) = &self.error {
            if let Some(f) = &self.font {
                scr.fill(0, 0, 640, 20, 0);
                story::draw_text(scr, f, 4, 2, &format!("오류: {e}"), 251);
            }
        }
        scr.rgba()
    }

    /// 저장: "판1;단계;켜진 플래그들;합류 장수들"
    pub fn snapshot(&self) -> String {
        let flags: Vec<String> = self.flags.iter().enumerate().filter(|(_, &f)| f).map(|(i, _)| i.to_string()).collect();
        let roster: Vec<String> = self.roster.iter().map(|r| r.to_string()).collect();
        format!("1;{};{};{}", self.stage, flags.join(","), roster.join(","))
    }

    /// 불러오기 (제목 화면에서 시작하면 그 단계부터)
    pub fn restore(&mut self, s: &str) -> bool {
        let f: Vec<&str> = s.split(';').collect();
        if f.len() < 4 || f[0] != "1" {
            return false;
        }
        let Ok(stage) = f[1].parse() else { return false };
        self.stage = stage;
        self.flags.fill(false);
        for x in f[2].split(',').filter_map(|v| v.parse::<usize>().ok()) {
            if x < self.flags.len() {
                self.flags[x] = true;
            }
        }
        self.roster = f[3].split(',').filter_map(|v| v.parse().ok()).collect();
        true
    }

    pub fn take_sounds(&mut self) -> Vec<Sound> {
        std::mem::take(&mut self.sounds)
    }
}
