//! 브라우저용 WASM 진입점. 원시 export만 쓴다 (JS 글루: web/ekd5.js).
//! JS가 `alloc`으로 메모리를 잡아 파일을 복사 → `provide` / `missing`. 엔진이 원하는 파일은 `take_requests`(줄바꿈 구분).
//! 매 틱 `tick(입력)` → `frame()` 의 640x480 RGBA → `take_events()`(JSON 줄).

use ekd5::engine::assets::{Asset, Assets};
use ekd5::engine::{Engine, Sound};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

#[derive(Default)]
struct WebAssets {
    files: HashMap<String, Rc<Vec<u8>>>,
    missing: HashSet<String>,
    requested: BTreeSet<String>,
    sent: HashSet<String>,
}

impl Assets for WebAssets {
    fn get(&mut self, name: &str) -> Asset {
        let key = name.to_ascii_uppercase();
        if let Some(d) = self.files.get(&key) {
            return Asset::Ready(d.clone());
        }
        if self.missing.contains(&key) {
            return Asset::Missing;
        }
        if !self.sent.contains(&key) {
            self.requested.insert(key);
        }
        Asset::Pending
    }
}

struct App {
    engine: Engine<WebAssets>,
    out: Vec<u8>,
}

thread_local! {
    static APP: RefCell<App> = RefCell::new(App { engine: Engine::new(WebAssets::default()), out: Vec::new() });
}

fn with<R>(f: impl FnOnce(&mut App) -> R) -> R {
    APP.with(|a| f(&mut a.borrow_mut()))
}

unsafe fn take_vec(ptr: *mut u8, len: usize) -> Vec<u8> {
    Vec::from_raw_parts(ptr, len, len)
}

unsafe fn str_at(ptr: *const u8, len: usize) -> String {
    String::from_utf8_lossy(std::slice::from_raw_parts(ptr, len)).to_string()
}

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `alloc`로 받은 포인터만
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    drop(take_vec(ptr, len));
}

/// # Safety
/// 포인터는 `alloc`로 받은 것
#[no_mangle]
pub unsafe extern "C" fn provide(name_ptr: *const u8, name_len: usize, data_ptr: *mut u8, data_len: usize) {
    let name = str_at(name_ptr, name_len).to_ascii_uppercase();
    let data = take_vec(data_ptr, data_len);
    with(|app| {
        let a = &mut app.engine.assets;
        a.requested.remove(&name);
        a.files.insert(name, Rc::new(data));
    });
}

/// # Safety
/// 유효한 UTF-8 포인터
#[no_mangle]
pub unsafe extern "C" fn missing(name_ptr: *const u8, name_len: usize) {
    let name = str_at(name_ptr, name_len).to_ascii_uppercase();
    with(|app| {
        let a = &mut app.engine.assets;
        a.requested.remove(&name);
        a.missing.insert(name);
    });
}

#[no_mangle]
pub extern "C" fn take_requests() -> *const u8 {
    with(|app| {
        let a = &mut app.engine.assets;
        let list: Vec<String> = std::mem::take(&mut a.requested).into_iter().collect();
        for n in &list {
            a.sent.insert(n.clone());
        }
        app.out = list.join("\n").into_bytes();
        app.out.as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn out_len() -> usize {
    with(|app| app.out.len())
}

#[no_mangle]
pub extern "C" fn tick(held: u32, mx: i32, my: i32, has_mouse: i32, click: i32) {
    with(|app| {
        app.engine.set_input(held, if has_mouse != 0 { Some((mx, my)) } else { None }, click != 0);
        app.engine.tick();
    })
}

#[no_mangle]
pub extern "C" fn frame() -> *const u8 {
    with(|app| app.engine.render().as_ptr())
}

/// 회차 공통 기록 (엔딩·보물도감) 불러오기
/// # Safety
/// 유효한 UTF-8 포인터
#[no_mangle]
pub unsafe extern "C" fn set_common(ptr: *const u8, len: usize) {
    let s = str_at(ptr, len);
    with(|app| app.engine.set_common(&s))
}

/// 동영상 재생 가능 알림 / 끝남
#[no_mangle]
pub extern "C" fn enable_movies() {
    with(|app| app.engine.movies = true)
}

#[no_mangle]
pub extern "C" fn movie_done() {
    with(|app| app.engine.movie_done())
}

/// 명성 (0..100, 원작 상태줄 게이지)
#[no_mangle]
pub extern "C" fn fame() -> i32 {
    with(|app| app.engine.fame)
}

/// 이벤트(JSON 한 줄씩): {"effect":"x"} {"log":"..."}
#[no_mangle]
pub extern "C" fn take_events() -> *const u8 {
    with(|app| {
        let mut lines = Vec::new();
        for s in app.engine.take_sounds() {
            match s {
                Sound::Effect(f) => lines.push(format!("{{\"effect\":{f:?}}}")),
                Sound::EffectLoop(f) => lines.push(format!("{{\"loop\":{f:?}}}")),
                Sound::EffectStop => lines.push("{\"loop\":\"\"}".to_string()),
                Sound::Movie(f) => lines.push(format!("{{\"movie\":{f:?}}}")),
                Sound::Ending(n) => lines.push(format!("{{\"ending\":{n}}}")),
                Sound::Music(n) => lines.push(format!("{{\"music\":{}}}", n.unwrap_or(-1))),
            }
        }
        if let Some(c) = app.engine.common_out.take() {
            lines.push(format!("{{\"common\":{c:?}}}"));
        }
        if let Some(sv) = app.engine.save_out.take() {
            lines.push(format!("{{\"save\":{sv:?}}}"));
        }
        if let Some(e) = &app.engine.error {
            lines.push(format!("{{\"error\":{e:?}}}"));
        }
        for l in app.engine.log.drain(..) {
            lines.push(format!("{{\"log\":{l:?}}}"));
        }
        app.out = lines.join("\n").into_bytes();
        app.out.as_ptr()
    })
}

/// 저장 문자열로 이어하기 (제목 화면 전에 부름). 성공 1
/// # Safety
/// 유효한 UTF-8 포인터
#[no_mangle]
pub unsafe extern "C" fn restore(ptr: *const u8, len: usize) -> i32 {
    let s = str_at(ptr, len);
    with(|app| app.engine.restore(&s) as i32)
}
