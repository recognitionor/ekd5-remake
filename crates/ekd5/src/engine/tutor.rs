//! 첫 전투의 관상가 노인 (원작 op10 → FUN_004354f9) 과 회차 공통 기록 (원작 SVCMN.E5S: 본 엔딩, 보물도감).
//! 대사는 추출 때 실행 파일에서 꺼낸 data/TUTOR.TXT ("묶음|말하는 사람|글", 0xFFFF = 선택지).
//! 흐름: intro → (엔딩 본 적 없음 first / 있음 again) → ask: 예 → yes, s1, (more → s2 → more → s3 → more → s4), end / 아니오 → no.
//! 그 뒤 엔딩 수 1·2·3+ 이면 ng1/ng2/ng3 (Lv +1/+2/+6·+3 또는 돈 2000/4000/10000), 보물도감을 다 채웠으면 book (보물 전부 또는 30000), bye.

use super::{story, Asset, Assets, Engine, Pending, Scene, Sound, Wait};
use std::collections::{HashMap, VecDeque};

#[derive(Clone, Debug)]
pub enum Act {
    /// 묶음의 대사 (선택지가 나오면 거기서 멈춤)
    Key(&'static str),
    /// 선택 결과로 갈림
    Branch(Br),
    LevelUp(u8),
    Gold(i32),
    AllTreasure,
    Sound(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub enum Br {
    Main,
    More(u8),
    Ng(u8),
    Book,
}

pub struct Tutor {
    pub lines: HashMap<String, Vec<(u16, String)>>,
    pub queue: VecDeque<Act>,
}

/// 공통 기록 문자열: "c1;엔딩 비트;보물 번호들"
pub fn common_string(endings: u8, book: &[bool]) -> String {
    let ids: Vec<String> = book.iter().enumerate().filter(|(_, &b)| b).map(|(i, _)| i.to_string()).collect();
    format!("c1;{endings};{}", ids.join(","))
}

impl<A: Assets> Engine<A> {
    /// 보물 아이템 (+24 구분)
    pub fn is_treasure(&self, id: u8) -> bool {
        self.items.get(id as usize * 25 + 24).is_some_and(|&v| v != 0)
    }

    /// 보물도감 갱신 (원작: 보물을 손에 넣으면 SVCMN 에 기록)
    pub fn note_treasures(&mut self) {
        let mut ids: Vec<u8> = self.inventory.iter().map(|x| x.0).collect();
        for p in &self.roster {
            if let Some(o) = self.officers.get(p) {
                ids.extend([o.weapon.map(|w| w.0), o.armor.map(|a| a.0), o.acc].into_iter().flatten());
            }
        }
        let mut changed = false;
        for id in ids {
            if (id as usize) < self.book.len() && self.is_treasure(id) && !self.book[id as usize] {
                self.book[id as usize] = true;
                changed = true;
            }
        }
        if changed {
            self.common_out = Some(common_string(self.endings, &self.book));
        }
    }

    /// 보물도감 (발견 수, 전체 수)
    pub fn book_count(&self) -> (usize, usize) {
        let all: Vec<u8> = (0..0x68u8).filter(|&i| self.is_treasure(i)).collect();
        (all.iter().filter(|&&i| self.book.get(i as usize).copied().unwrap_or(false)).count(), all.len())
    }

    pub fn set_common(&mut self, s: &str) {
        let f: Vec<&str> = s.split(';').collect();
        if f.first() != Some(&"c1") {
            return;
        }
        self.endings = f.get(1).and_then(|v| v.parse().ok()).unwrap_or(0);
        for i in f.get(2).unwrap_or(&"").split(',').filter_map(|v| v.parse::<usize>().ok()) {
            if i < self.book.len() {
                self.book[i] = true;
            }
        }
    }

    /// op10: 노인 시작. 자료가 아직이면 Err
    pub fn start_tutor(&mut self) -> Result<(), Pending> {
        let d = match self.assets.get("TUTOR.TXT") {
            Asset::Pending => return Err(Pending),
            Asset::Ready(d) => d,
            Asset::Missing => return Ok(()),
        };
        let text = String::from_utf8_lossy(&d).to_string();
        let mut lines: HashMap<String, Vec<(u16, String)>> = HashMap::new();
        for l in text.lines() {
            let mut it = l.splitn(3, '|');
            if let (Some(k), Some(w), Some(t)) = (it.next(), it.next(), it.next()) {
                lines.entry(k.to_string()).or_default().push((w.parse().unwrap_or(0x9d), t.replace("\\n", "\n")));
            }
        }
        let first = self.endings == 0;
        let queue: VecDeque<Act> = [Act::Key("intro"), Act::Key(if first { "first" } else { "again" }), Act::Key("ask"), Act::Branch(Br::Main)].into();
        self.tutor = Some(Tutor { lines, queue });
        Ok(())
    }

    fn after_lesson(&self) -> Vec<Act> {
        let mut v = Vec::new();
        let n = self.endings.count_ones() as u8;
        if n > 0 {
            let k = n.min(3);
            v.push(Act::Key(["ng1", "ng2", "ng3"][k as usize - 1]));
            v.push(Act::Branch(Br::Ng(k)));
            v.push(Act::Key("ngend"));
        }
        let (got, all) = self.book_count();
        if all > 0 && got == all {
            v.push(Act::Key("book"));
            v.push(Act::Branch(Br::Book));
        }
        v.push(Act::Key("bye"));
        v
    }

    /// 한 걸음. 끝나면 tutor = None
    pub fn tutor_tick(&mut self) {
        let Some(mut t) = self.tutor.take() else { return };
        let Some(act) = t.queue.pop_front() else {
            return;
        };
        let c = self.choice;
        match act {
            Act::Key(k) => {
                let list = t.lines.get(k).cloned().unwrap_or_default();
                let mut pages = Vec::new();
                let mut choice = None;
                for (who, text) in list {
                    if who == 0xFFFF {
                        choice = Some(text);
                        break;
                    }
                    let pid = who as usize;
                    let name = self.persons.get(pid).map(|p| p.name.clone()).unwrap_or_default();
                    let face = (pid < self.persons.len()).then(|| self.face_of(pid));
                    let ls: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
                    pages.push(story::Page { person: Some(pid), face, name, lines: ls });
                }
                if let Some(ch) = choice {
                    t.queue.push_front(Act::Sound("__choice__"));
                    t.lines.insert("__choice__".into(), vec![(0xFFFF, ch)]);
                }
                if !pages.is_empty() {
                    let (packs, assets) = (&mut self.packs, &mut self.assets);
                    let st = match &mut self.scene {
                        Scene::Battle(b) => {
                            b.ui.speaker_y = None;
                            b.ui.speaker_cell = None;
                            Some(&mut b.ui)
                        }
                        Scene::Story(s) => Some(&mut **s),
                        _ => None,
                    };
                    if let Some(st) = st {
                        if st.show_dialog(packs, assets, pages).is_err() {
                            t.queue.push_front(Act::Key(k));
                            self.tutor = Some(t);
                            return;
                        }
                        self.wait = Wait::Dialog;
                    }
                }
            }
            Act::Sound("__choice__") => {
                let ch = t.lines.get("__choice__").and_then(|v| v.first()).map(|x| x.1.clone()).unwrap_or_default();
                let st = match &mut self.scene {
                    Scene::Battle(b) => Some(&mut b.ui),
                    Scene::Story(s) => Some(&mut **s),
                    _ => None,
                };
                if let Some(st) = st {
                    st.choices = ch.split('\n').filter(|l| !l.is_empty()).map(|l| l.to_string()).collect();
                    st.choice_sel = 0;
                    self.wait = Wait::Choice;
                }
            }
            Act::Sound(name) => self.sounds.push(Sound::Effect(name.into())),
            Act::Branch(b) => {
                let next: Vec<Act> = match b {
                    Br::Main if c == 1 => vec![Act::Key("yes"), Act::Key("s1"), Act::Key("more"), Act::Branch(Br::More(2))],
                    Br::Main => {
                        let mut v = vec![Act::Key("no")];
                        v.extend(self.after_lesson());
                        v
                    }
                    Br::More(n) if c == 1 => {
                        let mut v = vec![Act::Key(["", "", "s2", "s3", "s4"][n as usize])];
                        if n < 4 {
                            v.push(Act::Key("more"));
                            v.push(Act::Branch(Br::More(n + 1)));
                        } else {
                            v.push(Act::Key("end"));
                            v.extend(self.after_lesson());
                        }
                        v
                    }
                    Br::More(_) => {
                        let mut v = vec![Act::Key("end")];
                        v.extend(self.after_lesson());
                        v
                    }
                    // 회차 보상 (원작 FUN_0043699f): 1회 Lv+1/2000, 2회 Lv+2/4000, 3회 이상 Lv+6·Lv+3/10000
                    Br::Ng(k) => {
                        let (yes, no) = (["ng1y", "ng2y", "ng3y"][k as usize - 1], ["ng1n", "ng2n", "ng3n"][k as usize - 1]);
                        let lv = match (k, c) {
                            (1, 1) => Some(1),
                            (2, 1) => Some(2),
                            (3, 1) => Some(6),
                            (3, 2) => Some(3),
                            _ => None,
                        };
                        match lv {
                            Some(l) => vec![Act::Key(yes), Act::Sound("SE39.WAV"), Act::LevelUp(l)],
                            None => vec![Act::Key(no), Act::Sound("SE10.WAV"), Act::Gold([2000, 4000, 10000][k as usize - 1])],
                        }
                    }
                    // 보물도감 완성 (원작 FUN_00436ed3): 보물 전부 또는 30000
                    Br::Book if c == 1 => vec![Act::Key("booky"), Act::Sound("SE39.WAV"), Act::AllTreasure, Act::Sound("SE14.WAV")],
                    Br::Book => vec![Act::Key("bookn"), Act::Sound("SE10.WAV"), Act::Gold(30000)],
                };
                for a in next.into_iter().rev() {
                    t.queue.push_front(a);
                }
            }
            Act::LevelUp(n) => {
                let jobs = self.jobs.clone();
                let mut o = self.officer(0);
                o.level_up(&jobs, n);
                self.officers.insert(0, o.clone());
                if let Scene::Battle(b) = &mut self.scene {
                    if let Some(u) = b.unit_of(0) {
                        let un = &mut b.units[u];
                        un.off = o.clone();
                        un.level = o.level as i32;
                        un.max_hp = o.hp_max(&self.items) as i32;
                        un.hp = un.max_hp;
                        un.mp = o.mp_max(&self.items) as i32;
                    }
                }
            }
            Act::Gold(g) => self.gold = (self.gold + g).min(999_999),
            Act::AllTreasure => {
                for id in 0..0x68u8 {
                    if self.is_treasure(id) {
                        let k = self.items.get(id as usize * 25 + 17).copied().unwrap_or(0xFF);
                        self.inventory.push((id, if k < 0x12 { 3 } else { 1 }));
                    }
                }
            }
        }
        if !t.queue.is_empty() {
            self.tutor = Some(t);
        }
    }
}
