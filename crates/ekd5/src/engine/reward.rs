//! 전투 보상 안내 (리메이크 추가 — 원작에는 없음): 지금 전투 스크립트의 절 1 사건 블록에서
//! op59 (전투 보상: 돈·아이템) · op3d (아이템 얻기) 를 찾아, 그 블록의 머리 조건과 안쪽 op01 조건을 말로 풀어 보여 준다.
//! 깃발 조건 (op05) 은 그 깃발을 켜는 곳이 선택지 (op12 → op13 갈래) 안이면 "'…' 선택" 으로 풀고, 나머지 (한 번만 도는 표시 등) 는 생략.
//! 블록 머리의 "꺼져 있어야" 깃발 가운데 그 블록 본문이 켜는 것이 지금 켜져 있으면 이미 받은 보상.

use super::{battle, Assets, Engine};
use crate::vm::{Arg, Vm};
use std::collections::HashMap;

#[derive(Clone)]
enum C {
    Raw(u16, Vec<Arg>),
    Choice(String),
    Else(Vec<C>),
}

struct Entry {
    what: String,
    cell: Option<(i32, i32)>,
    conds: Vec<C>,
    guards: Vec<i32>,
}

fn arg(args: &[Arg], i: usize) -> i32 {
    args.get(i).map_or(0, |a| a.i())
}

/// op05 → (켜짐 목록, 꺼짐 목록)
fn flags_of(args: &[Arg]) -> (Vec<i32>, Vec<i32>) {
    let n1 = arg(args, 0).max(0) as usize;
    let n2 = arg(args, 1 + n1).max(0) as usize;
    let on = args.get(1..1 + n1).map_or(Vec::new(), |s| s.iter().map(|a| a.i()).collect());
    let off = args.get(2 + n1..2 + n1 + n2).map_or(Vec::new(), |s| s.iter().map(|a| a.i()).collect());
    (on, off)
}

struct Walk<'a> {
    vm: Vm,
    items: &'a [u8],
    lv: i32,
    choices: Vec<String>,
    /// 깃발 → 켜는 선택지 (선택지 밖에서도 켜거나 다른 선택지가 켜면 None = 설명 안 함)
    origin: HashMap<i32, Option<String>>,
    out: Vec<Entry>,
    /// 지금 블록 본문이 켜는 깃발
    sets: Vec<i32>,
}

impl Walk<'_> {
    fn set_choices(&mut self, args: &[Arg]) {
        let (t, _, _) = encoding_rs::EUC_KR.decode(args.first().map_or(&[][..], |a| a.bytes()));
        self.choices = t.split('\n').map(|s| s.trim().trim_end_matches('.').to_string()).filter(|s| !s.is_empty()).collect();
    }

    fn choice_text(&self, k: i32) -> String {
        self.choices.get((k - 1).max(0) as usize).cloned().unwrap_or_else(|| format!("선택지 {k}"))
    }

    /// 조건 목록 (op00 까지)
    fn conds(&mut self, ctx_choice: &mut Option<String>) -> Option<Vec<C>> {
        let mut v = Vec::new();
        loop {
            let (op, args) = self.vm.fetch().ok()?;
            match op {
                0 => return Some(v),
                0x13 => {
                    let t = self.choice_text(arg(&args, 0));
                    *ctx_choice = Some(t.clone());
                    v.push(C::Choice(t));
                }
                _ => v.push(C::Raw(op, args)),
            }
        }
    }

    /// 본문 (end 까지). ctx = 바깥 조건들
    fn body(&mut self, end: usize, ctx: &[C], choice: Option<&str>) -> Option<()> {
        let mut prev: Vec<C> = Vec::new();
        while self.vm.pc < end {
            let (op, args) = self.vm.fetch().ok()?;
            match op {
                1 => {
                    // 안쪽 if: 조건 명령 하나 → u16 길이 → 본문 (끝에 op00). op03 = 앞 if 의 반대, op12 = 선택지 (본문 안 op13 으로 갈래)
                    let mut ch = choice.map(|s| s.to_string());
                    let (cop, cargs) = self.vm.fetch().ok()?;
                    let cs = match cop {
                        3 => vec![C::Else(prev.clone())],
                        0x12 => {
                            self.set_choices(&cargs);
                            Vec::new()
                        }
                        0x13 => {
                            let t = self.choice_text(arg(&cargs, 0));
                            ch = Some(t.clone());
                            vec![C::Choice(t)]
                        }
                        _ => vec![C::Raw(cop, cargs)],
                    };
                    let len = self.vm.read_u16().ok()? as usize;
                    let e = self.vm.pc + len;
                    let mut inner = ctx.to_vec();
                    inner.extend(cs.iter().cloned());
                    self.body(e, &inner, ch.as_deref())?;
                    self.vm.pc = e;
                    prev = cs;
                }
                0x12 => self.set_choices(&args),
                0x0B => {
                    let f = arg(&args, 0);
                    if arg(&args, 1) != 0 {
                        self.sets.push(f);
                        let c = choice.map(|c| c.to_string());
                        let e = self.origin.entry(f).or_insert_with(|| c.clone());
                        if *e != c {
                            *e = None;
                        }
                    }
                }
                0x59 => {
                    let money = self.lv * 100 + 700 + arg(&args, 0);
                    self.push(format!("보상금 {money}"), ctx);
                    for k in [1, 3, 5] {
                        let it = arg(&args, k);
                        if (0..0x68).contains(&it) {
                            self.push(crate::officer::item_name(self.items, it as u8), ctx);
                        }
                    }
                }
                0x3D => {
                    let it = arg(&args, 0);
                    if (0..0x68).contains(&it) {
                        self.push(crate::officer::item_name(self.items, it as u8), ctx);
                    }
                }
                _ => {}
            }
        }
        Some(())
    }

    fn push(&mut self, what: String, ctx: &[C]) {
        let cell = ctx.iter().find_map(|c| match c {
            C::Raw(0x25, a) => Some((arg(a, 1), arg(a, 2))),
            C::Raw(0x26, a) => Some(((arg(a, 1) + arg(a, 3)) / 2, (arg(a, 2) + arg(a, 4)) / 2)),
            _ => None,
        });
        self.out.push(Entry { what, cell, conds: ctx.to_vec(), guards: Vec::new() });
    }

    /// 블록 하나: 머리 조건 → 본문
    fn block(&mut self, start: usize) -> Option<()> {
        self.vm.pc = start;
        self.vm.ends.clear();
        let mut ch = None;
        let head = self.conds(&mut ch)?;
        // 패배 사건은 보상 없음
        if head.iter().any(|c| matches!(c, C::Raw(0x43, _))) {
            return Some(());
        }
        let len = self.vm.read_u16().ok()? as usize;
        let end = self.vm.pc + len;
        let first = self.out.len();
        self.sets.clear();
        self.body(end, &head, ch.as_deref())?;
        let off: Vec<i32> = head.iter().filter_map(|c| if let C::Raw(5, a) = c { Some(flags_of(a).1) } else { None }).flatten().collect();
        let guards: Vec<i32> = off.into_iter().filter(|f| self.sets.contains(f)).collect();
        for e in &mut self.out[first..] {
            e.guards = guards.clone();
        }
        Some(())
    }
}

impl<A: Assets> Engine<A> {
    /// 전투 메뉴 "전투 보상" 창
    pub fn reward_view(&mut self) -> Option<battle::Phase> {
        let script = self.vm.as_ref()?.script.clone();
        let lv = self.officer(0).level as i32;
        let mut w = Walk { vm: Vm::new(script), items: &self.items, lv, choices: Vec::new(), origin: HashMap::new(), out: Vec::new(), sets: Vec::new() };
        // 1차: 선택지가 켜는 깃발 (모든 절) / 2차: 절 1 보상
        let n = w.vm.sections();
        for s in 0..n {
            for (start, _) in w.vm.blocks(s).unwrap_or_default() {
                let _ = w.block(start);
            }
        }
        w.out.clear();
        if n > 1 {
            for (start, _) in w.vm.blocks(1).unwrap_or_default() {
                let _ = w.block(start);
            }
        }
        let mut entries = std::mem::take(&mut w.out);
        // 칸을 밟으면 얻는 숨은 아이템 (op25) 은 뒤로 — 승리·퇴각 보상이 먼저 보이게
        entries.sort_by_key(|e| e.conds.iter().any(|c| matches!(c, C::Raw(0x25, _))));
        let origin: HashMap<i32, String> = std::mem::take(&mut w.origin).into_iter().filter_map(|(f, c)| Some((f, c?))).collect();
        drop(w);

        let mut l = super::ui::ListBox::new("전투 보상", 580);
        l.y = 20;
        l.visible = 14;
        let mut cells = Vec::new();
        for e in &entries {
            let done = e.guards.iter().any(|&f| self.flags.get(f as usize).copied().unwrap_or(false));
            l.push(e.what.clone(), if done { "받음" } else { "" }, e.cell.is_some());
            cells.push(e.cell.map_or(usize::MAX, |(x, y)| ((x as usize) << 8) | y as usize));
            let words: Vec<String> = e.conds.iter().filter_map(|c| self.cond_text(c, &origin)).collect();
            let text = if words.is_empty() { "조건 없음".to_string() } else { words.join(" · ") };
            for (i, line) in wrap(&text, 26).into_iter().enumerate() {
                l.push(format!("{}{line}", if i == 0 { "  └ " } else { "     " }), "", false);
                cells.push(usize::MAX);
            }
        }
        if entries.is_empty() {
            l.push("이 전투에는 보상 사건이 없습니다", "", false);
            cells.push(usize::MAX);
        }
        l.footer = "칸 조건은 고르면 그 칸으로 · 보상금은 지금 조조 레벨 기준".into();
        Some(battle::Phase::Sys { kind: 5, list: Box::new(l), units: cells })
    }

    fn who(&self, w: i32) -> String {
        match w {
            0x400 => "누구든".into(),
            0x401 => "아군".into(),
            0x402 => "적군".into(),
            p => self.persons.get(p as usize).map_or_else(|| format!("인물{p}"), |p| p.name.clone()),
        }
    }

    fn cond_text(&self, c: &C, origin: &HashMap<i32, String>) -> Option<String> {
        let cmp = |k: i32, n: i32, unit: &str| match k {
            0 => format!("{n}{unit} 이상"),
            1 => format!("{n}{unit} 미만"),
            _ => format!("{n}{unit}"),
        };
        Some(match c {
            C::Choice(t) => format!("'{t}' 선택"),
            C::Else(prev) => {
                let p: Vec<String> = prev.iter().filter_map(|c| self.cond_text(c, origin)).collect();
                if p.is_empty() {
                    "그 밖의 경우".into()
                } else if p.len() == 1 && !p[0].contains(" · ") && p[0].ends_with(" 선택 안 함") {
                    p[0].trim_end_matches(" 안 함").to_string()
                } else if p.len() == 1 && !p[0].contains(" · ") && p[0].ends_with(" 선택") {
                    format!("{} 안 함", p[0])
                } else {
                    format!("({}) 아닐 때", p.join(" · "))
                }
            }
            C::Raw(op, a) => {
                let a = |i: usize| arg(a, i);
                match *op {
                    0x42 => "승리".into(),
                    0x3F => match a(1) {
                        0 => format!("{}턴 이후", a(0)),
                        1 => format!("{}턴 전", a(0)),
                        _ => format!("{}턴째", a(0)),
                    },
                    0x25 => format!("{} ({},{}) 칸 도착", self.who(a(0)), a(1), a(2)),
                    0x26 => format!("{} ({},{})-({},{}) 진입", self.who(a(0)), a(1), a(2), a(3), a(4)),
                    0x2E => format!("{}·{} 이웃", self.who(a(0)), self.who(a(1))),
                    0x36 if a(1) == 7 && a(2) == 0 && a(3) == 2 => format!("{} 퇴각", self.who(a(0))),
                    0x36 => return None,
                    0x41 => {
                        let camp = match a(0) {
                            0 => "아군",
                            1 => "우군",
                            2 | 5 => "적군",
                            4 => "아군·우군",
                            _ => "부대",
                        };
                        let area = if a(3) == 1 { format!("({},{})-({},{}) ", a(4), a(5), a(6), a(7)) } else { String::new() };
                        if a(1) == 0 && a(2) == 2 {
                            format!("{area}{camp} 전멸")
                        } else {
                            format!("{area}{camp} {}", cmp(a(2), a(1), "명"))
                        }
                    }
                    5 => {
                        let (on, off) = if let C::Raw(_, args) = c { flags_of(args) } else { unreachable!() };
                        let mut v: Vec<String> = on.iter().filter_map(|f| origin.get(f)).map(|t| format!("'{t}' 선택")).collect();
                        v.extend(off.iter().filter_map(|f| origin.get(f)).map(|t| format!("'{t}' 선택 안 함")));
                        if v.is_empty() {
                            return None;
                        }
                        v.join(" · ")
                    }
                    _ => return None,
                }
            }
        })
    }
}

/// 글자 수로 줄 나누기 (빈칸에서 끊기를 우선)
fn wrap(s: &str, n: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in s.split(' ') {
        let add = if cur.is_empty() { word.chars().count() } else { cur.chars().count() + 1 + word.chars().count() };
        if add > n && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
        while cur.chars().count() > n {
            let head: String = cur.chars().take(n).collect();
            let rest: String = cur.chars().skip(n).collect();
            out.push(head);
            cur = rest;
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}
