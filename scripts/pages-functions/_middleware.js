// 사이트 전체 비밀번호 잠금. 원본 게임 파일이 공개되지 않도록 모든 요청에 적용.
// 비밀번호는 Cloudflare secret `SITE_PASSWORD` (wrangler pages secret put SITE_PASSWORD)
// secret이 없으면 무조건 거부 (fail closed).
// 로그인 방법: ⓪ 전용 링크 /?k=토큰 (입력 없음) ① 로그인 화면에서 비밀번호 한 번 입력 → 쿠키 (아이폰 홈 화면 앱에서도 됨)
//              ② HTTP Basic Auth (ekd5 / 비밀번호) — 예전 방식, 시험 도구용
const USER = "ekd5";
const COOKIE = "ekd5_auth";

function safeEqual(a, b) {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

// 쿠키 값 = SHA-256(비밀번호 + 고정 문자열) — 비밀번호 자체는 쿠키에 넣지 않음
async function token(password) {
  const d = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(password + ":ekd5-cookie-v1"));
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function loginPage(msg) {
  const html = `<!doctype html><html lang="ko"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<meta name="apple-mobile-web-app-capable" content="yes"><meta name="theme-color" content="#000000">
<title>조조전 로그인</title>
<style>html,body{margin:0;height:100%;background:#000;color:#eee;font-family:-apple-system,"Apple SD Gothic Neo",sans-serif}
body{display:flex;align-items:center;justify-content:center}
form{display:flex;flex-direction:column;gap:12px;width:min(320px,86vw)}
h1{font-size:20px;margin:0 0 4px;text-align:center}
input,button{font-size:18px;padding:12px;border-radius:10px;border:1px solid #555;background:#111;color:#eee}
button{background:#8a5a2b;border-color:#c08040;font-weight:600}
p{margin:0;text-align:center;color:#f88;font-size:14px;min-height:18px}</style></head>
<body><form method="post" action="/__login">
<h1>삼국지 조조전</h1>
<input type="password" name="password" placeholder="비밀번호" autocomplete="current-password" autofocus required>
<button type="submit">들어가기</button>
<p>${msg || ""}</p>
</form></body></html>`;
  return new Response(html, { status: 401, headers: { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" } });
}

export async function onRequest({ request, env, next }) {
  const password = env.SITE_PASSWORD;
  if (!password) return new Response("서버 설정 오류: 잠금 비밀번호가 없습니다.", { status: 503 });
  const url = new URL(request.url);
  const good = await token(password);

  // 로그인 제출
  if (url.pathname === "/__login" && request.method === "POST") {
    const form = await request.formData().catch(() => null);
    const pw = (form && form.get("password")) || "";
    if (!safeEqual(String(pw), password)) return loginPage("비밀번호가 틀렸습니다");
    return new Response(null, {
      status: 303,
      headers: {
        Location: "/",
        "Set-Cookie": `${COOKIE}=${good}; Path=/; Max-Age=315360000; HttpOnly; Secure; SameSite=Lax`,
        "Cache-Control": "no-store",
      },
    });
  }

  // 전용 링크 (?k=토큰): 열면 바로 들어가고 기억 — 입력 없음
  const k = url.searchParams.get("k");
  if (k && safeEqual(k, good)) {
    url.searchParams.delete("k");
    return new Response(null, {
      status: 303,
      headers: {
        Location: url.pathname + url.search,
        "Set-Cookie": `${COOKIE}=${good}; Path=/; Max-Age=315360000; HttpOnly; Secure; SameSite=Lax`,
        "Cache-Control": "no-store",
      },
    });
  }

  // 쿠키 확인
  const cookies = request.headers.get("Cookie") || "";
  const m = cookies.match(new RegExp(`(?:^|;\\s*)${COOKIE}=([0-9a-f]+)`));
  let ok = !!(m && safeEqual(m[1], good));

  // Basic Auth 도 허용
  if (!ok) {
    const header = request.headers.get("Authorization") || "";
    if (header.startsWith("Basic ")) {
      try {
        const decoded = atob(header.slice(6));
        const sep = decoded.indexOf(":");
        ok = sep >= 0 && safeEqual(decoded.slice(0, sep), USER) && safeEqual(decoded.slice(sep + 1), password);
      } catch {}
    }
  }

  if (!ok) {
    // 페이지 요청이면 로그인 화면, 파일 요청이면 401 (브라우저 기본 로그인 창은 더 이상 띄우지 않음)
    const wantsHtml = (request.headers.get("Accept") || "").includes("text/html");
    return wantsHtml ? loginPage("") : new Response("로그인이 필요합니다.", { status: 401, headers: { "Cache-Control": "no-store" } });
  }

  // 홈 화면 앱 설정: 시작 주소에 전용 링크를 넣어, 앱이 저장소를 따로 써도 바로 들어가게
  if (url.pathname.endsWith("/manifest.webmanifest")) {
    const res = await next();
    const j = await res.json();
    j.start_url = "/?k=" + good;
    return new Response(JSON.stringify(j), { headers: { "Content-Type": "application/manifest+json", "Cache-Control": "private, no-store" } });
  }

  const res = await next();
  // 인증된 응답이 공유 캐시에 남지 않도록
  const out = new Response(res.body, res);
  out.headers.set("Cache-Control", "private, no-store");
  return out;
}
