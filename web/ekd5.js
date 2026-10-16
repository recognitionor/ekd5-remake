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
  let audioOk = false;
  function playWav(name, vol = 1) {
    if (!audioOk) return;
    const a = new Audio(DATA + name); a.volume = vol; a.play().catch(() => {});
  }
  // 반복 효과음 (원작 op23 방식 0) 과 배경음악 (원작 CD 트랙 → data/BGMnn.m4a, 반복)
  let loopFx = null, bgm = null, bgmTrack = -1, bgmWant = -1;
  function setLoop(name) {
    if (loopFx) { loopFx.pause(); loopFx = null; }
    if (name && audioOk) { loopFx = new Audio(DATA + name); loopFx.loop = true; loopFx.volume = 0.6; loopFx.play().catch(() => {}); }
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
  const unlockAudio = () => { if (!audioOk) { audioOk = true; if (bgmWant >= 0) setMusic(bgmWant); } };

  // ---------- 입력 ----------
  let held = 0, mouse = null, click = 0, pulse = 0, pulseTicks = 0, started = false;
  addEventListener("keydown", (e) => { const b = KEYMAP[e.code]; if (b) { held |= b; e.preventDefault(); } unlockAudio(); started = true; });
  addEventListener("keyup", (e) => { const b = KEYMAP[e.code]; if (b) held &= ~b; });
  function toGame(e) {
    const r = canvas.getBoundingClientRect();
    return [Math.round((e.clientX - r.left) * 640 / r.width), Math.round((e.clientY - r.top) * 480 / r.height)];
  }
  // 누르고 있는 동안 결정 키를 계속 누른 것으로 보냄 (전장 끌어서 스크롤)
  let ptrDown = 0;
  canvas.addEventListener("pointermove", (e) => { mouse = toGame(e); });
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
  holdBtn("skip", KEY.DECIDE);
  document.getElementById("full").addEventListener("click", async () => {
    try {
      if (!document.fullscreenElement) { await document.documentElement.requestFullscreen(); try { await screen.orientation.lock("landscape"); } catch (_) {} }
      else await document.exitFullscreen();
    } catch (_) {}
  });

  // ---------- 회차 공통 기록 (엔딩·보물도감) ----------
  try { const c = localStorage.getItem("ekd5-common"); if (c) withStr(c, (p, n) => w.set_common(p, n)); } catch (_) {}

  // ---------- 이어하기 ----------
  let save = null;
  try { save = localStorage.getItem("ekd5-save"); } catch (_) {}
  if (save && !/^[123];0;/.test(save)) {
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

  // ---------- 명성 게이지 (원작 FUN_00478905 / FUN_0040d717: 바뀔 때 다섯 번에 나눠 깜빡이며 움직임) ----------
  const fameEl = document.getElementById("fame"), fameFill = document.getElementById("fameFill"),
        fameBar = document.getElementById("fameBar"), fameNum = document.getElementById("fameNum");
  let fameShown = -1, fameStep = 0;
  function updateFame(v) {
    if (fameShown < 0) fameShown = v;
    if (fameShown !== v) {
      if (++fameStep >= 3) { fameStep = 0; fameShown += Math.sign(v - fameShown) * Math.max(1, Math.ceil(Math.abs(v - fameShown) / 5)); }
      fameEl.classList.toggle("blink", Math.floor(performance.now() / 120) % 2 === 0);
    } else fameEl.classList.remove("blink");
    fameFill.style.height = fameShown + "%";
    fameFill.classList.toggle("low", fameShown < 16);
    fameBar.classList.toggle("high", fameShown >= 85);
    fameNum.textContent = fameShown;
  }

  // ---------- 루프 ----------
  setStatus("화면을 탭하면 시작합니다");
  let last = performance.now(), acc = 0;
  function step() {
    const pulseBits = pulseTicks > 0 ? pulse : 0;
    if (pulseTicks > 0) pulseTicks--;
    const m = mouse || [0, 0];
    w.tick(held | pulseBits | ptrDown, m[0], m[1], mouse ? 1 : 0, click);
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
  function loop(now) {
    if (started) {
      acc += Math.min(250, now - last);
      while (acc >= TICK_MS) { step(); acc -= TICK_MS; }
      const p = w.frame();
      img.data.set(mem().subarray(p, p + 640 * 480 * 4)); ctx.putImageData(img, 0, 0);
      setStatus(loading > 0 ? "불러오는 중…" : "");
    }
    last = now;
    requestAnimationFrame(loop);
  }
  requestAnimationFrame(loop);
})();
