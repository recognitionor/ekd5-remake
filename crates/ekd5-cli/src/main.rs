//! 개발 도구: extract(원본 → data/), unpack(Ls12 항목 → png/bin), shots(헤드리스 화면 저장)
use ekd5::engine::{assets::DirAssets, Engine};
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(|s| s.as_str()) {
        Some("extract") if a.len() >= 3 => extract(Path::new(&a[1]), Path::new(&a[2])),
        Some("shots") if a.len() >= 4 => shots(PathBuf::from(&a[1]), PathBuf::from(&a[2]), a[3].parse().unwrap(), a.get(4).and_then(|s| s.parse().ok()).unwrap_or(30)),
        _ => eprintln!("사용: ekd5 extract <원본 폴더> <data>\n      ekd5 shots <data> <출력 폴더> <틱 수> [간격]"),
    }
}

/// 원작 튜토리얼(FUN_004354f9) 대사 주소: (묶음 이름, [(주소, 말하는 사람)]). 0x9D = 관상가 노인, 0 = 조조, 0xFFFF = 선택지
const TUTOR: &[(&str, &[(u32, u16)])] = &[
    ("intro", &[(0x496cf0, 0x9d), (0x496d0c, 0)]),
    ("first", &[(0x496d44, 0x9d), (0x496d88, 0x9d)]),
    ("again", &[(0x496de4, 0x9d), (0x496e18, 0x9d), (0x496e50, 0x9d)]),
    ("ask", &[(0x496e94, 0xffff)]),
    ("yes", &[(0x496eb8, 0)]),
    ("no", &[(0x496ee0, 0), (0x496f30, 0x9d)]),
    ("more", &[(0x496cb0, 0x9d), (0x496ccc, 0xffff)]),
    ("s1", &[(0x496ffc, 0x9d), (0x497018, 0x9d), (0x49703c, 0x9d), (0x497094, 0x9d), (0x4970d4, 0x9d), (0x4970f8, 0x9d), (0x497130, 0x9d), (0x49716c, 0x9d), (0x4971b0, 0x9d), (0x4971f8, 0x9d)]),
    ("s2", &[(0x497210, 0x9d), (0x497234, 0x9d), (0x497264, 0x9d), (0x4972a0, 0x9d), (0x4972dc, 0x9d), (0x497320, 0x9d), (0x49735c, 0x9d), (0x497388, 0x9d), (0x4973d8, 0x9d), (0x497428, 0x9d), (0x497470, 0x9d), (0x4974a0, 0x9d), (0x4974fc, 0x9d), (0x497538, 0x9d), (0x497578, 0x9d), (0x4975d8, 0x9d)]),
    ("s3", &[(0x497600, 0x9d), (0x49765c, 0x9d), (0x497690, 0x9d), (0x4976a4, 0x9d), (0x4976f0, 0x9d), (0x497740, 0x9d), (0x497780, 0x9d), (0x4977ac, 0x9d), (0x4977ec, 0x9d), (0x497834, 0x9d), (0x497864, 0x9d), (0x4978a4, 0x9d), (0x4978f8, 0x9d), (0x497920, 0x9d), (0x497948, 0x9d), (0x4979a0, 0x9d), (0x497a08, 0x9d)]),
    ("s4", &[(0x497a2c, 0x9d), (0x497a58, 0x9d), (0x497a7c, 0x9d), (0x497ac8, 0x9d), (0x497b10, 0x9d), (0x497b50, 0x9d), (0x497b64, 0x9d), (0x497b9c, 0x9d), (0x497c00, 0x9d), (0x497c60, 0x9d), (0x497ca4, 0x9d), (0x497cf4, 0x9d), (0x497d38, 0x9d), (0x497d84, 0x9d), (0x497dbc, 0x9d), (0x497df8, 0x9d), (0x497e1c, 0x9d), (0x497e54, 0x9d), (0x497ea8, 0x9d), (0x497efc, 0x9d), (0x497f20, 0x9d), (0x497f68, 0x9d), (0x497fac, 0x9d), (0x497fec, 0x9d), (0x498034, 0x9d)]),
    ("end", &[(0x496f7c, 0), (0x496fa0, 0x9d)]),
    ("ng1", &[(0x498078, 0x9d), (0x4980a4, 0x9d), (0x4980d0, 0x9d), (0x4980fc, 0xffff)]),
    ("ng1y", &[(0x498120, 0), (0x498140, 0x9d)]),
    ("ng1n", &[(0x49815c, 0), (0x498198, 0x9d)]),
    ("ng2", &[(0x4981e4, 0x9d), (0x49821c, 0x9d), (0x498248, 0x9d), (0x498274, 0xffff)]),
    ("ng2y", &[(0x498298, 0), (0x4982b8, 0x9d)]),
    ("ng2n", &[(0x4982d4, 0), (0x498310, 0x9d)]),
    ("ng3", &[(0x49835c, 0x9d), (0x498398, 0x9d), (0x4983c0, 0x9d), (0x4983f0, 0xffff)]),
    ("ng3y", &[(0x49842c, 0), (0x49844c, 0x9d)]),
    ("ng3n", &[(0x498468, 0), (0x4984a4, 0x9d)]),
    ("ngend", &[(0x4984f0, 0x9d)]),
    ("book", &[(0x498534, 0x9d), (0x498564, 0x9d), (0x49859c, 0x9d), (0x4985d8, 0x9d), (0x49860c, 0x9d), (0x498638, 0xffff)]),
    ("booky", &[(0x498660, 0), (0x498674, 0x9d)]),
    ("bookn", &[(0x4986d0, 0), (0x49870c, 0x9d)]),
    ("bye", &[(0x496c90, 0x9d)]),
];

/// 지형 이름: 파일 위치 0x94120 부터 4바이트 정렬된 C 문자열 30개 (평지·초원·숲 …)
fn terrain_names(exe: &[u8]) -> String {
    let mut o = 0x94120;
    let mut out = Vec::new();
    while out.len() < 30 && o < exe.len() {
        let e = exe[o..].iter().position(|&b| b == 0).unwrap_or(0);
        if e > 0 {
            out.push(encoding_rs::EUC_KR.decode(&exe[o..o + e]).0.to_string());
        }
        o = (o + e + 1 + 3) & !3;
    }
    out.join("\n")
}

/// 직업 이름 (원작 표 0x496200: u32 주소 × 53)
fn job_names(exe: &[u8]) -> String {
    let u16at = |o: usize| u16::from_le_bytes([exe[o], exe[o + 1]]) as usize;
    let u32at = |o: usize| u32::from_le_bytes([exe[o], exe[o + 1], exe[o + 2], exe[o + 3]]) as usize;
    let pe = u32at(0x3c);
    let (n, opt) = (u16at(pe + 6), u16at(pe + 20));
    let base = u32at(pe + 52);
    let secs: Vec<(usize, usize, usize)> = (0..n).map(|i| {
        let o = pe + 24 + opt + i * 40;
        (u32at(o + 12) + base, u32at(o + 8).max(u32at(o + 16)), u32at(o + 20))
    }).collect();
    let off = |va: usize| secs.iter().find(|&&(v, sz, _)| va >= v && va < v + sz).map(|&(v, _, ro)| ro + va - v);
    let Some(t) = off(0x496200) else { return String::new() };
    (0..53).map(|k| {
        let p = u32at(t + k * 4);
        off(p).map(|o| {
            let e = exe[o..].iter().position(|&b| b == 0).unwrap_or(0);
            encoding_rs::EUC_KR.decode(&exe[o..o + e]).0.to_string()
        }).unwrap_or_default()
    }).collect::<Vec<_>>().join("\n")
}

/// PE 가상 주소의 C 문자열 (EUC-KR → UTF-8, 줄바꿈은 \n 두 글자)
fn tutor_text(exe: &[u8]) -> String {
    let u16at = |o: usize| u16::from_le_bytes([exe[o], exe[o + 1]]) as usize;
    let u32at = |o: usize| u32::from_le_bytes([exe[o], exe[o + 1], exe[o + 2], exe[o + 3]]) as usize;
    let pe = u32at(0x3c);
    let (n, opt) = (u16at(pe + 6), u16at(pe + 20));
    let base = u32at(pe + 52);
    let secs: Vec<(usize, usize, usize)> = (0..n).map(|i| {
        let o = pe + 24 + opt + i * 40;
        (u32at(o + 12) + base, u32at(o + 8).max(u32at(o + 16)), u32at(o + 20))
    }).collect();
    let off = |va: usize| secs.iter().find(|&&(v, sz, _)| va >= v && va < v + sz).map(|&(v, _, ro)| ro + va - v);
    let mut out = String::new();
    for (key, list) in TUTOR {
        for &(va, who) in *list {
            let Some(o) = off(va as usize) else { continue };
            let e = exe[o..].iter().position(|&b| b == 0).unwrap_or(0);
            let (t, _, _) = encoding_rs::EUC_KR.decode(&exe[o..o + e]);
            out.push_str(&format!("{key}|{who}|{}\n", t.replace('\n', "\\n")));
        }
    }
    out
}

/// PE 리소스의 비트맵(8비트 DIB)들을 번호 순으로: (가로, 세로, 팔레트 RGB 768, 위에서부터 픽셀)
fn dll_bitmaps(d: &[u8]) -> Vec<(usize, usize, usize, Vec<u8>, Vec<u8>)> {
    let u16at = |o: usize| u16::from_le_bytes([d[o], d[o + 1]]) as usize;
    let u32at = |o: usize| u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) as usize;
    let pe = u32at(0x3c);
    let (n, opt) = (u16at(pe + 6), u16at(pe + 20));
    let rsrc = u32at(pe + 24 + 96 + 16);
    let secs: Vec<(usize, usize, usize)> = (0..n).map(|i| {
        let o = pe + 24 + opt + i * 40;
        (u32at(o + 12), u32at(o + 8).max(u32at(o + 16)), u32at(o + 20))
    }).collect();
    let off = |va: usize| secs.iter().find(|&&(v, sz, _)| va >= v && va < v + sz).map(|&(v, _, ro)| ro + va - v);
    let Some(base) = off(rsrc) else { return Vec::new() };
    let mut out = Vec::new();
    // 종류 → 번호 → 언어 → 자료
    let entries = |o: usize| -> Vec<(usize, usize)> { (0..u16at(o + 12) + u16at(o + 14)).map(|k| (u32at(o + 16 + k * 8), u32at(o + 20 + k * 8))).collect() };
    for (ty, t_off) in entries(base) {
        if ty != 2 || t_off & 0x8000_0000 == 0 {
            continue;
        }
        let mut ids = entries(base + (t_off & 0x7fff_ffff));
        ids.sort();
        for (id, i_off) in ids {
            let langs = entries(base + (i_off & 0x7fff_ffff));
            let Some(&(_, l_off)) = langs.first() else { continue };
            let (drva, dsz) = (u32at(base + l_off), u32at(base + l_off + 4));
            let Some(o) = off(drva) else { continue };
            let b = &d[o..o + dsz];
            let (w, h) = (u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize, i32::from_le_bytes([b[8], b[9], b[10], b[11]]));
            let used = u32::from_le_bytes([b[32], b[33], b[34], b[35]]) as usize;
            let nc = if used == 0 { 256 } else { used };
            let mut pal = vec![0u8; 768];
            for c in 0..nc.min(256) {
                let q = &b[40 + c * 4..44 + c * 4];
                pal[c * 3..c * 3 + 3].copy_from_slice(&[q[2], q[1], q[0]]);
            }
            let (ha, stride) = (h.unsigned_abs() as usize, (w + 3) & !3);
            let src = &b[40 + nc * 4..];
            let mut px = vec![0u8; w * ha];
            for y in 0..ha {
                let sy = if h > 0 { ha - 1 - y } else { y };
                if let Some(row) = src.get(sy * stride..sy * stride + w) {
                    px[y * w..y * w + w].copy_from_slice(row);
                }
            }
            out.push((id, w, ha, pal, px));
        }
    }
    out
}

/// 원본 폴더에서 게임 데이터 파일만 복사 (대문자 이름) + index.txt
fn extract(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    let mut names = Vec::new();
    for e in fs::read_dir(src).unwrap().flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        let up = n.to_ascii_uppercase();
        let keep = [".E5", ".EEX", ".WAV", ".16P", ".DAT"].iter().any(|x| up.ends_with(x)) && !up.starts_with("SETUP");
        if keep && e.path().is_file() {
            fs::copy(e.path(), dst.join(&up)).unwrap();
            names.push(up);
        }
    }
    // 아이콘 DLL 의 비트맵 → ICONS.BIN (원작 ItemIcon/MgcIcon/CmdIcon/MapAtr.dll)
    let mut icons = b"ICON".to_vec();
    for dll in ["ITEMICON.DLL", "MGCICON.DLL", "CMDICON.DLL", "MAPATR.DLL"] {
        let bmps = fs::read(src.join(dll)).map(|d| dll_bitmaps(&d)).unwrap_or_default();
        icons.extend((bmps.len() as u16).to_le_bytes());
        for (_, w, h, pal, px) in bmps {
            icons.extend((w as u16).to_le_bytes());
            icons.extend((h as u16).to_le_bytes());
            icons.extend(pal);
            icons.extend(px);
        }
    }
    // 묶음 4: 본진 단추 (실행 파일 비트맵 0x155 출진, 0x156 매각, 0x157 구입, 0x158 장비 — 원작 FUN_0041c2b2)
    let camp: Vec<_> = fs::read(src.join("EKD5.EXE")).map(|d| dll_bitmaps(&d)).unwrap_or_default().into_iter().filter(|b| (0x155..=0x158).contains(&b.0)).collect();
    icons.extend((camp.len() as u16).to_le_bytes());
    for (_, w, h, pal, px) in camp {
        icons.extend((w as u16).to_le_bytes());
        icons.extend((h as u16).to_le_bytes());
        icons.extend(pal);
        icons.extend(px);
    }
    fs::write(dst.join("ICONS.BIN"), icons).unwrap();
    names.push("ICONS.BIN".into());
    // 실행 파일 속 튜토리얼 대사 (op10, 관상가 노인) → TUTOR.TXT
    if let Ok(exe) = fs::read(src.join("EKD5.EXE")) {
        fs::write(dst.join("TUTOR.TXT"), tutor_text(&exe)).unwrap();
        names.push("TUTOR.TXT".into());
        // 직업 이름 표 (원작 0x496200 의 문자열 주소 53개) → JOBS.TXT
        fs::write(dst.join("JOBS.TXT"), job_names(&exe)).unwrap();
        names.push("JOBS.TXT".into());
        // 지형 이름 30개 (원작 0x494120 부터 이어진 문자열, 4바이트 정렬) → TERRAIN.TXT
        fs::write(dst.join("TERRAIN.TXT"), terrain_names(&exe)).unwrap();
        names.push("TERRAIN.TXT".into());
    }
    names.sort();
    fs::write(dst.join("index.txt"), names.join("\n")).unwrap();
    println!("{}개 파일 → {}", names.len(), dst.display());
}

fn shots(data: PathBuf, out: PathBuf, ticks: u32, every: u32) {
    fs::create_dir_all(&out).unwrap();
    let mut eng = Engine::new(DirAssets::new(data));
    eng.autoplay = std::env::var("EKD5_AUTOPLAY").is_ok();
    eng.cheat = std::env::var("EKD5_CHEAT").is_ok();
    eng.minimap_on = std::env::var("EKD5_MINIMAP").is_ok();
    if let Some(n) = std::env::var("EKD5_STAGE").ok().and_then(|v| v.parse().ok()) {
        eng.stage = n;
    }
    // 시험: 저장 문자열로 시작 (EKD5_RESTORE), 단계마다 저장 문자열 출력 (EKD5_SNAP)
    if let Ok(r) = std::env::var("EKD5_RESTORE") {
        eng.restore(&r);
    }
    let snap = std::env::var("EKD5_SNAP").is_ok();
    // 시험: 중간 저장 불러오기 (EKD5_QLOAD=파일), 그 틱에 중간 저장 (EKD5_QSAVE=틱:파일)
    if let Ok(f) = std::env::var("EKD5_QLOAD") {
        let s = fs::read_to_string(&f).unwrap();
        println!("중간 저장 불러오기 {}", eng.quick_restore(&s));
    }
    let qsave: Option<(u32, String)> = std::env::var("EKD5_QSAVE").ok().and_then(|v| { let (a, b) = v.split_once(':')?; Some((a.parse().ok()?, b.to_string())) });
    // 시험: 시작 플래그 (EKD5_FLAGS="0,100")
    for f in std::env::var("EKD5_FLAGS").unwrap_or_default().split(',').filter_map(|v| v.parse::<usize>().ok()) {
        if f < eng.flags.len() {
            eng.flags[f] = true;
        }
    }
    let press: Vec<(u32, u32)> = std::env::var("EKD5_PRESS").unwrap_or_default().split(',').filter_map(|s| {
        let (t, b) = s.split_once(':')?;
        Some((t.parse().ok()?, u32::from_str_radix(b, 16).ok()?))
    }).collect();
    for t in 1..=ticks {
        // 자동: 30틱마다 결정 키
        let auto = std::env::var("EKD5_AUTO").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let auto_end = std::env::var("EKD5_AUTO_END").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(u32::MAX);
        // EKD5_AUTO_RESUME=틱: AUTO_END 뒤 그 틱부터 다시 자동 결정 키
        let auto_resume = std::env::var("EKD5_AUTO_RESUME").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(u32::MAX);
        let held = press.iter().filter(|p| p.0 == t).fold(0, |a, p| a | p.1) | if auto > 0 && t % auto == 0 && (t < auto_end || t >= auto_resume) { 0x20 } else { 0 };
        // 탭: EKD5_TAPS="틱:x:y,..." (그 틱에 누르고 다음 틱에 뗌)
        let taps: Vec<(u32, i32, i32)> = std::env::var("EKD5_TAPS").unwrap_or_default().split(',').filter_map(|s| {
            let v: Vec<&str> = s.split(':').collect();
            Some((v.first()?.parse().ok()?, v.get(1)?.parse().ok()?, v.get(2)?.parse().ok()?))
        }).collect();
        let tap = taps.iter().find(|p| p.0 == t || p.0 + 1 == t);
        match tap {
            Some(&(tt, x, y)) => eng.set_input(if tt == t { held | 0x20 } else { held & !0x20 }, Some((x, y)), tt == t),
            None => eng.set_input(held, None, false),
        }
        // 정보 보기 (오른쪽 클릭·길게 누르기): EKD5_ALT="틱:x:y,..."
        if let Some(&(_, x, y)) = std::env::var("EKD5_ALT").unwrap_or_default().split(',').filter_map(|s| {
            let v: Vec<&str> = s.split(':').collect();
            Some((v.first()?.parse::<u32>().ok()?, v.get(1)?.parse::<i32>().ok()?, v.get(2)?.parse::<i32>().ok()?))
        }).collect::<Vec<_>>().iter().find(|p| p.0 == t) {
            eng.set_input(held, Some((x, y)), false);
            eng.set_pointer_kind(true, false);
        }
        // 시험: EKD5_LIMIT=턴 — 전투 턴 제한을 그 값으로 (턴 제한 초과 퇴각 시험)
        if let Some(n) = std::env::var("EKD5_LIMIT").ok().and_then(|v| v.parse::<i32>().ok()) {
            if let ekd5::engine::Scene::Battle(b) = &mut eng.scene {
                if b.started && b.turn_limit > n {
                    b.turn_limit = n;
                }
            }
        }
        // 시험: EKD5_TOOL=틱 — 그 틱에 첫 조조군 부대가 자기에게 첫 소모품을 씀 (도구 쓰기 연출)
        if std::env::var("EKD5_TOOL").ok().and_then(|v| v.parse::<u32>().ok()) == Some(t) {
            let items = eng.items.clone();
            if let ekd5::engine::Scene::Battle(b) = &mut eng.scene {
                let u = b.units.iter().position(|u| u.visible && u.hp > 0 && u.side == ekd5::engine::battle::Side::Player);
                if !b.inventory.iter().any(|&(id, _)| items.get(id as usize * 25 + 17).is_some_and(|&t| t >= 0x3F)) {
                    b.inventory.push((87, 1));
                }
                let inv = b.inventory.iter().position(|&(id, _)| items.get(id as usize * 25 + 17).is_some_and(|&t| t >= 0x3F));
                if let (Some(u), Some(inv)) = (u, inv) {
                    b.units[u].hp = (b.units[u].hp / 2).max(1);
                    let (x, y) = (b.units[u].x, b.units[u].y);
                    b.focus(x, y);
                    println!("도구 시험: 부대 {u} 소지품 {inv}");
                    b.phase = ekd5::engine::battle::Phase::ToolUse { unit: u, target: u, inv, t: 0 };
                }
            }
        }
        // 시험: EKD5_FX="틱:연출번호" — 그 틱에 화면 가운데에 연출
        if let Some((ft, id)) = std::env::var("EKD5_FX").ok().and_then(|v| { let (a, b) = v.split_once(':')?; Some((a.parse::<u32>().ok()?, b.parse::<u8>().ok()?)) }) {
            if ft == t {
                if let ekd5::engine::Scene::Battle(b) = &mut eng.scene {
                    b.fx_req = Some((id, vec![(b.cam.0 + 320, b.cam.1 + 240)]));
                }
            }
        }
        // 시험: EKD5_AUTOPLAY_FROM=틱 — 그 틱부터 자동 진행 (본진은 사람처럼 거친 뒤)
        if std::env::var("EKD5_AUTOPLAY_FROM").ok().and_then(|v| v.parse::<u32>().ok()) == Some(t) {
            eng.autoplay = true;
        }
        if let Some(e) = std::env::var("EKD5_AUTOPLAY_END").ok().and_then(|v| v.parse::<u32>().ok()) {
            if t == e {
                eng.autoplay = false;
            }
        }
        // 시험: EKD5_FORCE=틱:블록 (그 틱부터 절 1 의 그 사건 블록을 조건 없이 실행)
        if let Some((ft, fb)) = std::env::var("EKD5_FORCE").ok().and_then(|v| { let (a, b) = v.split_once(':')?; Some((a.parse::<u32>().ok()?, b.parse::<usize>().ok()?)) }) {
            if t == ft {
                eng.force_block = Some(fb);
            }
        }
        eng.tick();
        if let Some((qt, qf)) = &qsave {
            if t >= *qt && !Path::new(qf).exists() {
                if let Some(s) = eng.quick_save() {
                    fs::write(qf, s).unwrap();
                    println!("[{t:>6}] 중간 저장 → {qf}");
                }
            }
        }
        if let Some(sv) = eng.save_out.take() {
            if snap && !sv.is_empty() {
                println!("[{t:>6}] SNAP {sv}");
            }
        }
        for l in eng.log.drain(..) {
            println!("[{t:>6}] {l}");
        }
        if let Some(e) = &eng.error {
            println!("[{t:>6}] 오류 {e}");
            let rgba = eng.render().to_vec();
            write_png(&out.join(format!("t{t:05}.png")), 640, 480, &rgba);
            break;
        }
        if t % every == 0 {
            let rgba = eng.render().to_vec();
            write_png(&out.join(format!("t{t:05}.png")), 640, 480, &rgba);
        }
    }
}

fn write_png(path: &Path, w: usize, h: usize, rgba: &[u8]) {
    let f = fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}
