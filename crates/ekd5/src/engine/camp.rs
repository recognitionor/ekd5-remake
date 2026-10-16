//! 본진 (원작 op06 [1, 출진 수, 반드시 출진 ×5, 출진 불가 ×5] → 대화상자 4개 FUN_0041c2b2).
//! 다음 절의 블록들이 선택지: 머리 op07 = 출진, op2d [장수] = 그 장수와 대화.
//! 원작의 상점·장비 화면은 윈도 대화상자라 화면은 새로 만들고, 규칙은 아이템 표(종류·가격·고유 여부)와 직업 장비 가능 표를 따른다.
//! 출진 선택 (원작 FUN_0041aac4 / FUN_004606b3): op4a [출진 수, 반드시 ×5, 불가 ×5].

use super::assets::Assets;
use super::ui::{ListBox, Pick};
use super::{key, Engine, Scene};
use crate::officer::{item_name, item_stat};

#[derive(Clone, Debug)]
pub struct SortieRule {
    pub max: usize,
    pub forced: Vec<usize>,
    pub excluded: Vec<usize>,
}

impl SortieRule {
    pub fn from_args(max: i32, list: &[i32]) -> Self {
        let p = |v: &[i32]| v.iter().filter(|&&x| x != 0 && x != 0xFFFF).map(|&x| x as usize).collect();
        Self { max: if max <= 0 || max == 0xFFFF { 12 } else { max as usize }, forced: p(&list[..5.min(list.len())]), excluded: p(list.get(5..10).unwrap_or(&[])) }
    }
}

#[derive(Clone, Debug)]
pub enum CampView {
    Menu(ListBox),
    Talk(ListBox, Vec<usize>),
    EquipWho(ListBox, Vec<usize>),
    EquipSlot(ListBox, usize),
    EquipPick(ListBox, usize, u8, Vec<usize>),
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

/// 출진 고르기 화면
#[derive(Clone, Debug)]
pub struct SortieUi {
    pub list: ListBox,
    pub pids: Vec<usize>,
    pub chosen: Vec<usize>,
    pub rule: SortieRule,
}

fn menu_box() -> ListBox {
    let mut m = ListBox::new("본진", 220);
    m.y = 120;
    for t in ["대화", "장비", "상점", "도구 보기", "출진"] {
        m.push(t, "", true);
    }
    m
}

impl<A: Assets> Engine<A> {
    /// 아이템 가격 (원작 표 +19, ×10)
    pub fn item_price(&self, id: u8) -> i32 {
        self.items.get(id as usize * 25 + 19).copied().unwrap_or(0) as i32 * 10
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

    /// 직업이 장비할 수 있나 (직업 표 +9.. 종류별 가능)
    fn can_equip(&self, job: u8, id: u8) -> bool {
        let t = self.item_type(id);
        if t > 0x11 {
            return self.item_slot(id) == Some(2);
        }
        self.jobs.get(job as usize * 27 + 9 + t as usize).copied().unwrap_or(0) != 0
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
        self.camp = Some(Camp { section, rule, view: CampView::Menu(menu_box()), depart, talks });
    }

    fn run_camp_block(&mut self, body: usize, section: usize) {
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
        let Some(mut camp) = self.camp.take() else { return false };
        let inp = self.input.clone();
        let tapped = inp.pressed & key::DECIDE != 0;
        // 대화 블록이 끝나면 메뉴로
        if matches!(camp.view, CampView::Running) {
            if self.vm.as_ref().is_some_and(|v| v.running) || self.wait != super::Wait::None {
                self.camp = Some(camp);
                return false;
            }
            camp.view = CampView::Menu(menu_box());
        }
        let persons = self.persons.clone();
        let name = |p: usize| persons.get(p).map(|x| x.name.clone()).unwrap_or_default();
        match camp.view.clone() {
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
                    let (l, ps) = self.officer_list("누구의 장비를 바꿀까");
                    camp.view = CampView::EquipWho(l, ps);
                }
                Pick::Row(2) => {
                    let mut l = ListBox::new("상점", 220);
                    l.y = 120;
                    l.push("무기·방어구점", "", true);
                    l.push("도구점", "", true);
                    camp.view = CampView::ShopKind(l);
                }
                Pick::Row(3) => camp.view = CampView::Items(self.items_box()),
                Pick::Row(4) => {
                    if let Some(body) = camp.depart {
                        let s = camp.section;
                        camp.view = CampView::Running;
                        self.camp = None;
                        self.run_camp_block(body, s);
                        return false;
                    }
                }
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
                Pick::Cancel => camp.view = CampView::Menu(menu_box()),
                Pick::None => camp.view = CampView::Talk(l, ps),
            },
            CampView::EquipWho(mut l, ps) => match l.update(&inp, tapped) {
                Pick::Row(k) => camp.view = CampView::EquipSlot(self.slot_box(ps[k]), ps[k]),
                Pick::Cancel => camp.view = CampView::Menu(menu_box()),
                Pick::None => camp.view = CampView::EquipWho(l, ps),
            },
            CampView::EquipSlot(mut l, p) => match l.update(&inp, tapped) {
                Pick::Row(k) => {
                    let o = self.officer(p);
                    let mut b = ListBox::new(&format!("{} — 무엇을 장비할까", name(p)), 360);
                    let mut idx = Vec::new();
                    b.push("(벗기)", "", true);
                    idx.push(usize::MAX);
                    for (i, &(it, lv)) in self.inventory.iter().enumerate() {
                        if self.item_slot(it) == Some(k as u8) {
                            let ok = self.can_equip(o.job, it);
                            b.push(format!("{} Lv{}", item_name(&self.items, it), lv), if ok { "" } else { "불가" }, ok);
                            idx.push(i);
                        }
                    }
                    camp.view = CampView::EquipPick(b, p, k as u8, idx);
                }
                Pick::Cancel => {
                    let (l, ps) = self.officer_list("누구의 장비를 바꿀까");
                    camp.view = CampView::EquipWho(l, ps);
                }
                Pick::None => camp.view = CampView::EquipSlot(l, p),
            },
            CampView::EquipPick(mut l, p, slot, idx) => match l.update(&inp, tapped) {
                Pick::Row(k) => {
                    self.equip(p, slot, idx[k]);
                    camp.view = CampView::EquipSlot(self.slot_box(p), p);
                }
                Pick::Cancel => camp.view = CampView::EquipSlot(self.slot_box(p), p),
                Pick::None => camp.view = CampView::EquipPick(l, p, slot, idx),
            },
            CampView::ShopKind(mut l) => match l.update(&inp, tapped) {
                Pick::Row(k) => {
                    let (b, ids) = self.shop_box(k == 1);
                    camp.view = CampView::Shop(b, ids, k == 1);
                }
                Pick::Cancel => camp.view = CampView::Menu(menu_box()),
                Pick::None => camp.view = CampView::ShopKind(l),
            },
            CampView::Shop(mut l, ids, tools) => match l.update(&inp, tapped) {
                Pick::Row(k) => {
                    let it = ids[k];
                    let price = self.item_price(it);
                    if self.gold >= price && self.inventory.len() < 99 {
                        self.gold -= price;
                        self.inventory.push((it, 1));
                        self.sounds.push(super::Sound::Effect("SE03.WAV".into()));
                    }
                    let (b, ids) = self.shop_box(tools);
                    let sel = l.sel;
                    let mut b = b;
                    b.sel = sel;
                    b.top = l.top;
                    camp.view = CampView::Shop(b, ids, tools);
                }
                Pick::Cancel => {
                    let mut l = ListBox::new("상점", 220);
                    l.y = 120;
                    l.push("무기·방어구점", "", true);
                    l.push("도구점", "", true);
                    camp.view = CampView::ShopKind(l);
                }
                Pick::None => camp.view = CampView::Shop(l, ids, tools),
            },
            CampView::Items(mut l) => match l.update(&inp, tapped) {
                Pick::Cancel => camp.view = CampView::Menu(menu_box()),
                _ => camp.view = CampView::Items(l),
            },
            CampView::Running => {}
        }
        self.camp = Some(camp);
        true
    }

    fn officer_list(&mut self, title: &str) -> (ListBox, Vec<usize>) {
        let mut l = ListBox::new(title, 300);
        let roster = if self.roster.is_empty() { vec![0] } else { self.roster.clone() };
        for &p in &roster {
            let o = self.officer(p);
            let n = self.persons.get(p).map(|x| x.name.clone()).unwrap_or_default();
            l.push(n, format!("Lv{}", o.level), true);
        }
        (l, roster)
    }

    fn slot_box(&mut self, p: usize) -> ListBox {
        let o = self.officer(p);
        let n = self.persons.get(p).map(|x| x.name.clone()).unwrap_or_default();
        let mut l = ListBox::new(&format!("{n} 장비"), 360);
        let w = o.weapon.map_or("-".into(), |(i, lv)| format!("{} Lv{lv}", item_name(&self.items, i)));
        let a = o.armor.map_or("-".into(), |(i, lv)| format!("{} Lv{lv}", item_name(&self.items, i)));
        let c = o.acc.map_or("-".into(), |i| item_name(&self.items, i));
        l.push("무기", w, true);
        l.push("방어구", a, true);
        l.push("보조", c, true);
        l.footer = format!("공{} 방{} 정{} 순{} 사{}", o.value(&self.items, 0), o.value(&self.items, 1), o.value(&self.items, 2), o.value(&self.items, 3), o.value(&self.items, 4));
        l
    }

    /// 장비 바꾸기 (inv_idx = usize::MAX 이면 벗기)
    fn equip(&mut self, p: usize, slot: u8, inv_idx: usize) {
        let mut o = self.officer(p);
        let old: Option<(u8, u8)> = match slot {
            0 => o.weapon,
            1 => o.armor,
            _ => o.acc.map(|i| (i, 1)),
        };
        let new = if inv_idx == usize::MAX { None } else { Some(self.inventory.remove(inv_idx)) };
        if slot < 2 {
            o.gear_exp[slot as usize] = 0;
        }
        match slot {
            0 => o.weapon = new,
            1 => o.armor = new,
            _ => o.acc = new.map(|n| n.0),
        }
        if let Some(it) = old {
            self.inventory.push(it);
        }
        self.officers.insert(p, o);
    }

    fn shop_box(&self, tools: bool) -> (ListBox, Vec<u8>) {
        let mut l = ListBox::new(&format!("{}   소지금 {}", if tools { "도구점" } else { "무기·방어구점" }, self.gold), 400);
        let mut ids = Vec::new();
        // 진행에 따라 파는 물건이 늘어난다 (가격 상한)
        let cap = 300 + self.stage as i32 * 120;
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
        l
    }

    /// 출진 고르기 열기
    pub fn open_sortie(&mut self, rule: SortieRule) {
        let roster = if self.roster.is_empty() { vec![0] } else { self.roster.clone() };
        let mut pids: Vec<usize> = roster.into_iter().filter(|p| !rule.excluded.contains(p)).collect();
        // 조조와 반드시 출진은 앞에
        pids.sort_by_key(|p| if *p == 0 { 0 } else if rule.forced.contains(p) { 1 } else { 2 });
        let mut chosen: Vec<usize> = pids.iter().copied().filter(|p| *p == 0 || rule.forced.contains(p)).collect();
        for &p in &pids {
            if chosen.len() >= rule.max {
                break;
            }
            if !chosen.contains(&p) {
                chosen.push(p);
            }
        }
        let list = ListBox::new("", 340);
        let mut ui = SortieUi { list, pids, chosen, rule };
        self.refresh_sortie(&mut ui);
        self.sortie_ui = Some(ui);
    }

    fn refresh_sortie(&mut self, ui: &mut SortieUi) {
        let (sel, top) = (ui.list.sel, ui.list.top);
        let mut l = ListBox::new(&format!("출진 장수  {}/{}", ui.chosen.len(), ui.rule.max), 340);
        l.y = 40;
        l.visible = 14;
        l.push("▶ 출진한다", "", true);
        for &p in &ui.pids {
            let o = self.officer(p);
            let n = self.persons.get(p).map(|x| x.name.clone()).unwrap_or_default();
            let fixed = p == 0 || ui.rule.forced.contains(&p);
            l.rows.push((n, format!("Lv{}{}", o.level, if fixed { " 필수" } else { "" }), true, ui.chosen.contains(&p)));
        }
        l.sel = sel;
        l.top = top;
        ui.list = l;
    }

    /// 출진 고르기 입력. 끝나면 Some(고른 장수들)
    pub fn update_sortie(&mut self) -> Option<Vec<usize>> {
        let mut ui = self.sortie_ui.take()?;
        let inp = self.input.clone();
        match ui.list.update(&inp, inp.pressed & key::DECIDE != 0) {
            Pick::Row(0) => return Some(ui.chosen),
            Pick::Row(k) => {
                let p = ui.pids[k - 1];
                let fixed = p == 0 || ui.rule.forced.contains(&p);
                if ui.chosen.contains(&p) {
                    if !fixed {
                        ui.chosen.retain(|&x| x != p);
                    }
                } else if ui.chosen.len() < ui.rule.max {
                    ui.chosen.push(p);
                }
                self.refresh_sortie(&mut ui);
            }
            _ => {}
        }
        self.sortie_ui = Some(ui);
        None
    }

    pub fn render_camp(&mut self) {
        let Some(f) = self.font.as_ref() else { return };
        let hi = if let Scene::Battle(b) = &self.scene { b.col[0] } else { crate::palette::nearest(&self.screen.pal, [208, 176, 112]) };
        let scr = &mut self.screen;
        if let Some(c) = &self.camp {
            let box_ = match &c.view {
                CampView::Menu(l) | CampView::Talk(l, _) | CampView::EquipWho(l, _) | CampView::EquipSlot(l, _) | CampView::EquipPick(l, ..) | CampView::ShopKind(l) | CampView::Shop(l, ..) | CampView::Items(l) => Some(l),
                CampView::Running => None,
            };
            if let Some(l) = box_ {
                if !matches!(c.view, CampView::Menu(_)) {
                    let mut m = menu_box();
                    m.x = 20;
                    m.sel = match c.view {
                        CampView::Talk(..) => 0,
                        CampView::EquipWho(..) | CampView::EquipSlot(..) | CampView::EquipPick(..) => 1,
                        CampView::ShopKind(_) | CampView::Shop(..) => 2,
                        _ => 3,
                    };
                    m.render(scr, f, hi);
                }
                l.render(scr, f, hi);
                super::story::draw_text(scr, f, 20, 452, &format!("소지금 {}", self.gold), super::story::TEXT);
            }
        }
        if let Some(s) = &self.sortie_ui {
            s.list.render(scr, f, hi);
        }
    }
}
