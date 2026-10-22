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
    Sell(ListBox, Vec<usize>),
    SellConfirm(ListBox, usize),
    Talk(ListBox, Vec<usize>),
    ShopKind(ListBox),
    Shop(ListBox, Vec<u8>, bool),
    Items(ListBox),
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
}

/// 본진 단추 (화면 x, 그림 번호: 묶음 4 안 순서 0 출진 1 매각 2 구입 3 장비)
pub const CAMP_BTNS: [(i32, usize); 4] = [(440, 0), (490, 3), (540, 2), (590, 1)];
pub const CAMP_BTN_Y: i32 = 390;

fn menu_box() -> ListBox {
    let mut m = ListBox::new("본진", 220);
    m.y = 120;
    for t in ["대화", "장비", "구입", "매각", "도구 보기", "보물도감", "출진"] {
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

    fn sell_box(&self) -> (ListBox, Vec<usize>) {
        let mut l = ListBox::new("매각", 400);
        let mut idx = Vec::new();
        let mut ids = Vec::new();
        for (i, &(it, lv)) in self.inventory.iter().enumerate() {
            let lvs = if self.item_type(it) < 0x12 { format!(" Lv{lv}") } else { String::new() };
            match self.sell_price(it, lv) {
                Some(p) => l.push(format!("{}{lvs}", item_name(&self.items, it)), format!("{p}"), true),
                None => l.push(format!("{}{lvs}", item_name(&self.items, it)), "팔 수 없음", false),
            }
            idx.push(i);
            ids.push(Some(it));
        }
        if idx.is_empty() {
            l.push("팔 물건이 없다", "", false);
        }
        self.item_icons(&mut l, &ids);
        (l, idx)
    }

    fn item_type(&self, id: u8) -> u8 {
        self.items.get(id as usize * 25 + 17).copied().unwrap_or(0xFF)
    }

    /// 고유 아이템 (팔지 않음, +24 = 1)
    fn item_unique(&self, id: u8) -> bool {
        self.items.get(id as usize * 25 + 24).copied().unwrap_or(1) != 0
    }

    /// 소모품 (회복 등, 종류 0x3f..)
    pub fn item_consumable(&self, id: u8) -> bool {
        self.item_type(id) >= 0x3F
    }

    /// 장비 칸 (0 무기, 1 방어구, 2 보조)
    fn item_slot(&self, id: u8) -> Option<u8> {
        match self.item_type(id) {
            0..=0xD => Some(0),
            0xE..=0x11 => Some(1),
            0x12..=0x3E => Some(2),
            _ => None,
        }
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
        self.camp = Some(Camp { section, rule, view: CampView::Field, depart, talks });
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
                    Some(1) => self.open_equip(),
                    Some(2) => {
                        let mut l = ListBox::new("구입", 220);
                        l.y = 120;
                        l.push("무기·방어구점", "", true);
                        l.push("도구점", "", true);
                        camp.view = CampView::ShopKind(l);
                    }
                    Some(3) => {
                        let (l, idx) = self.sell_box();
                        camp.view = CampView::Sell(l, idx);
                    }
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
                    self.open_equip();
                    camp.view = CampView::Field;
                }
                Pick::Row(2) => {
                    let mut l = ListBox::new("구입", 220);
                    l.y = 120;
                    l.push("무기·방어구점", "", true);
                    l.push("도구점", "", true);
                    camp.view = CampView::ShopKind(l);
                }
                Pick::Row(3) => {
                    let (l, idx) = self.sell_box();
                    camp.view = CampView::Sell(l, idx);
                }
                Pick::Row(4) => camp.view = CampView::Items(self.items_box()),
                Pick::Row(5) => camp.view = CampView::Items(self.book_box()),
                Pick::Row(6) => return self.camp_depart(camp),
                Pick::Cancel => camp.view = CampView::Field,
                _ => camp.view = CampView::Menu(m),
            },
            CampView::Sell(mut l, idx) => match l.update(&inp, tapped) {
                Pick::Row(k) => {
                    if let Some(&i) = idx.get(k) {
                        let (it, lv) = self.inventory[i];
                        let p = self.sell_price(it, lv).unwrap_or(0);
                        // 원작: "… 금 n 입니다. 매각해도 좋습니까?"
                        let mut c = ListBox::new(&format!("{} 금 {p} — 매각할까", item_name(&self.items, it)), 360);
                        c.y = 160;
                        c.push("예", "", true);
                        c.push("아니오", "", true);
                        camp.view = CampView::SellConfirm(c, i);
                    }
                }
                Pick::Cancel => camp.view = CampView::Field,
                Pick::None => camp.view = CampView::Sell(l, idx),
            },
            CampView::SellConfirm(mut c, i) => match c.update(&inp, tapped) {
                Pick::Row(0) => {
                    if let Some(&(it, lv)) = self.inventory.get(i) {
                        if let Some(p) = self.sell_price(it, lv) {
                            self.inventory.remove(i);
                            self.gold = (self.gold + p).min(999_999);
                            self.sounds.push(super::Sound::Effect("SE10.WAV".into()));
                        }
                    }
                    let (l, idx) = self.sell_box();
                    camp.view = CampView::Sell(l, idx);
                }
                Pick::Row(_) | Pick::Cancel => {
                    let (l, idx) = self.sell_box();
                    camp.view = CampView::Sell(l, idx);
                }
                Pick::None => camp.view = CampView::SellConfirm(c, i),
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
            CampView::ShopKind(mut l) => match l.update(&inp, tapped) {
                Pick::Row(k) => {
                    let (b, ids) = self.shop_box(k == 1);
                    camp.view = CampView::Shop(b, ids, k == 1);
                }
                Pick::Cancel => camp.view = CampView::Field,
                Pick::None => camp.view = CampView::ShopKind(l),
            },
            CampView::Shop(mut l, ids, tools) => match l.update(&inp, tapped) {
                Pick::Row(k) => {
                    let it = ids[k];
                    let price = self.item_price(it);
                    if self.gold >= price && self.inventory.len() < super::equip::STORE_MAX {
                        self.gold -= price;
                        self.inventory.push((it, 1));
                        // 사기 (원작 FUN_0046fd73: SE10)
                        self.sounds.push(super::Sound::Effect("SE10.WAV".into()));
                    }
                    let (b, ids) = self.shop_box(tools);
                    let sel = l.sel;
                    let mut b = b;
                    b.sel = sel;
                    b.top = l.top;
                    camp.view = CampView::Shop(b, ids, tools);
                }
                Pick::Cancel => {
                    let mut l = ListBox::new("구입", 220);
                    l.y = 120;
                    l.push("무기·방어구점", "", true);
                    l.push("도구점", "", true);
                    camp.view = CampView::ShopKind(l);
                }
                Pick::None => camp.view = CampView::Shop(l, ids, tools),
            },
            CampView::Items(mut l) => match l.update(&inp, tapped) {
                Pick::Cancel => camp.view = CampView::Field,
                _ => camp.view = CampView::Items(l),
            },
            CampView::Running => {}
        }
        self.camp = Some(camp);
        true
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

    fn shop_box(&self, tools: bool) -> (ListBox, Vec<u8>) {
        let mut l = ListBox::new(&format!("{}   소지금 {}", if tools { "도구점" } else { "무기·방어구점" }, self.gold), 400);
        let mut ids = Vec::new();
        // 진행에 따라 파는 물건이 늘어난다 (가격 상한 — 원작 판매 목록은 미분석, 근사)
        let cap = (300 + self.stage as i32 * 120) * 10;
        for id in 0..(self.items.len() / 25) as u8 {
            if self.item_unique(id) || self.item_price(id) <= 0 {
                continue;
            }
            let cons = self.item_consumable(id);
            if cons != tools || (!tools && self.item_price(id) > cap) || (!tools && self.item_slot(id).is_none()) {
                continue;
            }
            let have = self.inventory.iter().filter(|x| x.0 == id).count();
            l.push(item_name(&self.items, id), format!("{}금{}", self.item_price(id), if have > 0 { format!(" ({have})") } else { String::new() }), self.gold >= self.item_price(id));
            ids.push(id);
        }
        l.footer = "누르면 하나 삽니다".into();
        self.item_icons(&mut l, &ids.iter().map(|&i| Some(i)).collect::<Vec<_>>());
        (l, ids)
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
            CampView::Menu(l) | CampView::Talk(l, _) | CampView::ShopKind(l) | CampView::Shop(l, ..) | CampView::Items(l) | CampView::Sell(l, _) => Some(l),
            CampView::SellConfirm(l, _) => Some(l),
            CampView::Field | CampView::Running => None,
        };
        if let Some(l) = box_ {
            l.render(scr, f, hi);
        }
    }
}
