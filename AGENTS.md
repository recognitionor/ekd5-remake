# ekd5-remake — 삼국지 영걸전 조조전(1998, 코에이) Rust/WASM 리메이크

원본: `~/Downloads/삼국지 조조전` (EKD5.EXE 분석). 원본 데이터·파생물은 커밋/공개 금지 (`data/`, `re/out` gitignore).
배포: Cloudflare Pages (비밀번호 잠금 — 사이트 주소·비밀번호는 저장소 밖 LOCAL.md 에 보관) — `./scripts/deploy.sh`.

## 구조
- `crates/ekd5`: ls12(Ls12 해제), palette(B,R,G 4비트), font(ALL_FONT.16P/ANKFONT.DAT), eex(컨테이너), vm(인자 해석·블록), engine/{mod, story, battle, screen, assets}
- `crates/ekd5-cli`: `ekd5 extract <원본> data`, `ekd5 shots data <out> <틱> [간격]` (EKD5_STAGE, EKD5_AUTO, EKD5_PRESS, EKD5_TAPS, EKD5_AUTOPLAY)
- `crates/ekd5-web` + `web/ekd5.js`: WASM 글루 (진행 자동 저장 → localStorage `ekd5-save`)
- `re/tools/eex.py`: EEX 디스어셈블러 (117개 전부 바이트 정확), `re/out`: Ghidra 덤프

## 형식 요약
- 단계 n: 짝수 R_(n/20)((n%20)/2) 이야기, 홀수 S_ 전투 (맵 번호 = n/2)
- 이야기 배경 MMAP[v+40] 8×8 타일, 인물 PMAPOBJ (48×64×20), 좌표 sx=(x−y)·8+312, sy=(x+y)·4−152
- 전투 HMnn(48칸 연속), HEXZMAP(머리 /3 = 칸 수), SPALET, UNIT_MOV(그룹×3+진영), DATA.E5: 0 인물 32B, 1 무기, 3 직업 27B, 4 지형(행=직업/3)
- 사건: 절1 블록 머리 조건이 맞는 것 전부 차례 실행

## 근사/미구현 (원작 분석 필요)
- 피해 공식, 적 AI, 경험치/레벨업, 책략·아이템·상점·출진 선택 화면 없음 (출진은 합류 순 자동)
- 성문(0x11)은 통과 가능으로 처리, BGM(CD 음원) 없음
