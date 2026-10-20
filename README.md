<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/banner-dark.png">
    <img src="docs/assets/banner-light.png" alt="AxDB — AX DB Tool" width="900">
  </picture>
</p>

<p align="center">
  <b>여러 데이터베이스를 한 화면에서 다루는 가볍고 빠른 데스크톱 DB 툴</b><br>
  관계형 · 그래프 · 문서 · 키-값을 각자의 모양 그대로 보여 주고, 필요한 드라이버만 골라 설치하며, AI 에이전트가 권한 안에서 함께 일합니다.
</p>

<p align="center">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-1.99.1-B7410E?logo=rust&logoColor=white">
  <img alt="Preact" src="https://img.shields.io/badge/Preact-11-673AB8?logo=preact&logoColor=white">
  <img alt="MCP" src="https://img.shields.io/badge/MCP-scoped%20tokens-8B7BFF">
  <img alt="Tests" src="https://img.shields.io/badge/tests-157%20Rust%20%C2%B7%20113%20UI%20%C2%B7%2039%20E2E-34D399">
  <img alt="License" src="https://img.shields.io/badge/license-MIT-lightgrey">
</p>

<p align="center">
  <img src="docs/assets/screens/pg-table-midnight.png" alt="PostgreSQL 결과 표 (Midnight 테마)" width="880">
</p>

---

## 왜 AxDB인가

| | |
|---|---|
| **DB마다 맞는 화면** | 모든 DB를 SQL과 표에 억지로 맞추지 않습니다. 관계형은 스키마·표·ERD, 그래프는 노드·관계·경로, 문서는 컬렉션·JSON 트리, 키-값은 키 계층과 자료형별 표로 보여 줍니다. |
| **필요한 드라이버만 설치** | 앱은 드라이버 없이 시작합니다. PostgreSQL · Neo4j · SQLite · MongoDB · Redis 중 쓰는 것만 설치하고, 드라이버는 별도 프로세스로 실행됩니다. 설치할 때 서명과 SHA-256을 검증합니다. |
| **안전한 쓰기** | 자동 쓰기는 하지 않습니다. 변경은 실행한 뒤 반드시 롤백하는 **미리보기**로 먼저 확인합니다. 표에서 셀을 고치거나 행을 추가·삭제해도 **편집 → 미리보기 → 적용**을 거치며, 적용은 한 트랜잭션입니다. 연결 단위 **읽기 전용**은 호스트가 강제합니다. |
| **에이전트와 함께** | MCP로 접속하는 에이전트마다 연결별 권한(none / read / preview / write-with-approval), 해시로 저장한 토큰, 감사 로그를 둡니다. |
| **스키마를 아는 AI** | 스키마와 작업 맥락을 읽고 쿼리를 제안합니다. 제안은 실행하지 않고 새 탭에 넣기만 합니다. 앱 안에서 Claude 로그인으로 쓸 수 있습니다. |

## 설치

[**GitHub Releases 최신 버전**](https://github.com/sangwookp9591/db-tool/releases/latest)에서 운영체제에 맞는 파일을 내려받습니다.

| 운영체제 | 파일 | 설치 |
|---|---|---|
| macOS (Apple Silicon · Intel) | `AxDB_<버전>_universal.dmg` | dmg를 열고 AxDB를 응용 프로그램 폴더로 끌어 놓습니다. |
| Windows (x64) | `AxDB_<버전>_x64-setup.exe` | 설치 파일을 실행합니다. WebView2 런타임이 없으면 설치 중에 내려받습니다. |

Windows 설치 파일에는 코드 서명이 없습니다. 처음 실행할 때 SmartScreen 경고가 뜨면 **추가 정보 → 실행**을 누르세요.

0.1.x에는 업데이트 기능이 없었으므로 0.2.0은 설치 파일로 한 번 설치해야 합니다. 그 이후 버전은 앱 안에서 업데이트합니다([자동 업데이트](#자동-업데이트)).

## 처음 실행하면

첫 실행 때 7장면 안내가 나옵니다. 메뉴에서 다시 볼 수 있습니다.

1. 환영
2. **DB 드라이버**: 필요한 드라이버를 여러 개 골라 바로 설치합니다.
3. 연결과 탐색
4. ERD와 뷰어
5. 권한과 안전
6. MCP 연결
7. 파일 내보내기와 AI

<table>
  <tr>
    <td width="50%"><img src="docs/design-shots/onboarding-2-midnight.png" alt="온보딩: DB 드라이버 선택"><br><sub><b>온보딩 2장면</b> · 필요한 드라이버를 골라 설치합니다</sub></td>
    <td width="50%"><img src="docs/design-shots/onboarding-6-midnight.png" alt="온보딩: MCP 연결"><br><sub><b>온보딩 6장면</b> · MCP 클라이언트 설정을 복사하고 범위 토큰을 발급합니다</sub></td>
  </tr>
</table>

## 드라이버 설치

사이드 레일의 **드라이버**에서 설치합니다. 연결을 추가할 때 드라이버가 없으면 같은 설치 화면으로 이어지고, 설치되지 않은 드라이버를 쓰는 기존 연결에는 "드라이버 필요 · 설치" 버튼이 나옵니다.

- **설치됨 / 사용 가능** 두 탭으로 나뉘고, 카드마다 버전·크기·설명과 설치·업데이트·삭제 버튼이 있습니다. 삭제는 5초 안에 되돌릴 수 있습니다.
- **설치 단계**는 다운로드 → 검증 → 서명 확인 → 설치 → 연결 확인입니다. 진행률, 전송 속도, 남은 시간을 보여 주고 중간에 취소하거나 실패하면 다시 시도할 수 있습니다. 여러 개를 고르면 순서대로 설치합니다.
- **서명 검증:** 앱에 내장한 공식 공개 키로 Ed25519 서명과 실행 파일의 SHA-256을 확인합니다. 압축 해제는 임시 폴더에서 끝낸 뒤 이름을 바꾸는 방식이라 설치가 중간에 끊겨도 기존 버전이 남습니다.
- **파일에서 설치:** 내려받은 `.axdriver` 파일을 직접 설치할 수 있습니다. 서명이 없거나 알 수 없는 키로 서명된 패키지는 "확인되지 않은 드라이버" 경고를 확인하고 동의해야 설치됩니다.
- 앱을 시작할 때 네트워크를 호출하지 않습니다. 설치된 드라이버는 앱 데이터 폴더에서만 읽고, 드라이버 화면을 열 때 서명된 목록 파일(`index.json`) 하나만 갱신합니다.
- 드라이버가 비정상 종료돼도 앱은 계속 동작하고, 다음 요청 때 드라이버가 다시 시작됩니다.

<table>
  <tr>
    <td width="50%"><img src="docs/design-shots/drivers-glass-midnight.png" alt="드라이버 화면"><br><sub><b>드라이버</b> · 사용 가능한 드라이버를 카드로 보여 줍니다</sub></td>
    <td width="50%"><img src="docs/design-shots/drivers-tabs-daylight.png" alt="드라이버 화면 (Daylight)"><br><sub><b>설치됨 · 사용 가능 탭</b> · Daylight 테마</sub></td>
  </tr>
</table>

> 드라이버 화면과 업데이트 화면의 캡처는 개발용 모의 데이터로 찍었습니다.

새 DB용 드라이버를 만들려면 [플러그인 가이드](docs/PLUGIN_GUIDE.md)를 보세요.

## 연결과 탐색

- 연결 폼은 드라이버가 선언한 필드로 그려집니다. 비밀번호는 OS 키체인에만 저장하고, 프로필 파일(권한 0600)과 로그에는 남기지 않습니다.
- 사이드바는 **연결**과 **탐색기** 두 영역으로 나뉘며 각각 접고 펼 수 있고 둘 사이 높이를 조절할 수 있습니다. 컴팩트 보기도 있습니다.
- 탐색기는 한 단계씩 불러옵니다. 노드의 동작(표 열기, 쿼리 열기, 다이어그램 열기)은 드라이버가 정합니다.
- ⌘K 명령 팔레트, 실행 이력, 실행마다 취소와 타임아웃을 지원합니다.

## 뷰어

결과에 들어 있는 뷰만 탭으로 보여 줍니다.

| 뷰어 | 내용 |
|---|---|
| **표** | 행·열을 가상화한 표. 정렬·필터, 타입 헤더, 정확한 int8·numeric·jsonb·timestamptz 표시 |
| **그래프** | Canvas 그래프. 이웃 확장, 검색, 라벨별 범례, 표와 선택 동기화 |
| **ERD** | PG 외래 키, SQLite, Neo4j 스키마, Mongo 샘플 기반 참조 추론을 같은 다이어그램으로 그립니다 |
| **문서** | 가상화한 JSON 트리. ObjectId · Decimal128 · Date 같은 BSON 고유 타입을 그대로 보여 줍니다 |
| **키-값** | Redis 키 계층(SCAN 기반)과 자료형별 표, TYPE · TTL |

외래 키 셀에서 참조 행으로 이동하는 연결 탐색과 탐색 이력도 있습니다.

<table>
  <tr>
    <td width="50%"><img src="docs/assets/screens/neo4j-graph-midnight.png" alt="Neo4j 그래프"><br><sub><b>그래프</b> · Cypher 결과를 그래프와 표로 오가며 이웃을 확장합니다</sub></td>
    <td width="50%"><img src="docs/assets/screens/mongo-documents-midnight.png" alt="MongoDB 문서 트리"><br><sub><b>문서</b> · 가상화한 JSON 트리에 BSON 고유 타입을 그대로 보여 줍니다</sub></td>
  </tr>
  <tr>
    <td><img src="docs/assets/screens/erd-daylight.png" alt="ERD"><br><sub><b>ERD</b> · 관계형·그래프·문서를 같은 다이어그램으로 그립니다</sub></td>
    <td><img src="docs/assets/screens/connection-midnight.png" alt="연결 추가"><br><sub><b>연결 추가</b> · 드라이버가 선언한 필드로 폼을 그립니다</sub></td>
  </tr>
</table>

<details>
<summary>Daylight(라이트) 테마</summary>
<img src="docs/assets/screens/pg-table-daylight.png" alt="Daylight 테마">
</details>

## 지원하는 데이터베이스

| DB | 패러다임 | 탐색 | 결과 | 미리보기 | 표 편집 | 읽기 전용 | ERD |
|---|---|---|---|---|---|---|---|
| **PostgreSQL** | 관계형 | 스키마 · 테이블 · 컬럼 · 인덱스 | 가상화 표, 정확한 int8 · numeric · jsonb · timestamptz | ✅ 롤백 | ✅ 기본 키 | ✅ DB 수준 | ✅ FK |
| **Neo4j** | 그래프 | 라벨 · 관계 · 속성 | 그래프 ↔ 표, 경로, 혼합 결과 | ✅ 롤백 | — | ✅ READ 모드 | ✅ |
| **MongoDB** | 문서 | DB · 컬렉션 | JSON 트리, ObjectId · Decimal128 · Date | — | — | ✅ | ✅ 참조 추론 |
| **Redis** | 키-값 | SCAN 기반 키 계층 | 자료형별 표, TYPE · TTL | — | — | ✅ 허용 목록 | — |
| **SQLite** | 관계형 | 테이블 · 컬럼 | 표 | ✅ 롤백 | ✅ 기본 키 | ✅ query_only | ✅ |

공통 UI는 드라이버가 선언한 기능(capabilities)만 보고 화면을 그립니다. 그래서 MongoDB와 Redis를 추가할 때 `ui/`와 `crates/dbcore`는 한 줄도 바꾸지 않았습니다.

## 변경 미리보기

- **쿼리 미리보기:** 실행한 뒤 반드시 롤백합니다. 영향 행 수, 변경 전·후 표본, 스키마 diff를 보여 줍니다. 롤백할 수 없는 문장(트랜잭션 제어문 등)은 실행하지 않고 이유를 알려 줍니다. PostgreSQL · Neo4j · SQLite가 지원합니다.
- **표 편집:** browse 표에서 셀 수정·행 추가·삭제를 모은 뒤 미리보기를 거쳐 한 트랜잭션으로 적용합니다(PostgreSQL · SQLite, 기본 키 필요). Update·Delete 영향 수가 정확히 1이 아니면 전체를 롤백합니다. 미리보기 없이 적용할 수 없고, 읽기 전용 연결과 기본 키 없는 표는 편집할 수 없습니다.

## 권한과 읽기 전용

- **연결별 읽기 전용:** 호스트가 UI 실행에도 강제합니다. 드라이버가 DB 수준 읽기 전용을 보증하지 못하면 승인 절차를 거칩니다.
- **에이전트 권한:** 클라이언트마다 연결별로 none / read / preview / write-with-approval을 정합니다. write-with-approval도 매번 앱 안에서 사용자가 승인해야 합니다.
- 에이전트가 실행하는 쿼리는 기본적으로 DB 수준에서 읽기 전용입니다.

## MCP 연결

상태바의 **MCP 브리지**를 켜면(기본은 꺼짐) localhost에 MCP 서버가 열립니다. 클라이언트 설정 JSON을 복사해 Claude 같은 HTTP MCP 클라이언트에 붙여 넣으세요.

- 토큰은 클라이언트마다 발급하는 범위 토큰이며 해시로 저장합니다. 회전·폐기할 수 있고, 호출은 감사 로그에 남습니다.
- 도구 목록과 입력 형식은 [AGENT.md](docs/AGENT.md)에 있습니다. `list_drivers`는 설치된 드라이버만 돌려줍니다.
- 복사한 JSON에는 토큰이 들어 있습니다. 브리지를 다시 시작하면 URL과 토큰을 다시 복사하세요.

<table>
  <tr>
    <td width="50%"><img src="docs/assets/screens/agent-tools-daylight.png" alt="에이전트 클라이언트"><br><sub><b>에이전트</b> · 실제 쿼리나 MCP 호출이 있을 때만 빔에 펄스가 흐르고, 도구 호출 기록을 남깁니다</sub></td>
    <td width="50%"><img src="docs/design-shots/sidebar-1280-compact.png" alt="사이드바 컴팩트 보기"><br><sub><b>사이드바</b> · 연결·탐색기를 접고 펼치며 컴팩트 보기로 바꿉니다(모의 데이터)</sub></td>
  </tr>
</table>

## AI 도우미

기본으로 꺼져 있습니다. 설정에서 인증 방식을 고르면 켜집니다.

- **Claude 로그인(ant CLI):** 패널의 "Claude 로그인" 버튼을 누르면 앱 안에서 로그인이 끝납니다.
- **API 키:** OS 키체인에 저장합니다.

스키마와 작업 맥락(활성 탭, 최근 오류 등)을 읽고 쿼리를 제안합니다. 제안은 자동으로 실행하지 않고 새 탭에 넣기만 합니다.

<p align="center">
  <img src="docs/assets/screens/ai-midnight.png" alt="AI 도우미" width="640"><br>
  <sub><b>AI 도우미</b> · 확인한 근거(구문 → 스키마 → EXPLAIN)를 신호로 보여 주며 대안 쿼리를 제안합니다</sub>
</p>

## 내보내기

결과 헤더의 내보내기 버튼으로 현재 불러온 값을 저장합니다.

| 대상 | 형식 |
|---|---|
| 표 | CSV(UTF-8 BOM, Excel 호환), JSON(큰 정수·decimal을 문자열로 보존), Markdown |
| 문서 | JSON, NDJSON |
| 그래프 | `nodes`/`edges` JSON, PNG |
| ERD | SVG, PNG(현재 테마 배경 포함 또는 투명) |

표는 필터와 정렬을 적용한 순서로 저장합니다. 결과가 잘렸거나 다음 페이지가 있으면 저장 전에 불러온 행 수를 알려 줍니다. 저장 경로는 앱이 연 저장 대화상자에서 고른 것만 쓸 수 있습니다.

<p align="center">
  <img src="docs/design-shots/export-erd-dialog.png" alt="ERD 이미지 내보내기 대화상자" width="720">
</p>

## 자동 업데이트

0.2.0부터 앱 안에서 업데이트합니다.

- 시작 10초 뒤와 이후 24시간마다 새 버전을 확인합니다. 새 버전이 있으면 상태바에 배지가 나타납니다.
- 배지를 누르면 현재 버전 → 새 버전과 릴리스 노트를 보여 주고, **지금 업데이트**로 내려받아 설치한 뒤 다시 시작합니다. **나중에**와 **이 버전 건너뛰기**도 있습니다.
- 업데이트 파일은 서명을 검증한 뒤 설치합니다. 업데이트 피드는 `https://github.com/sangwookp9591/db-tool/releases/latest/download/latest.json`입니다.
- 설정에서 자동 확인을 끄고 수동으로 확인할 수 있습니다. Windows에서는 설치할 때 앱이 종료되고 다시 열립니다.

<p align="center">
  <img src="docs/design-shots/update-notes-midnight.png" alt="업데이트 릴리스 노트 대화상자" width="720"><br>
  <sub>릴리스 노트 대화상자(모의 데이터)</sub>
</p>

## 성능

측정한 값만 적습니다. 환경·방법·한계는 각 보고서에 있고, 다른 환경으로 일반화하지 않습니다.

| 항목 | 측정값 | 출처 |
|---|---|---|
| 드라이버를 별도 프로세스로 분리한 비용(10만 행, 같은 하네스에서 내장 방식과 교차 7쌍) | 첫 UI 채널 행 +1.01%, 전체 완료 +11.63% (debug 빌드, 예산 +20% 이내) | [P1 보고서](docs/P1_PLUGIN_REPORT.md) |
| 드라이버 프로세스 유휴 RSS(연결된 상태) | 6~17MiB(Redis 6.14 · SQLite 6.36 · PostgreSQL 8.59 · Neo4j 11.84 · MongoDB 16.75), 세션을 닫으면 10초 CPU 증가 0 | [P1 보고서](docs/P1_PLUGIN_REPORT.md) |
| 같은 쿼리 50회 반복 뒤 host 메모리 | 696 → **63MiB** (macOS 기본 할당자가 해제한 메모리를 OS에 돌려주지 않는 문제를 찾아 mimalloc으로 해결) | [RESOURCE_OPT](docs/RESOURCE_OPT.md) |
| 연결 추가 흐름(네트워크를 거치지 않는 단계) | 모두 p95 77ms 이하, Connect(첫 트리 표시 포함) p50 134ms | [VERIFICATION](docs/VERIFICATION.md) |
| 드라이버 화면 목록 표시(Chromium 모의 환경) | 캐시가 있으면 2.7~18.7ms, 캐시가 없으면 스켈레톤 5.8ms | [P3 보고서](docs/P3_DRIVERS_UX_REPORT.md) |
| 뷰어(Chromium 모의 환경) | 300노드 그래프 팬·줌 p95 16.7ms, 10만 행 cold 첫 표시 83.0ms(50ms 목표에는 미달), 흔들림·CLS 0 | [D1 보고서](docs/D1_DESIGN_REPORT.md), [VIEWER_PERF](bench/VIEWER_PERF.md) |
| 유휴 CPU(네이티브, 60.8초 구간) | 0.0987%, 유휴 애니메이션 0 | [D1 보고서](docs/D1_DESIGN_REPORT.md) |

원자료와 측정 방법은 [VERIFICATION](docs/VERIFICATION.md), [RESOURCE_OPT](docs/RESOURCE_OPT.md), [VIEWER_PERF](bench/VIEWER_PERF.md)에 있습니다.

## 보안

- 비밀번호와 토큰은 OS 키체인에만 저장합니다. 프로필 파일(권한 0600)과 로그에는 남기지 않습니다.
- 앱은 자동으로 쓰지 않습니다. 에이전트가 실행하는 쿼리는 기본적으로 DB 수준에서 읽기 전용이고, 쓰기는 앱 안에서 사용자가 승인해야 합니다.
- TLS는 실제 서버로 검증했습니다. PG는 `sslmode`(require / verify-ca / verify-full)와 CA 파일 지정을, Neo4j는 `+s`와 `+ssc`를, Mongo는 `tls`와 `tlsCAFile`을 확인했습니다.
- 드라이버 패키지는 공식 공개 키의 Ed25519 서명과 SHA-256으로 검증합니다. 비밀번호는 드라이버 프로세스에 연결 요청으로만 전달하고 실행 인자나 환경 변수에 넣지 않습니다.

## 디자인

- **테마:** Midnight(다크), Daylight(라이트), 시스템 설정 따르기 세 가지입니다.
- **글꼴:** UI는 Pretendard, 코드와 결과 값은 JetBrains Mono를 씁니다. 둘 다 앱에 내장해 오프라인에서도 그대로 보입니다.
- **마이크로 인터랙션:** React Bits에서 가져온 컴포넌트(JellyRadio, SwipeRow, CometDial, WakeSlider, CodeSlots, LatticeLoader, BorderGlow, SpringCheck, FuseButton, CallChip, GlideSelect, BranchedMenu, 드라이버 화면의 GlassIcons)와 ⌘K 명령 팔레트, 토스트, 툴팁, Integration Beam 등을 씁니다.
- **모션 원칙:** 입력이 있을 때만 움직이고, 가만히 두면 애니메이션은 0입니다. `prefers-reduced-motion`을 따르고, 결과 그리드와 그래프 캔버스 안에는 인터랙션을 넣지 않습니다.
- **문구:** UI는 한국어입니다. Host·Port·TLS·SQL 같은 기술 용어와 DB가 돌려준 원본 오류는 그대로 둡니다.
- **창 크기:** 최소 960×600입니다.

세부 규칙은 [디자인 시스템](docs/DESIGN.md)에 있습니다.

## 개발

필요한 도구는 `rust-toolchain.toml`에 고정한 Rust 1.99.1, Node 22 이상, pnpm입니다. workspace는 Rust 2024 edition과 최소 Rust 1.99를 사용하며, rustup 전역 기본값은 바꾸지 않습니다.

### 빌드와 실행

```sh
pnpm --dir ui install
pnpm --dir ui build                              # ui/dist 생성 (Tauri가 임베드)

# 개발 빌드: 드라이버 실행 파일을 먼저 만들고 AXDB_DRIVER_DIR로 지정합니다.
CARGO_TARGET_DIR=target-dev cargo build --workspace --bins
CARGO_TARGET_DIR=target-dev TAURI_CONFIG='{"build":{"devUrl":null}}' \
AXDB_DRIVER_DIR="$PWD/target-dev/debug" cargo run -p axdb

# 설치 번들(.app / .dmg / .exe). 드라이버는 포함하지 않습니다.
pnpm dlx @tauri-apps/cli@2 build --config src-tauri/tauri.conf.json

# UI만 브라우저에서 (mock 백엔드)
pnpm --dir ui dev                                # http://127.0.0.1:5173
```

앱은 드라이버를 내장하지 않으므로 설치한 드라이버가 없으면 드라이버 목록이 비어 있습니다. 개발 중에는 `AXDB_DRIVER_DIR`(debug 빌드에서만 읽음)에 있는 `axdb-driver-*` 실행 파일을 등록합니다.

| 환경 변수 | 용도 |
|---|---|
| `AXDB_DRIVER_DIR` | 개발 빌드가 드라이버 실행 파일을 읽을 디렉터리(debug 전용). 테스트도 이 경로를 씁니다 |
| `AXDB_CONFIG_DIR` | 설정·이력·로그 위치를 바꿉니다(테스트 격리용) |
| `AXDB_AGENT_BRIDGE=1` | localhost MCP 브리지를 켭니다(기본은 꺼짐). 자세한 내용은 [AGENT.md](docs/AGENT.md) |

### 테스트

```sh
# 사용자 DB와 분리한 테스트 DB
docker run -d --name axdb-test-pg -e POSTGRES_PASSWORD=testpw -e POSTGRES_DB=axdb_test -p 127.0.0.1:55432:5432 postgres:16-alpine
docker run -d --name axdb-test-neo4j -e NEO4J_AUTH=neo4j/testpassword -p 127.0.0.1:57687:7687 neo4j:5-community

CARGO_TARGET_DIR=target-dev cargo build --workspace --bins
AXDB_DRIVER_DIR="$PWD/target-dev/debug" CARGO_TARGET_DIR=target-dev \
AXDB_TEST_PG='host=127.0.0.1 port=55432 user=postgres password=testpw dbname=axdb_test' \
AXDB_TEST_NEO4J='bolt://127.0.0.1:57687|neo4j|testpassword' \
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm --dir ui test && pnpm --dir ui exec tsc --noEmit

# 실제 앱 E2E (MCP 브리지 경유)
node bench/e2e-bridge.mjs && node bench/preview-e2e.mjs && node bench/permissions-e2e.mjs
```

테스트 DB 변수는 환경 변수가 없으면 해당 실DB 테스트를 건너뜁니다. 테스트는 격리된 테스트 서버만 허용합니다.

| 변수 | 대상 |
|---|---|
| `AXDB_TEST_PG` | PostgreSQL (포트 55432) |
| `AXDB_TEST_NEO4J` | Neo4j (포트 57687) |
| `AXDB_TEST_MONGO` | `mongodb://axdb:testmongo@127.0.0.1:57017/?authSource=admin` |
| `AXDB_TEST_REDIS` | `redis://:testredis@127.0.0.1:56379/0` |
| `AXDB_TEST_PG_TLS`, `AXDB_TEST_NEO4J_TLS`, `AXDB_TEST_MONGO_TLS`, `AXDB_TEST_TLS_CA`, `AXDB_TEST_TLS_WRONG_CA` | TLS 검증용 서버와 CA 파일 |

### 드라이버 만들기

crate를 만들어 `Driver`를 구현하고, SDK의 `serve`로 실행 파일을 만든 뒤, `cargo xtask package-drivers`로 패키지·서명하고 앱에서 설치합니다. 계약, capabilities 규칙, 프로토콜은 [플러그인 가이드](docs/PLUGIN_GUIDE.md)에, 설계 배경은 [ADR-0003](docs/adr/0003-installable-drivers.md)에 있습니다.

### 릴리스와 배포

배포 채널은 GitHub Releases 하나입니다. 이용자는 [최신 릴리스](https://github.com/sangwookp9591/db-tool/releases/latest)에서 설치 파일을 받고, 이미 설치한 앱은 같은 곳의 `latest.json`을 읽어 스스로 업데이트합니다. 드라이버는 앱과 별도로 `drivers-*` 릴리스에 올라갑니다.

**최초 1회 준비** (자세한 화면 경로는 [RELEASE_SIGNING](docs/RELEASE_SIGNING.md))

| 무엇 | 어디에 | 용도 |
|---|---|---|
| Developer ID Application 인증서 | 로그인 키체인 | macOS 앱·드라이버 코드 서명 |
| 공증 프로필 `axdb-notary`(App Store Connect API 키) | 키체인 | Apple 공증 제출 |
| 업데이트 서명 키 `~/.tauri/axdb-updater.key`, 암호는 키체인 `axdb-updater` | 로컬 파일 + 키체인 | 앱 자동 업데이트 서명. **잃어버리면 이미 설치된 앱에 업데이트를 보낼 수 없으니 반드시 백업합니다** |
| 드라이버 서명 키(`cargo xtask gen-signing-key`) | 키체인 | `.axdriver` 패키지 서명. 공개 키는 `crates/driver-package`에 내장 |
| Windows 크로스 빌드 도구 | `rustup target add x86_64-pc-windows-msvc`, `brew install llvm nsis`, `cargo install cargo-xwin` | macOS에서 `.exe` 빌드 |

**앱 릴리스 순서**

1. **버전 올리기:** `Cargo.toml`(workspace), `src-tauri/tauri.conf.json`, `ui/package.json`의 버전을 같게 맞춥니다.
2. **릴리스 노트:** `docs/release-notes/v<버전>.md`를 씁니다. `## 제목 {good|bad|neutral}` 섹션 하나가 앱의 "새로운 기능" 한 줄이 되고, 첫 항목이 요약으로 쓰입니다. 태그는 업데이트 피드에서는 자동으로 지워집니다.
3. **검증:** `cargo test --workspace`(테스트 DB와 `AXDB_DRIVER_DIR`), `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm --dir ui test`, `pnpm --dir ui exec tsc --noEmit`.
4. **드라이버가 바뀌었다면 먼저 패키징:**
   ```sh
   APPLE_SIGNING_IDENTITY="Developer ID Application: …" \
   cargo xtask package-drivers --target aarch64-apple-darwin --target x86_64-apple-darwin \
     --target x86_64-pc-windows-msvc --version <버전> --notarize
   ```
   `dist/drivers/`에 서명된 `.axdriver`와 `index.json`이 생기고, 앱 빌드 때 이 목록이 오프라인 스냅숏으로 함께 들어갑니다.
5. **앱 빌드·서명·공증:**
   ```sh
   scripts/release.sh <버전>            # dist/release/<버전>/에 산출물만 만든다(업로드 없음)
   ```
   universal `.dmg`(Developer ID 서명 → 공증 → staple), Windows NSIS `.exe`, 업데이트 아카이브와 서명(`.sig`), `latest.json`을 만듭니다. 공증 진행은 `xcrun notarytool history --keychain-profile axdb-notary`로 봅니다.
6. **설치 확인:** dmg를 다른 위치에 설치해 실행하고 `spctl -a -vv -t open --context context:primary-signature <dmg>`가 `Notarized Developer ID`인지 확인합니다. 드라이버 설치, 연결, 업데이트 확인까지 한 번 돌려 봅니다.
7. **게시(공개, 되돌리기 어려움):**
   ```sh
   scripts/publish-drivers.sh dist/drivers   # 드라이버가 바뀐 경우. drivers-* 릴리스는 latest로 표시되지 않는다
   scripts/release.sh <버전> --publish       # v<버전> 릴리스를 latest로 만들고 dmg·exe·latest.json을 올린다
   ```
   드라이버 릴리스는 `--latest=false`로 올라가므로 `releases/latest`는 항상 앱 릴리스를 가리킵니다. 앱 업데이트 피드와 외부 설치 링크가 이 주소를 따르기 때문입니다.
8. **배포 후:** 기존 버전 앱에서 상태바 업데이트 배지가 뜨는지 확인하고, 변경 사항을 커밋·푸시합니다.

**Windows 참고:** 설치 파일에 코드 서명이 없어 처음 실행할 때 SmartScreen 경고가 뜹니다(**추가 정보 → 실행**). 경고를 없애려면 OV/EV 코드 서명 인증서를 `bundle > windows > signCommand`에 연결합니다.

## 구조

```
crates/dbcore            드라이버 계약: Driver·Session, Manifest·Capabilities, typed Value, 다형 ResultSet
crates/driver-protocol   드라이버 프로세스 프로토콜(길이 접두사 + MessagePack 프레임)
crates/driver-sdk        axdb_driver_sdk::serve — Driver를 플러그인 실행 파일로 만든다
crates/driver-package    .axdriver 패키지와 서명된 목록(index.json): 생성·검증·안전한 압축 해제
crates/driver-*          postgres · neo4j · mongodb · redis · sqlite (+ example-kv 최소 예제). 각각 axdb-driver-<id> 실행 파일을 만든다
xtask                    cargo xtask package-drivers(빌드·패키지·서명), gen-signing-key
src-tauri                호스트: 프로필·키체인, plugin_host(드라이버 프로세스), 드라이버 설치, 실행·취소·타임아웃, 미리보기, 권한, MCP 브리지, AI, 내보내기, 업데이터
ui                       Preact UI · CodeMirror 6 · 가상 표 · Canvas 그래프(d3-force Worker) · 온보딩 · 드라이버 화면
scripts                  release.sh(앱 릴리스), publish-drivers.sh(드라이버 업로드), release-manifest.py
bench                    측정·E2E 스크립트와 원자료
docs                     ARCHITECTURE · PLUGIN_GUIDE · AGENT · DESIGN · VERIFICATION · RELEASE_SIGNING · ADR · 작업 보고서
```

문서 목록은 [docs/README.md](docs/README.md)에 있습니다.

## 라이선스와 고지

AxDB는 MIT 라이선스입니다. 사용한 외부 자료는 다음과 같습니다.

- **디자인 토큰 체계:** 외부 디자인 규약의 토큰 이름과 의미 규칙을 가져와 고쳐 썼습니다(Apache-2.0). 출처와 라이선스 전문은 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)에 있습니다.
- **React Bits:** 인터랙션을 이식했습니다(MIT + Commons Clause).
- **neo4rs:** vendor로 포함하고 패치 1건을 적용했습니다(MIT).
- **글꼴:** Pretendard와 JetBrains Mono(OFL)를 씁니다.
- **아이콘:** Lucide(ISC)를 씁니다.

전체 고지는 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)에 있습니다.
