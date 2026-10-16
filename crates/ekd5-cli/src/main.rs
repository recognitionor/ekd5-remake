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
    names.sort();
    fs::write(dst.join("index.txt"), names.join("\n")).unwrap();
    println!("{}개 파일 → {}", names.len(), dst.display());
}

fn shots(data: PathBuf, out: PathBuf, ticks: u32, every: u32) {
    fs::create_dir_all(&out).unwrap();
    let mut eng = Engine::new(DirAssets::new(data));
    eng.autoplay = std::env::var("EKD5_AUTOPLAY").is_ok();
    if let Some(n) = std::env::var("EKD5_STAGE").ok().and_then(|v| v.parse().ok()) {
        eng.stage = n;
    }
    let press: Vec<(u32, u32)> = std::env::var("EKD5_PRESS").unwrap_or_default().split(',').filter_map(|s| {
        let (t, b) = s.split_once(':')?;
        Some((t.parse().ok()?, u32::from_str_radix(b, 16).ok()?))
    }).collect();
    for t in 1..=ticks {
        // 자동: 30틱마다 결정 키
        let auto = std::env::var("EKD5_AUTO").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let held = press.iter().filter(|p| p.0 == t).fold(0, |a, p| a | p.1) | if auto > 0 && t % auto == 0 { 0x20 } else { 0 };
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
        eng.tick();
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
