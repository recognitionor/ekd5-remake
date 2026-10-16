//! 에셋 공급자. 원작은 리소스를 스레드로 읽고 끝날 때까지 기다렸다 — 같은 방식으로
//! 아직 준비 안 된 리소스는 `Pending`을 돌려주고, 요청한 명령은 다음 프레임에 다시 시도한다.

use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone)]
pub enum Asset {
    Ready(Rc<Vec<u8>>),
    Pending,
    Missing,
}

pub trait Assets {
    /// 파일 이름(대소문자 무관, 예: "LOGO.E5", "S_00.EEX")
    fn get(&mut self, name: &str) -> Asset;
}

/// 메모리/디스크 기반 (네이티브, 테스트용)
pub struct DirAssets {
    pub root: std::path::PathBuf,
    cache: HashMap<String, Option<Rc<Vec<u8>>>>,
}

impl DirAssets {
    pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
        Self { root: root.into(), cache: HashMap::new() }
    }
}

impl Assets for DirAssets {
    fn get(&mut self, name: &str) -> Asset {
        let key = name.to_ascii_uppercase();
        let root = self.root.clone();
        let e = self.cache.entry(key.clone()).or_insert_with(|| {
            std::fs::read(root.join(name)).or_else(|_| std::fs::read(root.join(&key))).ok().map(Rc::new)
        });
        match e {
            Some(d) => Asset::Ready(d.clone()),
            None => Asset::Missing,
        }
    }
}
