//! 본진 (원작 op06 [1, 출진 수, 반드시 출진 ×5, 출진 불가 ×5] → 단추 창 4개 FUN_0041c2b2).
//! 화면 오른쪽 아래 50×50 단추 (FUN_0041c4b2): 출진 (440,390) · 장비 (490) · 구입 (540) · 매각 (590), 그림 EKD5.EXE 0x155~0x158.
//! 대화는 화면의 장수를 눌러서 (원작 op2d = 누른 인물이 그 장수인가, 사건 플래그 0x20).
//! 다음 절의 블록들이 선택지: 머리 op07 = 출진, op2d [장수] = 그 장수와 대화.
//! 출진 단추 → 출진 장수 선택 (FUN_0041aac4). 결정하면 출진 블록, 취소하면 본진 그대로.
//! 가격 (원작 아이템 기록 +0x12 = 파일 +19): 구입 ×100 (FUN_0040e4c0), 매각 ×75, 무기·방어구는 ×(레벨+1)/2, 255 = 팔 수 없음 (FUN_0040cc2c).
//! 웹 메뉴 단추 (또는 취소 키) = 보조 목록 (대화 상대·도구 보기·보물도감 등).
//! 원작의 상점·장비 화면은 윈도 대화상자라 화면은 새로 만들고, 규칙은 아이템 표(종류·가격·고유 여부)와 직업 장비 가능 표를 따른다.
//! 출진 선택 (원작 FUN_0041aac4 / FUN_004606b3): op4a [출진 수, 반드시 ×5, 불가 ×5].

use super::assets::Assets;
use super::ui::{ListBox, Pick};
use super::{key, Engine, Scene};
use crate::officer::{item_name, item_stat};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SortieRule {
    pub max: usize,
    pub forced: Vec<usize>,
    pub excluded: Vec<usize>,
}

impl SortieRule {
    /// 원작 op06/op4a 인자 (FUN_00417f55 / FUN_0041aac4): 최대 (0 이면 1), 반드시 출진 ×5 (0 = 조조는 따로 처리, 0xFFFF 없음),
    /// 출진 불가 ×5 (0xFFFF 없음, 0 = 조조 불가)
    pub fn from_args(max: i32, list: &[i32]) -> Self {
        let forced = list[..5.min(list.len())].iter().filter(|&&x| x != 0 && x != 0xFFFF).map(|&x| x as usize).collect();
        let excluded = list.get(5..10).unwrap_or(&[]).iter().filter(|&&x| x != 0xFFFF).map(|&x| x as usize).collect();
        Self { max: if max <= 0 || max == 0xFFFF { 1 } else { max as usize }, forced, excluded }
    }
}

#[derive(Clone, Debug)]
pub enum CampView {
    /// 원작 화면: 장수 누르기 + 단추 4개
    Field,
    Menu(ListBox),
    /// 매각 (소지품 번호)
    Talk(ListBox, Vec<usize>),
    /// 단추 첫 사용 지시 대사 중 (원작 FUN_0041c4b2: 0 매각, 1 구입, 2 장비) — 끝나면 그 화면
    Order(u8),
    /// 목록 + 줄마다 아이템 (아이템, 레벨)
    Items(ListBox, Vec<Option<(u8, u8)>>),
    /// 대화 블록 실행 중
    Running,
}

#[derive(Clone, Debug)]
pub struct Camp {
    pub section: usize,
    pub rule: SortieRule,
    pub view: CampView,
    /// 블록 머리 (op07 출진 / op2d 대화 장수) → 본문 위치
    pub depart: Option<usize>,
    pub talks: Vec<(usize, usize)>,
    /// 이번 본진에서 단추 지시 대사를 했나 (매각, 구입, 장비)
    pub ordered: [bool; 3],
    /// 아이템 정보 창 (원작 대화상자 167 — 소유 아이템 일람·보물도감에서 누르면): 아이템, 레벨, Exp
    pub info: Option<(u8, u8, Option<u8>)>,
}

/// 본진 단추 (화면 x, 그림 번호: 묶음 4 안 순서 0 출진 1 매각 2 구입 3 장비)
pub const CAMP_BTNS: [(i32, usize); 4] = [(440, 0), (490, 3), (540, 2), (590, 1)];
pub const CAMP_BTN_Y: i32 = 390;

fn menu_box() -> ListBox {
    let mut m = ListBox::new("본진", 220);
    m.y = 120;
    for t in ["대화", "장비", "구입", "매각", "도구 보기", "보물도감", "장수 정보", "출진"] {
        m.push(t, "", true);
    }
    m
}

impl<A: Assets> Engine<A> {
    /// 아이템 구입 가격 (원작 FUN_0040e4c0: 기록 +0x12 = 파일 +19, ×100)
    pub fn item_price(&self, id: u8) -> i32 {
        self.items.get(id as usize * 25 + 19).copied().unwrap_or(0) as i32 * 100
    }

    /// 매각 가격 (원작 FUN_0040cc2c: 값×300/4, 무기·방어구는 ×(레벨+1)/2). 255 = 팔 수 없음
    pub fn sell_price(&self, id: u8, lv: u8) -> Option<i32> {
        let v = *self.items.get(id as usize * 25 + 19)?;
        if v == 0xFF {
            return None;
        }
        let p = v as i32 * 300 / 4;
        Some(if self.item_type(id) < 0x12 { p * (lv as i32 + 1) / 2 } else { p })
    }

    fn item_type(&self, id: u8) -> u8 {
        self.items.get(id as usize * 25 + 17).copied().unwrap_or(0xFF)
    }

    /// 소모품 (회복 등, 종류 0x3f..)
    pub fn item_consumable(&self, id: u8) -> bool {
        self.item_type(id) >= 0x3F
    }

    /// 본진 열기 (원작 op06 1)
    pub fn open_camp(&mut self, section: usize) {
        let rule = self.camp_rule.clone().unwrap_or(SortieRule { max: 12, forced: vec![], excluded: vec![] });
        let mut depart = None;
        let mut talks = Vec::new();
        if let Some(vm) = self.vm.as_mut() {
            if let Ok(blocks) = vm.blocks(section) {
                for (start, _) in blocks {
                    vm.pc = start;
                    let mut who = None;
                    let mut is_depart = false;
                    while let Ok((op, args)) = vm.fetch() {
                        match op {
                            0 => break,
                            7 => is_depart = true,
                            0x2D => who = args.first().map(|a| a.i() as usize),
                            _ => {}
                        }
                    }
                    let body = vm.pc;
                    if is_depart {
                        depart = Some(body);
                    } else if let Some(p) = who {
                        talks.push((p, body));
                    }
                }
            }
        }
        self.log.push("본진".into());
        self.last_camp_rule = Some(rule.clone());
        self.camp = Some(Camp { section, rule, view: CampView::Field, depart, talks, ordered: [false; 3], info: None });
    }

    pub(super) fn run_camp_block(&mut self, body: usize, section: usize) {
        if let Some(vm) = self.vm.as_mut() {
            vm.section = section;
            vm.ends.clear();
            vm.pc = body;
            if vm.enter_body(true).is_ok() {
                vm.running = true;
            }
        }
    }

    /// 본진 입력. 반환: 처리했으면 true (스크립트 멈춤)
    pub fn update_camp(&mut self) -> bool {
        // 재정비용 자동 저장: 이 단계의 작전회의 화면이 처음 열렸을 때 한 번 (원작에 없는 편의 기능)
        if self.camp.is_some() && self.camp_snap.as_ref().is_none_or(|s| s.0 != self.stage) {
            if let Some(q) = self.quick_save() {
                self.camp_snap = Some((self.stage, q));
                self.log.push("재정비용 작전회의 저장".into());
            }
        }
        let Some(mut camp) = self.camp.take() else { return false };
        let inp = self.input.clone();
        let tapped = inp.pressed & key::DECIDE != 0;
        // 메뉴 결정 SE02 / 취소 SE01 (원작 FUN_0043f648)
        if !matches!(camp.view, CampView::Running) {
            if tapped {
                self.sounds.push(super::Sound::Effect("SE02.WAV".into()));
            } else if inp.pressed & key::CANCEL != 0 {
                self.sounds.push(super::Sound::Effect("SE01.WAV".into()));
            }
        }
        // 지시 대사가 끝나면 그 화면
        if let CampView::Order(k) = camp.view {
            if self.wait != super::Wait::None {
                self.camp = Some(camp);
                return false;
            }
            camp.view = CampView::Field;
            self.camp_action(k);
        }
        // 대화 블록이 끝나면 메뉴로
        if matches!(camp.view, CampView::Running) {
            if self.vm.as_ref().is_some_and(|v| v.running) || self.wait != super::Wait::None {
                self.camp = Some(camp);
                return false;
            }
            camp.view = CampView::Field;
        }
        if std::mem::take(&mut self.camp_menu) {
            camp.view = CampView::Menu(menu_box());
        }
        // 아이템 정보 창: 아무 데나 누르거나 취소하면 닫음
        if camp.info.is_some() {
            if inp.click || inp.alt || inp.pressed & (key::CANCEL | key::DECIDE) != 0 {
                camp.info = None;
            }
            self.camp = Some(camp);
            return true;
        }
        let persons = self.persons.clone();
        let name = |p: usize| persons.get(p).map(|x| x.name.clone()).unwrap_or_default();
        match camp.view.clone() {
            CampView::Field => {
                // 단추 (원작 FUN_0041c4b2 자리, 48×48 그림이 창 (1,1))
                let mut act = None;
                let mut talk = None;
                if inp.click {
                    if let Some((mx, my)) = inp.mouse {
                        for (k, &(bx, _)) in CAMP_BTNS.iter().enumerate() {
                            if mx >= bx && mx < bx + 50 && my >= CAMP_BTN_Y && my < CAMP_BTN_Y + 50 {
                                act = Some(k);
                            }
                        }
                        // 장수 누르기 → 그 장수 대화 블록 (앞에 그려진 사람 먼저)
                        if act.is_none() {
                            if let Scene::Story(st) = &self.scene {
                                let mut hit: Option<(f32, usize)> = None;
                                for a in &st.actors {
                                    let (sx, sy) = st.screen_pos(a.x, a.y);
                                    if mx >= sx && mx < sx + 48 && my >= sy - 64 && my < sy && hit.is_none_or(|h| a.x + a.y > h.0) {
                                        hit = Some((a.x + a.y, a.id));
                                    }
                                }
                                talk = hit.map(|h| h.1);
                            }
                        }
                    }
                } else if inp.pressed & key::CANCEL != 0 || tapped {
                    camp.view = CampView::Menu(menu_box());
                }
                if let Some(p) = talk {
                    if let Some(&(_, body)) = camp.talks.iter().find(|t| t.0 == p) {
                        self.sounds.push(super::Sound::Effect("SE02.WAV".into()));
                        let s = camp.section;
                        camp.view = CampView::Running;
                        self.camp = Some(camp);
                        self.run_camp_block(body, s);
                        return false;
                    }
                }
                match act {
                    Some(0) => return self.camp_depart(camp),
                    Some(1) => self.camp_order(&mut camp, 2),
                    Some(2) => self.camp_order(&mut camp, 1),
                    Some(3) => self.camp_order(&mut camp, 0),
                    _ => {}
                }
                if act.is_some() {
                    self.sounds.push(super::Sound::Effect("SE02.WAV".into()));
                }
            }
            CampView::Menu(mut m) => match m.update(&inp, tapped) {
                Pick::Row(0) => {
                    let mut l = ListBox::new("누구와 이야기할까", 260);
                    let mut ps = Vec::new();
                    for (p, _) in &camp.talks {
                        l.push(name(*p), "", true);
                        ps.push(*p);
                    }
                    if ps.is_empty() {
                        l.push("이야기할 사람이 없다", "", false);
                    }
                    camp.view = CampView::Talk(l, ps);
                }
                Pick::Row(1) => {
                    camp.view = CampView::Field;
                    self.camp_order(&mut camp, 2);
                }
                Pick::Row(2) => {
                    camp.view = CampView::Field;
                    self.camp_order(&mut camp, 1);
                }
                Pick::Row(3) => {
                    camp.view = CampView::Field;
                    self.camp_order(&mut camp, 0);
                }
                Pick::Row(4) => {
                    let ids = self.inventory.iter().map(|&(i, l)| Some((i, l))).collect();
                    camp.view = CampView::Items(self.items_box(), ids);
                }
                Pick::Row(5) => {
                    let ids = (0..0x68u8).filter(|&i| self.is_treasure(i)).map(|i| self.book.get(i as usize).copied().unwrap_or(false).then_some((i, 1))).collect();
                    camp.view = CampView::Items(self.book_box(), ids);
                }
                Pick::Row(6) => {
                    // 장수 정보 (원작 대화상자 166)
                    let list: Vec<super::info::Who> = self.roster.iter().map(|&p| super::info::Who::Person(p)).collect();
                    camp.view = CampView::Field;
                    self.open_info(list, 0);
                }
                Pick::Row(7) => return self.camp_depart(camp),
                Pick::Cancel => camp.view = CampView::Field,
                _ => camp.view = CampView::Menu(m),
            },
            CampView::Talk(mut l, ps) => match l.update(&inp, tapped) {
                Pick::Row(k) => {
                    if let Some(&(_, body)) = camp.talks.iter().find(|t| Some(&t.0) == ps.get(k)) {
                        let s = camp.section;
                        camp.view = CampView::Running;
                        self.camp = Some(camp);
                        self.run_camp_block(body, s);
                        return false;
                    }
                }
                Pick::Cancel => camp.view = CampView::Field,
                Pick::None => camp.view = CampView::Talk(l, ps),
            },
            // 줄을 누르면 아이템 정보 (원작 FUN_004696ed / FUN_0047234c → FUN_0040c2e7)
            CampView::Items(mut l, ids) => match l.update(&inp, tapped) {
                Pick::Cancel => camp.view = CampView::Field,
                Pick::Row(k) => {
                    if let Some(&Some((id, lv))) = ids.get(k) {
                        let gear = crate::officer::item_stat(&self.items, id).is_some();
                        camp.info = Some((id, lv, gear.then_some(0)));
                    }
                    camp.view = CampView::Items(l, ids);
                }
                _ => camp.view = CampView::Items(l, ids),
            },
            CampView::Running | CampView::Order(_) => {}
        }
        self.camp = Some(camp);
        true
    }

    /// 본진 단추 (원작 FUN_0041c4b2): 이번 본진에서 처음 누르면 조조가 지시하고 담당 장수가 "예!" 한 뒤 그 화면.
    /// 담당 = 장마다 상점 자료 (DATA.E5[2]) 의 +4 (매각·구입), +0 (장비 "창고를 열어라!")
    fn camp_order(&mut self, camp: &mut Camp, k: u8) {
        self.sounds.push(super::Sound::Effect("SE02.WAV".into()));
        if !camp.ordered[k as usize] {
            camp.ordered[k as usize] = true;
            let ki = (self.stage as usize / 2).min(57);
            let rec = self.shops.get(ki * 36..ki * 36 + 4).map(|r| r.to_vec()).unwrap_or_default();
            if rec.len() == 4 {
                let who = if k == 2 { rec[0] as usize | (rec[1] as usize) << 8 } else { rec[2] as usize | (rec[3] as usize) << 8 };
                if who != 0 && who < self.persons.len() && self.roster.contains(&who) {
                    let name = self.persons[who].name.clone();
                    let order = ["물자를 매각하라!", "물자를 조달하라!", "창고를 열어라!"][k as usize];
                    let pages = vec![
                        super::story::Page { person: Some(0), face: Some(self.face_of(0)), name: self.persons[0].name.clone(), lines: vec![format!("{name}, {order}")] },
                        super::story::Page { person: Some(who), face: Some(self.face_of(who)), name, lines: vec!["예!".into()] },
                    ];
                    if self.battle_dialog(pages) {
                        camp.view = CampView::Order(k);
                        return;
                    }
                }
            }
        }
        self.camp_action(k);
    }

    fn camp_action(&mut self, k: u8) {
        match k {
            0 => self.open_sell(),
            1 => self.open_shop(),
            _ => self.open_equip(),
        }
    }

    /// 출진 단추 (원작 FUN_0041aac4): 출진 장수를 고른 뒤 출진 블록. 고를 것이 없으면 바로
    fn camp_depart(&mut self, camp: Camp) -> bool {
        let Some(body) = camp.depart else {
            self.camp = Some(camp);
            return true;
        };
        let s = camp.section;
        match self.open_sortie(camp.rule.clone(), Some((body, s))) {
            Some(ch) => {
                self.sortie_chosen = Some(ch);
                self.camp = None;
                self.run_camp_block(body, s);
                false
            }
            None => {
                self.camp = Some(camp);
                true
            }
        }
    }

    fn items_box(&self) -> ListBox {
        let mut l = ListBox::new(&format!("소지 아이템   소지금 {}", self.gold), 400);
        for &(it, lv) in &self.inventory {
            let s = item_stat(&self.items, it).map_or(String::new(), |_| format!("Lv{lv}"));
            l.push(item_name(&self.items, it), s, true);
        }
        if self.inventory.is_empty() {
            l.push("(없음)", "", false);
        }
        let ids: Vec<Option<u8>> = self.inventory.iter().map(|x| Some(x.0)).collect();
        self.item_icons(&mut l, &ids);
        l
    }

    /// 목록 줄마다 아이템 아이콘
    fn item_icons(&self, l: &mut ListBox, ids: &[Option<u8>]) {
        l.icons = ids.iter().map(|id| id.and_then(|i| self.icons.item(&self.items, i, &self.screen.pal))).collect();
    }

    /// 보물도감 (원작 "지금까지 발견한 보물: n / m", 회차 공통)
    fn book_box(&self) -> ListBox {
        let (got, all) = self.book_count();
        let mut l = ListBox::new(&format!("보물도감   지금까지 발견한 보물: {got:2} / {all:2}"), 460);
        let mut ids = Vec::new();
        for id in (0..0x68u8).filter(|&i| self.is_treasure(i)) {
            if self.book.get(id as usize).copied().unwrap_or(false) {
                l.push(item_name(&self.items, id), "", true);
                ids.push(Some(id));
            } else {
                l.push("？？？", "", false);
                ids.push(None);
            }
        }
        self.item_icons(&mut l, &ids);
        l
    }

    pub fn render_camp(&mut self) {
        let Some(f) = self.font.as_ref() else { return };
        let hi = if let Scene::Battle(b) = &self.scene { b.col[0] } else { crate::palette::nearest(&self.screen.pal, [208, 176, 112]) };
        let Some(c) = &self.camp else { return };
        if matches!(c.view, CampView::Running) || self.sortie_ui.is_some() {
            return;
        }
        // 단추 4개 (원작 창 50×50, 그림 48×48 이 (1,1))
        let pal = self.screen.pal;
        let pics: Vec<_> = CAMP_BTNS.iter().map(|&(_, k)| self.icons.pic(super::icons::CAMP, k, &pal)).collect();
        let scr = &mut self.screen;
        for (i, &(bx, _)) in CAMP_BTNS.iter().enumerate() {
            super::story::frame_box(scr, bx, CAMP_BTN_Y, 50, 50);
            if let Some(p) = &pics[i] {
                scr.blit(p, bx + 1, CAMP_BTN_Y + 1, None);
            }
        }
        let box_ = match &c.view {
            CampView::Menu(l) | CampView::Talk(l, _) | CampView::Items(l, _) => Some(l),
            CampView::Field | CampView::Running | CampView::Order(_) => None,
        };
        if let Some(l) = box_ {
            l.render(scr, f, hi);
        }
        if let Some((id, lv, exp)) = c.info {
            self.draw_item_info(id, lv, exp);
        }
    }
}
