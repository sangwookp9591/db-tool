<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/banner-dark.png">
    <img src="docs/assets/banner-light.png" alt="AxDB — AX DB Tool" width="900">
  </picture>
</p>

<p align="center">
  <b>여러 데이터베이스를 한 화면에서 다루는 가볍고 빠른 데스크톱 DB 툴</b><br>
  관계형 · 그래프 · 문서 · 키-값을 각자의 모양 그대로 보여 주고, AI 에이전트가 권한 안에서 함께 일합니다.
</p>

<p align="center">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-1.94-B7410E?logo=rust&logoColor=white">
  <img alt="Preact" src="https://img.shields.io/badge/Preact-11-673AB8?logo=preact&logoColor=white">
  <img alt="MCP" src="https://img.shields.io/badge/MCP-scoped%20tokens-8B7BFF">
  <img alt="Tests" src="https://img.shields.io/badge/tests-100%20Rust%20%C2%B7%2090%20UI%20%C2%B7%2039%20E2E-34D399">
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
| **가볍고 빠름** | 10만 행 쿼리에서 Tabularis 0.28.0은 첫 행이 549ms, 메모리가 974MiB였고 AxDB는 332ms, 255MiB였습니다. 앱 번들은 Tabularis의 절반 이하이고, 유휴 CPU는 0.1% 미만입니다. |
| **안전한 쓰기** | 자동 쓰기는 하지 않습니다. 변경은 실행한 뒤 반드시 롤백하는 **미리보기**로 먼저 확인합니다. 표에서 셀을 고치거나 행을 추가·삭제해도 **편집 → 미리보기 → 적용**을 거치며, 적용은 한 트랜잭션입니다. 연결 단위 **읽기 전용**은 호스트가 강제합니다. |
| **에이전트와 함께** | MCP로 접속하는 에이전트마다 연결별 권한(none / read / preview / write-with-approval), 해시로 저장한 토큰, 감사 로그를 둡니다. |
| **스키마를 아는 AI** | 스키마와 작업 맥락을 읽고 쿼리를 제안합니다. 제안은 실행하지 않고 새 탭에 넣기만 합니다. 앱 안에서 Claude 로그인으로 쓸 수 있습니다. |

## 화면

<table>
  <tr>
    <td width="50%"><img src="docs/assets/screens/neo4j-graph-midnight.png" alt="Neo4j 그래프"><br><sub><b>그래프</b> · Cypher 결과를 그래프와 표로 오가며 이웃을 확장하고, 라벨별 범례를 봅니다</sub></td>
    <td width="50%"><img src="docs/assets/screens/mongo-documents-midnight.png" alt="MongoDB 문서 트리"><br><sub><b>문서</b> · 가상화한 JSON 트리에 BSON 고유 타입을 그대로 보여 줍니다</sub></td>
  </tr>
  <tr>
    <td><img src="docs/assets/screens/erd-daylight.png" alt="ERD"><br><sub><b>ERD</b> · PG 외래 키, SQLite, Neo4j 스키마, Mongo 참조 추론을 같은 다이어그램으로 그립니다</sub></td>
    <td><img src="docs/assets/screens/agent-tools-daylight.png" alt="에이전트 클라이언트"><br><sub><b>에이전트</b> · 실제 쿼리나 MCP 호출이 있을 때만 빔에 펄스가 흐르고, 도구 호출 기록을 남깁니다</sub></td>
  </tr>
  <tr>
    <td><img src="docs/assets/screens/ai-midnight.png" alt="AI 도우미"><br><sub><b>AI 도우미</b> · 확인한 근거(구문 → 스키마 → EXPLAIN)를 신호로 보여 주며 대안 쿼리를 제안합니다</sub></td>
    <td><img src="docs/assets/screens/connection-midnight.png" alt="연결 추가"><br><sub><b>연결 추가</b> · 드라이버가 선언한 필드로 폼을 그리고, 비밀번호는 OS 키체인에만 저장합니다</sub></td>
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

새 DB는 드라이버 crate 하나를 만들고 몇 줄로 등록하면 붙습니다. 공통 UI는 드라이버가 선언한 기능(capabilities)만 보고 화면을 그립니다. 그래서 MongoDB와 Redis를 추가할 때 `ui/`와 `crates/dbcore`는 한 줄도 바꾸지 않았습니다. 자세한 방법은 [플러그인 가이드](docs/PLUGIN_GUIDE.md)에 있습니다.

## 디자인

- **테마:** Midnight(다크), Daylight(라이트), 시스템 설정 따르기 세 가지입니다.
- **글꼴:** UI는 Pretendard, 코드와 결과 값은 JetBrains Mono를 씁니다. 둘 다 앱에 내장해 오프라인에서도 그대로 보입니다.
- **마이크로 인터랙션 22종:**
  - React Bits에서 가져온 12종: 결과 뷰 전환(JellyRadio), 이력(SwipeRow), 타임아웃(CometDial), 행 제한(WakeSlider), 숫자 표시(CodeSlots), 실행 중 표시(LatticeLoader), 테두리 빛(BorderGlow), 체크박스(SpringCheck), 5초 안에 되돌릴 수 있는 삭제(FuseButton), 에이전트 도구 호출 칩(CallChip), 모든 선택 상자(GlideSelect), 사이드바 탐색 트리(BranchedMenu)
  - 그 밖의 10종: ⌘K 명령 팔레트, 토스트, 툴팁, 스포트라이트 카드, Integration Beam, 제안 카드, 도구 호출 카드 등
- **모션 원칙:** 입력이 있을 때만 움직이고, 가만히 두면 애니메이션은 0입니다. `prefers-reduced-motion`을 따르고, 결과 그리드와 그래프 캔버스 안에는 인터랙션을 넣지 않습니다.

- **문구:** UI는 한국어입니다. Host·Port·TLS·SQL 같은 기술 용어와 DB가 돌려준 원본 오류는 그대로 둡니다.
- **창 크기:** 최소 960×600입니다. 이 크기에서도 사이드바·탭·편집기·결과·상태바가 모두 보이도록 맞췄습니다.

세부 규칙은 [디자인 시스템](docs/DESIGN.md)에 있고, 실제 앱 UI 감사 기록은 [최종 감사](docs/FINAL_UI_AUDIT_3.md)에 있습니다.

## 시작하기

필요한 도구는 Rust 1.94 이상, Node 22 이상, pnpm입니다. macOS와 Linux에서 검증했습니다.

```sh
pnpm --dir ui install
pnpm --dir ui build                         # ui/dist 생성 (Tauri가 임베드)
cargo run -p axdb --release

# .app / 설치 번들
pnpm dlx @tauri-apps/cli@2 build --config src-tauri/tauri.conf.json

# UI만 브라우저에서 (mock 백엔드)
pnpm --dir ui dev                           # http://127.0.0.1:5173
```

| 환경 변수 | 용도 |
|---|---|
| `AXDB_CONFIG_DIR` | 설정·이력·로그 위치를 바꿉니다(테스트 격리용) |
| `AXDB_AGENT_BRIDGE=1` | localhost MCP 브리지를 켭니다(기본은 꺼짐). 자세한 내용은 [AGENT.md](docs/AGENT.md) |

**AI 도우미**는 기본으로 꺼져 있습니다. 설정에서 인증 방식을 고르면 켜지는데, 두 가지 중 하나를 쓸 수 있습니다.
- **Claude 로그인(ant CLI):** 패널의 "Claude 로그인" 버튼을 누르면 앱 안에서 로그인이 끝납니다.
- **API 키:** OS 키체인에 저장합니다.

## 성능

같은 머신에서 Tabularis 0.28.0과 행 한도를 같게 맞추고 비교했습니다. 조건마다 두 앱을 번갈아 5쌍씩 측정했고, 1분 평균 load가 8 이하인 표본만 썼습니다.

| 조건 | 첫 행 (Tabularis → AxDB) | 메모리 합계 | CPU |
|---|---|---|---|
| 생성 쿼리 10만 행 | 549 → **332ms** | 974 → **255MiB** | 217 → **73%** |
| big 테이블 10만 행 | 403 → **199ms** | 957 → **243MiB** | 205 → **74%** |
| 60초 유휴 | — | footprint 140 → **81MiB** | 둘 다 0.1% 미만 |

- **메모리 반납 문제 해결:** 같은 쿼리를 50번 반복하면 macOS 기본 할당자가 해제한 메모리를 OS에 돌려주지 않는 문제를 찾았습니다. 해제 즉시 반납하는 mimalloc으로 바꿔 host 메모리를 696MiB에서 63MiB로 줄였습니다.
- **번들 크기:** Tabularis는 32.1MiB, AxDB는 디자인 개편 전 11.9MiB였습니다. 이번 개편으로 내장 글꼴 2.9MiB가 늘었습니다.
- **Tabularis에서 "Choose a database"가 느린 이유:** 이 화면을 열 때마다 원격 플러그인 목록과 상세 정보 20건을 받아 와서 3~5초가 걸립니다. AxDB는 이 단계에서 원격 호출을 하지 않습니다.

원자료와 측정 방법은 [VERIFICATION](docs/VERIFICATION.md), [COMPARE](bench/COMPARE.md), [RESOURCE_OPT](docs/RESOURCE_OPT.md)에 있습니다.

## 보안

- 비밀번호와 토큰은 OS 키체인에만 저장합니다. 프로필 파일(권한 0600)과 로그에는 남기지 않습니다.
- 앱은 자동으로 쓰지 않습니다. 에이전트가 실행하는 쿼리는 기본적으로 DB 수준에서 읽기 전용이고, 쓰기는 앱 안에서 사용자가 승인해야 합니다.
- TLS는 실제 서버로 검증했습니다. PG는 `sslmode`(require / verify-ca / verify-full)와 CA 파일 지정을, Neo4j는 `+s`와 `+ssc`를, Mongo는 `tls`와 `tlsCAFile`을 확인했습니다.

## 구조

```
crates/dbcore            드라이버 계약: Driver·Session, Manifest·Capabilities, typed Value, 다형 ResultSet
crates/driver-*          postgres · neo4j · mongodb · redis · sqlite (+ example-kv 최소 예제)
src-tauri                호스트: 프로필·키체인, 실행·취소·타임아웃, 미리보기, 권한, MCP 브리지, AI
ui                       Preact UI · CodeMirror 6 · 가상 표 · Canvas 그래프(d3-force Worker) · ui/ 마이크로 컴포넌트
bench                    측정·E2E 스크립트와 원자료
docs                     ARCHITECTURE · PLUGIN_GUIDE · AGENT · DESIGN · VERIFICATION · ADR
```

## 테스트

```sh
# 사용자 DB와 분리한 테스트 DB
docker run -d --name axdb-test-pg -e POSTGRES_PASSWORD=testpw -e POSTGRES_DB=axdb_test -p 127.0.0.1:55432:5432 postgres:16-alpine
docker run -d --name axdb-test-neo4j -e NEO4J_AUTH=neo4j/testpassword -p 127.0.0.1:57687:7687 neo4j:5-community

AXDB_TEST_PG='host=127.0.0.1 port=55432 user=postgres password=testpw dbname=axdb_test' \
AXDB_TEST_NEO4J='bolt://127.0.0.1:57687|neo4j|testpassword' \
cargo test --workspace                       # Mongo·Redis·TLS 테스트는 AXDB_TEST_MONGO, _REDIS, _PG_TLS 등을 추가로 지정
cargo clippy --workspace --all-targets -- -D warnings
pnpm --dir ui test && pnpm --dir ui exec tsc --noEmit

# 실제 앱 E2E (MCP 브리지 경유)
node bench/e2e-bridge.mjs && node bench/preview-e2e.mjs && node bench/permissions-e2e.mjs
```

## 라이선스와 고지

AxDB는 MIT 라이선스입니다. 가져와 쓴 외부 자료는 다음과 같습니다.

- **Tabularis:** 디자인 토큰 체계를 가져와 고쳐 썼습니다(Apache-2.0).
- **React Bits:** 인터랙션을 이식했습니다(MIT + Commons Clause).
- **neo4rs:** vendor로 포함하고 패치 1건을 적용했습니다(MIT).
- **글꼴:** Pretendard와 JetBrains Mono(OFL)를 씁니다.
- **아이콘:** Lucide(ISC)를 씁니다.

전체 고지는 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)에 있습니다.
