# W5 SQLite 검증 보고

작업 시작 HEAD: `384e6b65f61881e87303867289abe2d889382f98`.
W3-agent와 W4-viewer-perf가 공유 작업트리에서 동시에 작업했다.

## 구현 범위

- `crates/driver-sqlite`: bundled rusqlite 0.40.2, FilePath/Bool 연결,
  relational SQL, 실제 interrupt와 timeout, 세션별 blocking 실행, 명시적 트랜잭션,
  multi-statement, 읽기 전용 실행과 원복, 탐색과 paging, 손실 없는 값 매핑.
- `crates/driver-example-kv`: 메모리 GET/SET/KEYS 예제와 테스트. 앱에 등록하지 않음.
- `docs/PLUGIN_GUIDE.md`: 계약, 선언 기준, 타입 정확성, 탐색, 실행 한도와 오류,
  테스트, 등록 및 외부 JSON-RPC 경계로 옮기는 방법.
- Workspace members와 lockfile, 앱 dependency 한 줄과 registry 한 줄.
- `bench/w5-sqlite-smoke.mjs`: 실제 debug 앱의 localhost MCP 브리지를 통한 검증.

W5는 `ui/`, `crates/dbcore`, PostgreSQL/Neo4j 드라이버와 `src-tauri/src/lib.rs`를
수정하지 않았다. 공유 파일 `src-tauri/Cargo.toml`과 `src-tauri/src/drivers.rs`는
직전에 다시 읽고 SQLite 한 줄씩만 추가했다. 다른 워커의 기존 줄은 보존했다.

## 검증

| 검증 | 상태 | 근거 |
|---|---|---|
| SQLite 실제 파일 통합 테스트 | PASS, 7개 | `/private/tmp/dbtool-w5-workspace-verified.log` |
| KV 예제 테스트 | PASS, 1개 | 같은 workspace 로그 |
| UI build (생성물만 갱신) | PASS | `/private/tmp/dbtool-w5-ui-build.log` |
| workspace test, PG/Neo4j 실제 통합 포함 | PASS, 전체 26개 | `/private/tmp/dbtool-w5-workspace-verified.log` |
| workspace Clippy, `-D warnings` | PASS | `/private/tmp/dbtool-w5-clippy-final.log` |
| debug 앱 빌드 + 실제 앱/MCP smoke | PASS, 12개 점검 | `bench/results-w5-sqlite-app.json`, `/private/tmp/dbtool-w5-app-final.log` |

SQLite 검증은 NULL/BLOB/JSON/int64/DATE/DATETIME/TIMESTAMP/무한대 실수,
선언 타입과 storage class, row_limit, 문법 오류, 없는 read-only 경로,
특수 문자 식별자, Tables/Views/PK/인덱스 정의, 첫/마지막 browse 페이지,
300ms 무한 CTE timeout과 cancel, 호출자 future 중단, 세션 대기 timeout,
미리 취소된 토큰, 같은 세션 복구, read-only 우회 거부와 원복,
trigger 내부 세미콜론, 동일 이름 trigger/index 구분, 세션 drop 후 미완료 쓰기 잠금
해제를 포함한다.

검증 명령은 모두 종료 코드 0이었다. workspace 테스트 실행 시 `DBTOOL_TEST_PG`와
`DBTOOL_TEST_NEO4J`를 코디네이터가 지정한 격리 loopback 테스트 서버로 설정했다.
실제 PostgreSQL/Neo4j 테스트가 `ok`로 실행되었으며 환경변수 미설정 skip 결과가 아니다.
명령은 `CARGO_TARGET_DIR=target-w5 cargo test --workspace`,
`CARGO_TARGET_DIR=target-w5 cargo clippy --workspace -- -D warnings`,
`CARGO_TARGET_DIR=target-w5 cargo build -p db-tool`, `pnpm --dir ui build`였다.

실제 앱은 `DBTOOL_CONFIG_DIR=/private/tmp/dbtool-w5-cfg DBTOOL_AGENT_BRIDGE=1
 target-w5/debug/db-tool`로 실행했다. 전용 `/private/tmp/dbtool-w5-smoke.db`에 테스트
스키마/데이터를 만들고 비공개 profiles.json을 준비했다. MCP open_connection → explore
→ open_query_tab → run_query → browse를 호출했다. get_ui_state에서 SQLite 연결명,
활성 편집기 SQL, 실제 표 결과 3행도 확인했다. GUI에서 프로필을 직접 입력하는 과정과
시각적 레이아웃 평가는 수행하지 않았다. 연결/쿼리/탐색/browse 동작은 브리지로 검증했다.

재실행: 위 전용 debug 앱을 켠 뒤
`DBTOOL_CONFIG_DIR=/private/tmp/dbtool-w5-cfg node bench/w5-sqlite-smoke.mjs`.
합성 데이터만 증거에 저장했고 bridge token은 저장하지 않았다.
검토를 위해 W5 앱과 탭을 열어 두었으며 다른 워커 앱은 종료하지 않았다.

구버전(0.32.1) metadata로 이미 시작했던 초기 빌드 2회는 소스의 0.40.2 전환과
겹쳐 API 불일치로 실패했다. 위 근거는 전환 완료 후 최종 0.40.2 및 마지막 수정으로
재실행한 성공 로그다. 초기 실패를 성공 증거로 사용하지 않았다.

## 결정과 한계

- SQL 파라미터는 요청 범위에 없어서 manifest에서 false이며, 전달 시 Unsupported다.
- KV KEYS는 정확한 키 또는 하나의 후행 `*`를 지원하며 그 외 wildcard는 거부한다.
- SQLite read-only 실행은 PRAGMA/ATTACH/DETACH/transaction/savepoint 제어를 거부한다.
  `SELECT`와 탐색 API로 조회한다. PRAGMA 기반 table-valued 함수도 거부될 수 있다. 읽기 전용이 임의 SQL sandbox라는
  뜻은 아니다. 로드 가능한 확장 기능은 켜지 않았다.
- BLOB은 공통 계약대로 전체 길이와 첫 64바이트 hex다. 결과 행 한도는 단일 셀 크기와
  SQLite 엔진 내부 작업 메모리까지 제한하지 않는다.
- browse는 OFFSET 방식이고 동시 변경 사이의 스냅샷/고정 정렬을 보장하지 않는다.
- 일반 multi-statement는 자동 트랜잭션으로 감싸지 않는다. 중간 오류 앞의 쓰기는
  SQLite의 원래 autocommit/사용자 BEGIN 의미를 따른다.
- commit/push를 하지 않았다. 배포/릴리스 빌드/사용자 DB 검증은 범위 밖이다.

## 변경 증명

W5 소유 변경만 기준으로 한 범위:

| 경로 | W5 변경 |
|---|---|
| ui/ | 0 |
| crates/dbcore | 0 |
| crates/driver-postgres, crates/driver-neo4j | 0 |
| src-tauri/src/lib.rs, storage.rs | 0 |
| src-tauri/src/drivers.rs | SQLite registry 등록 1줄 추가 |
| src-tauri/Cargo.toml | SQLite path 의존성 1줄 추가 |
| Cargo.toml, Cargo.lock | 두 workspace member와 해당 의존 그래프 |
| crates/driver-sqlite | Cargo.toml, src/lib.rs, tests/sqlite.rs 새 파일 |
| crates/driver-example-kv | Cargo.toml, src/lib.rs 새 파일 |
| docs | PLUGIN_GUIDE.md, 이 보고서 새 파일 |
| bench | w5-sqlite-smoke.mjs, results-w5-sqlite-app.json 새 파일 |

요청한 `git diff --stat <시작 HEAD>`의 실제 출력은 다음과 같다.
공유 작업트리이므로 W3/W4와 이미 존재하던 변경이 함께 나온다. 따라서 아래 전체 stat의
ui/dbcore/src-tauri 변경을 W5 변경으로 해석하면 안 된다. 미추적 새 파일도 이 stat에는
포함되지 않는다. worker_done의 정확한 files-modified 목록과 위 SQLite 두 줄로 W5
변경을 분리해서 검토/커밋해야 한다.

```text
 .gitignore                                   |    1 +
 Cargo.lock                                   |  111 ++-
 Cargo.toml                                   |    2 +-
 bench/VIEWER_PERF.md                         |  114 +++
 bench/agent-bridge-smoke.mjs                 |   44 +
 bench/e2e-bridge.mjs                         |  167 ++++
 bench/results-agent-bridge.json              |   12 +
 bench/results-agent-ui.json                  |   27 +
 bench/results-viewer-after.json              | 1157 ++++++++++++++++++++++++++
 bench/results-viewer-app-initial-failed.json |  271 ++++++
 bench/results-viewer-app.json                |  271 ++++++
 bench/results-viewer-baseline-layout.json    |  639 ++++++++++++++
 bench/results-viewer-baseline.json           | 1011 ++++++++++++++++++++++
 bench/results-viewer-graph-final.json        |  659 +++++++++++++++
 bench/results-viewer-graph.json              |  961 +++++++++++++++++++++
 bench/results-viewer-table.json              |  167 ++++
 bench/results-viewer-techniques.json         |   71 ++
 bench/results-viewer-worker16.json           | 1157 ++++++++++++++++++++++++++
 bench/results-viewer-workspace-baseline.json |   36 +
 bench/summarize-viewer.mjs                   |   44 +
 bench/verify-viewer.mjs                      |   46 +
 bench/viewer-bench-app.mjs                   |   20 +
 bench/viewer-bench.mjs                       |   17 +
 bench/viewer-verified.png                    |  Bin 0 -> 105271 bytes
 bench/viewer-workspace.mjs                   |   32 +
 crates/dbcore/src/lib.rs                     |    4 +
 crates/driver-postgres/examples/contract.rs  |    1 +
 crates/driver-postgres/src/lib.rs            |   66 +-
 crates/driver-postgres/tests/postgres.rs     |  133 +++
 docs/AGENT.md                                |   86 ++
 docs/ARCHITECTURE.md                         |    3 +-
 docs/PLUGIN_GUIDE.md                         |  132 +++
 docs/W3-agent-report.md                      |   51 ++
 docs/tasks/W5-sqlite.md                      |    2 +-
 docs/tasks/W6-rename-axdb.md                 |   36 +
 docs/tasks/W7-fixes.md                       |   41 +
 src-tauri/Cargo.toml                         |    3 +
 src-tauri/src/agent_bridge.rs                |  401 +++++++++
 src-tauri/src/drivers.rs                     |    2 +
 src-tauri/src/lib.rs                         |   85 +-
 src-tauri/src/storage.rs                     |    9 +
 src-tauri/src/stream.rs                      |  179 ++++
 src-tauri/src/viewer_bench.rs                |   30 +
 ui/package.json                              |    1 +
 ui/pnpm-lock.yaml                            |   19 +
 ui/src/App.tsx                               |   37 +-
 ui/src/Connections.tsx                       |    8 +-
 ui/src/GraphView.tsx                         |  184 ++--
 ui/src/QueryTab.tsx                          |   17 +-
 ui/src/Results.tsx                           |   23 +-
 ui/src/VirtualTable.tsx                      |   61 +-
 ui/src/agent/AgentPanel.tsx                  |   50 ++
 ui/src/agent/registry.test.ts                |   59 ++
 ui/src/agent/registry.ts                     |   92 ++
 ui/src/agent/webmcp.ts                       |   49 ++
 ui/src/api.ts                                |    4 +
 ui/src/contract.ts                           |    6 +-
 ui/src/graph.ts                              |   12 +
 ui/src/main.tsx                              |    2 +
 ui/src/mock.ts                               |   13 +-
 ui/src/perf/app.tsx                          |   89 ++
 ui/src/perf/columnar.ts                      |   17 +
 ui/src/perf/fixtureStream.ts                 |   14 +
 ui/src/perf/fixtures.ts                      |    8 +
 ui/src/perf/harness.tsx                      |   69 ++
 ui/src/perf/index.ts                         |   32 +
 ui/src/perf/spatial.ts                       |   23 +
 ui/src/perf/stream.ts                        |   44 +
 ui/src/perf/streamState.ts                   |   23 +
 ui/src/perf/tableIndex.ts                    |   31 +
 ui/src/perf/viewer.test.ts                   |   64 ++
 ui/src/style.css                             |    2 +
 ui/src/virtual.ts                            |    8 +
 ui/src/workers/fixture.worker.ts             |   13 +
 ui/src/workers/graph.worker.ts               |   26 +
 ui/viewer-bench.html                         |    1 +
 76 files changed, 9227 insertions(+), 175 deletions(-)
```

마지막 확인 시 코디네이터가 다른 lane 변경을 이미 커밋한 현재 HEAD는 `85dbcbb0242385a594e5e72acd7cd5fa6de621f1`였다.
이 HEAD에 대한 실제 보호 경로 diff는 다음과 같다.

```text
$ git diff --stat -- ui/ crates/dbcore crates/driver-postgres crates/driver-neo4j
 ui/src/GraphView.tsx           |  2 +-
 ui/src/agent/registry.ts       |  3 ++-
 ui/src/graph.ts                |  2 +-
 ui/src/perf/app.tsx            | 13 +++++++++----
 ui/src/perf/harness.tsx        | 13 +++++++------
 ui/src/workers/graph.worker.ts |  2 +-
 6 files changed, 21 insertions(+), 14 deletions(-)
$ git diff --stat -- src-tauri
 src-tauri/Cargo.toml          | 1 +
 src-tauri/src/drivers.rs      | 1 +
 src-tauri/src/lib.rs          | 1 +
 src-tauri/src/viewer_bench.rs | 9 +++++++++
 4 files changed, 12 insertions(+)
```

W5 debug 앱 PID: `61113` (실행 세션의 관측값; 이후 종료되면 유효하지 않음).
