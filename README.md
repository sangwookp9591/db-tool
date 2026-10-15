# AxDB — AX DB Tool

AxDB는 여러 종류의 DB를 한 곳에서 다루는 가벼운 데스크톱 DB 툴이다. 1차 범위는 **PostgreSQL**과 **Neo4j**이고, **SQLite**는 세 번째 DB로 추가해 확장 구조를 검증했다. DB마다 연결, 구조 조회, 실행, 취소, 값 변환을 맡는 드라이버가 따로 있고, 공통 UI는 드라이버가 선언한 기능(capabilities)과 결과에 실제로 담긴 뷰(표/그래프/문서)만 보고 화면을 구성한다.

- 스택: Tauri 2 + Rust(tokio) 백엔드, Preact + CodeMirror 6 UI, 가상 테이블, Canvas 그래프(d3-force, Web Worker)
- 결정 기록: [ADR-0001 독립 구현 선택](docs/adr/0001-independent-implementation.md), [ADR-0002 에이전트 인터페이스(WebMCP)](docs/adr/0002-agent-interface-webmcp.md)
- 계약: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) · 새 DB 추가: [docs/PLUGIN_GUIDE.md](docs/PLUGIN_GUIDE.md) · 에이전트/MCP: [docs/AGENT.md](docs/AGENT.md)

## 구성

```
crates/dbcore            드라이버 계약(Driver/Session, Manifest·Capabilities, typed Value, 다형 ResultSet, Registry)
crates/driver-postgres   tokio-postgres. 서버 측 커서로 행 한도, 텍스트 프로토콜로 정확한 값, 서버 측 취소
crates/driver-neo4j      neo4rs(vendor/neo4rs, 패치 1건). 표와 그래프 결과, 경로, temporal/point 타입, READ 모드
crates/driver-sqlite     rusqlite(bundled). interrupt 기반 취소, query_only 읽기 전용
crates/driver-example-kv 플러그인 가이드용 최소 예제(앱에는 등록하지 않음)
src-tauri                호스트: 프로필과 OS 키체인, 세션·실행·취소·타임아웃, 이력, 로그 redaction, MCP 브리지
ui                       공통 UI(연결/탐색/탭/에디터/결과/이력/오류), 에이전트 도구 레지스트리(WebMCP)
bench                    측정·E2E 스크립트와 원자료
```

## 빌드와 실행

필요한 도구: Rust 1.94 이상, Node 22 이상, pnpm, macOS(검증한 플랫폼). Linux와 Windows는 빌드 대상이지만 검증하지 않았다.

```sh
pnpm --dir ui install
pnpm --dir ui build                      # ui/dist 생성 (Tauri가 임베드)
cargo build -p axdb --release            # target/release/axdb
./target/release/axdb

# 개발 모드 (UI hot reload)
pnpm --dir ui dev &                      # http://127.0.0.1:5173 (브라우저 단독 실행 시 mock 백엔드)
cargo run -p axdb

# .app 번들
pnpm dlx @tauri-apps/cli@2 build --config src-tauri/tauri.conf.json
```

자주 쓰는 환경 변수는 다음과 같다.

| 변수 | 용도 |
|---|---|
| `AXDB_CONFIG_DIR` | 설정, 이력, 로그 디렉터리를 바꾼다. 테스트를 격리할 때 쓴다 |
| `AXDB_AGENT_BRIDGE=1` | localhost MCP 브리지를 켠다(기본은 꺼짐). 상세는 [AGENT.md](docs/AGENT.md) |
| `AXDB_TEST_PG` | 예: `host=127.0.0.1 port=55432 user=postgres password=testpw dbname=dbtool_test` |
| `AXDB_TEST_NEO4J` | 예: `bolt://127.0.0.1:57687\|neo4j\|testpassword` |

## 테스트

```sh
# 테스트 DB (사용자 DB와 분리)
docker run -d --name dbtool-test-pg -e POSTGRES_PASSWORD=testpw -e POSTGRES_DB=dbtool_test -p 55432:5432 postgres:16-alpine
docker run -d --name dbtool-test-neo4j -e NEO4J_AUTH=neo4j/testpassword -p 57474:7474 -p 57687:7687 neo4j:5-community

AXDB_TEST_PG='host=127.0.0.1 port=55432 user=postgres password=testpw dbname=dbtool_test' \
AXDB_TEST_NEO4J='bolt://127.0.0.1:57687|neo4j|testpassword' cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --dir ui test && pnpm --dir ui exec tsc --noEmit

# 실제 앱 E2E (MCP 브리지 경유, 스크립트 상단 주석의 프로필·fixture 준비 필요)
AXDB_CONFIG_DIR=<dir> AXDB_AGENT_BRIDGE=1 ./target/debug/axdb &
AXDB_CONFIG_DIR=<dir> node bench/e2e-bridge.mjs
```

## 보안 원칙

- **비밀번호:** 프로필 파일(`profiles.json`, 권한 0600)에는 secret 필드를 저장하지 않는다. 비밀번호는 OS 키체인(`keyring`, service `axdb`)에만 둔다.
- **로그:** 연결 필드는 이름만 기록하고 값은 남기지 않는다. 의존 라이브러리의 NOTICE와 쿼리 파라미터 값은 파일 로그에서 제외한다.
- **쓰기 작업:** 앱은 DB에 자동으로 쓰지 않는다. 에이전트가 실행하는 쿼리는 기본적으로 DB 수준 읽기 전용이다(PG `BEGIN READ ONLY`, Neo4j READ 모드, SQLite `query_only`). 쓰기는 앱 안의 승인 창에서 사용자가 승인해야 한다.

## 검증 결과와 성능

검증 결과, 성능 측정, 남은 한계는 [docs/VERIFICATION.md](docs/VERIFICATION.md)에 정리했다.

## 라이선스와 외부 코드

AxDB 자체는 MIT 라이선스다. 외부 코드 중 재사용한 것은 neo4rs(MIT, vendor, 패치 1건)뿐이다. Tabularis(Apache-2.0)와 Neo4j Browser(GPL-3.0)는 동작만 참고했고 코드는 가져오지 않았다. 상세는 [ADR-0001 재사용/참고 기록](docs/adr/0001-independent-implementation.md)과 [vendor/neo4rs/PATCHES.md](vendor/neo4rs/PATCHES.md)에 있다.
