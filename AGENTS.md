# ekd5-remake — 삼국지 영걸전 조조전(1998, 코에이) Rust/WASM 리메이크

원본: `~/Downloads/삼국지 조조전` (EKD5.EXE 분석). 원본 데이터·파생물은 커밋/공개 금지 (`data/`, `re/out` gitignore).
배포: Cloudflare Pages (비밀번호 잠금 — 사이트 주소·비밀번호는 저장소 밖 LOCAL.md 에 보관) — `./scripts/deploy.sh`.

## 구조
- `crates/ekd5`: ls12(Ls12 해제), palette(B,R,G 4비트), font(ALL_FONT.16P/ANKFONT.DAT), eex(컨테이너), vm(인자 해석·블록), officer(능력·레벨업·장비·장비 경험치·기본 장비), strategy(책략 표·공식), engine/{mod, story, battle, camp(본진·출진 선택), duel(일기토), ui(목록 상자), screen, assets}
- `crates/ekd5-cli`: `ekd5 extract <원본> data`, `ekd5 shots data <out> <틱> [간격]` (EKD5_STAGE, EKD5_AUTO, EKD5_PRESS, EKD5_TAPS, EKD5_AUTOPLAY, EKD5_CHEAT, EKD5_AUTO_END, EKD5_DUMP(턴마다 부대 위치), EKD5_FLAGS, EKD5_SNAP(단계마다 저장 문자열), EKD5_RESTORE)
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
- 무기·방어구 경험치 (FUN_004086c6: 공격 명중 3·빗나감 1·상대가 낮으면 2 / 피격 4·3, 최대 Lv 홀수 종류 9·짝수 3)
- 책략 (피해/회복/상태이상/능력 변화, 성공률, 지형 제한), 도구 사용, 본진(대화·장비·상점·도구 보기·출진), 출진 선택, 돈/명성/보상(op59)
- 맵 물체 op58 (성문: GATE.E5 3×3 덮어그림 + 지형 변경/복원, 불·짚더미: MARK.E5 겹침 + 지형 0x1A/0x1B), 불 op21. 성문 0x11 은 원작대로 막힘
- AI (표 0x498bf8): 0 적극 1 범위 안 2 고수 3 장수 공격 4 지점 5 따라가기 6 도주, op4e AI 변경(한 명/영역), 목록 부대 기본값 1
- 목록 부대(합류 안 한 장수)는 레벨 맞춤 기본 장비 (FUN_004083d7), op48 로 지정
- 증원 등장 op55, 목표 구역 op5b, 부대 대사 op15, 일기토 op68/60/63/64/65/66/5f (UNIT_ATK·UNIT_SPC 2배), 전투 공격 동작 UNIT_ATK
- 플래그는 단계 사이에 지워지지 않음 (원작과 같음, R_ 스크립트의 op0a 가 지움). 예: S_28 은 탈출이 승리 경로
- 자동 진행 검사(치트): 전 단계 엔진 오류 없음. 치트 없이는 S_02(적 기본 장비 반영 후)·S_08 등에서 시험용 AI 가 짐

## 근사/미구현
- 일기토 동작 16종은 그림만 근사 (원작 세부 연출 미분석), 공방 op65/66 종류별 차이 없음
- 연출·시스템 명령 일부 건너뜀: op08(모드 값), op0e/op49(전투 끝 상태), op0f(저장 후 제목), op10, op11, op20(설정), op22, op35, op37, op3e, op51(전원 회복), op5a, op5d, op69, op6b
- 패배하면 같은 전투 다시 (원작은 게임 오버)
- BGM(CD 음원) 없음
