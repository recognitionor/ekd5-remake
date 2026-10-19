//! 게임 엔진: 스크립트 + 화면 + 입력. 매 틱(30Hz) `set_input` → `tick` → `render`.
//! 진행: 단계 번호 n (원작 0x4baee8) — 짝수 = R_(n/20)((n%20)/2).EEX 이야기, 홀수 = S_ 전투 (원작 FUN_0041a35e).

pub mod assets;
pub mod battle;
pub mod camp;
pub mod duel;
pub mod equip;
pub mod fx;
pub mod icons;
pub mod quick;
pub mod tutor;
pub mod screen;
pub mod sortie;
pub mod story;
pub mod ui;

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
    /// 정보 보기 (원작 오른쪽 단추 놓기 0x205 — 웹: 오른쪽 클릭·길게 누르기), 이번 틱만
    pub alt: bool,
    /// 마우스처럼 올려 두기만 해도 되는 포인터 (터치는 아님)
    pub hover: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Sound {
    Effect(String),
    /// 효과음 반복 (원작 op23 방식 0: SND_LOOP) / 정지
    EffectLoop(String),
    EffectStop,
    /// 동영상 (원작 op22: 0 LOGO, 1 OPEN, 2 END)
    Movie(String),
    /// 엔딩 본 기록 (원작 op0f → SVCMN.E5S)
    Ending(i32),
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
    /// 엔딩 뒤
    End { t: u32 },
    /// 게임 오버 (원작 FUN_0042e65c: LOGO.E5[7] + SE40) 뒤 고르기 (0 다시, 1 제목)
    Over { pic: Option<Pic>, t: u32, sel: usize },
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
    /// 동영상 (웹이 다 보여주면 movie_done)
    Movie,
    /// 맵 연출 (op6b) 끝날 때까지
    Fx,
    /// 일기토 단계가 끝날 때까지
    Duel,
    /// 장 제목 카드 (단계 0 장 번호 / 1 제목, 틱)
    Chapter(u8, u32),
    /// 기다림 (누르면 넘김), 끝나면 op69 자막 지움
    TicksSkip(u32),
    /// 일기토 뒤 경험치 창·레벨업
    DuelExp,
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
    /// 조조군 장수 능력 (전투 사이 유지)
    pub officers: HashMap<usize, crate::officer::Officer>,
    /// DATA.E5[0] 원자료, 아이템 표
    pub person_data: Rc<Vec<u8>>,
    /// 불러온 장수 레벨 (자료 적재 뒤 적용)
    pending_offs: Vec<(usize, u8, u8, Option<(u8, u8)>, Option<(u8, u8)>, Option<u8>, [u8; 2])>,
    pub items: Rc<Vec<u8>>,
    pub strategies: Rc<Vec<crate::strategy::Strategy>>,
    /// 돈, 소지 아이템 (아이템, 레벨), 명성(op3a 2)
    pub gold: i32,
    pub inventory: Vec<(u8, u8)>,
    pub fame: i32,
    /// op11 로 단계 이동 대기
    jump_stage: Option<usize>,
    vm_drop: bool,
    /// 얼굴 상태 (원작 DAT_004baee8+0x11, op20): 아래 4비트 조조 차림, 0x10 35번, 0x20 8번, 0x40 4번, 0x80 하후돈 두 눈
    pub face_state: u8,
    /// 장 번호 −1 (원작 +6, op67): 2 이상이면 조조 나이 든 얼굴
    pub chapter: u8,
    /// 인물 그림 바꾸기 (원작 op35, 장수 기록 +2)
    pub sprite_over: std::collections::BTreeMap<usize, u16>,
    /// 회차 공통 기록 (원작 SVCMN.E5S): 본 엔딩 비트, 보물도감
    pub endings: u8,
    pub book: Vec<bool>,
    /// 웹이 저장할 공통 기록
    pub common_out: Option<String>,
    pub tutor: Option<tutor::Tutor>,
    /// 아이콘 (ICONS.BIN)
    pub icons: Rc<icons::Icons>,
    /// 전체 지도 켜기 (웹 버튼)
    pub minimap_on: bool,
    /// 직업 이름 (JOBS.TXT, 원작 표 0x496200)
    pub job_names: Rc<Vec<String>>,
    /// 지형 이름 (TERRAIN.TXT)
    pub terrain_names: Rc<Vec<String>>,
    /// 불러 둔 MCALL 번호 (op6b)
    mcall_loaded: Option<u8>,
    /// 불러올 중간 저장
    quick: Option<quick::QuickSave>,
    /// 지금 배경음악 (중간 저장용)
    music_now: Option<i32>,
    /// 불러온 뒤 열 본진 절
    pending_camp: Option<usize>,
    /// 마지막 이야기 장면 그림 (op27 방식, 값) — 중간 저장용
    pub last_map: Option<(i32, i32)>,
    /// 불러온 뒤 다시 그릴 장면 그림
    pending_map: Option<(i32, i32)>,
    pending_actors: Vec<(usize, u16, i32, i32, i32, i32)>,
    icons_loaded: bool,
    /// 동영상을 보여줄 수 있는 환경 (웹)
    pub movies: bool,
    logo_shown: bool,
    /// 본진
    pub camp: Option<camp::Camp>,
    pub camp_rule: Option<camp::SortieRule>,
    camp_pending: bool,
    /// 웹 메뉴 단추: 본진 보조 메뉴 열기
    pub camp_menu: bool,
    /// 난수 씨앗 (웹이 시각으로 줌, 0 = 시험용 고정). 원작은 전투마다 GetTickCount 로 씨앗 (FUN_00482060)
    pub entropy: u32,
    /// 장 제목 카드의 둘째 장 (제목)
    card_title: Option<duel::Mask>,
    /// 일기토 끝 (op5f) 처리 중
    duel_ending: bool,
    /// 시험: 이 사건 블록을 조건과 상관없이 다음 검사 때 실행
    pub force_block: Option<usize>,
    /// MARK.E5 (막대 무늬 등, 전투 밖에서도)
    pub mark_data: Rc<Vec<u8>>,
    /// 출진 고르기
    pub sortie_ui: Option<sortie::SortieUi>,
    /// 장비 화면 (원작 대화상자 328)
    pub equip_ui: Option<equip::EquipUi>,
    /// 아이템·책략·열전 설명 (IMSG.E5, 216바이트 기록)
    pub imsg: Option<Rc<Vec<u8>>>,
    /// 출진 화면 정렬 (원작 전역 DAT_00498c30 / DAT_00498c3c)
    sortie_sort: (u8, u8),
    sortie_rule: Option<camp::SortieRule>,
    /// 원작 DAT_004bb164 (op06 이 정한 출진 최대, op4a 는 이것을 씀)
    sortie_max: usize,
    /// 마지막으로 연 본진의 출진 규칙 (본진 출진 단추를 안 거친 경우용)
    last_camp_rule: Option<camp::SortieRule>,
    sortie_chosen: Option<Vec<usize>>,
    /// 사건 처리 뒤 돌아갈 전투 단계
    resume: Option<battle::Phase>,
    /// 실행할 사건 본문들 (뒤에서부터)
    event_queue: Vec<usize>,
    /// 승패 직전 사건 검사를 했는지
    end_checked: bool,
    /// 시험용: 아군도 컴퓨터가 둠
    pub autoplay: bool,
    /// 시험용: 아군 강화
    pub cheat: bool,
    /// 전투 시작 시점 저장 (패배 후 다시 하기용)
    saved: Option<(Vec<bool>, Vec<usize>, HashMap<usize, crate::officer::Officer>, Vec<(u8, u8)>, i32)>,
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
            officers: HashMap::new(),
            person_data: Rc::new(Vec::new()),
            pending_offs: Vec::new(),
            items: Rc::new(Vec::new()),
            strategies: Rc::new(Vec::new()),
            gold: 0,
            inventory: Vec::new(),
            fame: 0,
            jump_stage: None,
            vm_drop: false,
            face_state: 0,
            chapter: 0,
            sprite_over: Default::default(),
            endings: 0,
            book: vec![false; 0x68],
            common_out: None,
            tutor: None,
            icons: Rc::new(icons::Icons::default()),
            minimap_on: false,
            job_names: Rc::new(Vec::new()),
            terrain_names: Rc::new(Vec::new()),
            mcall_loaded: None,
            quick: None,
            music_now: None,
            pending_camp: None,
            last_map: None,
            pending_map: None,
            pending_actors: Vec::new(),
            icons_loaded: false,
            movies: false,
            logo_shown: false,
            camp: None,
            camp_rule: None,
            camp_pending: false,
            camp_menu: false,
            entropy: 0,
            card_title: None,
            duel_ending: false,
            force_block: None,
            mark_data: Rc::new(Vec::new()),
            sortie_ui: None,
            equip_ui: None,
            imsg: None,
            sortie_sort: (1, 2),
            sortie_rule: None,
            sortie_max: 0,
            last_camp_rule: None,
            sortie_chosen: None,
            resume: None,
            event_queue: Vec::new(),
            end_checked: false,
            autoplay: false,
            cheat: false,
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
        self.input.alt = false;
    }

    /// 정보 보기 신호와 올려 두기 가능 여부 (set_input 다음에)
    pub fn set_pointer_kind(&mut self, alt: bool, hover: bool) {
        self.input.alt = alt;
        self.input.hover = hover;
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
                    self.person_data = Rc::new(d.clone());
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
        if !self.icons_loaded {
            match (self.assets.get("ICONS.BIN"), self.assets.get("JOBS.TXT"), self.assets.get("TERRAIN.TXT")) {
                (Asset::Pending, _, _) | (_, Asset::Pending, _) | (_, _, Asset::Pending) => return false,
                (i, j, tn) => {
                    if let Asset::Ready(d) = tn {
                        self.terrain_names = Rc::new(String::from_utf8_lossy(&d).lines().map(|l| l.to_string()).collect());
                    }
                    if let Asset::Ready(d) = i {
                        self.icons = Rc::new(icons::Icons::parse(&d));
                    }
                    if let Asset::Ready(d) = j {
                        self.job_names = Rc::new(String::from_utf8_lossy(&d).lines().map(|l| l.to_string()).collect());
                    }
                }
            }
            self.icons_loaded = true;
        }
        if self.screen.window_tex.is_none() {
            match self.assets.get("MARK.E5") {
                Asset::Pending => return false,
                Asset::Ready(d) => {
                    self.screen.window_tex = d.get(0x18DC0..0x18DC0 + 32 * 48).map(|t| Pic::new(32, 48, t.to_vec()));
                    self.mark_data = d.clone();
                }
                Asset::Missing => self.screen.window_tex = Some(Pic::new(32, 48, vec![30; 32 * 48])),
            }
        }
        if self.jobs.is_empty() {
            match (self.packs.get(&mut self.assets, "DATA.E5", 3), self.packs.get(&mut self.assets, "DATA.E5", 4), self.packs.get(&mut self.assets, "DATA.E5", 1), self.packs.get(&mut self.assets, "DATA.E5", 5)) {
                (Ok(j), Ok(t), Ok(i), Ok(st)) => {
                    self.strategies = Rc::new(crate::strategy::parse(&st.unwrap_or_default()));
                    self.jobs = Rc::new(j.unwrap_or_default());
                    self.terrain_tab = Rc::new(t.unwrap_or_default());
                    self.items = Rc::new(i.unwrap_or_default());
                }
                _ => return false,
            }
        }
        let has_eq = self.pending_offs.iter().any(|o| o.3.is_some() || o.4.is_some() || o.5.is_some());
        for (p, lv, ex, w, ar, ac, ge) in std::mem::take(&mut self.pending_offs) {
            if let Some(r) = self.person_data.get(p * 32..p * 32 + 32) {
                let mut o = crate::officer::Officer::from_data(p, r);
                o.set_level(&self.jobs, lv);
                o.exp = ex;
                if has_eq {
                    o.weapon = w;
                    o.armor = ar;
                    o.acc = ac;
                    o.gear_exp = ge;
                    o.fix_legacy_gear();
                }
                self.officers.insert(p, o);
            }
        }
        true
    }

    /// 전투에서 자란 조조군 장수 능력을 반영
    fn commit_battle(&mut self) {
        let roster = self.roster.clone();
        if let Scene::Battle(b) = &self.scene {
            // 전투 중 쓴 도구를 빼고, 보상으로 늘어난 것은 엔진 쪽에 이미 들어 있음
            let used: Vec<(u8, u8)> = {
                let mut base = self.inventory.clone();
                for x in &b.inventory {
                    if let Some(i) = base.iter().position(|y| y == x) {
                        base.remove(i);
                    }
                }
                base
            };
            let start_inv = self.saved.as_ref().map(|s| s.3.clone()).unwrap_or_default();
            let mut inv = self.inventory.clone();
            for u in used {
                if start_inv.contains(&u) {
                    if let Some(i) = inv.iter().position(|y| *y == u) {
                        inv.remove(i);
                    }
                }
            }
            self.inventory = inv;
            for u in &b.units {
                if u.side == battle::Side::Player && roster.contains(&u.person) {
                    self.officers.insert(u.person, u.off.clone());
                }
            }
        }
    }

    /// 인물 능력: 조조군 장수는 저장된 것, 아니면 원자료
    pub fn officer(&mut self, pid: usize) -> crate::officer::Officer {
        if let Some(o) = self.officers.get(&pid) {
            return o.clone();
        }
        let r = self.person_data.get(pid * 32..pid * 32 + 32).map(|r| r.to_vec()).unwrap_or_else(|| vec![0; 32]);
        let o = crate::officer::Officer::from_data(pid, &r);
        if self.roster.contains(&pid) {
            self.officers.insert(pid, o.clone());
        }
        o
    }

    /// 조조군 평균 레벨 (원작 FUN_0041adb4: 출진 장수 목록 DAT_00495498 에서 위아래 1/4 빼고 평균).
    /// 아직 출진을 안 골랐으면 동료 전체
    fn army_level(&mut self) -> u8 {
        let roster = match &self.sortie_chosen {
            Some(c) if !c.is_empty() => c.clone(),
            _ if self.roster.is_empty() => vec![0],
            _ => self.roster.clone(),
        };
        let mut lv: Vec<u8> = roster.iter().map(|&p| self.officer(p).level).collect();
        lv.sort_unstable_by(|a, b| b.cmp(a));
        let q = lv.len() / 4;
        let mid = &lv[q..lv.len() - q];
        (mid.iter().map(|&v| v as u32).sum::<u32>() / mid.len().max(1) as u32).max(1) as u8
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

    /// 새 난수 씨앗 (웹에서만): 시각 씨앗 + 지금까지 틱
    pub fn fresh_seed(&self) -> Option<u32> {
        (self.entropy != 0).then(|| self.entropy.wrapping_mul(2654435761).wrapping_add(self.tick.wrapping_mul(40503)) | 1)
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
                if !self.logo_shown {
                    // 원작: 켜면 LOGO.AVI
                    self.logo_shown = true;
                    if self.movies {
                        self.sounds.push(Sound::Movie("LOGO.mp4".into()));
                        self.wait = Wait::Movie;
                    }
                }
                if self.wait == Wait::Movie {
                    return;
                }
                *t += 1;
                if *t > 10 && self.input.pressed & key::DECIDE != 0 {
                    self.scene = Scene::Story(Box::default());
                    self.vm = None;
                    self.screen.level = 16;
                }
            }
            Scene::End { t } => {
                if self.wait == Wait::Movie {
                    return;
                }
                *t += 1;
                // 원작: 엔딩 뒤 제목으로 (FUN_0042ded0)
                if *t > 90 && self.input.pressed & key::DECIDE != 0 {
                    self.scene = Scene::Boot;
                    self.stage = 0;
                    self.flags.fill(false);
                    self.roster.clear();
                    self.officers.clear();
                    self.inventory.clear();
                    self.gold = 0;
                    self.fame = 0;
                }
            }
            Scene::Over { pic, t, sel } => {
                if pic.is_none() {
                    let p = self.packs.get(&mut self.assets, "LOGO.E5", 7);
                    let pal = self.packs.get(&mut self.assets, "LOGO_P.E5", 2);
                    if let (Ok(Some(p)), Ok(Some(pal))) = (p, pal) {
                        self.screen.pal = palette::from_bytes(&pal);
                        *pic = Some(Pic::new(640, 400, p));
                    }
                    return;
                }
                *t += 1;
                let inp = self.input.clone();
                if inp.pressed & (key::UP | key::DOWN | key::LEFT | key::RIGHT) != 0 {
                    *sel ^= 1;
                }
                let mut pick = None;
                if *t > 40 {
                    if let (Some((mx, my)), true) = (inp.mouse, inp.click) {
                        if my >= 440 {
                            pick = Some(if mx < 320 { 0 } else { 1 });
                        }
                    } else if inp.pressed & key::DECIDE != 0 {
                        pick = Some(*sel);
                    }
                }
                match pick {
                    // 이 전투 다시 (단계 시작 값으로)
                    Some(0) => {
                        self.scene = Scene::Story(Box::default());
                        self.vm = None;
                        self.screen.level = 16;
                    }
                    // 제목으로 (원작은 제목 → 불러오기)
                    Some(_) => {
                        self.scene = Scene::Boot;
                        self.logo_shown = true;
                        self.vm = None;
                        if let Some((f, r, o, i, g)) = self.saved.clone() {
                            self.flags = f;
                            self.roster = r;
                            self.officers = o;
                            self.inventory = i;
                            self.gold = g;
                        }
                    }
                    None => {}
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
                if let Scene::Battle(b) = &mut self.scene {
                    if let Some(t) = b.intro {
                        b.intro = if t >= 30 { None } else { Some(t + 1) };
                    }
                }
                if self.autoplay {
                    // 시험: 출진은 기본 선택, 본진은 바로 출진
                    if let Some(ui) = self.sortie_ui.take() {
                        let mut ch = ui.chosen.clone();
                        for &p in &ui.pids {
                            if ch.len() < ui.rule.max && !ch.contains(&p) {
                                ch.push(p);
                            }
                        }
                        self.sortie_chosen = Some(ch);
                    }
                    if let Some(c) = self.camp.take() {
                        if self.sortie_chosen.is_none() {
                            self.sortie_chosen = Some(self.auto_sortie(&c.rule));
                        }
                        if let Some(body) = c.depart {
                            if let Some(vm) = self.vm.as_mut() {
                                vm.section = c.section;
                                vm.ends.clear();
                                vm.pc = body;
                                if vm.enter_body(true).is_ok() {
                                    vm.running = true;
                                }
                            }
                        }
                    }
                }
                if self.sortie_ui.is_some() {
                    let depart = self.sortie_ui.as_ref().and_then(|u| u.depart);
                    match self.update_sortie() {
                        Some(Some(ch)) => {
                            self.sortie_chosen = Some(ch);
                            // 본진 출진: 출진 블록 실행
                            if let Some((body, section)) = depart {
                                self.camp = None;
                                self.run_camp_block(body, section);
                            }
                        }
                        Some(None) => {}
                        None => {}
                    }
                    return;
                }
                // 중간 저장에서: 장면 그림·인물 다시 놓기
                if self.restore_story() {
                    return;
                }
                if self.equip_ui.is_some() {
                    self.update_equip();
                    return;
                }
                if self.camp.is_some() && self.update_camp() {
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
            Asset::Missing if n > 100 => {
                // 마지막 이야기 뒤 = 끝
                self.scene = Scene::End { t: 0 };
                self.saved = None;
                self.save_out = Some(String::new());
                false
            }
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
                    // 본진(이야기 단계)에서 고른 출진 장수는 다음 전투 단계까지 둠
                    if n % 2 == 0 {
                        self.sortie_chosen = None;
                    }
                    self.sortie_rule = None;
                    self.event_queue.clear();
                    self.resume = None;
                    self.end_checked = false;
                    self.log.push(format!("단계 {n}: {name}"));
                    self.note_treasures();
                    if self.quick.is_none() {
                        self.save_out = Some(self.snapshot());
                    }
                    let mut vm = Vm::new(Rc::new(s));
                    if n % 2 == 1 && self.quick.is_none() {
                        match &self.saved {
                            Some((f, r, o, inv, g)) => {
                                self.flags = f.clone();
                                self.roster = r.clone();
                                self.officers = o.clone();
                                self.inventory = inv.clone();
                                self.gold = *g;
                            }
                            None => self.saved = Some((self.flags.clone(), self.roster.clone(), self.officers.clone(), self.inventory.clone(), self.gold)),
                        }
                        let mut b = battle::Battle::new((n / 2) as usize, self.jobs.clone(), self.terrain_tab.clone());
                        if let Some(r) = self.fresh_seed() {
                            b.rng = r;
                        }
                        b.cheat = self.cheat;
                        b.icons = self.icons.clone();
                        b.names = Rc::new(self.persons.iter().map(|p| p.name.clone()).collect());
                        b.job_names = self.job_names.clone();
                        b.terrain_names = self.terrain_names.clone();
                        b.items = self.items.clone();
                        b.strategies = self.strategies.clone();
                        b.inventory = self.inventory.clone();
                        self.scene = Scene::Battle(Box::new(b));
                    } else if !matches!(self.scene, Scene::Story(_)) {
                        self.scene = Scene::Story(Box::default());
                    }
                    if !self.apply_quick(&mut vm) {
                        if let Err(e) = self.start_section(&mut vm, 0) {
                            self.error = Some(e.0);
                        }
                    }
                    self.vm = Some(vm);
                    if let Some(sec) = self.pending_camp.take() {
                        self.open_camp(sec);
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
            3 => !self.last_if,
            // 값 비교 (원작 FUN_00413ca4): 0 돈 / 2 명성, 값, 0 이상 / 1 미만 / 그 외 같음. 명성 85 이상 → 가상 모드 분기
            0x37 => {
                let a = |i: usize| args.get(i).map_or(0, |a| a.i());
                let v = match a(0) {
                    0 => self.gold,
                    2 => self.fame,
                    _ => 0,
                };
                let r = match a(2) {
                    0 => v >= a(1),
                    1 => v < a(1),
                    _ => v == a(1),
                };
                self.log.push(format!("비교 {} = {v} {} {} → {r}", ["돈", "?", "명성"].get(a(0) as usize).unwrap_or(&"?"), ["≥", "<", "="].get(a(2) as usize).unwrap_or(&"="), a(1)));
                r
            }
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
            Wait::Movie => {}
            Wait::Duel => {
                let mut done = true;
                if let Scene::Battle(b) = &mut self.scene {
                    if let Some(d) = b.duel.as_mut() {
                        d.update(decided);
                        for o in d.out.drain(..) {
                            match o {
                                duel::Out::Sound(n) => self.sounds.push(Sound::Effect(n)),
                                duel::Out::Music(m) => {
                                    self.music_now = m;
                                    self.sounds.push(Sound::Music(m));
                                }
                            }
                        }
                        done = !d.busy();
                    }
                }
                if done {
                    self.wait = Wait::None;
                    if std::mem::take(&mut self.duel_ending) {
                        self.end_duel();
                    }
                }
            }
            Wait::DuelExp => {
                if let Scene::Battle(b) = &mut self.scene {
                    if b.dmg_win.as_ref().is_some_and(|w| w.t < 40) {
                        return;
                    }
                    b.dmg_win = None;
                    if b.run_levelup() {
                        return;
                    }
                }
                self.wait = Wait::None;
            }
            Wait::Fx => {
                if let Scene::Battle(b) = &mut self.scene {
                    if b.fx_req.is_none() {
                        let done = match b.fx.as_mut() {
                            Some(f) => {
                                if f.sound_now() {
                                    self.sounds.push(Sound::Effect(format!("SE_M_{:02}.WAV", f.sound)));
                                }
                                f.advance()
                            }
                            None => true,
                        };
                        if done {
                            b.fx = None;
                            // 줄 선 연출이 더 있으면 이어서
                            if !b.next_status_fx() {
                                self.wait = Wait::None;
                            }
                        }
                    }
                } else {
                    self.wait = Wait::None;
                }
            }
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
            Wait::TicksSkip(n) => {
                if decided || n <= 1 {
                    if std::mem::take(&mut st.card_key) {
                        st.card = None;
                    }
                    self.wait = Wait::None;
                } else {
                    self.wait = Wait::TicksSkip(n - 1);
                }
            }
            Wait::Chapter(ph, t) => {
                // 원작 FUN_0040bd20: 페이드 인 (10단계) → 1초 → 누를 때까지 최대 2초 (제목은 4초) → 페이드 아웃
                let hold_max = if ph == 0 { 60 } else { 120 };
                let (fin, hold, skip_end) = (16, 16 + 30, 16 + 30 + hold_max);
                let mut t = t + 1;
                if t <= fin {
                    self.screen.level = t.min(16) as u8;
                } else if t > hold && t < skip_end && decided {
                    t = skip_end;
                }
                if t >= skip_end {
                    let k = t - skip_end;
                    self.screen.level = 16u32.saturating_sub(k) as u8;
                    if k >= 16 {
                        if ph == 0 {
                            st.big_card = self.card_title.take();
                            self.wait = Wait::Chapter(1, 0);
                        } else {
                            st.big_card = None;
                            self.wait = Wait::None;
                        }
                        return;
                    }
                }
                self.wait = Wait::Chapter(ph, t);
            }
            Wait::Fade(to) => {
                // 원작 FUN_004209b5: 10단계 × 1단위 (+3) ≈ 16틱
                let l = if level < to { (level + 1).min(to) } else { level.saturating_sub(1).max(to) };
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
                    if std::mem::take(&mut st.card_key) {
                        st.card = None;
                    }
                    self.wait = Wait::None;
                }
            }
            Wait::None => {}
        }
    }

    fn run_script(&mut self) {
        // 관상가 노인 (op10) 진행 중이면 스크립트 멈춤
        if self.tutor.is_some() {
            if self.wait == Wait::None {
                self.tutor_tick();
            }
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
        if self.vm_drop {
            self.vm_drop = false;
            self.vm = None;
            return;
        }
        if let Some(n) = self.jump_stage.take() {
            if matches!(self.scene, Scene::Battle(_)) {
                self.commit_battle();
            }
            self.camp = None;
            self.camp_pending = false;
            self.stage = n as u32;
            self.saved = None;
            self.vm = None;
            return;
        }
        if !vm.running && self.error.is_none() && self.stage % 2 == 1 && self.wait == Wait::None {
            // 전투: 절 0(배치·도입) 끝 → 턴 시작. 절 1 사건 본문 끝 → 하던 단계로. 절 2(끝) → 다음 단계
            if vm.section == 0 {
                let need_pick = self.battle().is_some_and(|b| !b.started && b.slots.get(1).is_some_and(|&s| s != (0, 0)));
                if need_pick && self.sortie_chosen.is_none() {
                    // 원작은 본진 출진 단추나 op4a 에서 고름. 둘 다 안 거쳤으면 여기서
                    let rule = self.sortie_rule.clone().or_else(|| self.last_camp_rule.clone()).unwrap_or(camp::SortieRule { max: 12, forced: vec![], excluded: vec![] });
                    match self.open_sortie(rule, None) {
                        Some(ch) => self.sortie_chosen = Some(ch),
                        None => {
                            self.vm = Some(vm);
                            return;
                        }
                    }
                }
                let roster = match &self.sortie_chosen {
                    Some(c) => c.clone(),
                    None if self.roster.is_empty() => vec![0],
                    None => self.roster.clone(),
                };
                let offs: Vec<crate::officer::Officer> = roster.iter().map(|&p| self.officer(p)).collect();
                if let Some(b) = self.battle() {
                    if !b.started {
                        // 출진 (원작은 출진 화면에서 고름): 출진 칸이 둘 이상 정해져 있으면 합류 순서대로 자동 배치
                        if b.slots.get(1).is_some_and(|&s| s != (0, 0)) {
                            for (k, &pid) in roster.iter().enumerate() {
                                let Some(&(x, y)) = b.slots.get(k) else { break };
                                if k > 0 && (x, y) == (0, 0) {
                                    break;
                                }
                                if b.unit_of(pid).is_none() {
                                    let u = b.make_unit(offs[k].clone(), battle::Side::Player, x, y);
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
                self.vm = Some(vm);
                if self.next_event() {
                    return;
                }
                let Some(vm) = self.vm.take() else { return };
                if let (Some(p), Some(b)) = (self.resume.take(), self.battle()) {
                    b.phase = p;
                }
                self.vm = Some(vm);
                return;
            }
        }
        if self.camp.is_some() {
            self.vm = Some(vm);
            return;
        }
        if !vm.running && self.error.is_none() && self.wait == Wait::None && self.camp_pending && vm.section + 1 < vm.sections() {
            // 본진 (op06 1) — 다음 절은 선택지 블록들
            self.camp_pending = false;
            let next = vm.section + 1;
            self.vm = Some(vm);
            self.open_camp(next);
            return;
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
                // 패배 (플래그 1) → 원작 게임 오버 화면
                self.log.push("전투 패배 — 다시".into());
                self.game_over();
            } else {
                self.commit_battle();
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
                // 기다림 (원작 FUN_00417f1f → FUN_0042ea8b: n × 2단위 = n × 0.1 초 → 30틱 기준 3n)
                if a(0) > 0 {
                    self.wait = Wait::Ticks(a(0) as u32 * 3);
                }
            }
            0x24 => {
                let n = a(0);
                self.music_now = if n == 0xFF { None } else { Some(n + 2) };
                self.sounds.push(Sound::Music(self.music_now));
            }
            0x23 => {
                // 효과음 (원작 FUN_00476c2d / FUN_0047699d): 번호 <100 SEnn, 100.. SE_M_nn, 200.. SE_E_nn.
                // 방식 0 반복, 0xFF 정지, 1 한 번, 그 외 n 번
                let n = a(0);
                let name = match n {
                    0..=99 => format!("SE{n:02}.WAV"),
                    100..=199 => format!("SE_M_{:02}.WAV", n - 100),
                    _ => format!("SE_E_{:02}.WAV", n - 200),
                };
                match a(1) {
                    0 => self.sounds.push(Sound::EffectLoop(name)),
                    0xFF => self.sounds.push(Sound::EffectStop),
                    k => {
                        for _ in 0..k.clamp(1, 4) {
                            self.sounds.push(Sound::Effect(name.clone()));
                        }
                    }
                }
            }
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
                            self.last_map = Some((mode, v));
                        }
                    }
                }
                return Ok(false);
            }
            0x30 => {
                // 인물 놓기 (인물, x, y, 방향, 자세)
                let pid = a(0) as usize;
                let sprite = self.sprite_over.get(&pid).copied().unwrap_or_else(|| self.persons.get(pid).map_or(0, |p| p.sprite));
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
                if !self.battle_dialog(pages) {
                    vm.pc = at;
                    return Ok(false);
                }
            }
            0x12 => self.start_choice(&args),
            0x1C => self.wait = Wait::Fade(16),
            0x1D => self.wait = Wait::Fade(0),
            0x16 | 0x19 => {
                // 해설 창 (원작 FUN_0042fa65(글, 0xffff): 가운데 352×144). 19 = 전투 승패 조건
                let (t, _, _) = encoding_rs::EUC_KR.decode(args[0].bytes());
                if op == 0x19 {
                    if let Scene::Battle(b) = &mut self.scene {
                        b.victory_text = t.to_string();
                    }
                }
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
                // 장 제목 카드 (원작 FUN_00415f3c → FUN_0040bd20): 0 서장, 1~3 제n장, 4 종장 → 제목. 검은 화면 큰 글자
                let (t, _, _) = encoding_rs::EUC_KR.decode(args[1].bytes());
                let label = ["서장", "제1장", "제2장", "제3장", "종장"].get(a(0) as usize).copied();
                if let Some(label) = label {
                    let (m1, m2) = match (self.text_mask(label, 64), self.text_mask(&t, 64)) {
                        (Ok(x), Ok(y)) => (x, y),
                        _ => {
                            vm.pc = at;
                            return Ok(false);
                        }
                    };
                    self.chapter = (a(0) - 1).max(0) as u8;
                    self.card_title = Some(m2);
                    if let Scene::Story(st) = &mut self.scene {
                        st.big_card = Some(m1);
                        self.screen.level = 0;
                        self.wait = Wait::Chapter(0, 0);
                    }
                }
            }
            0x06 => {
                // 본진: 1 = 열기, 0 = 닫기. 출진 규칙 (최대, 반드시 ×5, 불가 ×5) 은 어느 쪽이든 정함 (원작 FUN_00417f55)
                let list: Vec<i32> = args[2..].iter().map(|x| x.i()).collect();
                let rule = camp::SortieRule::from_args(a(1), &list);
                self.sortie_max = rule.max;
                if a(0) == 1 {
                    self.camp_rule = Some(rule);
                    self.camp_pending = true;
                } else {
                    self.camp = None;
                    self.camp_pending = false;
                }
            }
            0x4A => {
                // 출진 규칙 (전투, 원작 FUN_004181a2): 반드시·불가만 바꾸고 최대는 op06 이 정한 것
                let list: Vec<i32> = args[1..].iter().map(|x| x.i()).collect();
                let mut rule = camp::SortieRule::from_args(a(0), &list);
                rule.max = self.sortie_max;
                self.sortie_rule = Some(rule);
            }
            0x3A => {
                // 돈 (0: 설정/더하기/빼기) · 명성 게이지 (2)
                let (kind, how, v) = (a(0), a(1), a(2));
                match kind {
                    0 => {
                        self.gold = match how {
                            0 => v,
                            1 => self.gold + v,
                            _ => (self.gold - v).max(0),
                        }
                        .min(999_999)
                    }
                    2 => {
                        self.fame = match how {
                            0 => v,
                            1 => (self.fame + v).min(100),
                            _ => (self.fame - v).max(0),
                        }
                    }
                    _ => {}
                }
            }
            0x3D => {
                // 아이템 얻기
                let it = a(0);
                if (0..0x68).contains(&it) {
                    self.inventory.push((it as u8, 1));
                    let n = crate::officer::item_name(&self.items, it as u8);
                    if let Some(b) = self.battle() {
                        b.message = Some((format!("{n}을(를) 손에 넣었다!"), 90));
                    }
                }
            }
            0x59 => {
                // 전투 보상: 돈 (조조 레벨×100 + 700 + 인자), 아이템 셋
                let lv = self.officer(0).level as i32;
                let money = lv * 100 + 700 + a(0);
                self.gold = (self.gold + money).min(999_999);
                for k in [1, 3, 5] {
                    let it = a(k);
                    if (0..0x68).contains(&it) {
                        self.inventory.push((it as u8, 1));
                    }
                }
                if let Some(b) = self.battle() {
                    b.message = Some((format!("보상금 {money}"), 90));
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
            0x58 => {
                // 맵 물체 (원작 FUN_00418af1): 종류, 0 놓기/1 치우기, 지형, x, y, 화면 이동, 조용히
                let (kind, rm, terr, x, y, look, quiet) = (a(0) as u8, a(1), a(2) as u8, a(3), a(4), a(5), a(6) != 0);
                let mut se = None;
                self.log.push(format!("물체 {kind} {} ({x},{y})", if rm == 0 { "놓기" } else { "치우기" }));
                if let Some(b) = self.battle() {
                    if look != 0 && !quiet {
                        b.focus(x, y);
                    }
                    if rm == 0 {
                        let near = b.place_obj(kind, terr, x, y);
                        se = match (kind, terr) {
                            (0 | 2, _) => Some(0x13),
                            (4.., 0x0C) => Some(0x10),
                            (4.., 0x0F) => Some(0x0F),
                            (4.., 0x11) if !near => Some(0x12),
                            _ => None,
                        };
                    } else if b.remove_obj(kind, x, y) {
                        se = Some(0x12);
                    }
                }
                if !quiet {
                    if let Some(n) = se {
                        self.sounds.push(Sound::Effect(format!("SE{n:02}.WAV")));
                    }
                    if kind >= 4 {
                        self.wait = Wait::Ticks(15);
                    }
                }
            }
            0x21 => {
                // 불 (원작 FUN_004162fa → op58 종류 0): x, y, 0 붙이기 / 1 끄기, 화면 이동, 조용히
                let (x, y, off, look, quiet) = (a(0), a(1), a(2), a(3), a(4) != 0 && a(4) != 0xFFFF);
                let mut placed = false;
                if let Some(b) = self.battle() {
                    if look != 0 && look != 0xFFFF && !quiet {
                        b.focus(x, y);
                    }
                    if off == 0 {
                        b.place_obj(0, 0x1A, x, y);
                        placed = true;
                    } else {
                        b.remove_obj(0, x, y);
                    }
                }
                if placed && !quiet {
                    self.sounds.push(Sound::Effect("SE19.WAV".into()));
                }
            }
            0x15 => {
                // 부대 말 (원작 FUN_0045b757): 인물 a, 없으면 b 가 말함. 둘 다 없으면 해설
                let (t, _, _) = encoding_rs::EUC_KR.decode(args[2].bytes());
                let lines: Vec<String> = t.split('\n').filter(|l| !l.is_empty()).map(|l| l.to_string()).collect();
                let on_map = |e: &Self, p: i32| -> bool {
                    matches!(&e.scene, Scene::Battle(b) if b.unit_of(p as usize).is_some_and(|u| b.units[u].visible && b.units[u].hp > 0))
                };
                let who = if on_map(self, a(0)) { Some(a(0) as usize) } else if on_map(self, a(1)) { Some(a(1) as usize) } else { None };
                let pages: Vec<story::Page> = match who {
                    Some(pid) => {
                        let name = self.persons.get(pid).map(|p| p.name.clone()).unwrap_or_default();
                        let face = (pid < self.persons.len()).then(|| self.face_of(pid));
                        lines.chunks(4).map(|c| story::Page { person: Some(pid), face, name: name.clone(), lines: c.to_vec() }).collect()
                    }
                    None => lines.chunks(6).map(|c| story::Page { person: None, face: None, name: String::new(), lines: c.to_vec() }).collect(),
                };
                let (packs, assets) = (&mut self.packs, &mut self.assets);
                if let Scene::Battle(b) = &mut self.scene {
                    match who.and_then(|p| b.unit_of(p)) {
                        Some(u) => {
                            let (x, y) = (b.units[u].x, b.units[u].y);
                            if x * battle::CELL < b.cam.0 + 24 || x * battle::CELL > b.cam.0 + 568 || y * battle::CELL < b.cam.1 + 24 || y * battle::CELL > b.cam.1 + 408 {
                                b.focus(x, y);
                            }
                            b.ui.speaker_y = Some(y * battle::CELL - b.cam.1 + 24);
                            b.ui.speaker_cell = Some((x, y));
                            b.ui.narration = false;
                        }
                        None => {
                            b.ui.speaker_cell = None;
                            b.ui.narration = true;
                        }
                    }
                    if b.ui.show_dialog(packs, assets, pages).is_err() {
                        vm.pc = at;
                        return Ok(false);
                    }
                    self.wait = Wait::Dialog;
                }
            }
            // 표지 (블록 머리 이름) · 모드 값 (원작 FUN_0042ee18: 저장 화면 구분용) — 화면 동작 없음
            0x02 | 0x08 => {}
            0x20 => {
                // 얼굴 상태 (원작 FUN_004193de): 0xFF = 아래 5비트 지움, 그 외 = 아래 4비트 바꾸고 값의 위 비트 더함
                let v = a(0) as u8;
                self.face_state = if v == 0xFF { self.face_state & 0xE0 } else { (self.face_state & 0xF0) | v };
            }
            0x35 => {
                // 인물 그림 바꾸기 (원작 FUN_004159dd)
                self.sprite_over.insert(a(0) as usize, a(1) as u16);
            }
            0x69 => {
                // 시간 경과 자막 ("잠시 후…….", 원작 FUN_00415e04 → FUN_00431c4c(2): 글을 띄우고 누를 때까지 기다린 뒤 지움)
                let (t, _, _) = encoding_rs::EUC_KR.decode(args[0].bytes());
                if let Scene::Story(st) = &mut self.scene {
                    st.card = Some(t.to_string());
                    st.card_key = true;
                    // (메시지 속도 1 + 1) × 10 × 0.1 초 = 2초, 누르면 넘김
                    self.wait = Wait::TicksSkip(60);
                }
            }
            0x31 if self.battle().is_none() => {
                // 이야기 장면 퇴장 (원작 FUN_004154ec → FUN_0042b1f2)
                if let Scene::Story(st) = &mut self.scene {
                    let pid = a(1) as usize;
                    st.actors.retain(|x| x.id != pid);
                }
            }
            0x6B => {
                // 맵 연출 (원작 FUN_0041645a): x, y, 번호, 화면 이동(0 이 아니면 이동).
                // 번호 100 미만 = MEFF 바로 재생, 109 = 불러 둔 MCALL 재생 (FUN_00423f11), 그 외 100 이상 = MCALL 불러 두기만 (FUN_00423e97)
                let (x, y, id) = (a(0), a(1), a(2));
                let preload = self.mcall_loaded;
                if let Some(b) = self.battle() {
                    if a(3) != 0 {
                        b.focus(x, y);
                    }
                    let on_screen = x * battle::CELL >= b.cam.0 && x * battle::CELL < b.cam.0 + 640 && y * battle::CELL >= b.cam.1 && y * battle::CELL < b.cam.1 + 480;
                    let center = vec![(x * battle::CELL + battle::CELL / 2, y * battle::CELL + battle::CELL / 2)];
                    let play = match id {
                        0..=99 => Some(id as u8),
                        109 => preload.map(|m| m + 100),
                        _ => None,
                    };
                    if let Some(fid) = play.filter(|_| on_screen) {
                        b.fx_req = Some((fid, center));
                        self.wait = Wait::Fx;
                    }
                }
                if (100..200).contains(&id) && id != 109 {
                    self.mcall_loaded = Some((id - 100) as u8);
                }
            }
            0x5A => {
                // 전투 시작 연출 (원작 FUN_0044db19): 맵을 어둡게 덮고 "작전개시" (맵 폭 < 720 이면 48px, 아니면 64px), 1초 뒤 걷음
                let h = self.battle().map_or(64, |b| if b.cols * battle::CELL < 0x2d0 { 48 } else { 64 });
                let m = match self.text_mask("작전개시", h) {
                    Ok(m) => m,
                    Err(Pending) => {
                        vm.pc = at;
                        return Ok(false);
                    }
                };
                if let Some(b) = self.battle() {
                    b.intro = Some(0);
                    b.intro_mask = Some(m);
                    self.wait = Wait::Ticks(31);
                }
            }
            0x62 => {
                // 일기토 승패 (원작 FUN_0044b25a): 인자 = 이긴 쪽
                if let Some(b) = self.battle() {
                    let wt = b.win_target;
                    if let Some(d) = b.duel.as_mut() {
                        d.defeat(a(0) as usize, wt);
                        self.wait = Wait::Duel;
                    }
                }
            }
            0x5D => {
                // 턴 제한 바꾸기 (원작 FUN_0041946e): 0 = 정하기, 그 외 = 늘리기
                if let Some(b) = self.battle() {
                    let v = a(1);
                    let lim = if a(0) == 0 { v } else { b.turn_limit + v };
                    if lim > b.turn {
                        b.turn_limit = lim;
                        b.message = Some((format!("제한 턴 {lim}"), 60));
                    }
                }
            }
            0x6A => {
                // 목표 구역 끄기 (원작 DAT_004be488 = 0)
                if let Some(b) = self.battle() {
                    b.goal_zone = None;
                }
            }
            0x51 => {
                // 전원 회복·상태 풀기 (원작 FUN_0041867a)
                let items = self.items.clone();
                if let Some(b) = self.battle() {
                    for u in b.units.iter_mut().filter(|u| u.visible && u.hp > 0) {
                        u.hp = u.max_hp;
                        u.mp = u.off.mp_max(&items) as i32;
                        u.status = 0;
                        u.state = [3; 5];
                    }
                }
            }
            0x54 => {
                // 쓰러진 부대 치우기 (원작 FUN_0044ec5a)
                if let Some(b) = self.battle() {
                    for u in b.units.iter_mut().filter(|u| u.visible && u.hp <= 0) {
                        u.visible = false;
                        u.gone = true;
                    }
                }
            }
            // 전투 끝 상태 (원작 FUN_00457fdd: 2 승리, 3 패배). 승패 처리는 스크립트 플래그(0·1)로 함
            0x49 | 0x0E => {}
            0x3E => {
                // 장수 장비 지정 (원작 FUN_0041871f → FUN_004081dc): 인물, 무기, 레벨, 방어구, 레벨, 보조.
                // 값 0 = 그대로, 1 = 빼기, 그 외 = 무기 값−2 / 방어구 값+36 / 보조 값+51 (원작은 빼기 판정을 늘 무기 값으로 함)
                let (items, jobs) = (self.items.clone(), self.jobs.clone());
                let mut i = 0;
                while i + 5 < args.len() {
                    let v = |k: usize| {
                        let x = args[i + k].i();
                        if x == 0xFFFF { 0 } else { x }
                    };
                    let pid = args[i].i() as usize;
                    if pid < self.persons.len() {
                        let mut o = self.officer(pid);
                        let (w, wl, ar, al, ac) = (v(1), v(2), v(3), v(4), v(5));
                        for (slot, val, lv) in [(0u8, w, wl), (1, ar, al), (2, ac, 0)] {
                            if val == 0 {
                                continue;
                            }
                            if w == 1 {
                                match slot {
                                    0 => o.weapon = None,
                                    1 => o.armor = None,
                                    _ => o.acc = None,
                                }
                            } else {
                                o.script_equip(&items, &jobs, slot, val, lv);
                            }
                        }
                        self.officers.insert(pid, o);
                        self.note_treasures();
                    }
                    i += 6;
                }
            }
            0x22 => {
                // 동영상 (원작 FUN_00431d07: 0 LOGO 1 OPEN 2 END)
                if self.movies {
                    let name = ["LOGO", "OPEN", "END"].get(a(0) as usize).copied().unwrap_or("OPEN");
                    self.sounds.push(Sound::Movie(format!("{name}.mp4")));
                    self.wait = Wait::Movie;
                    return Ok(false);
                }
            }
            0x10 => {
                // 첫 전투의 관상가 노인 (원작 FUN_00417d1d → FUN_004354f9)
                if self.start_tutor().is_err() {
                    vm.pc = at;
                }
                return Ok(false);
            }
            0x0F => {
                // 엔딩 (원작 FUN_00417cc6: 엔딩 번호를 SVCMN.E5S 에 기록 → END.AVI → 제목)
                self.log.push(format!("엔딩 {}", a(0)));
                if (0..3).contains(&a(0)) {
                    self.endings |= 1 << a(0);
                }
                self.note_treasures();
                self.common_out = Some(tutor::common_string(self.endings, &self.book));
                self.sounds.push(Sound::Ending(a(0)));
                self.sounds.push(Sound::Music(None));
                self.sounds.push(Sound::EffectStop);
                if self.movies {
                    self.sounds.push(Sound::Movie("END.mp4".into()));
                    self.wait = Wait::Movie;
                }
                self.scene = Scene::End { t: 0 };
                self.save_out = Some(String::new());
                vm.running = false;
                self.vm_drop = true;
                return Ok(false);
            }
            0x2F => {
                // 인물 모두 치우기 (원작 FUN_004292a5)
                if let Scene::Story(st) = &mut self.scene {
                    st.actors.clear();
                }
            }
            // 화면 갱신 (원작 반환 2: 이번 틱 끝)
            0x1E => return Ok(false),
            0x48 if self.battle().is_some() => {
                // 목록 부대 장비 (원작 FUN_004151a6 → FUN_00444b4e): 인물, 무기, 무기 레벨, 방어구, 방어구 레벨, 보조.
                // 0 = 기본 장비 (FUN_004083d7), 1 = 없음, 그 외 = FUN_004081dc (무기 값−2 / 방어구 값+36 / 보조 값+51)
                let (items, jobs) = (self.items.clone(), self.jobs.clone());
                let v = |i: usize| if a(i) == 0xFFFF { 0 } else { a(i) };
                let (pid, w, wl, ar, al, ac) = (a(0) as usize, v(1), v(2), v(3), v(4), v(5));
                if let Some(b) = self.battle() {
                    if let Some(u) = b.units.iter_mut().find(|u| u.person == pid && u.side != battle::Side::Player) {
                        let o = &mut u.off;
                        for (slot, val, lv) in [(0u8, w, wl), (1, ar, al), (2, ac, 0)] {
                            match val {
                                0 if slot < 2 => o.auto_equip(&jobs, slot),
                                0 => {}
                                1 => match slot {
                                    0 => o.weapon = None,
                                    1 => o.armor = None,
                                    _ => o.acc = None,
                                },
                                _ => o.script_equip(&items, &jobs, slot, val, lv),
                            }
                        }
                    }
                }
            }
            0x68 => {
                // 일기토 시작 (원작 FUN_0044bb1f): 조조군·우군 장수가 오른쪽(1)
                let (p1, p2) = (a(0) as usize, a(1) as usize);
                let friendly = |e: &mut Self, p: usize| e.battle().and_then(|b| b.unit_of(p).map(|u| b.units[u].side != battle::Side::Enemy)).unwrap_or(false);
                let who = if self.roster.contains(&p1) {
                    [p2, p1]
                } else if self.roster.contains(&p2) || !friendly(self, p1) {
                    [p1, p2]
                } else {
                    [p2, p1]
                };
                match self.make_duel(who) {
                    Err(Pending) => {
                        vm.pc = at;
                        return Ok(false);
                    }
                    Ok(d) => {
                        self.log.push(format!("일기토 {} 대 {}", who[0], who[1]));
                        if let Some(b) = self.battle() {
                            b.duel = Some(d);
                        }
                        self.wait = Wait::Duel;
                    }
                }
            }
            0x5F => {
                // 끝 (원작 FUN_0044ba60)
                if let Some(d) = self.battle().and_then(|b| b.duel.as_mut()) {
                    d.finish();
                    self.duel_ending = true;
                    self.wait = Wait::Duel;
                }
            }
            0x61 => {
                if let Some(d) = self.battle().and_then(|b| b.duel.as_mut()) {
                    d.versus();
                    self.wait = Wait::Duel;
                }
            }
            0x64 => {
                if let Some(d) = self.battle().and_then(|b| b.duel.as_mut()) {
                    d.motion((a(0) as usize).min(1), a(1) as u8);
                    self.wait = Wait::Duel;
                }
            }
            0x65 | 0x66 => {
                // 공격 (쪽, 종류, 모아 치기) / 반격 (쪽, 종류, 막음)
                if let Some(d) = self.battle().and_then(|b| b.duel.as_mut()) {
                    let s = (a(0) as usize).min(1);
                    if op == 0x65 {
                        d.attack(s, a(1) as u8, a(2) != 0);
                    } else {
                        d.counter(s, a(1) as u8, a(2) != 0);
                    }
                    self.wait = Wait::Duel;
                }
            }
            0x60 | 0x63 => {
                // 등장+대사 (쪽, 글, 동작) / 대사 (쪽, 글, 기다림)
                let side = (a(0) as usize).min(1);
                let (t, _, _) = encoding_rs::EUC_KR.decode(args[1].bytes());
                let lines: Vec<String> = t.split('\n').map(|l| l.to_string()).filter(|l| !l.is_empty()).collect();
                if let Some(d) = self.battle().and_then(|b| b.duel.as_mut()) {
                    if op == 0x60 {
                        d.appear(side, lines, a(2) as u8);
                    } else {
                        d.say(side, lines, a(2) != 0);
                    }
                    self.wait = Wait::Duel;
                }
            }
            0x11 => {
                // 단계 이동 (원작 FUN_00418077: 진행 단계 = 값, 새 스크립트 읽기). 갈림길·가상 모드·엔딩
                self.log.push(format!("단계 이동 → {}", a(0)));
                self.jump_stage = Some(a(0) as usize);
                vm.running = false;
                return Ok(false);
            }
            0x5B => {
                // 목표 구역 표시 (원작 FUN_004190c1): x1, y1, x2, y2, 끄기
                let show = matches!(a(4), 0 | 0xFFFF);
                let z = (a(0), a(1), a(2), a(3));
                if let Some(b) = self.battle() {
                    b.goal_zone = show.then_some(z);
                }
            }
            0x5C => {
                // 부대 보여 주기 (원작 FUN_0041921d: 그 부대로 화면을 옮기고 세 번 깜빡임)
                if let Some(b) = self.battle() {
                    if let Some(u) = b.unit_of(a(0) as usize).filter(|&u| b.units[u].visible && b.units[u].hp > 0) {
                        let (x, y) = (b.units[u].x, b.units[u].y);
                        b.focus(x, y);
                        b.cursor = (x, y);
                    }
                }
            }
            0x44..=0x47 | 0x4B..=0x4F | 0x31 | 0x50 | 0x53 | 0x55 if self.battle().is_some() => self.battle_op(op, &args),
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
            // 중간 저장에서: 바뀐 지형·겹침과 성문 그림 되살리기
            if let Some((t, o)) = b.restore_terrain.take() {
                b.terrain = t;
                b.overlay = o;
                let gates: Vec<(usize, i32, i32)> = b.objs.iter().filter(|o| o.2 >= 4).map(|o| (o.2 as usize * 2 - 8, o.0, o.1)).collect();
                b.gate_queue.extend(gates);
            }
            self.screen.pal = palette::from_bytes(&pal);
            let p = &self.screen.pal;
            b.col = [
                // 이동 범위: 원작 FUN_00453d9d 팔레트 0x3C (아군 부대) 바둑판, 칸 테두리 없음
                0x3C,
                // 공격 대상 고르기: 원작 팔레트 0x28 바둑판 (FUN_0045416f 의 '(' )
                0x28,
                palette::nearest(p, [80, 160, 255]),
                palette::nearest(p, [255, 60, 40]),
                palette::nearest(p, [255, 255, 255]),
                palette::nearest(p, [255, 230, 60]),
                palette::nearest(p, [0, 0, 0]),
            ];
        }
        // 전체 지도 (SMLMAP.E5)
        if b.minimap.is_none() {
            match self.packs.get(&mut self.assets, "SMLMAP.E5", b.map_no) {
                Err(Pending) => return false,
                Ok(d) => b.minimap = Some(Rc::new(Pic::new(b.cols as usize * 6, b.rows as usize * 6, d.unwrap_or_default()))),
            }
        }
        b.show_minimap = self.minimap_on;
        // 겹침 그림 (MARK.E5)
        if b.mark_tiles.is_empty() {
            match self.assets.get("MARK.E5") {
                Asset::Pending => return false,
                Asset::Ready(d) => {
                    b.mark = d.clone();
                    let tile = |o: usize| d.get(o..o + 256).map(|t| t.to_vec()).unwrap_or_default();
                    b.ui.arrows = Some(Rc::new((tile(0x194C0), tile(0x193C0))));
                    b.mark_tiles = [0x8BC0, 0x94C0, 0x9DC0, 0xA6C0, 0xAFC0, 0xB8C0, 0x1B2E0, 0x1BBE0]
                        .iter()
                        .map(|&o| Pic::new(48, 48, d.get(o..o + 2304).map(|t| t.to_vec()).unwrap_or_default()))
                        .collect();
                }
                Asset::Missing => b.mark_tiles = vec![Pic::new(48, 48, Vec::new()); 8],
            }
        }
        // 성문 그림 (GATE.E5)
        while let Some(&(idx, x, y)) = b.gate_queue.first() {
            match self.packs.get(&mut self.assets, "GATE.E5", idx) {
                Err(Pending) => return false,
                Ok(d) => {
                    if let Some(d) = d {
                        b.patch_gate(&d, x, y);
                    }
                    b.gate_queue.remove(0);
                }
            }
        }
        // 책략 연출 (MEFF.E5 항목 / MCALLnn.E5)
        if let Some((id, centers)) = b.fx_req.clone() {
            let data: Option<Rc<Vec<u8>>> = if id < 100 {
                match self.packs.get(&mut self.assets, "MEFF.E5", id as usize) {
                    Err(Pending) => return false,
                    Ok(d) => d.map(Rc::new),
                }
            } else {
                match self.assets.get(&format!("MCALL{:02}.E5", id - 100)) {
                    Asset::Pending => return false,
                    Asset::Ready(d) => Some(d),
                    Asset::Missing => None,
                }
            };
            b.fx_req = None;
            b.fx = data.and_then(|d| fx::Fx::parse(d, &self.screen.pal, centers));
        }
        for s in std::mem::take(&mut b.sounds) {
            self.sounds.push(Sound::Effect(s));
        }
        // 부대 그림
        for i in 0..b.units.len() {
            if b.units[i].sprite.is_none() {
                match self.packs.get(&mut self.assets, "UNIT_MOV.E5", b.units[i].sprite_idx) {
                    Err(Pending) => return false,
                    Ok(d) => b.units[i].sprite = Some(Rc::new(Pic::new(48, 48 * 11, d.unwrap_or_default()))),
                }
            }
            if b.units[i].atk.is_none() {
                match (self.packs.get(&mut self.assets, "UNIT_ATK.E5", b.units[i].sprite_idx), self.packs.get(&mut self.assets, "UNIT_SPC.E5", b.units[i].sprite_idx)) {
                    (Ok(a), Ok(s)) => {
                        b.units[i].atk = Some(Rc::new(Pic::new(64, 64 * 12, a.unwrap_or_default())));
                        b.units[i].spc = Some(Rc::new(Pic::new(48, 48 * 5, s.unwrap_or_default())));
                    }
                    _ => return false,
                }
            }
        }
        true
    }

    /// 전투 부대 명령
    fn battle_op(&mut self, op: u16, args: &[Arg]) {
        let a = |i: usize| args.get(i).map_or(0, |a| a.i());
        let persons = self.persons.clone();
        let roster = if self.roster.is_empty() { vec![0] } else { self.roster.clone() };
        let avg = self.army_level() as i32;
        let jobs = self.jobs.clone();
        // 필요한 인물 능력 미리
        let mut offs: HashMap<usize, crate::officer::Officer> = HashMap::new();
        let want: Vec<usize> = match op {
            0x46 | 0x47 => {
                let n = if op == 0x46 { 11 } else { 12 };
                args.chunks(n).map(|r| r[0].i() as usize).filter(|&p| p < persons.len()).collect()
            }
            0x4C => roster.clone(),
            _ => vec![],
        };
        for p in want {
            if p < persons.len() {
                let o = self.officer(p);
                offs.insert(p, o);
            }
        }
        let Some(b) = self.battle() else { return };
        // op45 인자 3 = 전투 전체 레벨 보정 (원작 FUN_004146c1: 0xFFFF 0, 50 이하 +값, 그 위 −(값−50))
        if op == 0x45 {
            let v = a(3);
            b.level_bias = if v == 0xFFFF { 0 } else if v <= 50 { v } else { -(v - 50) };
        }
        // 기준 레벨 = 평균 + 보정 (1 이상). 부대 조정치: 0xFFFF 0, 50 이하 = 기준 + 값 (최대 50), 그 위 = 기준 − (값−50) (최소 1)
        // (원작 FUN_00414995 op46 / FUN_00414d74 op47)
        let base = (avg + b.level_bias).max(1);
        let level_of = |adj: i32| -> u8 {
            if adj == 0xFFFF {
                base.min(50) as u8
            } else if adj <= 50 {
                (base + adj).clamp(1, 50) as u8
            } else {
                (base - (adj - 50)).clamp(1, 50) as u8
            }
        };
        match op {
            0x44 => b.units.clear(),
            // 전투 설정 (원작 FUN_004146c1): 인자 2 턴 제한, 5 승리 목표 장수, 7 패배 목표 장수 (0xFFFF 없음)
            0x45 => {
                b.turn_limit = a(2);
                let who = |v: i32| (v != 0xFFFF && v >= 0).then_some(v as usize);
                b.win_target = who(a(5));
                b.lose_target = who(a(7));
            }
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
                    let mut o = offs.get(&(pid as usize)).cloned().unwrap();
                    if !roster.contains(&(pid as usize)) {
                        o.set_level(&jobs, level_of(lv));
                        // 원작: 목록 부대는 레벨에 맞는 무기·방어구를 자동 장착 (op48 로 바꿀 수 있음)
                        o.auto_equip(&jobs, 0);
                        o.auto_equip(&jobs, 1);
                    }
                    let mut u = b.make_unit(o, side, x, y);
                    u.visible = !hidden;
                    u.ai = if ai == 0xFFFF { 1 } else { ai as u8 };
                    if matches!(u.ai, 4 | 6) {
                        u.ai_target = Some((tx, ty));
                    }
                    if matches!(u.ai, 3 | 5) {
                        u.ai_person = Some(if op == 0x46 { r[8].i() } else { r[9].i() } as usize);
                    }
                    b.units.retain(|o| o.person != pid as usize);
                    b.units.push(u);
                    if op == 0x47 {
                        b.list47.push(pid as usize);
                    }
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
                                if let Some(o) = offs.get(&pid) {
                                    let u = b.make_unit(o.clone(), battle::Side::Player, x, y);
                                    b.units.push(u);
                                }
                            }
                        }
                    }
                } else if let Some(u) = b.unit_of(a(1) as usize) {
                    b.units[u].visible = true;
                    b.units[u].fade = 16;
                }
            }
            0x55 => {
                // 증원 등장 (원작 FUN_00417d2c → FUN_0045b181): 0 = 인물, 1 = 적 목록 번호. 숨어 있던 부대만 (x, y) 에 나타남
                let pid = if a(0) == 1 { b.list47.get(a(2) as usize).copied() } else { Some(a(1) as usize) };
                if let Some(u) = pid.and_then(|p| b.unit_of(p)) {
                    if !b.units[u].visible && b.units[u].hp > 0 {
                        let (x, y) = b.free_near(b.units[u].job, a(3), a(4));
                        let un = &mut b.units[u];
                        un.visible = true;
                        un.x = x;
                        un.y = y;
                        un.px = (x * battle::CELL) as f32;
                        un.py = (y * battle::CELL) as f32;
                        un.path.clear();
                        b.focus(x, y);
                        self.sounds.push(Sound::Effect("SE15.WAV".into()));
                        self.wait = Wait::Ticks(6);
                    }
                }
            }
            0x4D => {
                // 부대 상태 바꾸기 (원작 FUN_004174fe → FUN_0045ca55 / FUN_0045cdde):
                // 0 방식(0 인물, 1 목록 번호, 2 영역), 1 인물, 2 목록 번호, 3..6 x1 y1 x2 y2, 7 진영,
                // 8 능력 종류(0..4, FFFF 없음), 9 정도(0 내림·1 보통·2 올림), 10 상태이상 비트(0x80 = 풀기), 11 HP 깎기, 12 MP 깎기
                let (mode, x1, y1, x2, y2, side) = (a(0), a(3), a(4), a(5), a(6), a(7));
                let pick: Vec<usize> = (0..b.units.len())
                    .filter(|&i| {
                        let u = &b.units[i];
                        if !u.visible || u.hp <= 0 {
                            return false;
                        }
                        match mode {
                            1 => b.list47.get(a(2) as usize) == Some(&u.person),
                            2 => {
                                let in_side = match side {
                                    0 => u.side == battle::Side::Player,
                                    1 => u.side == battle::Side::Ally,
                                    2 | 3 | 5 => u.side == battle::Side::Enemy,
                                    4 => u.side != battle::Side::Enemy,
                                    _ => true,
                                };
                                in_side && (x1..=x2).contains(&u.x) && (y1..=y2).contains(&u.y)
                            }
                            _ => u.person == a(1) as usize,
                        }
                    })
                    .collect();
                let (stat, lvl, st, hp, mp) = (a(8), a(9), a(10), a(11), a(12));
                b.fx_queue.clear();
                let mut effs: Vec<(Option<u8>, battle::FxEffect)> = Vec::new();
                for i in pick {
                    if stat != 0xFFFF && (0..5).contains(&stat) {
                        let new = match lvl {
                            0 => 1,
                            2 => 5,
                            _ => 3,
                        };
                        let old = b.units[i].state[stat as usize];
                        let id = (new != old).then(|| battle::Battle::stat_fx_id(stat as u8, new > old));
                        effs.push((id, battle::FxEffect { u: i, set: 0, clear: 0, stat: Some((stat as usize, new)) }));
                    }
                    if st != 0xFFFF && st != 0 {
                        if st & 0x80 == 0 {
                            effs.push((battle::Battle::status_fx_id(st as u8), battle::FxEffect { u: i, set: st as u8, clear: 0, stat: None }));
                        } else {
                            effs.push((Some(44), battle::FxEffect { u: i, set: 0, clear: st as u8 & 0x7F, stat: None }));
                        }
                    }
                    // HP·MP 는 그만큼 깎되 1 은 남김 (값은 바이트)
                    let un = &mut b.units[i];
                    if hp != 0 {
                        un.hp = (un.hp - (hp & 0xFF)).max(1);
                    }
                    if mp != 0 {
                        un.mp = (un.mp - (mp & 0xFF)).max(1);
                    }
                }
                for (id, e) in effs {
                    b.queue_effect(id, e);
                }
                if b.next_status_fx() {
                    self.wait = Wait::Fx;
                }
            }
            0x4E => {
                // AI 바꾸기 (원작 FUN_00416aeb): 0 = 인물 하나, 그 외 = 영역 (x1,y1)-(x2,y2) 안의 진영 부대.
                // 진영: 0 조조군 1 우군 2·3 적 4 아군 전체 5 적 전체 6 모두. 이어서 AI 종류, 대상 인물, 대상 x, y
                let (mode, pid, ai, tp, tx, ty) = (a(0), a(1), a(7), a(8), a(9), a(10));
                let (x1, y1, x2, y2, side) = (a(2), a(3), a(4), a(5), a(6));
                let pick: Vec<usize> = (0..b.units.len())
                    .filter(|&i| {
                        let u = &b.units[i];
                        if !u.visible || u.hp <= 0 {
                            return false;
                        }
                        if mode == 0 || mode == 0xFFFF {
                            return u.person == pid as usize;
                        }
                        let in_side = match side {
                            0 => u.side == battle::Side::Player,
                            1 => u.side == battle::Side::Ally,
                            2 | 3 | 5 => u.side == battle::Side::Enemy,
                            4 => u.side != battle::Side::Enemy,
                            _ => true,
                        };
                        in_side && (x1..=x2).contains(&u.x) && (y1..=y2).contains(&u.y)
                    })
                    .collect();
                if ai < 7 {
                    for i in pick {
                        let u = &mut b.units[i];
                        u.ai = ai as u8;
                        u.ai_target = matches!(ai, 4 | 6).then_some((tx, ty));
                        u.ai_person = matches!(ai, 3 | 5).then_some(tp as usize);
                    }
                }
            }
            0x31 | 0x53 => {
                // 퇴장 op31 / 퇴각·사망 op53 (원작 FUN_004154ec·FUN_00417992 → FUN_004438a5):
                // 방식 0 = 인물 하나, 그 외 = 영역 (x1,y1)-(x2,y2) 의 진영. op53 마지막 인자 1 = 사망
                let (mode, pid, x1, y1, x2, y2, side) = (a(0), a(1), a(2), a(3), a(4), a(5), a(6));
                let pick: Vec<usize> = (0..b.units.len())
                    .filter(|&i| {
                        let u = &b.units[i];
                        if !u.visible || u.leave.is_some() {
                            return false;
                        }
                        if mode == 0 || mode == 0xFFFF {
                            return u.person == pid as usize;
                        }
                        let in_side = match side {
                            0 => u.side == battle::Side::Player,
                            1 => u.side == battle::Side::Ally,
                            2 | 3 | 5 => u.side == battle::Side::Enemy,
                            4 => u.side != battle::Side::Enemy,
                            _ => true,
                        };
                        in_side && (x1..=x2).contains(&u.x) && (y1..=y2).contains(&u.y)
                    })
                    .collect();
                let kind = if op == 0x31 { 1 } else if a(7) == 1 { 2 } else { 0 };
                let mut wait = 0;
                for u in pick {
                    if op == 0x53 {
                        b.units[u].hp = 0;
                    }
                    let (x, y) = (b.units[u].x, b.units[u].y);
                    if mode == 0 || mode == 0xFFFF {
                        b.focus(x, y);
                    }
                    wait = wait.max(b.start_leave(u, kind));
                }
                if wait > 0 {
                    self.wait = Wait::Ticks(wait);
                }
                if std::env::var("EKD5_DUMP").is_ok() {
                    self.log.push(format!("op{op:02x} {pid} kind{kind} wait{wait}"));
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
                // 동작 (원작 FUN_00418539 → FUN_0045976f): 인물, 동작 0..11 (0 = 보통). 소리: 6·7 SE07, 8 SE06, 그 외 SE08
                if let Some(u) = b.unit_of(a(0) as usize) {
                    let m = a(1);
                    // 걸리는 틱 (원작 대기 수): 9 = 2장 × 6번 × 8, 6·7 공격 4장, 8·10·12 짧은 반복, 그 외 한 장
                    // 연출 길이 (스크립트 대기). 그림은 원작처럼 다시 그릴 때까지 남음
                    // 원작 FUN_0045976f 의 대기 (단위 1/20 초): 6 4장×1, 7 2번×4장×2, 8 16장×2, 9 6장×8, 10 2+3, 11 9장×2 (레벨업 돌기), 12 5장×2
                    let dur_units: u32 = match m {
                        6 => 4,
                        7 => 16,
                        8 => 32,
                        9 => 48,
                        10 => 5,
                        11 => 18,
                        12 => 10,
                        _ => 0,
                    };
                    b.units[u].pose = None;
                    b.units[u].motion = (1..=12).contains(&m).then_some((m as u8, if m == 11 { b.anim_tick } else { u32::MAX }));
                    let se = match m {
                        0 => None,
                        6 | 7 => Some("SE07.WAV"),
                        8 => Some("SE06.WAV"),
                        _ => Some("SE08.WAV"),
                    };
                    if let Some(se) = se {
                        b.sounds.push(se.into());
                    }
                    // 앞뒤 대기 (인자 3·4, 동작 0·11 제외)
                    let extra = if m != 0 && m != 11 { (if a(2) != 0 { 8 } else { 0 }) + (if a(3) != 0 { 8 } else { 0 }) } else { 0 };
                    let w = dur_units + extra;
                    if w > 0 {
                        self.wait = Wait::Ticks((w * 3).div_ceil(2));
                    }
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
            // 승리 (원작 FUN_0044e6af): 아군이 아닌 승리 목표 장수가 퇴각했거나, 맵 위 적군이 없음
            0x42 => {
                if b.cheat && b.turn > 3 {
                    return true;
                }
                battle_won(b)
            }
            // 패배 (원작 FUN_0044e7a4): 아군 패배 목표 장수가 퇴각했거나, 맵 위 아군이 없음 (턴 제한은 엔진이 따로)
            0x43 => battle_lost(b),
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
            0x38 | 0x48 => false,
            _ => {
                self.note(op);
                false
            }
        }
    }

    /// 절 1 사건 검사: 머리 조건이 맞는 블록들을 차례로 실행하도록 줄 세움. 하나라도 있으면 true
    fn check_events(&mut self) -> bool {
        let Some(mut vm) = self.vm.take() else { return false };
        let mut found = Vec::new();
        if vm.sections() > 1 && !vm.running {
            if let Ok(blocks) = vm.blocks(1) {
                for (bi, (start, _)) in blocks.into_iter().enumerate() {
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
                    if ok || self.force_block == Some(bi) {
                        if self.force_block == Some(bi) {
                            self.force_block = None;
                        }
                        if self.autoplay {
                            self.log.push(format!("사건 블록 {bi}"));
                        }
                        found.push(vm.pc);
                    }
                }
            }
        }
        self.vm = Some(vm);
        if found.is_empty() {
            return false;
        }
        self.event_queue = found;
        self.event_queue.reverse();
        self.next_event();
        if let Some(b) = self.battle() {
            let p = std::mem::replace(&mut b.phase, battle::Phase::Script);
            if self.resume.is_none() {
                self.resume = Some(p);
            }
        }
        true
    }

    /// 줄 선 사건 본문 하나 시작. 없으면 false
    fn next_event(&mut self) -> bool {
        let Some(body) = self.event_queue.pop() else { return false };
        let Some(vm) = self.vm.as_mut() else { return false };
        vm.section = 1;
        vm.ends.clear();
        vm.pc = body;
        if vm.enter_body(true).is_ok() {
            vm.running = true;
            true
        } else {
            false
        }
    }

    /// 전투 진행 (스크립트가 쉬는 동안)
    fn update_battle(&mut self) {
        let running = self.vm.as_ref().is_some_and(|v| v.running);
        let inp0 = self.input.clone();
        if running || self.wait != Wait::None {
            // 대사·연출 중에도 화면은 끌어서 볼 수 있게 (탭은 대사 넘기기로 쓰이니 무시)
            if let Some(b) = self.battle() {
                if b.duel.is_none() {
                    b.drag_input(&inp0);
                }
            }
            return;
        }
        if let Some(b) = self.battle() {
            // 적·우군 차례, 차례 표시 중에도 끌기
            if matches!(b.phase, battle::Phase::Ai { .. } | battle::Phase::TurnBanner { .. } | battle::Phase::Attack { .. } | battle::Phase::Cast { .. }) {
                b.drag_input(&inp0);
            }
        }
        // 시험 자동 진행: 내 차례 대기면 AI 에게
        if self.autoplay {
            if let Some(b) = self.battle() {
                if b.phase == battle::Phase::PlayerIdle {
                    b.phase = battle::Phase::Ai { side: battle::Side::Player, idx: 0, stage: 0 };
                }
            }
        }
        // 쓰러진 부대: 사건(사망 대사 등)이 먼저 처리된 뒤 남은 부대만 퇴각. 조조군 주요 장수는 먼저 퇴각 대사
        if self.imsg.is_none() {
            match self.assets.get("IMSG.E5") {
                Asset::Pending => return,
                Asset::Ready(d) => self.imsg = Some(d),
                Asset::Missing => {}
            }
        }
        if let Some(pid) = self.battle().and_then(|b| b.shout.take()) {
            let r3 = self.battle().map_or(0, |b| b.rand3());
            if let Some(pages) = self.shout_dialog(pid, r3) {
                if self.autoplay {
                    self.log.push(format!("회심 외침 {}: {}", pages[0].name, pages[0].lines.join(" / ")));
                }
                if !self.battle_dialog(pages) {
                    if let Some(b) = self.battle() {
                        b.shout = Some(pid);
                    }
                }
                return;
            }
        }
        if let Some(pid) = self.battle().and_then(|b| b.retire_speaker()) {
            if let Some(pages) = self.retire_dialog(pid) {
                if self.autoplay {
                    self.log.push(format!("퇴각 대사 {}: {}", pages[0].name, pages[0].lines.join(" / ")));
                }
                if !self.battle_dialog(pages) {
                    if let Some(b) = self.battle() {
                        b.retire_said.retain(|&p| p != pid);
                    }
                }
                return;
            }
        }
        let w = self.battle().map_or(0, |b| b.retreat_fallen());
        if w > 0 {
            self.wait = Wait::Ticks(w);
            return;
        }
        let inp = self.input.clone();
        let autoplay = self.autoplay;
        // 명령 메뉴 결정 SE02 / 취소 SE01 (원작 FUN_0043f648, FUN_0045731d)
        if let Some(b) = self.battle() {
            use battle::Phase as P;
            if matches!(b.phase, P::Selected { .. } | P::Menu { .. } | P::Targeting { .. } | P::StratList { .. } | P::StratTarget { .. } | P::ToolList { .. } | P::ToolTarget { .. }) {
                let se = if inp.pressed & key::DECIDE != 0 { Some("SE02.WAV") } else if inp.pressed & key::CANCEL != 0 { Some("SE01.WAV") } else { None };
                if let Some(se) = se {
                    self.sounds.push(Sound::Effect(se.into()));
                }
            }
        }
        let casts: Vec<String> = match self.battle() {
            Some(b) => b.cast_log.drain(..).collect(),
            None => return,
        };
        if autoplay {
            for n in casts {
                self.log.push(match n.strip_prefix('@') {
                    Some(m) => m.to_string(),
                    None => format!("책략 {n}"),
                });
            }
        }
        // 차례 알림 큰 글자 준비 (원작 FUN_0044cc86: "아군 차례"/"적군 차례", 아군이면 "제 n턴" 또는 "마지막 차례")
        let need = self.battle().and_then(|b| match b.phase {
            battle::Phase::TurnBanner { t: 0, side } if b.banner.is_none() && side != battle::Side::Ally => {
                Some((side, b.turn, b.turn_limit, if b.cols * battle::CELL < 0x2d0 { 48 } else { 64 }))
            }
            _ => None,
        });
        if let Some((side, turn, lim, h)) = need {
            let first = if side == battle::Side::Enemy { "적군 차례" } else { "아군 차례" };
            let m1 = match self.text_mask(first, h) {
                Ok(m) => m,
                Err(Pending) => return,
            };
            let m2 = if side == battle::Side::Player {
                let s = if turn < lim { format!("제{turn:2}턴") } else { "마지막 차례".to_string() };
                match self.text_mask(&s, h) {
                    Ok(m) => Some(m),
                    Err(Pending) => return,
                }
            } else {
                None
            };
            if let Some(b) = self.battle() {
                b.banner = Some((m1, m2));
            }
        }
        let Some(b) = self.battle() else { return };
        if !b.started {
            return;
        }
        let mut check = false;
        match b.phase.clone() {
            battle::Phase::Script => {}
            battle::Phase::TurnBanner { t, side } => {
                if t == 0 {
                    // 차례가 바뀌면 원작은 맵 전체를 다시 그림 → 스크립트 동작 자세가 풀림
                    for u in b.units.iter_mut() {
                        u.motion = None;
                    }
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
                // 차례 표시 길이 (원작 FUN_0044cc86): 아군 10+20 단위, 적군 20 단위, 우군은 표시 없음
                let end = match side {
                    battle::Side::Player => 45,
                    battle::Side::Enemy => 30,
                    battle::Side::Ally => 1,
                };
                // 차례 표시가 끝날 무렵 상태 회복 처리 (연출이 있으면 재생)
                if t + 1 == end {
                    b.start_side(side);
                    if b.next_status_fx() {
                        b.phase = battle::Phase::TurnBanner { side, t: t + 1 };
                        self.wait = Wait::Fx;
                        return;
                    }
                }
                if t >= end {
                    b.banner = None;
                    if side == battle::Side::Player {
                        for u in &mut b.units {
                            // 혼란이면 행동 못함
                            u.acted = u.status & crate::strategy::status::CONFUSE != 0;
                        }
                        b.phase = battle::Phase::PlayerIdle;
                        let t = b.turn;
                        if self.autoplay {
                            if let Some(b) = self.battle() {
                                b.phase = battle::Phase::Ai { side: battle::Side::Player, idx: 0, stage: 0 };
                            }
                        }
                        self.log.push(format!("아군 차례 {t}턴"));
                        if std::env::var("EKD5_DUMP").is_ok() {
                            if let Some(b) = self.battle() {
                                let l: Vec<String> = b.units.iter().filter(|u| u.hp > 0).map(|u| format!("{:?}{}@{},{}{}a{}s{}j{}", u.side, u.person, u.x, u.y, if u.visible { "" } else { "h" }, u.ai, u.sprite_idx, u.job)).collect();
                                let l = l.join(" ");
                                self.log.push(l);
                            }
                        }
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
                        self.commit_battle();
                        self.stage += 1;
                        self.saved = None;
                    }
                    self.vm = None;
                    self.resume = None;
                    if !won {
                        self.game_over();
                    }
                    return;
                }
                b.phase = battle::Phase::Over { won, t: t + 1 };
            }
            _ => check = b.update_player(&inp),
        }
        if self.force_block.is_some() && matches!(self.battle().map(|b| &b.phase), Some(battle::Phase::PlayerIdle)) {
            check = true;
        }
        if check {
            // 사건이 연달아 생길 수 있으니 하나씩
            self.check_events();
        }
        // 승패가 날 만하면 사건 검사를 한 번 더 (승패는 스크립트 op42/op43 사건이 정함)
        let ended = self.battle().is_some_and(|b| {
            b.started && !matches!(b.phase, battle::Phase::Script | battle::Phase::Over { .. }) && (battle_won(b) || battle_lost(b) || b.turn > b.turn_limit)
        });
        if !ended {
            self.end_checked = false;
        } else if !self.end_checked {
            self.end_checked = true;
            if self.check_events() {
                return;
            }
        }
        // 시험(치트) 자동 진행 전용: 사건을 못 일으켜도 적이 없으면 승리 (원작에는 없음)
        if let Some(b) = self.battle() {
            if b.cheat && b.started && b.turn > 0 && !matches!(b.phase, battle::Phase::Script | battle::Phase::Over { .. }) && !b.units.iter().any(|u| u.side == battle::Side::Enemy && u.hp > 0) {
                b.phase = battle::Phase::Over { won: true, t: 0 };
            }
        }
        // 턴 제한 (원작 FUN_00450f0b: 마지막 턴이 끝나면 조조 퇴각 → 패배)
        if let Some(b) = self.battle() {
            if b.started && !matches!(b.phase, battle::Phase::Script | battle::Phase::Over { .. }) && b.turn > b.turn_limit {
                self.log.push("턴 제한 — 패배".into());
                let Some(b) = self.battle() else { return };
                b.phase = battle::Phase::Over { won: false, t: 0 };
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

    /// 대사 창 열기 (전투 중이면 말하는 부대에 화면을 맞추고 말풍선). 얼굴을 받는 중이면 false
    fn battle_dialog(&mut self, pages: Vec<story::Page>) -> bool {
        if let (Some(pid), Scene::Battle(b)) = (pages.first().and_then(|p| p.person), &mut self.scene) {
            if let Some(u) = b.unit_of(pid).filter(|&u| b.units[u].visible) {
                let (x, y) = (b.units[u].x, b.units[u].y);
                if x * battle::CELL < b.cam.0 + 24 || x * battle::CELL > b.cam.0 + 568 || y * battle::CELL < b.cam.1 + 24 || y * battle::CELL > b.cam.1 + 408 {
                    b.focus(x, y);
                }
                b.ui.speaker_y = Some(y * battle::CELL - b.cam.1 + 24);
                b.ui.speaker_cell = Some((x, y));
            } else {
                b.ui.speaker_y = None;
                b.ui.speaker_cell = None;
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
                return false;
            }
            self.wait = Wait::Dialog;
        }
        true
    }

    /// 퇴각 대사 (원작 FUN_0040c3fd: IMSG 650+인물, "\n" 은 줄바꿈)
    fn retire_dialog(&mut self, pid: usize) -> Option<Vec<story::Page>> {
        let m = self.imsg.as_ref()?;
        let r = m.get((650 + pid) * 216..(651 + pid) * 216)?;
        let e = r.iter().position(|&b| b == 0).unwrap_or(r.len());
        let t = encoding_rs::EUC_KR.decode(&r[..e]).0.to_string();
        let lines: Vec<String> = t.split("\\n").filter(|l| !l.is_empty()).map(|l| l.to_string()).collect();
        if lines.is_empty() {
            return None;
        }
        let name = self.persons.get(pid).map(|p| p.name.clone()).unwrap_or_default();
        Some(vec![story::Page { person: Some(pid), face: Some(self.face_of(pid)), name, lines }])
    }

    /// 회심 공격 외침 (원작 FUN_0040c4b2: 고유 대사 21명은 IMSG 700+순번, 그 외는 721 + 인물 +16 목소리 종류×3 + 난수 3)
    fn shout_dialog(&mut self, pid: usize, r3: usize) -> Option<Vec<story::Page>> {
        const OWN: [usize; 21] = [0, 1, 2, 3, 6, 7, 9, 13, 15, 16, 17, 33, 34, 36, 57, 84, 95, 108, 109, 119, 32];
        let k = match OWN.iter().rposition(|&p| p == pid) {
            Some(i) => 700 + i,
            None => 721 + self.person_data.get(pid * 32 + 16).copied().unwrap_or(0) as usize * 3 + r3,
        };
        let m = self.imsg.as_ref()?;
        let r = m.get(k * 216..(k + 1) * 216)?;
        let e = r.iter().position(|&b| b == 0).unwrap_or(r.len());
        let t = encoding_rs::EUC_KR.decode(&r[..e]).0.to_string();
        let lines: Vec<String> = t.split("\\n").filter(|l| !l.is_empty()).map(|l| l.trim_end().to_string()).collect();
        let name = self.persons.get(pid).map(|p| p.name.clone()).unwrap_or_default();
        Some(vec![story::Page { person: Some(pid), face: Some(self.face_of(pid)), name, lines }])
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
        // 원작 FUN_00407997
        let st = self.face_state;
        let normal = self.persons.get(pid).map_or(7, |p| p.face + 7);
        match pid {
            0 => {
                let k = match st & 0xF {
                    v @ 1..=3 => v as u16,
                    _ => 0,
                };
                if self.chapter < 2 { k } else { k + 4 }
            }
            1 if st & 0xF == 4 => 0xE0,
            1 if st & 0x80 == 0 => 0xDF,
            0x23 if st & 0x10 != 0 => 0xD6,
            8 if st & 0x20 != 0 => 0xE2,
            4 if st & 0x40 != 0 => 0xE3,
            _ => normal,
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
            Scene::Over { pic, t, sel } => {
                scr.clear(0);
                if let Some(p) = pic {
                    scr.blit(p, 0, 40, None);
                }
                scr.level = (*t / 2).min(16) as u8;
                if *t > 40 {
                    if let Some(f) = &self.font {
                        let hi = palette::nearest(&scr.pal, [255, 255, 255]);
                        let lo = palette::nearest(&scr.pal, [120, 100, 80]);
                        // 아래 검은 띠에 나란히
                        for (k, s) in ["이 전투 다시 하기", "제목으로"].iter().enumerate() {
                            let c = if *sel == k { hi } else { lo };
                            let x = if k == 0 { 120 } else { 420 };
                            story::draw_text(scr, f, x, 452, s, c);
                        }
                    }
                }
            }
            Scene::End { .. } => {
                scr.clear(0);
                scr.level = 16;
                if let Some(f) = &self.font {
                    story::draw_text(scr, f, 250, 220, "- 끝 -", 18);
                    story::draw_text(scr, f, 176, 260, "플레이해 주셔서 고맙습니다", 18);
                }
            }
        }
        self.render_camp();
        self.render_equip();
        self.render_sortie();
        let scr = &mut self.screen;
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
        let offs: Vec<String> = self
            .officers
            .values()
            .map(|o| {
                let it = |x: Option<(u8, u8)>| x.map_or("255.0".into(), |(i, l)| format!("{i}.{l}"));
                format!("{}:{}:{}:{}:{}:{}:{}:{}", o.person, o.level, o.exp, it(o.weapon), it(o.armor), o.acc.unwrap_or(255), o.gear_exp[0], o.gear_exp[1])
            })
            .collect();
        let inv: Vec<String> = self.inventory.iter().map(|(i, l)| format!("{i}.{l}")).collect();
        let spr: Vec<String> = self.sprite_over.iter().map(|(p, v)| format!("{p}.{v}")).collect();
        format!("3;{};{};{};{};{};{};{};{};{};{}", self.stage, flags.join(","), roster.join(","), offs.join(","), self.gold, inv.join(","), self.fame, self.face_state, self.chapter, spr.join(","))
    }

    /// 불러오기 (제목 화면에서 시작하면 그 단계부터)
    pub fn restore(&mut self, s: &str) -> bool {
        let f: Vec<&str> = s.split(';').collect();
        if f.len() < 4 || !matches!(f[0], "1" | "2" | "3") {
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
        self.pending_offs = f.get(4).map_or(Vec::new(), |o| {
            o.split(',').filter_map(|e| {
                let v: Vec<&str> = e.split(':').collect();
                let n = |i: usize| v.get(i).and_then(|x| x.parse::<u32>().ok());
                let eq = |i: usize| -> Option<(u8, u8)> {
                    let (a, b) = v.get(i)?.split_once('.')?;
                    let a: u8 = a.parse().ok()?;
                    (a != 255).then_some((a, b.parse().ok()?))
                };
                Some((n(0)? as usize, n(1)? as u8, n(2)? as u8, eq(3), eq(4), n(5).filter(|&x| x != 255).map(|x| x as u8), [n(6).unwrap_or(0) as u8, n(7).unwrap_or(0) as u8]))
            }).collect()
        });
        self.gold = f.get(5).and_then(|g| g.parse().ok()).unwrap_or(0);
        self.inventory = f.get(6).map_or(Vec::new(), |s| {
            s.split(',').filter_map(|e| {
                let (a, b) = e.split_once('.')?;
                Some((a.parse().ok()?, b.parse().ok()?))
            }).collect()
        });
        self.fame = f.get(7).and_then(|g| g.parse().ok()).unwrap_or(0);
        self.face_state = f.get(8).and_then(|g| g.parse().ok()).unwrap_or(0);
        self.chapter = f.get(9).and_then(|g| g.parse().ok()).unwrap_or(0);
        self.sprite_over = f.get(10).map_or(Default::default(), |s| {
            s.split(',').filter_map(|e| {
                let (a, b) = e.split_once('.')?;
                Some((a.parse().ok()?, b.parse().ok()?))
            }).collect()
        });
        true
    }

    /// 명성 게이지 색 (원작 FUN_00478905: MARK 8×8 무늬 0x00 채움, 0xC0 나머지, 0x40 강조) — 무늬 줄마다 RGB, 3×8×3 바이트
    pub fn gauge_colors(&mut self) -> Vec<u8> {
        let Asset::Ready(m) = self.assets.get("MARK.E5") else { return Vec::new() };
        let mut out = Vec::new();
        for o in [0x00usize, 0xC0, 0x40] {
            for y in 0..8 {
                let c = m.get(o + y * 8).copied().unwrap_or(0) as usize;
                out.extend(self.screen.pal[c]);
            }
        }
        out
    }

    /// 큰 글자 모양 (웹이 원작 굴림체 굵게로 그려 줌 "@txt/높이/글", 없으면 비트맵 글꼴)
    fn text_mask(&mut self, text: &str, h: i32) -> Result<duel::Mask, Pending> {
        let fb = |e: &Self| e.font.as_ref().map(|f| duel::fallback_mask(f, text, h)).unwrap_or(duel::Mask { w: 0, h: 0, px: Vec::new() });
        match self.assets.get(&format!("@txt/{h}/{text}")) {
            Asset::Pending => Err(Pending),
            Asset::Ready(d) => Ok(duel::parse_mask(&d).unwrap_or_else(|| fb(self))),
            Asset::Missing => Ok(fb(self)),
        }
    }

    /// 일기토 준비 (원작 FUN_0044b778): 두 장수의 그림·얼굴·이름, 배경 LOGO.E5[5]. 받는 중이면 Err
    fn make_duel(&mut self, who: [usize; 2]) -> Result<duel::Duel, Pending> {
        let mut fs = Vec::new();
        for &p in &who {
            let info = self.battle().and_then(|b| b.unit_of(p).map(|u| {
                let un = &b.units[u];
                (un.side, un.job, un.level, un.sprite_idx)
            }));
            let (side, job, level, sidx) = match info {
                Some(i) => i,
                None => {
                    let o = self.officer(p);
                    (battle::Side::Enemy, o.job, o.level as i32, battle::sprite_index(p, o.job, battle::Side::Enemy))
                }
            };
            let mov = self.packs.get(&mut self.assets, "UNIT_MOV.E5", sidx)?.unwrap_or_default();
            let atk = self.packs.get(&mut self.assets, "UNIT_ATK.E5", sidx)?.unwrap_or_default();
            let spc = self.packs.get(&mut self.assets, "UNIT_SPC.E5", sidx)?.unwrap_or_default();
            let face_no = self.face_of(p);
            let face = self.packs.get(&mut self.assets, "FACE.E5", face_no as usize)?.unwrap_or_default();
            let name = self.persons.get(p).map(|x| x.name.clone()).unwrap_or_default();
            let big_name = self.text_mask(&name, 64)?;
            // 병과 (원작 FUN_00407bc0): 2·4 = 말 탄 병종, 걷는 소리 (FUN_00441634 → FUN_004414fb)
            let g = if job < 0x27 { job as usize / 3 } else if job < 0x35 { job as usize - 0x1A } else { 0x1B };
            let step = match g {
                0 | 3 | 4 | 11 | 13 => Some(24),
                5 | 21 | 22 => Some(25),
                20 => None,
                _ => Some(23),
            }
            .map(|n| format!("SE{n:02}.WAV"));
            fs.push(duel::Fighter {
                person: p,
                name,
                pics: duel::Sprites {
                    mov: Rc::new(Pic::new(48, 48 * 11, mov)),
                    atk: Rc::new(Pic::new(64, 64 * 12, atk)),
                    spc: Rc::new(Pic::new(48, 48 * 5, spc)),
                },
                face: Pic::new(64, 80, face),
                side: match side {
                    battle::Side::Player => 0,
                    battle::Side::Ally => 1,
                    battle::Side::Enemy => 2,
                },
                cavalry: g == 2 || g == 4,
                step,
                level,
                big_name,
                shown: false,
                x: 0,
                right: false,
                last: None,
                dark: false,
            });
        }
        let bg = self.packs.get(&mut self.assets, "LOGO.E5", 5)?.map(|d| Rc::new(Pic::new(624, 384, d)));
        let vs = self.text_mask("VS", 64)?;
        let win = self.text_mask("승 부", 96)?;
        let f1 = fs.pop().unwrap();
        let f0 = fs.pop().unwrap();
        Ok(duel::Duel::new([f0, f1], bg, vs, win, self.screen.pal, self.music_now))
    }

    /// 일기토 끝 (원작 FUN_0044ba60): 지우고, 이긴 오른쪽 아군 장수 경험치 창 (FUN_00452964) 과 레벨업 (FUN_00408d42)
    fn end_duel(&mut self) {
        let jobs = self.jobs.clone();
        let Some(b) = self.battle() else { return };
        let Some(d) = b.duel.take() else { return };
        b.ui.speaker_y = None;
        b.ui.speaker_cell = None;
        let p = d.f[1].person;
        if d.exp > 0 && d.f[1].side == 0 {
            if let Some(u) = b.unit_of(p) {
                let un = &mut b.units[u];
                let (ex, lv) = (un.off.exp as i32, un.off.level);
                un.off.gain_exp(&jobs, d.exp as u32);
                let to = if un.off.level > lv { 100 } else { un.off.exp as i32 };
                let (hp, mp) = (un.hp, un.mp);
                b.dmg_win = Some(battle::DmgWin { u, hp: (hp, hp), mp: (mp, mp), ex: (ex, to), t: 0 });
                if b.units[u].off.level as i32 > b.units[u].level {
                    let nl = b.units[u].off.level as i32;
                    b.levelups.push_back((u, nl));
                }
                self.wait = Wait::DuelExp;
            }
        }
    }

    /// 게임 오버 (원작 FUN_0042e65c)
    fn game_over(&mut self) {
        self.sounds.push(Sound::Music(None));
        self.sounds.push(Sound::EffectStop);
        self.sounds.push(Sound::Effect("SE40.WAV".into()));
        self.scene = Scene::Over { pic: None, t: 0, sel: 0 };
        self.vm = None;
        self.wait = Wait::None;
    }

    /// 전투 메뉴 열기 (웹 버튼)
    pub fn open_menu(&mut self) {
        if self.camp.is_some() {
            self.camp_menu = true;
        } else if let Scene::Battle(b) = &mut self.scene {
            b.open_menu = true;
        }
    }

    /// 대사·자막을 넘기길 기다리는 중인가 (빨리 감기용)
    pub fn waiting_text(&self) -> bool {
        matches!(self.wait, Wait::Dialog | Wait::Key)
    }

    /// 웹: 동영상 끝
    pub fn movie_done(&mut self) {
        if self.wait == Wait::Movie {
            self.wait = Wait::None;
        }
    }

    pub fn take_sounds(&mut self) -> Vec<Sound> {
        std::mem::take(&mut self.sounds)
    }
}

/// 맵 위에서 물러난 부대 (원작 부대 상태 3·4)
fn fallen(u: &battle::Unit) -> bool {
    u.gone || (u.visible && u.hp <= 0)
}

/// 원작 FUN_0044e6af: 승리 목표 장수(아군 아님)가 물러났거나 맵 위 적군이 없음
fn battle_won(b: &battle::Battle) -> bool {
    if let Some(p) = b.win_target {
        if b.units.iter().any(|u| u.side != battle::Side::Player && u.person == p && fallen(u)) {
            return true;
        }
    }
    !b.units.iter().any(|u| u.side == battle::Side::Enemy && u.visible && u.hp > 0)
}

/// 원작 FUN_0044e7a4: 패배 목표 장수(아군)가 물러났거나 아군이 모두 물러남 (아직 안 나온 부대 = 상태 1 은 남은 것으로 셈)
fn battle_lost(b: &battle::Battle) -> bool {
    if let Some(p) = b.lose_target {
        if b.units.iter().any(|u| u.side == battle::Side::Player && u.person == p && fallen(u)) {
            return true;
        }
    }
    b.units.iter().any(|u| u.side == battle::Side::Player) && !b.units.iter().any(|u| u.side == battle::Side::Player && !fallen(u))
}
