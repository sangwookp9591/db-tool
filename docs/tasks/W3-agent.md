# W3-agent: 에이전트 조작 인터페이스 (WebMCP 레지스트리 + 읽기 전용 실행 + localhost MCP 브리지)

이 작업은 반드시 끝까지 완료한다. 합리적인 가정으로 계속 진행하고, 결정한 내용은 마지막 보고에 적는다. 사용자 지시가 skill 지침보다 우선한다.

## Target
- 저장소: `/Users/iron/Project/psw/db-tool`
- **먼저 읽을 문서:** `docs/adr/0002-agent-interface-webmcp.md`(이 작업의 명세), `docs/ARCHITECTURE.md`, `docs/W1-backend-report.md`(있으면)
- 이미 구현된 것: `crates/dbcore`, `crates/driver-postgres`, `src-tauri`, `ui/`
- 동시 작업: 다른 워커가 `crates/driver-neo4j`를 만들고 있다.

## Change
1. **dbcore (가산적 변경, 가장 먼저):**
   - `ExecRequest`에 `read_only: bool`을 추가한다(`#[serde(default)]`, 기본 false).
   - `Capabilities`에 `read_only_exec: bool`을 추가한다(`#[serde(default)]`).
   - 바꾼 직후 `docs/ARCHITECTURE.md` 계약 블록에도 반영한다. Neo4j 워커가 이 변경을 보고 Neo4j 드라이버에 READ 접근 모드를 구현한다.
2. **driver-postgres:**
   - `read_only_exec: true`를 선언한다.
   - `read_only=true`이면 각 실행을 `BEGIN READ ONLY` 트랜잭션 안에서 하고 끝나면 ROLLBACK한다. 사용자가 연 트랜잭션 안이라면 `SET TRANSACTION READ ONLY`를 쓸 수 없는 경우가 있으므로 거부(Unsupported)하고 사유를 남긴다.
   - 통합 테스트: 테스트 DB(55432)에서 read_only로 `CREATE TABLE`/`INSERT`가 거부되는지, SELECT는 되는지 확인한다.
3. **ui/src/agent/** (도구 레지스트리, 단일 원천):
   - `registry.ts`:
     - ADR의 도구 목록을 JSON Schema inputSchema, title, description(영문, 에이전트용으로 정확하게), annotations와 함께 정의한다.
     - 도구는 manifest capabilities와 열린 세션에서 **파생**한다. `browse`와 `expand_node`는 조건부로 등록하고 세션이 바뀌면 다시 등록한다.
     - **드라이버 id 문자열로 분기하지 않는다.**
     - `run_query`:
       - 입력: `session_id`, `text`, `params?`, `row_limit?`(≤1000), `timeout_ms?`, `read_only`(기본 true)
       - 세션 드라이버가 `read_only_exec`를 지원하지 않거나 `read_only=false`이면 앱 안의 확인 모달을 띄운다. 모달에는 쿼리 원문, 연결 이름, "에이전트 요청" 표시, 승인/거부 버튼을 둔다. 승인한 경우에만 실행하고, 거부하면 `{error:{kind:"UserDenied"}}`를 돌려준다.
       - 결과 압축: 행 200개, 노드 200개로 제한하고 truncated를 표시한다. 값은 typed Value 그대로 둔다.
       - 실행 이력에 source를 남긴다. 호스트의 history 엔트리에 `source: "ui"|"agent"` 필드를 추가하는 것도 이 워커가 한다.
     - `get_ui_state`: 열린 연결, 활성 탭(연결, 에디터 텍스트), 마지막 결과 요약을 돌려준다.
     - `open_query_tab`: 사용자가 볼 수 있게 탭을 연다.
     - 데이터를 반환하는 도구에는 `untrustedContentHint: true`를 붙인다.
   - `webmcp.ts`:
     - `document.modelContext ?? navigator.modelContext`가 있으면 각 도구를 `registerTool(tool, {signal})`로 등록한다. 재등록할 때는 이전 AbortController를 abort한다.
     - 항상 `window.dbtool.agent = { listTools, callTool }`을 노출한다.
     - `callTool`은 입력을 JSON Schema로 최소 검증한다. 필수 필드와 타입을 직접 검사하고 ajv는 쓰지 않는다. 결과는 JSON 직렬화할 수 있는 객체여야 한다.
   - 상태줄에 "에이전트 활동" 표시를 둔다. 최근 에이전트 호출 수를 보여 주고, 실행 중이면 펄스한다.
   - 주요 컨트롤에 `aria-label`과 `data-testid`를 단다: 연결 추가/테스트/연결, 트리 항목, 실행, 취소, 결과 뷰 전환, 페이지 이동, 승인 모달.
4. **src-tauri: localhost MCP 브리지** (기본 꺼짐):
   - 켜는 조건은 `DBTOOL_AGENT_BRIDGE=1` 또는 UI 설정 토글(`agent_bridge_set(enabled)` command)이다. 설정은 `settings.json`에 저장한다.
   - `127.0.0.1:0`(임의 포트)에 HTTP 서버를 연다. 크레이트는 의존성이 가장 작은 쪽(axum 또는 hyper 직접)을 쓴다.
   - **MCP Streamable HTTP 최소 구현:**
     - `POST /mcp`에서 JSON-RPC 2.0을 처리한다: `initialize`, `notifications/initialized`, `tools/list`, `tools/call`, `ping`.
     - 응답은 `application/json` 단일 응답이면 충분하다. SSE는 필수가 아니다.
     - `Authorization: Bearer <token>`이 없거나 틀리면 401을 돌려준다.
     - `Origin` 헤더가 있으면 거부해 브라우저발 요청을 막는다. 이는 DNS rebinding 방지 목적이다.
   - **연결 정보:** `{config_dir}/agent-bridge.json`(권한 0600)에 `{url, token}`을 기록하고 앱이 종료되면 삭제한다.
   - **요청 전달:**
     - `tools/list`와 `tools/call`은 Tauri 이벤트로 웹뷰에 보낸다. 웹뷰는 `window.dbtool.agent`로 처리하고 `agent_bridge_reply(request_id, result)` command로 응답한다.
     - 타임아웃은 run_query의 timeout_ms에 여유를 더하거나 기본 60초로 한다.
     - 승인 모달을 기다리는 동안에도 타임아웃이 적용된다. 시간이 지나면 거부로 처리한다.
   - **로그:** 브리지 로그에 토큰과 쿼리 파라미터 값을 남기지 않는다.
   - **브리지 시험용 Node 스크립트 `bench/agent-bridge-smoke.mjs`:**
     - `agent-bridge.json`을 읽어 initialize → tools/list → 테스트 PG 프로필 open_connection → explore → run_query(read_only) 순서로 호출한다.
     - 쓰기 시도가 승인 대기 중 타임아웃으로 거부되는지 확인한다.
5. **문서:** `docs/AGENT.md`를 작성한다. 내용: 도구 목록, 스키마 요약, 브리지를 켜는 방법, Claude Code나 다른 MCP 클라이언트 연결 예시(`{"type":"http","url":...,"headers":{"Authorization":"Bearer ..."}}`), 안전 모델.

## Constraints
- `crates/driver-neo4j`와 `src-tauri/src/drivers.rs`는 수정하지 않는다. 다른 워커의 영역이다.
- 사용자 데이터가 있는 DB(5432, 5439, 32802)에는 접속하지 않는다. 테스트 DB만 쓴다: PG `127.0.0.1:55432`(testpw).
- 앱 안에서 LLM을 호출하는 기능은 넣지 않는다.
- UI에서 드라이버 id로 분기하지 않는다. 다음 grep 결과가 비어 있어야 한다: `grep -rnE "'(postgres|neo4j|sqlite|mysql|mongodb|redis)'" ui/src --include=*.ts --include=*.tsx | grep -v mock`
- git commit을 하지 않는다.

## Observable acceptance
- `cargo test --workspace` 통과(`DBTOOL_TEST_PG` 설정), `cargo clippy --workspace -- -D warnings` 통과
- `pnpm --dir ui build`, `tsc --noEmit`, vitest 통과. 레지스트리 단위 테스트는 다음을 다룬다:
  - capability에 따른 조건부 도구 등록
  - read_only 기본값
  - 승인 거부 경로
  - 결과 압축
- **목 백엔드(브라우저 dev):**
  - `window.dbtool.agent.listTools()`와 `callTool('run_query', …)`이 동작한다.
  - `document.modelContext`를 흉내 내는 테스트 shim에 registerTool이 호출되는지 vitest로 확인한다.
- **실제 앱:** `DBTOOL_AGENT_BRIDGE=1 DBTOOL_CONFIG_DIR=<임시 디렉터리>`로 debug 앱을 띄우고 `node bench/agent-bridge-smoke.mjs`가 통과해야 한다. 테스트 PG 프로필은 스크립트나 UI로 미리 만든다.
- 보고: 결정 사항, WebMCP 초안과 다른 점, 남은 한계
