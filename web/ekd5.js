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
  function playWav(name, vol = 1) {
    if (!audioOk || !actx) return;
    sfxPlay(name, vol, false);
  }
  // 반복 효과음 (원작 op23 방식 0) 과 배경음악 (원작 CD 트랙 → data/BGMnn.m4a, 반복)
  let loopFx = null, bgm = null, bgmTrack = -1, bgmWant = -1;
  function setLoop(name) {
    if (loopFx) { loopFx.stop(); loopFx = null; }
    if (name && audioOk && actx) loopFx = sfxPlay(name, 0.6, true);
  }
  function setMusic(n) {
    bgmWant = n;
    if (!audioOk || n === bgmTrack) return;
    if (bgm) { bgm.pause(); bgm = null; }
    bgmTrack = n;
    if (n >= 0) {
      bgm = new Audio(DATA + "BGM" + String(n).padStart(2, "0") + ".m4a");
      bgm.loop = true; bgm.volume = 0.8; bgm.play().catch(() => {});
    }
  }
  // 브라우저는 첫 터치 전에는 소리를 못 냄 → 터치하면 그때 원하던 음악 시작
  // 아이폰: 무음 스위치와 상관없이 소리 (Web Audio 는 기본이 무음 스위치를 따름)
  try { if (navigator.audioSession) navigator.audioSession.type = "playback"; } catch (_) {}
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
  const applyFit = () => { document.body.classList.toggle("fill", fill); fitBtn.classList.toggle("on", fill); fitBtn.innerHTML = fill ? "비율<br>유지" : "꽉<br>채우기"; };
  applyFit();
  fitBtn.addEventListener("click", (e) => { e.preventDefault(); fill = !fill; applyFit(); try { localStorage.setItem("ekd5-fill", fill ? "1" : "0"); } catch (_) {} });
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

  // ---------- 회차 공통 기록 (엔딩·보물도감) ----------
  try { const c = localStorage.getItem("ekd5-common"); if (c) withStr(c, (p, n) => w.set_common(p, n)); } catch (_) {}

  // ---------- 중간 저장 (그 화면 그대로: 전투 중 내 차례·본진 메뉴) ----------
  const toastEl = document.getElementById("toast");
  let toastTimer = 0;
  function toast(t) { toastEl.textContent = t; toastEl.style.display = "block"; clearTimeout(toastTimer); toastTimer = setTimeout(() => toastEl.style.display = "none", 1800); }
  document.getElementById("save").addEventListener("click", (e) => {
    e.preventDefault();
    const s = readOut(w.quick_save());
    if (!s) { toast("지금은 저장할 수 없습니다 (전투 중 내 차례 · 본진에서 저장)"); return; }
    try { localStorage.setItem("ekd5-quick", s); localStorage.setItem("ekd5-quick-time", new Date().toLocaleString("ko-KR")); toast("저장했습니다"); }
    catch (_) { toast("저장 실패 (브라우저 저장소)"); }
  });
  let quickDone = false;
  try {
    const q = localStorage.getItem("ekd5-quick");
    if (q) {
      const when = localStorage.getItem("ekd5-quick-time") || "";
      let stage = "?"; try { stage = JSON.parse(q).stage; } catch (_) {}
      if (confirm(`중간 저장(${stage}단계, ${when})에서 이어서 할까요?\n취소를 누르면 다른 저장/처음부터를 고릅니다.`)) {
        quickDone = !!withStr(q, (p, n) => w.quick_restore(p, n));
      }
    }
  } catch (_) {}

  // ---------- 이어하기 ----------
  let save = null;
  try { save = localStorage.getItem("ekd5-save"); } catch (_) {}
  if (!quickDone && save && !/^[123];0;/.test(save)) {
    const stage = save.split(";")[1];
    const ok = confirm(`저장된 진행(${stage}단계)이 있습니다. 이어서 할까요?\n취소를 누르면 처음부터 시작합니다.`);
    if (ok) withStr(save, (p, n) => w.restore(p, n));
  }

  // ---------- 동영상 (원작 LOGO/OPEN/END.AVI → mp4). 탭하면 건너뜀 ----------
  const movie = document.getElementById("movie");
  function endMovie() {
    if (!movie.classList.contains("on")) return;
    movie.pause(); movie.removeAttribute("src"); movie.load(); movie.classList.remove("on");
    if (bgm && bgmTrack >= 0) bgm.play().catch(() => {});
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
    if (started) {
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
