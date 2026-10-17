// 홈 화면 앱 설치용 서비스 워커. 캐시는 하지 않음 (항상 최신, 비밀번호 잠금 그대로)
self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (e) => e.waitUntil(self.clients.claim()));
self.addEventListener("fetch", () => {});
