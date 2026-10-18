//! 일기토 (원작 일기토 객체 0x4be3f8, FUN_00445xxx ~ FUN_0044bxxx 를 그대로 옮김).
//! 창 624×384 (FUN_0044b778: 화면 가운데), 배경 LOGO.E5[5]. 장수 그림은 원작 부대 그림을 2배 (FUN_00433026, 색 보간)로 키워
//! 가운데 띠 (창 y 128..256) 에 그림. 원작은 "띠 배경 다시 깔기 → 그림 몇 장 → 대기" 를 차례로 하므로 여기서는 같은 순서를
//! 단계 목록으로 만들어 재생함. 대기 단위는 원작 1/20 초 (GetTickCount/50).
//! 명령: op68 시작 (FUN_0044bb1f·FUN_0044a874), op60 등장+대사 (FUN_0044b600), op63 대사 (FUN_0044532f), op64 동작 16가지
//! (FUN_0044a6a4), op65 공격 5가지 (FUN_0044a514), op66 반격 3가지 (FUN_0044a5fb), op61 "승 부" (FUN_0044b175),
//! op62 승패 (FUN_0044b25a), op5f 끝 (FUN_0044ba60).

use super::screen::{Pic, Screen};
use super::story::{draw_text, frame_box};
use crate::font::Font;
use crate::palette::{nearest, Palette};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

/// 창 자리 (화면) 와 크기
pub const WX: i32 = 8;
pub const WY: i32 = 48;
pub const WW: i32 = 624;
pub const WH: i32 = 384;
/// 장수 그림 y (창 안, 96 크기; 128 크기는 −16)
const FY: i32 = 144;
/// 얼굴 자리 (창 안): 0 왼쪽 위, 1 오른쪽 아래
const FACE: [(i32, i32); 2] = [(24, 24), (WW - 88, WH - 104)];

#[derive(Clone)]
pub struct Sprites {
    pub mov: Rc<Pic>,
    pub atk: Rc<Pic>,
    pub spc: Rc<Pic>,
}

/// 큰 글자 모양 (호스트가 원작 굴림체 굵게로 그려 줌, 없으면 비트맵 글꼴을 키움): 1 = 글자
#[derive(Clone)]
pub struct Mask {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u8>,
}

pub struct Fighter {
    pub person: usize,
    pub name: String,
    pub pics: Sprites,
    pub face: Pic,
    /// 원작 FUN_00441d7b 진영 (0 아군 1 우군 2 적)
    pub side: u8,
    /// 말 탄 병종 (원작 FUN_00442203: 병과 2·4)
    pub cavalry: bool,
    /// 걷는 소리 (원작 FUN_00441634)
    pub step: Option<String>,
    pub level: i32,
    pub big_name: Mask,
    pub shown: bool,
    /// 띠 안 x (원작 +0x20 − 창 x)
    pub x: i32,
    /// 오른쪽을 봄 (원작 +0xe == 1)
    pub right: bool,
    /// 마지막으로 그린 그림 (원작 0x1c000/0x20000 보관, FUN_00445c6d 가 다시 그림)
    pub last: Option<Rc<Pic>>,
    pub dark: bool,
}

enum Step {
    /// 띠를 다시 깔고 그림들 (창 안 좌표)
    Frame(Vec<(Rc<Pic>, i32, i32)>),
    /// 대기 (원작 단위), 눌러서 넘길 수 있나
    Wait(u32, bool),
    Sound(String),
    Music(Option<i32>),
    Win(Win),
}

#[derive(Clone)]
enum Win {
    /// 0 처음 창 (무늬 창 + 그림자), 1 배경 LOGO.E5[5]
    Stage(u8),
    Face(usize),
    BigName(usize),
    Vs,
    SmallNames,
    /// 대사 한 쪽 (쪽, 줄들) / 지우기
    Dialog(usize, Vec<String>),
    ClearDialog(usize),
    /// 창 다시 깔기 (대사 지움)
    Restore,
    BigText(bool),
    Dark(usize),
}

pub struct Duel {
    pub f: [Fighter; 2],
    steps: VecDeque<Step>,
    wait: u32,
    wait_skip: bool,
    band: Vec<(Rc<Pic>, i32, i32)>,
    stage: Option<u8>,
    faces: [bool; 2],
    big_names: [bool; 2],
    vs: bool,
    small_names: bool,
    dialog: [Option<Vec<String>>; 2],
    big_text: bool,
    bg: Option<Rc<Pic>>,
    vs_mask: Mask,
    win_mask: Mask,
    pal: Palette,
    rgb4: HashMap<u16, u8>,
    /// 원작 +0x84: 이긴 아군 장수 경험치 (FUN_0044b25a)
    pub exp: i32,
    /// 끝난 뒤 되돌릴 음악 (원작 +0x64)
    pub music_back: Option<i32>,
    /// 메시지 속도 (원작 설정 0..2): 대사 쪽마다 5/12/20 × 2 단위
    pub speed: u8,
    pub out: Vec<Out>,
}

pub enum Out {
    Sound(String),
    Music(Option<i32>),
}

/// 원작 1/20 초 → 30틱
fn units(n: u32) -> u32 {
    (n * 3).div_ceil(2)
}

impl Duel {
    pub fn new(f: [Fighter; 2], bg: Option<Rc<Pic>>, vs_mask: Mask, win_mask: Mask, pal: Palette, music_back: Option<i32>) -> Self {
        let mut d = Duel {
            f,
            steps: VecDeque::new(),
            wait: 0,
            wait_skip: false,
            band: Vec::new(),
            stage: None,
            faces: [false; 2],
            big_names: [false; 2],
            vs: false,
            small_names: false,
            dialog: [None, None],
            big_text: false,
            bg,
            vs_mask,
            win_mask,
            pal,
            rgb4: HashMap::new(),
            exp: 0,
            music_back,
            speed: 1,
            out: Vec::new(),
        };
        // 자리 (FUN_0044b778): 왼쪽 x = 가운데 − 144, 오른쪽 = 그 + 192. 왼쪽 장수는 오른쪽을 봄
        d.f[0].x = WW / 2 - 144;
        d.f[1].x = WW / 2 - 144 + 192;
        d.f[0].right = true;
        d.f[1].right = false;
        d.intro();
        d
    }

    /// 단계가 남았나 (스크립트 대기)
    pub fn busy(&self) -> bool {
        !self.steps.is_empty() || self.wait > 0
    }

    /// 한 틱. skip = 눌렀음 (넘길 수 있는 대기만)
    pub fn update(&mut self, skip: bool) {
        if self.wait > 0 {
            if skip && self.wait_skip {
                self.wait = 0;
            } else {
                self.wait -= 1;
                return;
            }
        }
        while let Some(s) = self.steps.pop_front() {
            match s {
                Step::Frame(v) => self.band = v,
                Step::Wait(n, sk) => {
                    self.wait = units(n);
                    self.wait_skip = sk;
                    if self.wait > 0 {
                        return;
                    }
                }
                Step::Sound(n) => self.out.push(Out::Sound(n)),
                Step::Music(m) => self.out.push(Out::Music(m)),
                Step::Win(w) => self.apply(w),
            }
        }
    }

    fn apply(&mut self, w: Win) {
        match w {
            Win::Stage(s) => {
                self.stage = Some(s);
                if s == 1 {
                    // 배경이 큰 이름·VS 를 덮음
                    self.big_names = [false; 2];
                    self.vs = false;
                    self.faces = [true; 2];
                }
            }
            Win::Face(i) => self.faces[i] = true,
            Win::BigName(i) => self.big_names[i] = true,
            Win::Vs => self.vs = true,
            Win::SmallNames => self.small_names = true,
            Win::Dialog(i, l) => self.dialog[i] = Some(l),
            Win::ClearDialog(i) => self.dialog[i] = None,
            Win::Restore => self.dialog = [None, None],
            Win::BigText(on) => self.big_text = on,
            Win::Dark(i) => self.f[i].dark = true,
        }
    }

    // ---------- 그림 ----------

    fn rgb4(&mut self, c: [u8; 3]) -> u8 {
        let k = ((c[0] as u16) << 8) | ((c[1] as u16) << 4) | c[2] as u16;
        if let Some(&v) = self.rgb4.get(&k) {
            return v;
        }
        let v = nearest(&self.pal, [c[0] * 17, c[1] * 17, c[2] * 17]).max(1);
        self.rgb4.insert(k, v);
        v
    }

    fn c4(&self, i: u8) -> [u8; 3] {
        let p = self.pal[i as usize];
        [p[0] / 16, p[1] / 16, p[2] / 16]
    }

    fn avg(&mut self, a: u8, b: u8) -> u8 {
        let (x, y) = (self.c4(a), self.c4(b));
        self.rgb4([((x[0] + y[0]) / 2).min(15), ((x[1] + y[1]) / 2).min(15), ((x[2] + y[2]) / 2).min(15)])
    }

    /// 2배 키우기 (원작 FUN_00433026: 이웃과 4비트 색 평균으로 메움)
    fn scale2(&mut self, src: &[u8], w: usize, h: usize) -> Rc<Pic> {
        let mut d = vec![0u8; w * h * 4];
        for yy in 0..h {
            for xx in 0..w {
                let p = src[yy * w + xx];
                let i1 = yy * w * 4 + xx * 2;
                let i3 = i1 + 1;
                let i2 = yy * w * 4 + w * 2 + xx * 2;
                let i4 = i2 + 1;
                d[i1] = p;
                d[i2] = p;
                d[i3] = p;
                d[i4] = p;
                if xx != w - 1 && p != 0 && src[yy * w + xx + 1] != 0 {
                    let v = self.avg(src[yy * w + xx + 1], p);
                    d[i4] = v;
                    d[i3] = v;
                }
                if yy != h - 1 && p != 0 && src[(yy + 1) * w + xx] != 0 {
                    d[i2] = self.avg(src[(yy + 1) * w + xx], p);
                }
                if d[i1] != d[i3] || d[i1] != d[i2] {
                    d[i4] = self.avg(d[i3], d[i2]);
                    if yy != h - 1 && xx != w - 1 && src[(yy + 1) * w + xx + 1] != 0 {
                        d[i4] = self.avg(src[(yy + 1) * w + xx + 1], d[i4]);
                    }
                }
            }
        }
        Rc::new(Pic::new(w * 2, h * 2, d))
    }

    fn frame_of(pic: &Pic, size: usize, k: usize, flip: bool) -> Vec<u8> {
        let mut v = vec![0u8; size * size];
        for y in 0..size {
            for x in 0..size {
                let sx = if flip { size - 1 - x } else { x };
                v[y * size + x] = pic.px.get(k * size * size + y * size + sx).copied().unwrap_or(0);
            }
        }
        v
    }

    /// 이동 그림 k (원작 DAT_004b52f8: 부대 방향대로 뒤집힌 그림 — 오른쪽을 보면 뒤집음) · extra = 한 번 더 뒤집기
    fn mov(&mut self, s: usize, k: usize, extra: bool) -> Rc<Pic> {
        let pic = self.f[s].pics.mov.clone();
        let src = Self::frame_of(&pic, 48, k, self.f[s].right != extra);
        self.scale2(&src, 48, 48)
    }

    fn atk(&mut self, s: usize, k: usize, flip: bool) -> Rc<Pic> {
        let pic = self.f[s].pics.atk.clone();
        let src = Self::frame_of(&pic, 64, k, flip);
        self.scale2(&src, 64, 64)
    }

    fn spc(&mut self, s: usize, k: usize, flip: bool) -> Rc<Pic> {
        let pic = self.f[s].pics.spc.clone();
        let src = Self::frame_of(&pic, 48, k, flip);
        self.scale2(&src, 48, 48)
    }

    /// 밝게 (원작 FUN_00432970: 4비트 채널 + n, 15 까지)
    fn bright(&mut self, p: &Pic, n: u8) -> Rc<Pic> {
        let mut px = Vec::with_capacity(p.px.len());
        for &c in &p.px {
            px.push(if c == 0 {
                0
            } else {
                let q = self.c4(c);
                self.rgb4([(q[0] + n).min(15), (q[1] + n).min(15), (q[2] + n).min(15)])
            });
        }
        Rc::new(Pic::new(p.w, p.h, px))
    }

    /// 흑백 (원작 FUN_00432e2a)
    fn gray(&mut self, p: &Pic) -> Rc<Pic> {
        let mut px = Vec::with_capacity(p.px.len());
        for &c in &p.px {
            px.push(if c == 0 {
                0
            } else {
                let q = self.c4(c);
                let g = ((q[0] as u32 + q[1] as u32 + q[2] as u32) / 3) as u8;
                self.rgb4([g, g, g])
            });
        }
        Rc::new(Pic::new(p.w, p.h, px))
    }

    // ---------- 단계 만들기 ----------

    fn sound(&mut self, n: u32) {
        self.steps.push_back(Step::Sound(format!("SE{n:02}.WAV")));
    }

    fn wait(&mut self, n: u32) {
        self.steps.push_back(Step::Wait(n, false));
    }

    /// 원작 FUN_00445d9d 의 x 자리 (창 밖으로 안 나가게)
    fn clamp(x: i32, size: i32) -> i32 {
        let mut x = x.max(0);
        if WW <= x + size {
            x = WW - size - 1;
        }
        x
    }

    /// 띠 그리기 한 번 (FUN_00420b8b..FUN_00420ba6). other = FUN_00445c6d 로 다시 그릴 장수, draws = (장수, 그림, x, y)
    fn frame(&mut self, other: Option<usize>, draws: Vec<(usize, Rc<Pic>, i32, i32)>) {
        let mut v = Vec::new();
        if let Some(o) = other {
            if let (true, Some(p)) = (self.f[o].shown, self.f[o].last.clone()) {
                let off = if p.w == 96 { 0 } else { -16 };
                v.push((p, self.f[o].x + off, FY + off));
            }
        }
        for (s, p, x, y) in draws {
            let x = Self::clamp(x, p.w as i32);
            self.f[s].last = Some(p.clone());
            v.push((p, x, y));
        }
        self.steps.push_back(Step::Frame(v));
    }

    /// 두 장수를 마지막 그림대로 다시 (FUN_00445c6d 둘)
    fn redraw_band(&mut self) {
        let mut v = Vec::new();
        for o in [1, 0] {
            if let (true, Some(p)) = (self.f[o].shown, self.f[o].last.clone()) {
                let off = if p.w == 96 { 0 } else { -16 };
                v.push((p, self.f[o].x + off, FY + off));
            }
        }
        self.steps.push_back(Step::Frame(v));
    }

    /// 시작 화면 (원작 FUN_0044a874): 무늬 창 → 음악 10 → 왼쪽 얼굴·이름 → VS → 오른쪽 얼굴·이름 → 배경
    fn intro(&mut self) {
        self.steps.push_back(Step::Win(Win::Stage(0)));
        self.steps.push_back(Step::Music(Some(10)));
        self.wait(10);
        self.steps.push_back(Step::Win(Win::Face(0)));
        self.steps.push_back(Step::Win(Win::BigName(0)));
        self.wait(20);
        self.steps.push_back(Step::Win(Win::Vs));
        self.wait(20);
        self.steps.push_back(Step::Win(Win::Face(1)));
        self.steps.push_back(Step::Win(Win::BigName(1)));
        self.wait(0x4b);
        self.steps.push_back(Step::Win(Win::Stage(1)));
        self.steps.push_back(Step::Win(Win::SmallNames));
        self.steps.push_back(Step::Frame(Vec::new()));
    }

    /// op60 등장 (원작 FUN_0044b600): 서기 그림이 하얗게 나타남 (밝기 15→0, 16번), 동작, 대사
    pub fn appear(&mut self, s: usize, lines: Vec<String>, motion: u8) {
        self.f[s].shown = true;
        let p = self.mov(s, 8, false);
        let o = 1 - s;
        for n in (0..16u8).rev() {
            let b = self.bright(&p, n);
            let x = self.f[s].x;
            self.frame(Some(o), vec![(s, b, x, FY)]);
            self.wait(1);
        }
        self.wait(5);
        if motion != 0 {
            self.motion(s, motion);
        }
        self.say(s, lines, true);
    }

    /// op63 대사 (원작 FUN_0044532f): 흰 칸에 세 줄씩. timed = 쪽마다 기다림 (메시지 속도), 아니면 바로 다음
    pub fn say(&mut self, s: usize, lines: Vec<String>, timed: bool) {
        if lines.is_empty() {
            self.steps.push_back(Step::Win(Win::ClearDialog(s)));
            return;
        }
        let w = [5, 12, 20][self.speed.min(2) as usize] * 2;
        for page in lines.chunks(3) {
            self.steps.push_back(Step::Win(Win::Dialog(s, page.to_vec())));
            if timed {
                self.steps.push_back(Step::Wait(w, true));
            }
        }
    }

    fn here(&self, s: usize) -> i32 {
        self.f[s].x
    }

    /// op64 동작 (원작 FUN_0044a6a4)
    pub fn motion(&mut self, s: usize, m: u8) {
        let o = 1 - s;
        let dir = if self.f[s].right { 1 } else { -1 };
        match m {
            // 서기 (FUN_00445e66)
            0 => {
                let p = self.mov(s, 8, false);
                self.sound(8);
                let x = self.here(s);
                self.frame(Some(o), vec![(s, p, x, FY)]);
            }
            // 돌아서기 (FUN_00446e4f)
            1 => {
                self.f[s].right = !self.f[s].right;
                let p = self.mov(s, 8, false);
                self.sound(6);
                let x = self.here(s);
                self.frame(Some(o), vec![(s, p, x, FY)]);
            }
            // 빨리 나아가기 (FUN_00446bfc) / 천천히 (FUN_00446ab9) / 물러서기 (FUN_00446f2d)
            2..=4 => {
                let walk = [self.mov(s, 4, false), self.mov(s, 5, false)];
                let dx = match m {
                    2 => 24 * dir,
                    3 => 8 * dir,
                    _ => -8 * dir,
                };
                if m == 3 {
                    self.step_sound(s);
                }
                if m == 4 {
                    self.sound(6);
                }
                for c in 0..4 {
                    if m == 2 {
                        self.step_sound(s);
                    }
                    self.f[s].x += dx;
                    let x = self.here(s);
                    self.frame(Some(o), vec![(s, walk[c >> 1].clone(), x, FY)]);
                    self.wait(1);
                }
            }
            // 이김 (FUN_00445f7f) · 막기 (FUN_004460d1) · 맞음 (FUN_00446271)
            5..=7 => {
                let r = self.f[s].right;
                let p = match m {
                    5 => self.spc(s, 4, false),
                    6 => self.spc(s, 2, r),
                    _ => self.spc(s, 3, false),
                };
                self.sound(8);
                let x = self.here(s);
                self.frame(Some(o), vec![(s, p, x, FY)]);
            }
            // 공격 자세 (FUN_0044646a)
            8 => {
                let r = self.f[s].right;
                let p = self.atk(s, 8, r);
                self.sound(8);
                let x = self.here(s);
                self.frame(Some(o), vec![(s, p, x - 16, FY - 16)]);
            }
            // 휘두르기 한 번 (FUN_00446616) · 두 번 (FUN_00446851)
            9 | 10 => {
                let a = self.swing(s);
                for _ in 0..(if m == 9 { 1 } else { 2 }) {
                    self.sound(7);
                    for p in &a {
                        let x = self.here(s);
                        self.frame(Some(o), vec![(s, p.clone(), x - 16, FY - 16)]);
                        self.wait(if m == 9 { 1 } else { 2 });
                    }
                }
            }
            // 지침 (FUN_004463c3)
            11 => {
                let p = self.mov(s, 9, false);
                self.sound(8);
                let x = self.here(s);
                self.frame(Some(o), vec![(s, p, x, FY)]);
            }
            // 헐떡임 (FUN_00446d3f)
            12 => {
                let a = [self.mov(s, 9, false), self.mov(s, 10, false)];
                self.sound(8);
                for c in 0..6 {
                    let x = self.here(s);
                    self.frame(Some(o), vec![(s, a[c % 2].clone(), x, FY)]);
                    self.wait(8);
                }
            }
            // 사라짐 (FUN_00445f0d)
            13 => {
                let p = Rc::new(Pic::new(96, 96, Vec::new()));
                let x = self.here(s);
                self.frame(Some(o), vec![(s, p, x, FY)]);
            }
            14 => self.dance(s),
            15 => self.look_around(s),
            _ => {}
        }
    }

    fn step_sound(&mut self, s: usize) {
        if let Some(n) = self.f[s].step.clone() {
            self.steps.push_back(Step::Sound(n));
        }
    }

    fn put(&mut self, s: usize, p: Rc<Pic>) {
        let x = self.f[s].x;
        let off = if p.w == 96 { 0 } else { -16 };
        self.frame(Some(1 - s), vec![(s, p, x + off, FY + off)]);
    }

    /// 동작 14 (원작 FUN_0044706a): 음악 21, 돌며 무기를 휘두르는 긴 연출
    fn dance(&mut self, s: usize) {
        let sp = [self.spc(s, 0, false), self.spc(s, 2, false), self.spc(s, 1, false), self.spc(s, 2, true)];
        let win = self.spc(s, 4, false);
        self.steps.push_back(Step::Music(Some(0x15)));
        for i in 0..17 {
            self.put(s, sp[i % 4].clone());
            self.wait(4);
        }
        self.wait(4);
        self.put(s, win.clone());
        self.wait(40);
        for i in 0..9 {
            self.f[s].x -= 8;
            self.put(s, sp[i % 4].clone());
            self.wait(4);
        }
        self.wait(4);
        self.put(s, win.clone());
        self.wait(20);
        for i in 0..9 {
            self.f[s].x += 8;
            self.put(s, sp[i % 4].clone());
            self.wait(4);
        }
        self.wait(4);
        self.put(s, win);
        self.wait(40);
        let walk = [self.mov(s, 4, false), self.mov(s, 5, false)];
        let tired = self.mov(s, 9, false);
        for i in 0..8 {
            self.f[s].x += 16;
            self.put(s, walk[i % 2].clone());
            self.wait(2);
        }
        self.put(s, tired.clone());
        self.wait(40);
        for _ in 0..4 {
            self.f[s].x -= 8;
            self.put(s, tired.clone());
            self.wait(8);
        }
        self.wait(40);
        let at = [self.atk(s, 3, false), self.atk(s, 11, false), self.atk(s, 7, false), self.atk(s, 11, true)];
        for i in 0..20 {
            self.f[s].x -= 4;
            self.put(s, at[i % 4].clone());
            self.wait(2);
        }
        self.wait(40);
        for i in 0..18 {
            self.f[s].x += 4;
            self.put(s, at[i % 4].clone());
            self.wait(2);
        }
        self.wait(40);
    }

    /// 동작 15 (원작 FUN_00447ac3): 음악 20, 앞뒤·좌우를 번갈아 봄
    fn look_around(&mut self, s: usize) {
        self.steps.push_back(Step::Music(Some(0x14)));
        let pairs = [
            [self.mov(s, 0, false), self.mov(s, 1, false)],
            [self.mov(s, 2, false), self.mov(s, 3, false)],
            [self.mov(s, 4, false), self.mov(s, 5, false)],
            [self.mov(s, 4, true), self.mov(s, 5, true)],
        ];
        self.wait(20);
        for c in 0..6 {
            let pr = if c < 4 { pairs[c % 2].clone() } else { pairs[2 + c % 2].clone() };
            for k in 0..6 {
                self.put(s, pr[k % 2].clone());
                self.wait(if k == 0 || k > 2 { 4 } else { 8 });
            }
            self.wait(10);
        }
    }

    /// 휘두르는 공격 그림 4장 (원작 FUN_00447e01: 옆 공격 8..11, 오른쪽을 보면 뒤집음)
    fn swing(&mut self, a: usize) -> Vec<Rc<Pic>> {
        let r = self.f[a].right;
        (0..4).map(|k| self.atk(a, 8 + k, r)).collect()
    }

    /// 공격 준비 (원작 FUN_00447f94): 공격 그림 0·1. power = 번쩍이며 모음 (SE33), 말 탄 병종은 SE37
    fn windup(&mut self, a: usize, power: bool, at: &[Rc<Pic>]) {
        let d = 1 - a;
        let x = self.f[a].x - 16;
        let cav = self.f[a].cavalry;
        let flash = |me: &mut Self, p: &Rc<Pic>| {
            me.sound(0x21);
            for n in 0..12u8 {
                let b = me.bright(p, n);
                me.frame(Some(d), vec![(a, b, x, FY - 16)]);
                me.wait(1);
            }
        };
        // 첫 장
        if !cav {
            if !power {
                self.frame(Some(d), vec![(a, at[0].clone(), x, FY - 16)]);
                self.wait(2);
            } else {
                flash(self, &at[0]);
            }
        } else {
            self.frame(Some(d), vec![(a, at[0].clone(), x, FY - 16)]);
            self.sound(0x25);
            self.wait(1);
        }
        // 둘째 장
        if !cav {
            self.frame(Some(d), vec![(a, at[1].clone(), x, FY - 16)]);
            self.wait(1);
        } else if !power {
            self.frame(Some(d), vec![(a, at[1].clone(), x, FY - 16)]);
            self.wait(8);
        } else {
            flash(self, &at[1]);
        }
    }

    /// op65 공격 (원작 FUN_0044a514): 0 맞음, 1 막음, 2 막고 밀림, 3 밀려 걸음, 4 꿰뚫고 지나감. power = 모아 치기
    pub fn attack(&mut self, a: usize, kind: u8, power: bool) {
        let d = 1 - a;
        let at = self.swing(a);
        let ar = self.f[a].right;
        let dir = if ar { 1 } else { -1 };
        match kind {
            0..=2 => {
                // 맞는 그림: 0 맞음 SPC3, 1·2 막기 SPC2 (공격자가 오른쪽을 안 보면 뒤집음)
                let dp = if kind == 0 { self.spc(d, 3, false) } else { self.spc(d, 2, !ar) };
                let flash = self.bright(&dp, 12);
                let k14 = 32 * dir;
                self.windup(a, power, &at);
                for i in 0..6 {
                    if kind == 2 {
                        if i == 1 || i == 2 {
                            self.f[d].x += k14;
                        } else if i == 3 || i == 4 {
                            self.f[d].x += k14 / 2;
                        }
                    }
                    let (dx, ax) = (self.f[d].x, self.f[a].x - 16);
                    let draws = if i < 2 {
                        match kind {
                            0 => {
                                self.sound(0x24);
                                vec![(d, flash.clone(), dx + k14, FY), (a, at[i + 2].clone(), ax, FY - 16)]
                            }
                            1 => {
                                self.sound(0x1f);
                                vec![(d, dp.clone(), dx + k14, FY), (a, at[i + 2].clone(), ax, FY - 16)]
                            }
                            _ => {
                                self.sound(0x1f);
                                vec![(d, flash.clone(), dx, FY), (a, at[i + 2].clone(), ax, FY - 16)]
                            }
                        }
                    } else {
                        vec![(d, dp.clone(), dx, FY), (a, at[3].clone(), ax, FY - 16)]
                    };
                    self.frame(None, draws);
                    self.wait(1);
                }
            }
            3 => {
                // FUN_00448b6b: 맞는 쪽이 걸음 그림으로 밀려남
                let w = [self.mov(d, 4, false), self.mov(d, 5, false)];
                let (k14, k18) = (40 * dir, 16 * dir);
                self.windup(a, power, &at);
                for i in 0..6 {
                    let ax = self.f[a].x - 16;
                    let draws = match i {
                        0 | 1 => {
                            self.sound(7);
                            self.f[d].x += k14;
                            vec![(d, w[0].clone(), self.f[d].x + k18, FY), (a, at[i + 2].clone(), ax, FY - 16)]
                        }
                        2 => {
                            self.f[d].x += k18;
                            vec![(d, w[0].clone(), self.f[d].x, FY), (a, at[3].clone(), ax, FY - 16)]
                        }
                        _ => vec![(d, w[1].clone(), self.f[d].x, FY), (a, at[3].clone(), ax, FY - 16)],
                    };
                    self.frame(None, draws);
                    self.wait(1);
                }
            }
            _ => {
                // FUN_00448f36: 맞는 쪽이 공격자와 같은 쪽을 보게 돌고, 공격자가 지나간 뒤 192 떨어진 곳에 나타남
                self.f[d].right = ar;
                let dp = self.mov(d, 8, false);
                self.windup(a, power, &at);
                for i in 0..6 {
                    let ax = self.f[a].x - 16;
                    let draws = if i < 2 {
                        self.sound(7);
                        vec![(a, at[i + 2].clone(), ax, FY - 16)]
                    } else if i < 5 {
                        vec![(a, at[3].clone(), ax, FY - 16)]
                    } else {
                        self.f[d].x += 192 * dir;
                        vec![(a, at[3].clone(), ax, FY - 16), (d, dp.clone(), self.f[d].x, FY)]
                    };
                    self.frame(None, draws);
                    self.wait(1);
                }
            }
        }
    }

    /// op66 반격 (원작 FUN_0044a5fb): 0 뛰어들어 치기, 1 잔상 돌진, 2 서로 달려들어 부딪침. guard = 막음
    pub fn counter(&mut self, a: usize, kind: u8, guard: bool) {
        let d = 1 - a;
        let ar = self.f[a].right;
        let dir = if ar { 1 } else { -1 };
        match kind {
            0 => {
                // FUN_0044929f
                let at = self.swing(a);
                let dp = if guard { self.spc(d, 2, !ar) } else { self.spc(d, 3, false) };
                let flash = self.bright(&dp, 12);
                let k14 = 32 * dir;
                self.windup(a, false, &at);
                for i in 0..6 {
                    if i == 0 {
                        self.f[a].x += k14;
                        self.f[d].x += k14;
                    }
                    let (ax, dx) = (self.f[a].x - 16, self.f[d].x);
                    let draws = if i < 2 {
                        self.sound(if guard { 0x1f } else { 0x24 });
                        vec![(d, flash.clone(), dx + k14, FY), (a, at[i + 2].clone(), ax, FY - 16)]
                    } else {
                        vec![(d, dp.clone(), dx, FY), (a, at[3].clone(), ax, FY - 16)]
                    };
                    self.frame(None, draws);
                    self.wait(1);
                }
            }
            1 => {
                // FUN_0044973e: 잔상을 남기며 64씩 돌진
                let at = self.swing(a);
                let dp = if guard { self.spc(d, 2, !ar) } else { self.spc(d, 3, false) };
                let flash = self.bright(&dp, 12);
                let (k14, k2, y) = (32 * dir, 64 * dir, FY - 16);
                for i in 0..8 {
                    let dl = self.f[d].last.clone();
                    let dx0 = self.f[d].x;
                    let mut v: Vec<(usize, Rc<Pic>, i32, i32)> = Vec::new();
                    // 처음 두 장은 맞는 쪽이 원래 그림대로 남음 (FUN_00445c6d(this, 맞는 쪽, 0))
                    if i < 2 {
                        if let Some(p) = dl {
                            let off = if p.w == 96 { 0 } else { -16 };
                            v.push((d, p, dx0 + off, FY + off));
                        }
                    }
                    let b = self.f[a].x - 16;
                    match i {
                        0 => {
                            v.push((a, at[0].clone(), b, y));
                            v.push((a, at[0].clone(), b + k14, y));
                            self.f[a].x += k2;
                            v.push((a, at[0].clone(), b + k2, y));
                        }
                        1 => {
                            v.push((a, at[0].clone(), b - k2, y));
                            v.push((a, at[0].clone(), b - k2 + k14, y));
                            v.push((a, at[1].clone(), b, y));
                            v.push((a, at[1].clone(), b + k14, y));
                            self.f[a].x += k2;
                            v.push((a, at[1].clone(), b + k2, y));
                        }
                        2 => {
                            if guard {
                                self.sound(0x1f);
                                v.push((d, dp.clone(), dx0 + k14, FY));
                            } else {
                                self.sound(0x24);
                                v.push((d, flash.clone(), dx0 + k14, FY));
                            }
                            v.push((a, at[0].clone(), b - 2 * k2, y));
                            v.push((a, at[0].clone(), b - 2 * k2 + k14, y));
                            v.push((a, at[1].clone(), b - k2, y));
                            v.push((a, at[1].clone(), b - k2 + k14, y));
                            v.push((a, at[2].clone(), b, y));
                            v.push((a, at[2].clone(), b + k14, y));
                            self.f[a].x += k2;
                            v.push((a, at[2].clone(), b + k2, y));
                        }
                        3 => {
                            v.push((d, if guard { dp.clone() } else { flash.clone() }, dx0 + k14, FY));
                            v.push((a, at[2].clone(), b - k2, y));
                            v.push((a, at[2].clone(), b - k2 + k14, y));
                            v.push((a, at[3].clone(), b, y));
                        }
                        _ => {
                            v.push((d, dp.clone(), dx0, FY));
                            v.push((a, at[3].clone(), b, y));
                        }
                    }
                    self.frame(None, v);
                    self.wait(1);
                }
            }
            _ => {
                // FUN_0044a184: 서로 달려들어 부딪침
                let aw = [self.mov(a, 4, false), self.mov(a, 5, false)];
                let dw = [self.mov(d, 4, false), self.mov(d, 5, false)];
                let (af, df) = (self.bright(&aw[1], 12), self.bright(&dw[1], 12));
                let (k7, k1) = (24 * dir, 48 * dir);
                for i in 0..8 {
                    let k = if i < 4 { k7 } else { k1 };
                    self.f[a].x += k;
                    self.f[d].x -= k;
                    let (ax, dx) = (self.f[a].x, self.f[d].x);
                    if ax == dx {
                        self.sound(0x1f);
                        self.frame(None, vec![(a, af.clone(), ax, FY), (d, df.clone(), dx, FY)]);
                    } else {
                        self.frame(None, vec![(a, aw[i % 2].clone(), ax, FY), (d, dw[i % 2].clone(), dx, FY)]);
                    }
                    self.wait(1);
                }
                self.wait(20);
            }
        }
    }

    /// op61 "승 부" (원작 FUN_0044b175)
    pub fn versus(&mut self) {
        self.wait(20);
        self.steps.push_back(Step::Win(Win::Restore));
        self.redraw_band();
        self.steps.push_back(Step::Win(Win::BigText(true)));
        self.wait(20);
        self.steps.push_back(Step::Win(Win::BigText(false)));
        self.redraw_band();
        self.wait(10);
    }

    /// op62 승패 (원작 FUN_0044b25a): 진 쪽 그림·얼굴을 흑백으로. 이긴 쪽이 오른쪽 아군이면 경험치
    pub fn defeat(&mut self, winner: usize, win_target: Option<usize>) {
        let w = winner.min(1);
        let l = 1 - w;
        self.wait(20);
        if let Some(p) = self.f[l].last.clone() {
            let g = self.gray(&p);
            self.f[l].last = Some(g);
        }
        self.steps.push_back(Step::Win(Win::Restore));
        self.steps.push_back(Step::Win(Win::Dark(l)));
        self.redraw_band();
        if w == 1 && self.f[1].side == 0 {
            let (wl, ll) = (self.f[1].level, self.f[0].level);
            let mut e = if wl < ll { ((ll - wl) * 8 + 32).min(200) } else { (32 - (wl - ll) * 4).max(10) };
            if win_target == Some(self.f[0].person) {
                e *= 2;
            }
            self.exp = e;
        }
        self.wait(20);
    }

    /// op5f 끝 (원작 FUN_0044ba60): 30 기다림 (눌러서 넘김) 뒤 음악 되돌림
    pub fn finish(&mut self) {
        self.steps.push_back(Step::Wait(60, true));
        self.steps.push_back(Step::Music(self.music_back));
    }

    // ---------- 그리기 ----------

    fn draw_mask(scr: &mut Screen, m: &Mask, x: i32, y: i32, c: u8) {
        for yy in 0..m.h {
            for xx in 0..m.w {
                if m.px[(yy * m.w + xx) as usize] != 0 {
                    scr.fill(x + xx, y + yy, 1, 1, c);
                }
            }
        }
    }

    /// 큰 글자 층 (원작 FUN_004212a8: 글자를 (k,k) 만큼 밀어 색 c 로)
    fn layered(scr: &mut Screen, m: &Mask, x: i32, y: i32, layers: &[(i32, u8)]) {
        for &(k, c) in layers {
            Self::draw_mask(scr, m, x + k, y + k, c);
        }
    }

    pub fn render(&self, scr: &mut Screen, f: &Font, mark: &[u8]) {
        let Some(stage) = self.stage else { return };
        let (wx, wy) = (WX, WY);
        // 창 바탕 (FUN_004752f2 무늬 창 / LOGO.E5[5])
        match (&self.bg, stage) {
            (Some(bg), 1) => scr.blit(bg, wx, wy, None),
            _ => frame_box(scr, wx, wy, WW, WH),
        }
        // 그림자·테두리 (FUN_0044a874: 아래·오른쪽 12줄, 위·왼쪽 8줄)
        let line = |scr: &mut Screen, x1: i32, y1: i32, x2: i32, y2: i32, c: u8| {
            scr.fill(x1.min(x2), y1.min(y2), (x2 - x1).abs() + 1, (y2 - y1).abs() + 1, c);
        };
        for i in 0..12 {
            let c = if stage == 0 { 0x19 - (i / 2) as u8 } else { 0x1f - i as u8 };
            line(scr, wx, wy + WH - 12 + i, wx + WW - 1, wy + WH - 12 + i, c);
            line(scr, wx + WW - 12 + i, wy, wx + WW - 12 + i, wy + WH - 1, c);
        }
        for i in 0..8 {
            let (c, cut) = if i < 3 {
                (0x13, 0)
            } else if i < 5 {
                (0x11 + i as u8, 0)
            } else if stage == 0 {
                (0x11 + i as u8, (i - 4) * 3)
            } else {
                (((i - 5) * 3 + 0x16) as u8, (i - 4) * 3)
            };
            line(scr, wx + i, wy + i, wx + WW - 1 - cut, wy + i, c);
            line(scr, wx + i, wy + i, wx + i, wy + WH - 1 - cut, c);
        }
        // 얼굴 (진 쪽은 흑백 — FUN_00445b8d → FUN_00432e2a)
        for s in 0..2 {
            if !self.faces[s] {
                continue;
            }
            let (fx, fy) = FACE[s];
            let face = &self.f[s].face;
            if self.f[s].dark {
                for yy in 0..face.h as i32 {
                    for xx in 0..face.w as i32 {
                        let c = face.px[(yy * face.w as i32 + xx) as usize];
                        let q = self.pal[c as usize];
                        let g = ((q[0] as u32 / 16 + q[1] as u32 / 16 + q[2] as u32 / 16) / 3) as u8 * 17;
                        scr.fill(wx + fx + xx, wy + fy + yy, 1, 1, nearest(&self.pal, [g, g, g]));
                    }
                }
            } else {
                scr.blit(face, wx + fx, wy + fy, None);
            }
        }
        // 큰 이름 (FUN_004458f3 0): 왼쪽 장수는 얼굴 오른쪽, 오른쪽 장수는 얼굴 왼쪽. 글자 폭 = 바이트 × 32
        for s in 0..2 {
            if !self.big_names[s] {
                continue;
            }
            let m = &self.f[s].big_name;
            let n = encoding_rs::EUC_KR.encode(&self.f[s].name).0.len() as i32;
            let (fx, fy) = FACE[s];
            let x = if s == 0 { fx + 96 } else { fx - n * 32 - 32 };
            let y = fy + 8 + (64 - m.h) / 2;
            let layers: [(i32, u8); 5] = if s == 0 { [(7, 0x18), (6, 0x18), (2, 0xac), (1, 0x3c), (0, 0x3a)] } else { [(7, 0x18), (6, 0x18), (2, 0x2a), (1, 0x2a), (0, 0x28)] };
            Self::layered(scr, m, wx + x, wy + y, &layers);
        }
        let (cx, cy) = (wx + WW / 2, wy + WH / 2);
        if self.vs {
            let m = &self.vs_mask;
            Self::layered(scr, m, cx - m.w / 2, cy - m.h / 2, &[(3, 0x1f), (2, 0x18), (1, 0x15), (0, 0x12)]);
        }
        // 작은 이름 (FUN_004458f3 1 → FUN_0044250b: 진영 색 테두리)
        if self.small_names {
            for s in 0..2 {
                let (fx, fy) = FACE[s];
                let name = &self.f[s].name;
                let n = encoding_rs::EUC_KR.encode(name).0.len() as i32;
                let x = (((wx + fx + 32) >> 3) - (n >> 1)) * 8;
                let y = wy + if s == 0 { fy + 0x58 } else { fy - 0x18 };
                let edge = match self.f[s].side {
                    0 => 0x2a,
                    1 => 0xad,
                    _ => 0xac,
                };
                for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    draw_text(scr, f, x + dx, y + dy, name, edge);
                }
                draw_text(scr, f, x, y, name, 0x12);
            }
        }
        // 띠 (장수 그림)
        if stage == 1 {
            for (p, x, y) in &self.band {
                scr.blit(p, wx + x, wy + y, Some(0));
            }
        }
        // 대사 (FUN_0044532f): 흰 칸 + 괄호 그림 + 말하는 쪽 표시
        for s in 0..2 {
            let Some(lines) = &self.dialog[s] else { continue };
            let (fx, fy) = FACE[s];
            let (x, y) = if s == 0 { (wx + fx + 96, wy + fy + 8) } else { (wx + fx - 352, wy + fy + 8) };
            let arrow_x = if s == 0 { x - 16 } else { x + 0x140 };
            let mk = |o: usize, w: i32, h: i32, scr: &mut Screen, dx: i32, dy: i32| {
                for yy in 0..h {
                    for xx in 0..w {
                        let c = mark.get(o + (yy * w + xx) as usize).copied().unwrap_or(0);
                        if c & 0xf != 0 {
                            scr.fill(dx + xx, dy + yy, 1, 1, c);
                        }
                    }
                }
            };
            scr.fill(x + 16, y, 0x130 - 16 + 1, 0x40, 0x12);
            mk(if s == 0 { 0x195c0 } else { 0x196c0 }, 16, 16, scr, arrow_x, y + 0x18);
            mk(0x197c0, 16, 64, scr, x, y);
            mk(0x19bc0, 16, 64, scr, x + 0x130, y);
            for (i, l) in lines.iter().enumerate() {
                draw_text(scr, f, ((x + 16) >> 3) * 8, y + 5 + i as i32 * 20, l, 0x1f);
            }
        }
        if self.big_text {
            let m = &self.win_mask;
            Self::layered(scr, m, cx - m.w / 2, cy - m.h / 2, &[(3, 0x1f), (2, 0x18), (1, 0x15), (0, 0x12)]);
        }
    }
}

/// 글자 모양 받기 실패 때: 비트맵 글꼴을 키움 (원작은 굴림체 굵게)
pub fn fallback_mask(f: &Font, text: &str, h: i32) -> Mask {
    let mut scr = Screen::default();
    scr.clear(0);
    draw_text(&mut scr, f, 0, 0, text, 1);
    let n = encoding_rs::EUC_KR.encode(text).0.len() as i32;
    let (w0, h0) = (n * 8, 16);
    let k = (h / 16).max(1);
    let mut px = vec![0u8; (w0 * k * h0 * k) as usize];
    for y in 0..h0 * k {
        for x in 0..w0 * k {
            if scr.px[((y / k) * 640 + x / k) as usize] != 0 {
                px[(y * w0 * k + x) as usize] = 1;
            }
        }
    }
    Mask { w: w0 * k, h: h0 * k, px }
}

/// 호스트가 그린 글자 모양 ("@txt/높이/글" 자료: 폭 u16, 높이 u16, 칸마다 0/1)
pub fn parse_mask(d: &[u8]) -> Option<Mask> {
    let w = u16::from_le_bytes([*d.first()?, *d.get(1)?]) as i32;
    let h = u16::from_le_bytes([*d.get(2)?, *d.get(3)?]) as i32;
    let px = d.get(4..4 + (w * h) as usize)?.to_vec();
    Some(Mask { w, h, px })
}
