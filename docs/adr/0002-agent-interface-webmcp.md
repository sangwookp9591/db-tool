# ADR-0002: 에이전트 조작 인터페이스 — 앱 내부 WebMCP + 선택적 localhost MCP 브리지

- 상태: 채택 (2026-10-15, 사용자 요청: "Agent가 동작하기 좋게, WebMCP 개념 도입")

## 배경

- WebMCP는 W3C Web Machine Learning CG의 Draft Community Group Report다(2026-10-15판, 표준 트랙 아님). 페이지가 `document.modelContext.registerTool({ name, title, description, inputSchema, execute(input, {signal}), annotations: { readOnlyHint, consequentialHint, untrustedContentHint } }, { signal })`로 도구를 선언하고, 에이전트가 이 도구를 호출한다. 등록 해제는 `signal`을 abort해서 한다. 이전 초안의 `navigator.modelContext`를 구현한 브라우저도 있다.
- 우리 앱은 Tauri의 WKWebView 안에서 돈다. 외부 에이전트가 이 웹뷰의 `modelContext`에 직접 닿을 수단은 없다(사실).
- 제품 제약: 앱 안에 AI 기능(LLM 호출)을 넣지 않는다. 사용자 데이터가 있는 DB에 자동으로 쓰지 않는다.

## 결정

1. **도구 레지스트리는 하나만 둔다.** 위치는 `ui/src/agent/`이다. 도구는 manifest의 capabilities와 현재 열린 세션에서 **파생**된다. DB id로 분기하지 않는다.
   - 항상 있는 도구: `list_drivers`, `list_connections`, `open_connection`, `close_connection`, `explore`, `run_query`, `get_history`, `get_ui_state`, `open_query_tab`
   - 조건부 도구: `paged_browse`를 지원하는 세션이 있을 때 `browse`, `graph_expand`를 지원하는 세션이 있을 때 `expand_node`. 세션이 바뀔 때마다 다시 등록한다.
2. **노출 경로는 세 가지다.**
   - (a) `document.modelContext`, 없으면 `navigator.modelContext`. WebMCP를 지원하는 호스트에서 쓰인다.
   - (b) `window.dbtool.agent = { listTools(), callTool(name, input) }`. 자동화와 테스트에 쓴다.
   - (c) **localhost MCP 브리지.** 기본값은 꺼짐이다.
     - 켜는 방법: 설정 토글 또는 `DBTOOL_AGENT_BRIDGE=1`
     - 주소: `127.0.0.1` 임의 포트의 MCP Streamable HTTP(JSON-RPC: `initialize`, `tools/list`, `tools/call`)
     - 인증: 무작위 bearer 토큰. 접속 정보는 `{config_dir}/agent-bridge.json`(권한 0600)에 둔다
     - 동작: 호스트는 요청을 웹뷰의 레지스트리로 전달하고 결과를 그대로 돌려준다. 도구 정의는 UI 한 곳에만 있다.
3. **안전장치**
   - `run_query`의 기본값은 `read_only: true`다. capability `read_only_exec`를 선언한 드라이버는 DB 수준에서 읽기 전용을 강제한다(PG `BEGIN READ ONLY`, Neo4j READ 접근 모드, SQLite `PRAGMA query_only`). 이 capability를 선언하지 않은 드라이버는 에이전트의 실행 요청마다 사용자 승인을 받는다.
   - `read_only: false`인 요청은 앱 안의 확인 창에 쿼리 원문, 연결 이름, 요청 출처(에이전트)를 보여 준다. 사용자가 승인해야만 실행한다. 거부하면 `UserDenied` 오류를 돌려준다.
   - 에이전트에게 돌려주는 결과는 행 200개, 그래프 노드 200개로 제한하고 `truncated`로 표시한다. 값은 타입 태그가 붙은 정확한 표현 그대로다.
   - DB 데이터가 담긴 결과를 내는 도구는 `untrustedContentHint: true`로 표시한다(프롬프트 인젝션 대비). 비밀번호 같은 secret은 어떤 도구도 반환하지 않는다.
   - 에이전트 호출은 실행 이력에 `source: "agent"`로 남는다. UI 상태줄에 "에이전트 활동" 표시를 띄운다.
4. **사람용 UI도 에이전트가 읽기 쉽게 만든다.** 주요 컨트롤에 `aria-label`과 `data-testid`를 단다. computer-use 계열 에이전트와 Playwright가 쓴다.

## 결과

- WebMCP 초안이 바뀌어도 수정은 레지스트리의 노출 어댑터 한 파일로 끝난다.
- 새 DB 플러그인을 추가해도 에이전트 도구는 자동으로 따라온다. 확장성 검증 항목 6과 같은 원리다.
- 이 기능은 앱 안에 LLM을 넣는 "AI 기능"이 아니다. 자동화 인터페이스이며, 브리지는 기본으로 꺼져 있다.
