# 1. 역공학 준비

목표는 "원작 실행 파일이 무엇을 하는지 **읽을 수 있는 형태**로 전부 꺼내 두는 것"입니다. 한 번 뽑아 두면 이후 작업은 대부분 텍스트 검색입니다.

## 1.1 무엇을 분석하나

조조전 설치본에는 대략 이런 것들이 있습니다.

| 종류 | 예 | 쓰임 |
|---|---|---|
| 실행 파일 | `EKD5.EXE` | 게임 규칙·연출·화면 배치가 전부 여기 있음 |
| DLL | `ITEMICON.DLL`, `MGCICON.DLL`, `CMDICON.DLL`, `MAPATR.DLL` | 아이콘 비트맵 리소스 |
| 압축 묶음 | `*.E5` | 그림, 표, 지도, 글 (2장) |
| 스크립트 | `R_nn.EEX`, `S_nn.EEX` | 이야기·전투 이벤트 바이트코드 (3장) |
| 소리 | `SEnn.WAV`, CD 음악 트랙 | 효과음, 배경음악 |

규칙과 공식은 실행 파일에, 내용물은 데이터 파일에 있습니다. 재구현은 **실행 파일을 옮기고 데이터 파일을 읽는 것**입니다.

## 1.2 Ghidra 를 화면 없이 돌려 전체 덤프

Ghidra 화면에서 함수를 하나씩 보는 것은 느립니다. 처음에 **모든 함수를 한 텍스트 파일로** 디컴파일해 두면, 이후에는 `grep` 과 편집기로 충분합니다.

`re/scripts/DumpAll.java` 가 그 일을 합니다.
- `decompiled.c`: 모든 함수 디컴파일 (`// ===== FUN_xxxxxxxx @ 주소` 머리로 구분)
- `strings.txt`: 정의된 문자열과, 그것을 참조하는 함수 목록
- `imports.txt`: 가져온 윈도 API

실행 예 (경로는 환경에 맞게):

```sh
# 프로젝트를 만들고 EXE 를 가져와 자동 분석한 뒤 덤프
$GHIDRA/support/analyzeHeadless /tmp/ghproj ekd5 \
  -import "/path/to/EKD5.EXE" \
  -scriptPath re/scripts -postScript DumpAll.java re/out
```

결과는 `re/out/` 에 생기며 원작 파생물이라 `.gitignore` 로 막혀 있습니다. 조조전은 `decompiled.c` 가 약 9만 줄이었습니다.

### 덤프에서 함수 하나 꺼내 보기

```sh
awk '$0 ~ "^// ===== FUN_0043dd5b "{p=1} p{print} p&&/^}/{exit}' re/out/decompiled.c
```

### 이 줄을 부르는 함수 찾기

```sh
N=$(grep -n "FUN_00452964(" re/out/decompiled.c | head -1 | cut -d: -f1)
awk -v N=$N 'NR<=N && /^\/\/ ===== FUN_/{l=$3} NR==N{print l}' re/out/decompiled.c
```

이 두 가지(함수 꺼내기, 호출한 함수 찾기)를 수백 번 반복하는 것이 분석의 대부분입니다.

## 1.3 스크립트 명령 처리 표 찾기

이벤트 스크립트가 있는 게임이라면 **명령 번호 → 처리 함수** 표가 실행 파일 어딘가에 있습니다. 이 표를 찾으면 스크립트 엔진 전체를 명령 단위로 옮길 수 있습니다.

찾는 법:
1. 스크립트 파일을 여는 함수를 찾습니다 (`strings.txt` 에서 `.EEX` 를 참조하는 함수).
2. 그 근처에서 "명령 번호를 읽고 `(*표[번호])()` 로 부르는" 반복문을 찾습니다.
3. 표 주소가 나오면 `re/scripts/VmHandlers.java` 처럼, 표의 각 칸을 따라가 함수를 만들고 명령 번호별로 디컴파일합니다. 칸이 점프(`E9`)면 한 번 더 따라갑니다.

조조전은 256칸 표였고, 인자 서명을 정한 명령은 105개였습니다. 같은 함수를 여러 번호가 공유하기도 합니다.

## 1.4 리소스와 문자열을 실행 파일에서 직접 읽기

디컴파일 코드는 **숫자로 된 주소**만 보여 줍니다. 그 주소에 무엇이 있는지는 직접 읽어야 합니다.

### 문자열·포인터 표: `re/tools/pe_strings.py`

Ghidra 에 `(&PTR_DAT_00495f78)[n]` 처럼 표를 쓰는 코드가 보이면:

```sh
python3 -I re/tools/pe_strings.py EKD5.EXE p 0x495f78 11   # 포인터 11개 → 각 문자열
python3 -I re/tools/pe_strings.py EKD5.EXE s 0x48fb40      # 문자열 하나
python3 -I re/tools/pe_strings.py EKD5.EXE b 0x495248 84   # 바이트 (숫자 표)
```

가상 주소 → 파일 위치 변환(PE 섹션 표)과 EUC-KR 해독을 해 줍니다. 한국어판은 문자열이 EUC-KR 입니다.

### 대화상자 리소스: `re/tools/pe_dialogs.py`

원작이 윈도 대화상자(장비 창, 출진 창 등)를 쓴다면 **화면 배치가 리소스에 그대로** 들어 있습니다. 컨트롤 ID, 위치, 크기, 글이 다 나옵니다.

```sh
python3 -I re/tools/pe_dialogs.py EKD5.EXE          # 대화상자 목록
python3 -I re/tools/pe_dialogs.py EKD5.EXE 328      # "장비" 창의 모든 컨트롤
```

컨트롤 ID(예: `0x42e` = 설명 글)는 디컴파일 코드의 `SetDlgItemTextA(hwnd, 0x42e, ...)` 와 바로 이어집니다. **ID 로 검색하면 그 칸을 채우는 함수가 나옵니다.** 이 방법으로 장비 창과 아이템 정보 창을 원작 그대로 옮겼습니다 (8장 사례 5).

### 비트맵·아이콘

DLL 과 EXE 의 비트맵 리소스는 `ekd5 extract` 가 꺼내서 `ICONS.BIN` 하나로 묶습니다 (`crates/ekd5-cli`). 웹에서는 파일 하나를 받는 것이 빠릅니다.

## 1.5 작업 폴더 원칙

- `data/`: 원작에서 꺼낸 자료. **커밋 금지.**
- `re/out/`: 디컴파일 결과. **커밋 금지.**
- 저장소에는 **도구와 코드만.** 원작 파일을 가진 사람이 도구를 돌려 직접 만들게 합니다.

이 원칙을 처음부터 `.gitignore` 로 걸어 두면, 나중에 공개할 때 기록을 뒤질 일이 없습니다.
