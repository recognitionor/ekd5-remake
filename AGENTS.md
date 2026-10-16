# ekd5-remake — 삼국지 영걸전 조조전(1998, 코에이) Rust/WASM 리메이크

원본: `~/Downloads/삼국지 조조전` (EKD5.EXE 분석). 원본 데이터·파생물은 커밋/공개 금지 (`data/`, `re/out` gitignore).
배포: Cloudflare Pages (비밀번호 잠금 — 사이트 주소·비밀번호는 저장소 밖 LOCAL.md 에 보관) — `./scripts/deploy.sh`.

## 자료 만들기
- `ekd5 extract <설치 폴더> data` (E5/EEX/WAV + DLL 아이콘 → ICONS.BIN + EXE 튜토리얼 대사 → TUTOR.TXT)
- `python3 -I scripts/extract-cd.py <CD.ccd> data` (CD 음악 트랙 2~22 → BGMnn.m4a(afconvert), 설치본에 없는 MCALL09.. 등, AVI → mp4(ffmpeg))

## 구조
- `crates/ekd5`: ls12(Ls12 해제), palette(B,R,G 4비트), font(ALL_FONT.16P/ANKFONT.DAT), eex(컨테이너), vm(인자 해석·블록), officer(능력·레벨업·장비·장비 경험치·기본 장비), strategy(책략 표·공식), engine/{mod, story, battle, camp(본진·출진·보물도감), duel(일기토), fx(책략·맵 연출 MEFF/MCALL), icons(DLL 아이콘), tutor(관상가 노인·회차 기록), ui, screen, assets}
- `crates/ekd5-cli`: `ekd5 extract <원본> data`, `ekd5 shots data <out> <틱> [간격]` (EKD5_STAGE, EKD5_AUTO, EKD5_PRESS, EKD5_TAPS, EKD5_AUTOPLAY, EKD5_CHEAT, EKD5_AUTO_END, EKD5_DUMP(턴마다 부대 위치), EKD5_FLAGS, EKD5_SNAP(단계마다 저장 문자열), EKD5_RESTORE)
- 빌드: cargo 는 PATH 에 없을 수 있음 → `~/.cargo/bin/cargo`
- `crates/ekd5-web` + `web/ekd5.js`: WASM 글루 (진행 자동 저장 → localStorage `ekd5-save`)
- `re/tools/eex.py`: EEX 디스어셈블러 (117개 전부 바이트 정확), `re/out`: Ghidra 덤프

## 형식 요약
- 단계 n: 짝수 R_(n/20)((n%20)/2) 이야기, 홀수 S_ 전투 (맵 번호 = n/2)
- 이야기 배경 MMAP[v+40] 8×8 타일, 인물 PMAPOBJ (48×64×20), 좌표 sx=(x−y)·8+312, sy=(x+y)·4−152
- 전투 HMnn(48칸 연속), HEXZMAP(머리 /3 = 칸 수), SPALET, UNIT_MOV(그룹×3+진영), DATA.E5: 0 인물 32B, 1 무기, 3 직업 27B, 4 지형(행=직업/3)
- 사건: 절1 블록 머리 조건이 맞는 것 전부 차례 실행

## 구현됨 (원작 분석)
- 전투 공식: 피해·명중·치명·연속공격·반격(혼란 불가, 청룡언월도 봉쇄, 자웅일대검 2회)·병종상성·지형효과, ZOC(적토마 예외), 범위 물리공격(투석 직업·사모)
- 경험치/레벨업, 장비 보정, 무기·방어구 경험치, 장비 특수 효과 ~30종 (FUN_00407df9: 무기·방어구 +18/+22, 보조 +17/+21)
- 책략 (피해/회복/상태/능력, 성공률, 지형 제한, 연출 MEFF·MCALL + 효과음 SE_M), 도구, 턴 시작: 상태이상·능력 상태 복귀·지형 회복(0x12~0x16)·장비 회복/경험치
- 본진(대화·장비·상점·도구·보물도감·출진), 출진 선택, 돈/명성(게이지)/보상, 엔딩 3종(op0f)·회차 공통 기록(엔딩·보물도감, localStorage ekd5-common)
- 스크립트: 단계 이동 op11(갈림길·가상 모드·엔딩), 명성 비교 op37, 성문·불·짚더미 op58/op21, AI 7종·op4e, 증원 op55, 일기토, 부대 대사 op15, 얼굴 상태 op20·장 op67, 그림 바꾸기 op35, 턴 제한 op5d, 작전개시 op5a, 맵 연출 op6b, 관상가 튜토리얼 op10(+회차 보상)
- 소리: BGM(CD 트랙=op24 인자+2), 효과음 SEnn/SE_M/SE_E·반복(op23), 전투·메뉴 효과음, 동영상 LOGO/OPEN/END
- 아이콘: 아이템(+20), 책략(계통), 명령, 지형(정보창)
- 자동 진행 검사(치트): 전 단계 → 엔딩 → 2회차까지 오류 없음

## 근사/미구현
- 일기토 동작 16종 그림 근사, 책략 연출의 반투명은 점무늬, 겹 그림(마스크) 미사용
- op08(모드 값)·op0e/op49(전투 끝 상태)·op02(표지)는 화면 동작 없음
- 패배하면 같은 전투 다시 (원작은 게임 오버 → 불러오기)
