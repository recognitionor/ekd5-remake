// ekd5 웹 글루: WASM 엔진 구동, 입력, 파일 받기, 소리.
(async () => {
  const DATA = "data/";
  const TICK_MS = 1000 / 30;
  const KEY = { DECIDE: 0x20, CANCEL: 0x40, UP: 0x1000, RIGHT: 0x2000, DOWN: 0x4000, LEFT: 0x8000 };
  const KEYMAP = {
    ArrowUp: KEY.UP, ArrowDown: KEY.DOWN, ArrowLeft: KEY.LEFT, ArrowRight: KEY.RIGHT,
    Enter: KEY.DECIDE, Space: KEY.DECIDE, KeyZ: KEY.DECIDE, Escape: KEY.CANCEL, KeyX: KEY.CANCEL, Backspace: KEY.CANCEL,
  };
  const status = document.getElementById("status");
  const canvas = document.getElementById("screen");
  const ctx = canvas.getContext("2d");
  const img = ctx.createImageData(640, 480);
  const setStatus = (t) => { status.textContent = t; };

  const { instance } = await WebAssembly.instantiateStreaming(fetch("ekd5_web.wasm"), {});
  const w = instance.exports;
  // 난수 씨앗 (원작은 전투마다 시각으로 정함)
  w.seed(((Date.now() ^ (Math.random() * 0x7fffffff)) >>> 0) || 1);
  const mem = () => new Uint8Array(w.memory.buffer);
  const enc = new TextEncoder(), dec = new TextDecoder();
  function putBytes(bytes) { const p = w.alloc(bytes.length); mem().set(bytes, p); return p; }
  function withStr(s, f) { const b = enc.encode(s); const p = putBytes(b); const r = f(p, b.length); w.dealloc(p, b.length); return r; }
  function readOut(ptr) { const n = w.out_len(); return n ? dec.decode(mem().slice(ptr, ptr + n)) : ""; }

  // ---------- 파일 ----------
  const inflight = new Set();
  let loading = 0;
  async function fetchFile(name) {
    if (inflight.has(name)) return;
    inflight.add(name); loading++;
    try {
      const r = await fetch(DATA + name);
      if (!r.ok) throw new Error(r.status);
      const b = new Uint8Array(await r.arrayBuffer());
      const p = putBytes(b);
      withStr(name, (np, nl) => w.provide(np, nl, p, b.length));
    } catch (e) {
      withStr(name, (np, nl) => w.missing(np, nl));
    } finally {
      inflight.delete(name); loading--;
    }
  }

  // ---------- 소리 ----------
  // 효과음은 Web Audio: 파일마다 한 번만 받아 풀어 두고 바로 재생 (매번 new Audio 로 받으면 늦고 버벅임)
  let audioOk = false, actx = null;
  const sfx = new Map();
  function sfxBuffer(name) {
    let p = sfx.get(name);
    if (!p) {
      p = fetch(DATA + name).then((r) => { if (!r.ok) throw new Error(r.status); return r.arrayBuffer(); })
        .then((b) => new Promise((ok, no) => actx.decodeAudioData(b, ok, no)));
      p.catch(() => sfx.delete(name));
      sfx.set(name, p);
    }
    return p;
  }
  function sfxPlay(name, vol, loop) {
    const h = { src: null, stopped: false, stop() { this.stopped = true; if (this.src) try { this.src.stop(); } catch (_) {} } };
    sfxBuffer(name).then((buf) => {
      if (h.stopped) return;
      const g = actx.createGain(); g.gain.value = vol; g.connect(actx.destination);
      const s = actx.createBufferSource(); s.buffer = buf; s.loop = !!loop; s.connect(g); s.start();
      h.src = s;
    }).catch(() => {});
    return h;
  }
  // 소리 설정 (기억): 배경음악·효과음 켜고 끄기
  let bgmOn = true, sfxOn = true;
  try { bgmOn = localStorage.getItem("ekd5-bgm") !== "0"; sfxOn = localStorage.getItem("ekd5-sfx") !== "0"; } catch (_) {}
  function playWav(name, vol = 1) {
    if (!audioOk || !actx || !sfxOn) return;
    sfxPlay(name, vol, false);
  }
  // 반복 효과음 (원작 op23 방식 0) 과 배경음악 (원작 CD 트랙 → data/BGMnn.m4a, 반복)
  let loopFx = null, loopWant = null, bgm = null, bgmTrack = -1, bgmWant = -1;
  function setLoop(name) {
    loopWant = name || null;
    if (loopFx) { loopFx.stop(); loopFx = null; }
    if (name && audioOk && actx && sfxOn) loopFx = sfxPlay(name, 0.6, true);
  }
  function setMusic(n) {
    bgmWant = n;
    if (!audioOk || !bgmOn) { if (bgm) { bgm.pause(); bgm = null; } bgmTrack = -1; return; }
    if (n === bgmTrack) return;
    if (bgm) { bgm.pause(); bgm = null; }
    bgmTrack = n;
    if (n >= 0) {
      bgm = new Audio(DATA + "BGM" + String(n).padStart(2, "0") + ".m4a");
      bgm.loop = true; bgm.volume = 0.8; bgm.play().catch(() => {});
    }
  }
  // 아이폰 소리 방식: 배경음악을 켜면 playback (무음 스위치와 상관없이 남, 다른 앱 음악은 멈춤),
  // 끄면 ambient (다른 앱 음악과 같이 남, 무음 스위치를 따름)
  function setSession() { try { if (navigator.audioSession) navigator.audioSession.type = bgmOn ? "playback" : "ambient"; } catch (_) {} }
  setSession();
  // 아이폰은 AudioContext 도 터치 안에서 만들거나 다시 깨워야 소리가 남
  const unlockAudio = () => {
    if (!actx) { try { actx = new (window.AudioContext || window.webkitAudioContext)(); } catch (_) {} }
    if (actx && actx.state !== "running") actx.resume().catch(() => {});
    if (!audioOk) { audioOk = true; if (bgmWant >= 0) setMusic(bgmWant); }
  };

  // ---------- 입력 ----------
  let held = 0, mouse = null, click = 0, pulse = 0, pulseTicks = 0, started = true;
  addEventListener("keydown", (e) => { const b = KEYMAP[e.code]; if (b) { held |= b; e.preventDefault(); } unlockAudio(); started = true; });
  addEventListener("keyup", (e) => { const b = KEYMAP[e.code]; if (b) held &= ~b; });
  function toGame(e) {
    const r = canvas.getBoundingClientRect();
    // 앱을 돌려 그렸으면 (세로 폰에서 가로 고정 등) 좌표도 돌림
    if (document.body.classList.contains("rotcw"))
      return [Math.round((e.clientY - r.top) * 640 / r.height), Math.round((r.right - e.clientX) * 480 / r.width)];
    if (document.body.classList.contains("rotccw"))
      return [Math.round((r.bottom - e.clientY) * 640 / r.height), Math.round((e.clientX - r.left) * 480 / r.width)];
    return [Math.round((e.clientX - r.left) * 640 / r.width), Math.round((e.clientY - r.top) * 480 / r.height)];
  }
  // 누르고 있는 동안 결정 키를 계속 누른 것으로 보냄 (전장 끌어서 스크롤)
  let ptrDown = 0;
  canvas.addEventListener("pointermove", (e) => { mouse = toGame(e); });
  // 터치 기기에서 링크로 들어왔으면 첫 터치 때 자동 전체 화면 (브라우저는 사용자 동작이 있어야 허락)
  const standalone = matchMedia("(display-mode: fullscreen), (display-mode: standalone)").matches || navigator.standalone;
  // 홈 화면 앱으로 실행했으면 전체화면 버튼 필요 없음 (전체 화면 API 의 fullscreen 과 구별하려고 시작 때 한 번만 판정)
  if (standalone) document.getElementById("full").style.display = "none";
  let autoFull = !standalone && matchMedia("(pointer: coarse)").matches && !!document.documentElement.requestFullscreen;
  function tryAutoFull() {
    if (!autoFull || document.fullscreenElement) return;
    document.documentElement.requestFullscreen({ navigationUI: "hide" })
      .then(() => { autoFull = false; screen.orientation && screen.orientation.lock && screen.orientation.lock("landscape").catch(() => {}); })
      .catch(() => {});
  }
  // 터치는 손을 뗄 때(pointerup)에야 '사용자 동작'으로 인정됨
  canvas.addEventListener("pointerup", tryAutoFull);
  addEventListener("touchend", () => { if (actx && actx.state !== "running") actx.resume().catch(() => {}); }, { capture: true });
  addEventListener("touchend", tryAutoFull, { capture: true });
  addEventListener("click", tryAutoFull, { capture: true });
  canvas.addEventListener("pointerdown", (e) => {
    e.preventDefault(); unlockAudio(); started = true;
    mouse = toGame(e);
    try { canvas.setPointerCapture(e.pointerId); } catch (_) {}
    if (e.button === 2) { pulse = KEY.CANCEL; pulseTicks = 3; } else { ptrDown = KEY.DECIDE; click = 1; pulse = KEY.DECIDE; pulseTicks = 2; }
  });
  const ptrUp = (e) => { if (e) mouse = toGame(e); ptrDown = 0; };
  canvas.addEventListener("pointerup", ptrUp);
  canvas.addEventListener("pointercancel", () => ptrUp());
  canvas.addEventListener("contextmenu", (e) => e.preventDefault());
  function holdBtn(id, bit) {
    const b = document.getElementById(id);
    if (!b) return;
    b.addEventListener("pointerdown", (e) => { e.preventDefault(); unlockAudio(); b.classList.add("on"); held |= bit; });
    const up = () => { b.classList.remove("on"); held &= ~bit; };
    b.addEventListener("pointerup", up); b.addEventListener("pointercancel", up); b.addEventListener("pointerleave", up);
  }
  holdBtn("cancel", KEY.CANCEL);
  // 전투 메뉴 (원작 메뉴 막대: 장수·아이템·지형 일람, 승리 조건, 미실행 부대 검색, 턴 종료)
  document.getElementById("menu").addEventListener("click", (e) => { e.preventDefault(); w.open_menu(); });
  // 화면 맞춤: 비율 유지 ↔ 꽉 채우기 (기억)
  const fitBtn = document.getElementById("fit");
  let fill = false;
  try { fill = localStorage.getItem("ekd5-fill") === "1"; } catch (_) {}
  const applyFit = () => { document.body.classList.toggle("fill", fill); fitBtn.textContent = "화면 맞춤: " + (fill ? "꽉 채우기" : "비율 유지"); };
  applyFit();
  fitBtn.addEventListener("click", (e) => { e.preventDefault(); fill = !fill; applyFit(); try { localStorage.setItem("ekd5-fill", fill ? "1" : "0"); } catch (_) {} });
  // 화면 방향: 자동 (폰 방향대로) / 세로 고정 / 가로 고정. 폰이 고정을 못 하면 (아이폰 등) 앱을 90도 돌려 그림
  const orientBtn = document.getElementById("orient");
  let orient = "auto";
  try { orient = localStorage.getItem("ekd5-orient") || "auto"; } catch (_) {}
  function applyOrient() {
    const devPort = innerHeight > innerWidth;
    const want = orient === "auto" ? (devPort ? "port" : "land") : orient;
    const devWant = want === "port" ? devPort : !devPort;
    let rot = "";
    if (!devWant) {
      // 전체 화면이면 진짜 방향 고정을 먼저 시도 (안드로이드)
      try { screen.orientation && screen.orientation.lock && screen.orientation.lock(want === "port" ? "portrait" : "landscape").catch(() => {}); } catch (_) {}
      rot = devPort ? "rotcw" : "rotccw";
    }
    const b = document.body.classList;
    b.toggle("port", want === "port");
    b.toggle("rotcw", rot === "rotcw");
    b.toggle("rotccw", rot === "rotccw");
    // 돌릴 때는 실제 창 크기로 (100vh 가 주소창 등으로 실제와 다를 수 있음)
    const app = document.getElementById("app"), bs = document.body.style;
    if (rot) {
      const W = innerWidth, H = innerHeight;
      app.style.width = H + "px"; app.style.height = W + "px";
      app.style.left = rot === "rotcw" ? W + "px" : "0px"; app.style.top = rot === "rotcw" ? "0px" : H + "px";
      bs.setProperty("--vw", H + "px"); bs.setProperty("--vh", W + "px");
    } else {
      app.style.width = app.style.height = app.style.left = app.style.top = "";
      bs.removeProperty("--vw"); bs.removeProperty("--vh");
    }
    orientBtn.textContent = "화면 방향: " + ({ auto: "자동 (폰 방향대로)", port: "세로 고정", land: "가로 고정" })[orient];
  }
  applyOrient();
  addEventListener("resize", applyOrient);
  addEventListener("orientationchange", () => setTimeout(applyOrient, 200));
  orientBtn.addEventListener("click", () => {
    orient = ({ auto: "land", land: "port", port: "auto" })[orient];
    try { localStorage.setItem("ekd5-orient", orient); } catch (_) {}
    applyOrient();
  });
  // 홈 화면 앱 (PWA)
  if ("serviceWorker" in navigator) { navigator.serviceWorker.register("sw.js").catch(() => {}); }
  // 빨리 감기: 4배 속도 + 대사·자막 자동 넘김 (선택지·전투 조작은 그대로)
  let fast = false, fastTick = 0;
  try { fast = localStorage.getItem("ekd5-fast") === "1"; } catch (_) {}
  const fastBtn = document.getElementById("fast");
  fastBtn.classList.toggle("on", fast);
  const toggleFast = () => { fast = !fast; fastBtn.classList.toggle("on", fast); try { localStorage.setItem("ekd5-fast", fast ? "1" : "0"); } catch (_) {} };
  fastBtn.addEventListener("click", (e) => { e.preventDefault(); toggleFast(); });
  addEventListener("keydown", (e) => { if (e.code === "KeyF") toggleFast(); });
  // 전체 지도 (원작 미니맵 창)
  const mapBtn = document.getElementById("map");
  const toggleMap = () => { const on = w.toggle_minimap(); mapBtn.classList.toggle("on", !!on); };
  mapBtn.addEventListener("click", (e) => { e.preventDefault(); toggleMap(); });
  addEventListener("keydown", (e) => { if (e.code === "KeyM") toggleMap(); });
  holdBtn("skip", KEY.DECIDE);
  document.getElementById("full").addEventListener("click", async () => {
    try {
      if (!document.fullscreenElement) { await document.documentElement.requestFullscreen({ navigationUI: "hide" }); try { await screen.orientation.lock("landscape"); } catch (_) {} }
      else await document.exitFullscreen();
    } catch (_) {}
  });

  // ---------- 소리 설정 창 ----------
  const sndPanel = document.getElementById("sndPanel"), sndBgm = document.getElementById("sndBgm"), sndSfx = document.getElementById("sndSfx");
  const sndLabels = () => { sndBgm.textContent = "배경음악: " + (bgmOn ? "켜짐 🔊" : "꺼짐 🔇"); sndSfx.textContent = "효과음: " + (sfxOn ? "켜짐 🔊" : "꺼짐 🔇"); };
  sndLabels();
  document.getElementById("snd").addEventListener("click", (e) => { e.preventDefault(); unlockAudio(); sndPanel.style.display = sndPanel.style.display === "flex" ? "none" : "flex"; });
  document.getElementById("sndClose").addEventListener("click", () => { sndPanel.style.display = "none"; });
  sndBgm.addEventListener("click", () => {
    bgmOn = !bgmOn; try { localStorage.setItem("ekd5-bgm", bgmOn ? "1" : "0"); } catch (_) {}
    setSession(); setMusic(bgmWant); sndLabels();
  });
  sndSfx.addEventListener("click", () => {
    sfxOn = !sfxOn; try { localStorage.setItem("ekd5-sfx", sfxOn ? "1" : "0"); } catch (_) {}
    setLoop(loopWant); sndLabels();
  });

  // ---------- 새 버전 알림 (홈 화면 앱은 새로고침 단추가 없음) ----------
  // 배포 때 version.txt 가 바뀜. 앱으로 돌아올 때·10분마다 확인해서, 바뀌었으면 단추를 띄움 (누를 때만 새로고침 — 하던 게임은 먼저 저장)
  const updBtn = document.getElementById("update");
  let myVer = null;
  async function checkVer() {
    try {
      const v = (await (await fetch("version.txt", { cache: "no-store" })).text()).trim();
      if (myVer === null) myVer = v; else if (v && v !== myVer) updBtn.style.display = "block";
    } catch (_) {}
  }
  checkVer();
  setInterval(checkVer, 600000);
  document.addEventListener("visibilitychange", () => { if (!document.hidden) checkVer(); });
  updBtn.addEventListener("click", () => location.reload());

  // ---------- 회차 공통 기록 (엔딩·보물도감) ----------
  try { const c = localStorage.getItem("ekd5-common"); if (c) withStr(c, (p, n) => w.set_common(p, n)); } catch (_) {}

  // ---------- 저장 슬롯 (그 화면 그대로: 전투 중 내 차례·본진) — 5칸 + 단계 시작 자동 저장 ----------
  const toastEl = document.getElementById("toast");
  let toastTimer = 0;
  function toast(t) { toastEl.textContent = t; toastEl.style.display = "block"; clearTimeout(toastTimer); toastTimer = setTimeout(() => toastEl.style.display = "none", 1800); }
  const NSLOT = 5;
  const ls = { get: (k) => { try { return localStorage.getItem(k); } catch (_) { return null; } },
    set: (k, v) => { try { localStorage.setItem(k, v); return true; } catch (_) { return false; } },
    del: (k) => { try { localStorage.removeItem(k); } catch (_) {} } };
  // 예전 한 칸 저장 → 1번 칸
  if (ls.get("ekd5-quick") && !ls.get("ekd5-slot-1")) {
    ls.set("ekd5-slot-1", ls.get("ekd5-quick")); ls.set("ekd5-slot-1-time", ls.get("ekd5-quick-time") || "");
    ls.del("ekd5-quick"); ls.del("ekd5-quick-time");
  }
  function slotInfo(i) {
    const q = ls.get("ekd5-slot-" + i);
    if (!q) return null;
    let d = "";
    try { const j = JSON.parse(q); const where = j.battle ? (j.battle.objective ? "전투 · " + j.battle.objective : "전투") : (j.place && (j.place[1] || j.place[0])) || "본진"; d = `${j.stage}단계 · ${where}`; } catch (_) { d = "?"; }
    return { text: d, time: ls.get("ekd5-slot-" + i + "-time") || "" };
  }
  let paused = false;
  const slotsEl = document.getElementById("slots"), slotList = document.getElementById("slotList"), slotTitle = document.getElementById("slotTitle");
  const slotExtra = document.getElementById("slotExtra");
  function closeSlots() { slotsEl.style.display = "none"; paused = false; }
  // mode: "save" | "load" | "start"
  function openSlots(mode) {
    paused = true;
    slotTitle.textContent = mode === "save" ? "어느 칸에 저장할까요?" : "어느 칸을 불러올까요?";
    slotList.innerHTML = "";
    for (let i = 1; i <= NSLOT; i++) {
      const info = slotInfo(i);
      const b = document.createElement("button");
      b.className = "slot";
      b.innerHTML = `<b>${i}</b> ${info ? info.text + "<small>" + info.time + "</small>" : "<span style='opacity:.5'>비어 있음</span>"}`;
      if (mode !== "save" && !info) b.disabled = true;
      b.addEventListener("click", () => {
        if (mode === "save") {
          const s = readOut(w.quick_save());
          if (!s) { toast("지금은 저장할 수 없습니다 (전투 중 내 차례 · 본진에서 저장)"); return; }
          if (info && !confirm(`${i}번 칸에 덮어쓸까요?`)) return;
          if (ls.set("ekd5-slot-" + i, s)) { ls.set("ekd5-slot-" + i + "-time", new Date().toLocaleString("ko-KR")); toast(`${i}번 칸에 저장했습니다`); closeSlots(); }
          else toast("저장 실패 (브라우저 저장소)");
        } else if (mode === "start") {
          withStr(ls.get("ekd5-slot-" + i), (p, n) => w.quick_restore(p, n));
          closeSlots();
        } else {
          // 게임 도중 불러오기: 처음부터 다시 띄워서 그 칸으로
          try { sessionStorage.setItem("ekd5-load", String(i)); } catch (_) {}
          location.reload();
        }
      });
      slotList.appendChild(b);
    }
    slotExtra.innerHTML = "";
    const add = (label, fn) => { const b = document.createElement("button"); b.className = "slot small"; b.textContent = label; b.addEventListener("click", fn); slotExtra.appendChild(b); };
    if (mode === "save") add("불러오기로", () => openSlots("load"));
    if (mode === "load") add("저장으로", () => openSlots("save"));
    const auto = ls.get("ekd5-save");
    if (mode !== "save" && auto && !/^[123];0;/.test(auto)) {
      add(`자동 저장 (${auto.split(";")[1]}단계 시작)`, () => {
        if (mode === "start") { withStr(auto, (p, n) => w.restore(p, n)); closeSlots(); }
        else { try { sessionStorage.setItem("ekd5-load", "auto"); } catch (_) {} location.reload(); }
      });
    }
    if (mode === "start") add("처음부터", closeSlots);
    else add("닫기", closeSlots);
    slotsEl.style.display = "flex";
  }
  document.getElementById("save").addEventListener("click", (e) => { e.preventDefault(); openSlots("save"); });
  // 시작: 도중에 고른 칸이 있으면 그것, 아니면 저장이 있을 때 고르기 창
  let want = null;
  try { want = sessionStorage.getItem("ekd5-load"); sessionStorage.removeItem("ekd5-load"); } catch (_) {}
  if (want === "auto" && ls.get("ekd5-save")) withStr(ls.get("ekd5-save"), (p, n) => w.restore(p, n));
  else if (want && ls.get("ekd5-slot-" + want)) withStr(ls.get("ekd5-slot-" + want), (p, n) => w.quick_restore(p, n));
  else {
    const any = [...Array(NSLOT)].some((_, k) => ls.get("ekd5-slot-" + (k + 1)));
    const auto = ls.get("ekd5-save");
    if (any || (auto && !/^[123];0;/.test(auto))) openSlots("start");
  }

  // ---------- 동영상 (원작 LOGO/OPEN/END.AVI → mp4). 탭하면 건너뜀 ----------
  const movie = document.getElementById("movie");
  function endMovie() {
    if (!movie.classList.contains("on")) return;
    movie.pause(); movie.removeAttribute("src"); movie.load(); movie.classList.remove("on");
    if (bgm && bgmTrack >= 0 && bgmOn) bgm.play().catch(() => {});
    w.movie_done();
  }
  function playMovie(name) {
    if (bgm) bgm.pause();
    movie.src = DATA + name; movie.muted = !audioOk; movie.classList.add("on");
    movie.play().catch(() => { movie.muted = true; movie.play().catch(endMovie); });
  }
  movie.addEventListener("ended", endMovie);
  movie.addEventListener("error", endMovie);
  movie.addEventListener("pointerdown", (e) => { e.preventDefault(); unlockAudio(); endMovie(); });
  w.enable_movies();

  // ---------- 명성 게이지 (원작 FUN_00478905 / FUN_0040d717): 가로 200, 채움 = 명성×2, MARK 무늬
  // 채움 0x00 (16 미만이면 0x40), 나머지 0xC0 (85 이상이면 0x40). 바뀔 때 다섯 번에 나눠 깜빡이며 움직임
  const fameEl = document.getElementById("fame"), fameCv = document.getElementById("fameCv"), fameNum = document.getElementById("fameNum");
  const fctx = fameCv.getContext("2d");
  let fameShown = -1, fameStep = 0, fameDrawn = -2, gcol = null, gcolAge = 0;
  function drawFame(v) {
    if (!gcol || gcol.length < 72) { const p = w.gauge_colors(); const n = w.out_len(); if (n >= 72) gcol = mem().slice(p, p + 72); else return; }
    const row = (k, y) => `rgb(${gcol[k * 24 + y * 3]},${gcol[k * 24 + y * 3 + 1]},${gcol[k * 24 + y * 3 + 2]})`;
    const fill = Math.max(1, v * 2), a = v < 16 ? 2 : 0, b = v >= 85 ? 2 : 1;
    for (let y = 0; y < 8; y++) {
      fctx.fillStyle = row(a, y); fctx.fillRect(0, y, fill, 1);
      fctx.fillStyle = row(b, y); fctx.fillRect(fill, y, 200 - fill, 1);
    }
  }
  function updateFame(v) {
    if (fameShown < 0) fameShown = v;
    if (fameShown !== v) {
      if (++fameStep >= 3) { fameStep = 0; fameShown += Math.sign(v - fameShown) * Math.max(1, Math.ceil(Math.abs(v - fameShown) / 5)); }
      fameEl.classList.toggle("blink", Math.floor(performance.now() / 120) % 2 === 0);
    } else fameEl.classList.remove("blink");
    // 장면마다 팔레트가 바뀌니 색을 가끔 새로 받음
    if (++gcolAge > 30) { gcolAge = 0; gcol = null; fameDrawn = -2; }
    if (fameDrawn !== fameShown || !gcol) { drawFame(fameShown); fameDrawn = fameShown; }
    fameNum.textContent = fameShown;
  }

  // ---------- 루프 ----------
  let last = performance.now(), acc = 0;
  function step() {
    const pulseBits = pulseTicks > 0 ? pulse : 0;
    if (pulseTicks > 0) pulseTicks--;
    const m = mouse || [0, 0];
    // 빨리 감기 중 대사 대기면 8틱마다 결정 키
    let auto = 0;
    if (fast && w.waiting_text()) { if (++fastTick >= 8) { fastTick = 0; auto = KEY.DECIDE; } } else fastTick = 0;
    w.tick(held | pulseBits | ptrDown | auto, m[0], m[1], mouse ? 1 : 0, click);
    click = 0;
    updateFame(w.fame());
    const req = readOut(w.take_requests());
    if (req) req.split("\n").forEach(fetchFile);
    const ev = readOut(w.take_events());
    if (ev) for (const line of ev.split("\n")) {
      const e = JSON.parse(line);
      if ("effect" in e) playWav(e.effect, 0.7);
      else if ("loop" in e) setLoop(e.loop);
      else if ("music" in e) setMusic(e.music);
      else if ("movie" in e) playMovie(e.movie);
      else if ("common" in e) { try { localStorage.setItem("ekd5-common", e.common); } catch (_) {} }
      else if ("ending" in e) { try { const k = "ekd5-endings", v = new Set((localStorage.getItem(k) || "").split(",").filter(Boolean)); v.add(String(e.ending)); localStorage.setItem(k, [...v].join(",")); } catch (_) {} }
      else if ("save" in e) { try { if (e.save) localStorage.setItem("ekd5-save", e.save); else localStorage.removeItem("ekd5-save"); } catch (_) {} }
      else if ("log" in e) console.log("[ekd5]", e.log);
    }
  }
  const soundHint = document.getElementById("soundHint");
  function loop(now) {
    if (started && !paused) {
      acc += Math.min(250, now - last);
      while (acc >= TICK_MS) { for (let k = fast ? 4 : 1; k > 0; k--) step(); acc -= TICK_MS; }
      const p = w.frame();
      img.data.set(mem().subarray(p, p + 640 * 480 * 4)); ctx.putImageData(img, 0, 0);
      // 브라우저는 첫 터치 전엔 소리를 못 냄 → 그동안 작은 안내만
      setStatus(loading > 0 ? "불러오는 중…" : "");
      soundHint.style.display = audioOk ? "none" : "block";
    }
    last = now;
    requestAnimationFrame(loop);
  }
  requestAnimationFrame(loop);
})();
