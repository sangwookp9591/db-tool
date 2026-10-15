# C1-change-preview: 데이터·스키마 변경 미리보기 (계약 + 드라이버 + 호스트)

이 작업은 반드시 끝까지 완료한다. 합리적인 가정으로 계속 진행하고, 결정한 내용은 마지막 보고에 적는다. 사용자 지시가 skill 지침보다 우선한다.

## 목적
쓰기 문장(DML, DDL, Cypher 쓰기)을 **실제로 커밋하지 않고** 영향을 미리 보여 준다. 사용자와 에이전트의 쓰기는 모두 "미리보기 → 확인 → 적용"을 거칠 수 있어야 한다. 미리보기 지원 여부는 DB마다 다르므로 capability로 선언한다.

## Change
1. **dbcore(가산적 변경만, 기존 직렬화 호환):**
   - `Capabilities.preview_changes: PreviewSupport { None | Transactional | DryRun }`(serde default None)
   - `Session::preview(&self, req: &ExecRequest, cancel) -> Result<ChangePreview, DbError>`. 기본 구현은 Unsupported를 돌려준다.
   - `ChangePreview`의 필드:
     - `statements: Vec<StatementPreview>`
     - `committed: false`(항상)
     - `notes: Vec<String>`. 예: "DDL은 트랜잭션 롤백으로 되돌렸습니다", "시퀀스 값은 롤백되지 않습니다"
   - `StatementPreview`의 필드:
     - `text`
     - `kind`(Insert, Update, Delete, Ddl, GraphWrite, Other)
     - `affected: Option<u64>`
     - `before: Option<Table>`, `after: Option<Table>`: 영향을 받는 행의 전후. 각각 최대 `sample_limit`(기본 50)개다.
     - `schema_diff: Option<SchemaDiff>`: DDL에서 객체 단위 added/removed/changed, 예를 들어 테이블과 컬럼 목록 비교
     - `counters: Vec<(String, i64)>`: Neo4j의 nodes_created 같은 값
     - `warnings`
   - `ExecRequest`에 `preview_sample_limit`을 추가한다(default 50).
   - ARCHITECTURE.md 계약 블록에도 반영한다.
2. **PostgreSQL(Transactional):**
   - `BEGIN` 안에서 실행하고, 영향 행의 전후를 수집한 뒤 반드시 `ROLLBACK`한다.
   - 수집 방법은 보고서에 쓴다. 예:
     - UPDATE/DELETE: 같은 WHERE로 대상 행을 먼저 `SELECT ... FOR UPDATE`하고, 실행은 RETURNING으로 after를 얻는다. 사용자 문장에 RETURNING이 없으면 RETURNING *를 덧붙인 변형을 실행한다. 안전하게 덧붙일 수 없으면 affected만 얻는다.
     - INSERT: RETURNING *로 after를 얻는다.
     - DDL: 실행 전후로 `information_schema`와 `pg_catalog` 스냅샷(테이블, 컬럼, 인덱스)을 떠서 diff를 만든다.
   - 롤백할 수 없는 문장(VACUUM, CREATE DATABASE, CREATE INDEX CONCURRENTLY 등)은 실행하지 않고 거부하며, 거부 사유를 notes에 남긴다.
   - 사용자가 연 트랜잭션 안에서는 SAVEPOINT를 쓰고 ROLLBACK TO로 되돌린다.
3. **Neo4j(Transactional):** 명시적 트랜잭션 안에서 실행해 summary counters를 얻는다. 결과로 반환된 노드와 관계를 after 샘플로 쓰고, 반드시 rollback한다.
4. **SQLite(Transactional):** `SAVEPOINT`, 실행, 샘플 수집, `ROLLBACK TO`, `RELEASE` 순서로 한다. DDL은 `sqlite_master` diff로 처리한다.
5. **MongoDB:** M1 워커가 따로 만든다. 이 작업에서는 계약만 제공하고 Mongo 구현은 하지 않는다. 보고에 "Mongo는 None 또는 DryRun(explain)으로 선언 권장"이라고 적는다.
6. **호스트:**
   - `preview(session_id, run_id, req)` command를 추가한다. 타임아웃과 취소를 적용하고 이력에는 `kind: preview`로 기록한다.
   - 에이전트 도구 `preview_changes(session_id, text)`를 `ui/src/agent/registry.ts`에 추가한다. 이 도구는 쓰기를 커밋하지 않으므로 readOnlyHint를 붙인다. capability에서 파생해 조건부로 등록한다.
   - UI는 Wave 3에서 만든다. 단, 쿼리 탭에 `capabilities.preview_changes !== 'None'`이면 보이는 "미리보기" 버튼과 최소 결과 표시(문장별 affected, before/after 표, schema diff 목록)는 이 워커가 간단히 붙인다.
7. **테스트(실제 DB, 테스트 DB만):** 각 DB에서 다음을 확인한다.
   - UPDATE, DELETE, INSERT, DDL(ADD COLUMN, CREATE TABLE, DROP INDEX), Neo4j CREATE/SET/DELETE를 미리보기한다.
   - **미리보기 후 실제 데이터와 스키마가 바뀌지 않았는지** 다시 조회해 확인한다.
   - 롤백할 수 없는 문장의 거부
   - 취소와 타임아웃 뒤에 세션이 깨끗한지(트랜잭션이 열린 채로 남지 않는지)

## Constraints
- 사용자 DB에는 접속하지 않는다. 테스트 DB만 쓴다: PG 55432(axdb_test), Neo4j 57687, SQLite 임시 파일.
- UI 변경은 최소한으로 하고 드라이버 id로 분기하지 않는다(grep 0).
- `CARGO_TARGET_DIR=target-c1`을 쓰고, `CARGO_BUILD_JOBS=4`로 제한한다(W8 측정 중).
- 동시 작업: M1(Mongo 드라이버, dbcore를 건드리지 않음), P1(리소스 최적화). 공유 파일은 수정 직전에 다시 읽는다.
- git commit을 하지 않는다. 완료할 때 `--files-modified`를 정확히 적는다.

## Observable acceptance
- `cargo test --workspace`(AXDB_TEST_PG, AXDB_TEST_NEO4J 설정) 통과, clippy 통과, UI tsc, vitest, build 통과
- 기존 E2E 회귀 21/21(`bench/e2e-bridge.mjs`): 코디네이터가 확인한다
- 보고서 `docs/C1_PREVIEW_REPORT.md`: DB별 미리보기 방식, 한계(시퀀스, 트리거 부작용, 잠금 시간), 검증 증거
