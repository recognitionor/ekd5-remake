# ekd5-remake — 삼국지 영걸전 조조전(1998, 코에이) Rust/WASM 리메이크

원본: `~/Downloads/삼국지 조조전` (EKD5.EXE 분석). 원본 데이터·파생물은 커밋/공개 금지 (`data/`, `re/out` gitignore).
배포: Cloudflare Pages (비밀번호 잠금 — 사이트 주소·비밀번호는 저장소 밖 LOCAL.md 에 보관) — `./scripts/deploy.sh`.

## 구조
- `crates/ekd5`: ls12(Ls12 해제), palette(B,R,G 4비트), font(ALL_FONT.16P/ANKFONT.DAT), eex(컨테이너), vm(인자 해석·블록), officer(능력·레벨업·장비), strategy(책략 표·공식), engine/{mod, story, battle, camp(본진·출진 선택), ui(목록 상자), screen, assets}
- `crates/ekd5-cli`: `ekd5 extract <원본> data`, `ekd5 shots data <out> <틱> [간격]` (EKD5_STAGE, EKD5_AUTO, EKD5_PRESS, EKD5_TAPS, EKD5_AUTOPLAY, EKD5_CHEAT, EKD5_AUTO_END)
- 빌드: cargo 는 PATH 에 없을 수 있음 → `~/.cargo/bin/cargo`
- `crates/ekd5-web` + `web/ekd5.js`: WASM 글루 (진행 자동 저장 → localStorage `ekd5-save`)
- `re/tools/eex.py`: EEX 디스어셈블러 (117개 전부 바이트 정확), `re/out`: Ghidra 덤프

## 형식 요약
- 단계 n: 짝수 R_(n/20)((n%20)/2) 이야기, 홀수 S_ 전투 (맵 번호 = n/2)
- 이야기 배경 MMAP[v+40] 8×8 타일, 인물 PMAPOBJ (48×64×20), 좌표 sx=(x−y)·8+312, sy=(x+y)·4−152
- 전투 HMnn(48칸 연속), HEXZMAP(머리 /3 = 칸 수), SPALET, UNIT_MOV(그룹×3+진영), DATA.E5: 0 인물 32B, 1 무기, 3 직업 27B, 4 지형(행=직업/3)
- 사건: 절1 블록 머리 조건이 맞는 것 전부 차례 실행

## 구현됨 (원작 공식)
- 피해·명중·치명·연속공격·반격·병종상성·지형효과, 경험치/레벨업(직업+초기 등급), 장비 보정, 출진 평균 레벨 조정
- 책략 (피해/회복/상태이상/능력 변화, 성공률, 지형 제한), 도구 사용, 본진(대화·장비·상점·도구 보기·출진), 출진 선택, 돈/명성/보상(op59)
- 자동 진행 검사: 전 단계 엔진 오류 없음 (S_03·S_52 등은 자동 AI 가 가끔 짐 — 엔진 문제 아님)

## 근사/미구현
- 무기 경험치/무기 레벨업, 보물(op58), 적 AI 일부 유형(목표 인물 추적 등), 연출용 명령 다수(0x08, 0x2f, 0x49 …) 건너뜀
- 성문(0x11)은 통과 가능으로 처리, BGM(CD 음원) 없음
