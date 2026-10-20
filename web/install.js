// 홈 화면 설치 안내 (처음 한 번만, 설정 창에서 다시 볼 수 있음).
// 안드로이드 크롬·삼성 인터넷·PC 크롬/엣지: 브라우저 설치 창을 [설치하기] 단추로 띄움 (beforeinstallprompt)
// 아이폰 사파리·크롬: 설치 창을 띄울 수 없어서 공유 → 홈 화면에 추가 안내
// 카톡 등 앱 안 브라우저: 설치가 안 되므로 사파리·크롬으로 열라고 안내
(() => {
  const KEY = "ekd5-install-shown";
  const ua = navigator.userAgent;
  const standalone = matchMedia("(display-mode: fullscreen), (display-mode: standalone)").matches || navigator.standalone === true;
  const ios = /iPhone|iPad|iPod/.test(ua) || (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1);
  const android = /Android/.test(ua);
  const inApp = /KAKAOTALK|Instagram|FBAN|FBAV|FB_IAB|NAVER\(inapp|Line\/|DaumApps|everytimeApp|; wv\)/i.test(ua);
  const iosOther = ios && /CriOS|FxiOS|EdgiOS/.test(ua);
  const mobile = ios || android || matchMedia("(pointer: coarse)").matches;
  let deferred = null;
  const get = (k) => { try { return localStorage.getItem(k); } catch (_) { return null; } };
  const set = (k, v) => { try { localStorage.setItem(k, v); } catch (_) {} };

  window.ekd5Install = { open: null, isOpen: false, standalone };

  addEventListener("beforeinstallprompt", (e) => {
    e.preventDefault();
    deferred = e;
    // 이미 안내가 떠 있으면 [설치하기] 단추로 바꿔 줌
    if (window.ekd5Install.isOpen) render();
  });
  addEventListener("appinstalled", () => { set(KEY, "1"); close(); });

  let panel, body, actions;
  function el(tag, attrs, html) {
    const n = document.createElement(tag);
    Object.assign(n, attrs || {});
    if (html != null) n.innerHTML = html;
    return n;
  }
  function build() {
    if (panel) return;
    panel = el("div", { id: "installPanel" });
    const box = el("div", { className: "installBox" });
    box.appendChild(el("div", { className: "installTitle" }, "📲 홈 화면에 설치하기"));
    body = el("div", { className: "installBody" });
    actions = el("div", { className: "installActions" });
    box.appendChild(body);
    box.appendChild(actions);
    panel.appendChild(box);
    document.body.appendChild(panel);
    // 게임 화면으로 눌림이 넘어가지 않게
    for (const t of ["pointerdown", "pointerup", "touchstart", "touchend", "click"]) panel.addEventListener(t, (e) => e.stopPropagation());
  }
  function button(label, primary, fn) {
    const b = el("button", { className: primary ? "installBtn primary" : "installBtn" }, label);
    b.addEventListener("click", fn);
    actions.appendChild(b);
  }
  const why = "<p>홈 화면에 설치하면 주소창 없이 <b>전체 화면</b>으로, 일반 앱처럼 아이콘을 눌러 바로 시작할 수 있어요.</p>";
  function render() {
    build();
    actions.innerHTML = "";
    panel.querySelectorAll(".installArrow").forEach((a) => a.remove());
    if (inApp) {
      body.innerHTML = why +
        "<p>지금은 카카오톡 같은 <b>앱 안의 브라우저</b>라서 설치할 수 없어요.</p>" +
        "<ol><li>화면 구석의 <b>⋮</b> 또는 <b>…</b> 메뉴를 누르고</li>" +
        `<li><b>${ios ? "Safari로 열기" : "다른 브라우저로 열기"}</b>를 고른 뒤</li>` +
        "<li>열린 브라우저에서 다시 이 안내를 따라 주세요.</li></ol>" +
        "<p class='installNote'>새 브라우저에서 비밀번호를 물으면 처음 받은 링크로 다시 열어 주세요.</p>";
    } else if (deferred) {
      body.innerHTML = why + "<p>아래 <b>[설치하기]</b>를 누르고, 뜨는 창에서 한 번 더 <b>설치</b>를 누르면 끝이에요.</p>";
      button("설치하기", true, async () => {
        const d = deferred;
        deferred = null;
        if (!d) return;
        d.prompt();
        try { const r = await d.userChoice; if (r && r.outcome === "accepted") close(); else render(); } catch (_) { render(); }
      });
    } else if (ios && !iosOther) {
      body.innerHTML = why +
        "<ol><li>사파리 <b>아래쪽</b>(또는 주소창 옆)의 <b>공유 버튼</b> <span class='installIcon'>⬆︎</span> 을 누르고<br><span class='installNote'>안 보이면 <b>···</b> 메뉴 안에 있어요</span></li>" +
        "<li>목록을 올려서 <b>홈 화면에 추가</b>를 누른 뒤</li>" +
        "<li>오른쪽 위 <b>추가</b>를 누르세요.</li></ol>";
      panel.appendChild(el("div", { className: "installArrow bottom" }, "⬇︎"));
    } else if (ios) {
      body.innerHTML = why +
        "<ol><li>주소창 <b>오른쪽 위</b>의 <b>공유 버튼</b> <span class='installIcon'>⬆︎</span> 을 누르고</li>" +
        "<li><b>홈 화면에 추가</b>를 누른 뒤</li>" +
        "<li>오른쪽 위 <b>추가</b>를 누르세요.</li></ol>" +
        "<p class='installNote'>메뉴에 없으면 iOS 가 오래된 것이니 사파리로 열어서 추가해 주세요.</p>";
      panel.appendChild(el("div", { className: "installArrow top" }, "⬆︎"));
    } else {
      body.innerHTML = why +
        "<ol><li>브라우저 오른쪽 위 <b>⋮</b> 메뉴를 누르고</li>" +
        "<li><b>앱 설치</b> 또는 <b>홈 화면에 추가</b>를 누르세요.</li></ol>";
    }
    button(deferred && !inApp ? "나중에" : "확인", false, close);
    panel.style.display = "flex";
    window.ekd5Install.isOpen = true;
  }
  function close() {
    if (panel) panel.style.display = "none";
    window.ekd5Install.isOpen = false;
  }
  window.ekd5Install.open = render;

  // 처음 접속 때 한 번만 (이미 설치된 앱으로 열었으면 띄우지 않음)
  if (standalone || get(KEY)) return;
  addEventListener("load", () => {
    // 안드로이드·PC 는 설치 창 신호를 조금 기다림. PC 는 신호가 없으면 안내하지 않음
    setTimeout(() => {
      if (get(KEY) || standalone) return;
      if (!mobile && !deferred) return;
      set(KEY, "1");
      render();
    }, ios ? 1200 : 3000);
  });
})();
