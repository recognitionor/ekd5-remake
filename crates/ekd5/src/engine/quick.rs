//! 중간 저장 (그 화면 그대로). 저장할 수 있는 때: 전투 중 내 차례 명령 대기, 본진 메뉴.
//! 그림·자료 같은 캐시는 저장하지 않고 불러온 뒤 다시 읽음 (맵 그림은 성문 등 물체를 다시 덧그림).

use super::battle::{self, Battle, Unit};
use super::camp::SortieRule;
use super::{Assets, Engine, Scene, Sound, Wait};
use crate::officer::Officer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone)]
pub struct BattleSave {
    pub map_no: usize,
    pub units: Vec<Unit>,
    pub turn: i32,
    pub turn_limit: i32,
    pub cursor: (i32, i32),
    pub cam: (i32, i32),
    pub slots: Vec<(i32, i32)>,
    pub targets: Vec<usize>,
    pub objective: String,
    pub inventory: Vec<(u8, u8)>,
    pub rng: u32,
    pub terrain: Vec<u8>,
    pub overlay: Vec<u8>,
    pub objs: Vec<(i32, i32, u8, u8)>,
    pub goal_zone: Option<(i32, i32, i32, i32)>,
    pub list47: Vec<usize>,
    #[serde(default)]
    pub win_target: Option<usize>,
    #[serde(default)]
    pub lose_target: Option<usize>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct QuickSave {
    pub v: u32,
    pub stage: u32,
    pub flags: Vec<usize>,
    pub roster: Vec<usize>,
    pub officers: Vec<Officer>,
    pub gold: i32,
    pub inventory: Vec<(u8, u8)>,
    pub fame: i32,
    pub face_state: u8,
    pub chapter: u8,
    pub sprite_over: Vec<(usize, u16)>,
    pub music: Option<i32>,
    pub mcall: Option<u8>,
    /// 전투 다시 하기용 단계 시작 값 (플래그, 합류, 장수, 소지품, 돈)
    pub saved: Option<(Vec<usize>, Vec<usize>, Vec<Officer>, Vec<(u8, u8)>, i32)>,
    pub sortie_rule: Option<SortieRule>,
    pub sortie_chosen: Option<Vec<usize>>,
    pub camp_rule: Option<SortieRule>,
    /// 본진 메뉴 (본진 절 번호)
    pub camp_section: Option<usize>,
    #[serde(default)]
    pub map: Option<(i32, i32)>,
    /// 이야기 장면 인물 (인물, 그림, x, y, 방향, 자세) 과 장소·장 이름
    #[serde(default)]
    pub actors: Vec<(usize, u16, i32, i32, i32, i32)>,
    #[serde(default)]
    pub place: (String, String),
    pub battle: Option<BattleSave>,
}

fn on(flags: &[bool]) -> Vec<usize> {
    flags.iter().enumerate().filter(|(_, &f)| f).map(|(i, _)| i).collect()
}

impl<A: Assets> Engine<A> {
    /// 지금 중간 저장할 수 있나
    pub fn can_quick_save(&self) -> bool {
        if self.error.is_some() || self.tutor.is_some() || self.wait != Wait::None {
            return false;
        }
        if let Some(c) = &self.camp {
            return matches!(c.view, super::camp::CampView::Menu(_) | super::camp::CampView::Field);
        }
        let running = self.vm.as_ref().is_some_and(|v| v.running);
        match &self.scene {
            Scene::Battle(b) => {
                !running
                    && b.started
                    && matches!(b.phase, battle::Phase::PlayerIdle)
                    && b.fx.is_none()
                    && b.fx_req.is_none()
                    && b.strikes.is_empty()
                    && b.duel.is_none()
                    && !b.units.iter().any(|u| !u.path.is_empty() || u.leave.is_some())
            }
            _ => false,
        }
    }

    /// 중간 저장 문자열 (JSON). 저장할 수 없는 때면 None
    pub fn quick_save(&self) -> Option<String> {
        if !self.can_quick_save() {
            return None;
        }
        let battle = match &self.scene {
            Scene::Battle(b) if self.camp.is_none() => Some(BattleSave {
                map_no: b.map_no,
                units: b.units.clone(),
                turn: b.turn,
                turn_limit: b.turn_limit,
                cursor: b.cursor,
                cam: b.cam,
                slots: b.slots.clone(),
                targets: b.targets.clone(),
                objective: b.objective.clone(),
                inventory: b.inventory.clone(),
                rng: b.rng,
                terrain: b.terrain.clone(),
                overlay: b.overlay.clone(),
                objs: b.objs.clone(),
                goal_zone: b.goal_zone,
                list47: b.list47.clone(),
                win_target: b.win_target,
                lose_target: b.lose_target,
            }),
            _ => None,
        };
        let q = QuickSave {
            v: 1,
            stage: self.stage,
            flags: on(&self.flags),
            roster: self.roster.clone(),
            officers: self.officers.values().cloned().collect(),
            gold: self.gold,
            inventory: self.inventory.clone(),
            fame: self.fame,
            face_state: self.face_state,
            chapter: self.chapter,
            sprite_over: self.sprite_over.iter().map(|(a, b)| (*a, *b)).collect(),
            music: self.music_now,
            mcall: self.mcall_loaded,
            saved: self.saved.as_ref().map(|(f, r, o, i, g)| (on(f), r.clone(), o.values().cloned().collect(), i.clone(), *g)),
            sortie_rule: self.sortie_rule.clone(),
            sortie_chosen: self.sortie_chosen.clone(),
            camp_rule: self.camp_rule.clone(),
            camp_section: self.camp.as_ref().map(|c| c.section),
            map: self.last_map,
            actors: match &self.scene {
                Scene::Story(st) => st.actors.iter().map(|a| (a.id, a.sprite, a.x.round() as i32, a.y.round() as i32, a.dir, a.pose)).collect(),
                _ => Vec::new(),
            },
            place: match &self.scene {
                Scene::Story(st) => (st.place_name.clone(), st.chapter_name.clone()),
                _ => Default::default(),
            },
            battle,
        };
        serde_json::to_string(&q).ok()
    }

    /// 중간 저장 불러오기 (제목 화면 전에). 실제 장면은 단계 자료를 읽을 때 만듦
    pub fn quick_restore(&mut self, s: &str) -> bool {
        let Ok(q) = serde_json::from_str::<QuickSave>(s) else { return false };
        self.stage = q.stage;
        self.flags.fill(false);
        for &i in &q.flags {
            if i < self.flags.len() {
                self.flags[i] = true;
            }
        }
        self.roster = q.roster.clone();
        self.officers = q.officers.iter().map(|o| (o.person, o.clone())).collect();
        self.gold = q.gold;
        self.inventory = q.inventory.clone();
        self.fame = q.fame;
        self.face_state = q.face_state;
        self.chapter = q.chapter;
        self.sprite_over = q.sprite_over.iter().cloned().collect();
        self.mcall_loaded = q.mcall;
        self.saved = q.saved.as_ref().map(|(f, r, o, i, g)| {
            let mut fl = vec![false; self.flags.len()];
            for &k in f {
                if k < fl.len() {
                    fl[k] = true;
                }
            }
            let om: HashMap<usize, Officer> = o.iter().map(|x| (x.person, x.clone())).collect();
            (fl, r.clone(), om, i.clone(), *g)
        });
        self.sortie_rule = q.sortie_rule.clone();
        self.sortie_chosen = q.sortie_chosen.clone();
        self.camp_rule = q.camp_rule.clone();
        self.quick = Some(q);
        true
    }

    /// 단계 스크립트를 읽은 직후: 중간 저장이 있으면 그 장면으로 (반환 true = 처리함)
    pub(super) fn apply_quick(&mut self, vm: &mut crate::vm::Vm) -> bool {
        let Some(q) = self.quick.take() else { return false };
        if let Some(m) = q.music {
            self.sounds.push(Sound::Music(Some(m)));
            self.music_now = Some(m);
        }
        if let Some(bs) = q.battle {
            let mut b = Battle::new(bs.map_no, self.jobs.clone(), self.terrain_tab.clone());
            b.cheat = self.cheat;
            b.items = self.items.clone();
            b.strategies = self.strategies.clone();
            b.icons = self.icons.clone();
            b.names = std::rc::Rc::new(self.persons.iter().map(|p| p.name.clone()).collect());
            b.job_names = self.job_names.clone();
            b.terrain_names = self.terrain_names.clone();
            b.units = bs.units;
            for u in &mut b.units {
                u.px = (u.x * battle::CELL) as f32;
                u.py = (u.y * battle::CELL) as f32;
            }
            b.turn = bs.turn;
            b.turn_limit = bs.turn_limit;
            b.cursor = bs.cursor;
            b.cam = bs.cam;
            b.slots = bs.slots;
            b.targets = bs.targets;
            b.objective = bs.objective;
            b.inventory = bs.inventory;
            // 원작은 불러와도 난수가 이어지지 않음 (전투마다 시각 씨앗) → 웹에서는 새로
            b.rng = self.fresh_seed().unwrap_or(bs.rng);
            b.objs = bs.objs;
            b.goal_zone = bs.goal_zone;
            b.list47 = bs.list47;
            b.win_target = bs.win_target;
            b.lose_target = bs.lose_target;
            b.restore_terrain = Some((bs.terrain, bs.overlay));
            b.started = true;
            b.phase = battle::Phase::PlayerIdle;
            self.scene = Scene::Battle(Box::new(b));
            // 사건 검사용 절 1 에서 멈춘 상태
            vm.section = 1.min(vm.sections().saturating_sub(1));
            vm.running = false;
            self.end_checked = false;
            return true;
        }
        if let Some(sec) = q.camp_section {
            self.scene = Scene::Story(Box::default());
            vm.section = sec.saturating_sub(1);
            vm.running = false;
            self.camp_pending = false;
            self.pending_camp = Some(sec);
            self.pending_map = q.map;
            self.pending_actors = q.actors.clone();
            if let Scene::Story(st) = &mut self.scene {
                st.place_name = q.place.0.clone();
                st.chapter_name = q.place.1.clone();
                st.title_timer = 150;
            }
            return true;
        }
        false
    }

    /// 이야기 장면 되살리기 (그림·인물). 아직 읽는 중이면 true
    pub(super) fn restore_story(&mut self) -> bool {
        let Scene::Story(st) = &mut self.scene else { return false };
        if let Some((mode, v)) = self.pending_map {
            match st.load_map(&mut self.packs, &mut self.assets, mode, v) {
                Err(super::Pending) => return true,
                Ok(pal) => {
                    if let Some(p) = pal {
                        self.screen.pal = p;
                    }
                    self.pending_map = None;
                    self.last_map = Some((mode, v));
                }
            }
        }
        while let Some(&(id, sprite, x, y, dir, pose)) = self.pending_actors.first() {
            if st.place(&mut self.packs, &mut self.assets, id, sprite, x, y, dir, pose).is_err() {
                return true;
            }
            self.pending_actors.remove(0);
        }
        false
    }
}
