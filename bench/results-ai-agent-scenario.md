# 외부 AI 에이전트 시나리오 — 코디네이터(Claude) 실측 (2026-10-15 20:0x KST)

- 실행 환경: 실제 debug 앱(AI1 통합 트리), MCP 브리지. 접근한 DB는 테스트 PG(`dbtool_test`)와 Neo4j(57687)뿐이다.
- MCP 클라이언트: `bench/mcp-call.mjs`
- 권한 설정: 범위 클라이언트 `ai-scenario`를 만들었다. PG는 `preview`, Neo4j는 `read`, 허용 도구는 9개, `max_rows`는 50이다.
- 에이전트 역할: 코디네이터(Claude)가 직접 맥락을 읽고 쿼리를 작성했다. 앱 안에서 LLM을 호출하지 않았다.

| 단계 | 결과 |
|---|---|
| `tools/list` | 허용한 도구만 보였다. `preview_changes`는 미리보기를 지원하는 세션이 열린 뒤에 조건부로 보였다 |
| `list_connections` | 허용한 2개 연결만 보였다 |
| `get_schema_context`(Neo4j, focus `DbtE_Person`) | ✅ 라벨, 관계 방향(KNOWS, ACTED_IN), 속성 타입이 들어 있었다. `selected_rows: null`, `schema_byte_limit` 6000 |
| `get_schema_context`(PG) | ❌→수정 중. 테이블이 `public` 밖(`e2e`)에 있으면 schema가 빈 문자열이었다(AI1b) |
| 맥락으로 작성한 Cypher를 `open_query_tab`으로 열기 | ✅ 에디터에만 들어갔다. 실행 이력에 변화가 없어 미실행이 확인됐다 |
| `get_work_context` | ✅ 활성 에디터 텍스트가 일치했다 |
| `run_query`(read) | ✅ `Bob → Carol → Matrix`. fixture와 일치한다 |
| 오타 쿼리 → `explain_error(run)` | ✅ 오류(Query, 위치 포함), 실패한 쿼리 원문, 관련 스키마(DbtE_Person)를 돌려줬다. 이 맥락만으로 고친 쿼리가 `['Alice','Bob']`를 반환했다 |
| Neo4j `read` 권한으로 `preview_changes` | ✅ Forbidden |
| PG `preview` 권한으로 `preview_changes` UPDATE | ✅ `committed:false`, `affected 1`. 다시 조회한 값이 그대로였다 |
| PG `preview` 권한으로 쓰기 실행 | ✅ Forbidden |
| 모든 연결 권한을 `none`으로 변경 | ✅ 맥락 도구 3개가 목록에서 사라졌다. 호출하면 NotFound(연결 존재를 노출하지 않음) |
| 토큰 폐기 | ✅ HTTP 401 |
| 유출 점검 | ✅ 설정, 이력, 로그, `agent_clients.json`에 토큰과 테스트 비밀번호가 없었다 |

앱 안 Anthropic 도우미의 실제 호출은 **미검증**이다. 키체인에 API 키가 없고, `ant` CLI의 OAuth 프로필은 앱이 쓰는 `x-api-key` 경로와 호환되지 않는다. 실제 호출은 사용자 계정에 과금된다. 요청 조립, SSE 파싱, refusal 처리, `cache_control` 위치는 mock HTTP로 검증했다(AI1 보고서).
