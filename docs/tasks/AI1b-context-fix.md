# AI1b-context-fix: 스키마 맥락이 비는 결함 수정

이 작업은 반드시 끝까지 완료한다. 사용자 지시가 skill 지침보다 우선한다.

## 코디네이터 재현 (실제 앱 + MCP)
- 테스트 PG `dbtool_test`의 테이블은 `e2e` 스키마에 있고 `public` 스키마는 비어 있다.
- 이 상태에서 `get_schema_context({session_id, focus:"e2e" | "typed" | 생략})`을 호출하면 `"schema": ""`, `schema_truncated:false`가 나온다.

## 원인
- `src-tauri/src/context.rs`의 `schema()`는 `session.schema_graph(None)`을 호출한다.
- PG 드라이버는 scope가 없으면 `public`만 조회해 빈 그래프를 돌려준다.
- 맥락 빌더는 빈 그래프를 그대로 성공으로 처리하고, explorer로 대체하지 않는다.

## Change (드라이버 id로 분기하지 않는다)
1. **맥락 빌더:** `schema_graph(None)`의 결과가 비었거나 focus가 있으면 다음 순서로 처리한다.
   - `list_children(None)`으로 루트 노드(스키마, 폴더 등)를 얻는다.
   - focus와 일치하는 루트를 먼저, 나머지는 그다음 순서로 각 노드 id를 scope로 `schema_graph(Some(id))`를 호출해 합친다.
   - 바이트 예산(6000)과 방문 상한을 지킨다.
   - 루트가 OpenSchemaGraph 액션을 가지면 그 scope를 쓴다.
   - 그래도 비면 기존 explorer 경로로 대체한다.
   - focus 매칭은 엔티티 이름뿐 아니라 스키마와 루트 라벨까지 고려한다.
2. 빈 요약을 성공으로 돌려주지 않는다. 정말 메타데이터가 없으면 `schema_note`로 사유를 둔다.
3. **회귀 테스트(실제 DB):**
   - PG: public이 비어 있고 비-public 스키마에 FK가 있는 테이블들이 있는 fixture(고유 이름)를 만든다. focus 없이, 스키마명 focus, 테이블명 focus로 각각 호출해 엔티티와 FK가 포함되는지 확인한다.
   - Neo4j, SQLite, Mongo는 기존 동작을 유지하는지 확인한다.
4. AgentPanel의 결과 콜백이 `lastResultsTab`을 갱신하지 않는 문제(P1 지적)도 확인해 고친다.

## Constraints
- 테스트 DB만 쓴다: PG 55432(dbtool_test 또는 axdb_test), Neo4j 57687, Mongo 57017.
- `CARGO_TARGET_DIR=target-ai1b`를 쓴다. git commit을 하지 않는다. 완료할 때 `--files-modified`를 정확히 적는다.

## Observable acceptance
- 관련 cargo test(실제 DB), clippy `--all-targets`, UI tsc, vitest 통과
