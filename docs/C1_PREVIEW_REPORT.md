# C1 변경 미리보기 구현·검증 보고서

작성일: 2026-10-15. 명세: [tasks/C1-change-preview.md](tasks/C1-change-preview.md). 계약: [ARCHITECTURE.md](ARCHITECTURE.md).

## 구현

`Capabilities.preview_changes`는 `None | Transactional | DryRun`이며 이전 manifest JSON에서는 `None`으로 역직렬화된다. `ExecRequest.preview_sample_limit`은 이전 요청 JSON에서 50으로 기본 설정된다. 샘플은 각각 요청 한도 이내이며 호스트/드라이버는 최대 1,000행, 에이전트 도구는 최대 200행으로 제한한다. 기존 Rust 전체 struct literal은 새 필드를 명시해야 하므로 기존 호출자·예제·테스트도 갱신했다. `Session::preview`의 기본 구현은 Unsupported이고, 세 구현의 성공/거부 응답은 항상 `committed: false`다.

| DB | 방식 | 샘플·스키마 | 정리 |
| --- | --- | --- | --- |
| PostgreSQL (`Transactional`) | 세션 실행 잠금을 유지하며 BEGIN; 사용자 트랜잭션 안에서는 이름이 고유한 SAVEPOINT | 단순 단일 테이블 UPDATE/DELETE: 같은 WHERE의 SELECT * … LIMIT n FOR UPDATE로 before. INSERT/UPDATE: 기존 RETURNING이 없으면 줄바꿈 후 RETURNING *를 붙여 after. 기존 RETURNING은 사용자 projection을 유지하며 경고. 전체 결과를 drain해 CommandComplete의 정확한 affected를 수집하고 샘플만 보관. DDL은 pg_class/pg_namespace, information_schema.columns, pg_indexes의 전후 객체 identity/정의를 비교 | 성공·오류·취소 뒤 ROLLBACK 또는 ROLLBACK TO + RELEASE. 취소는 기존 서버 취소/응답 대기 경로를 재사용. 정리 실패 시 연결 폐기 |
| SQLite (`Transactional`) | blocking pool에서 연결 lease를 유지하며 SAVEPOINT | 동일 단순 UPDATE/DELETE 조건으로 before, RETURNING으로 after. 전체 RETURNING을 drain한 뒤 sqlite changes()로 직접 변경 행 수 수집. sqlite_master와 sqlite_temp_master의 객체 type/name/SQL 정의 비교 | progress handler/interrupt로 취소한 뒤 취소 hooks를 해제하고 ROLLBACK TO + RELEASE. 엔진이 이미 전체 트랜잭션을 자동 롤백했으면 autocommit 확인 후 중복 정리를 생략. 정리 실패 시 연결 폐기 |
| Neo4j (`Transactional`) | 실행마다 전용 pool을 만들고 명시적 Bolt 트랜잭션 시작 | RETURN으로 돌려준 노드/관계/값을 typed Table after 샘플로 보관. 전체 stream을 drain해 summary의 nodes_created, properties_set, relationships_created 등 12개 counter 수집 | 항상 명시적 rollback 시도. 취소/타임아웃 시에도 rollback을 최대 3초 기다리고, 전용 pool/socket 폐기로 미완료 연결을 재사용하지 않음 |

SQL의 주석·문자열·식별자·중첩 괄호를 구분해 샘플 쿼리를 만든다. 별도 의존성은 추가하지 않았다. 조인·alias·CTE처럼 단일 대상/WHERE를 안전하게 추출할 수 없는 SQL은 before를 추측하지 않는다. 영향 수와 반환 결과를 제공하고 샘플 한계를 표시한다. DDL 스냅샷은 주로 테이블/컬럼/인덱스 범위이며 전체 권한·함수·트리거 카탈로그 diff가 아니다.

## 거부와 안전 경계

SQL은 배치 전체를 사전 검사한다. 하나라도 거부 문장이 있으면 앞 문장을 포함해 아무것도 실행하지 않고 notes/문장 warnings에 이유를 반환한다. BEGIN/COMMIT/ROLLBACK/SAVEPOINT 등 트랜잭션 제어, SET/PRAGMA/ATTACH 등 세션 제어, VACUUM, CREATE DATABASE, CREATE/DROP INDEX CONCURRENTLY, tablespace/subscription/virtual-table 작업 등은 거부한다. SQLite authorizer는 실행 중에도 트랜잭션/세션 제어와 load_extension/writefile/readfile 함수를 차단한다. Neo4j는 CALL(프로시저/서브쿼리), LOAD, database 관리, IN TRANSACTIONS 관련 요청을 보수적으로 거부한다.

**SQLite의 열린 사용자 트랜잭션 안 미리보기는 실행 전에 거부한다.** SQLite는 중단된 INSERT/UPDATE/DELETE 또는 ROLLBACK 충돌 정책에서 저장점 밖의 전체 트랜잭션을 취소할 수 있다. 사용자의 미커밋 작업을 보호하려면 먼저 그 트랜잭션을 끝내야 한다. PG의 사용자 트랜잭션은 SAVEPOINT로 보호하며 별도 검증했다. [SQLite interrupt 문서](https://www.sqlite.org/c3ref/interrupt.html), [충돌 처리 문서](https://www.sqlite.org/conflict.html).

미리보기는 실제 SQL/Cypher를 실행한다. 따라서 다음 한계는 notes/도구 설명과 함께 전달한다.

- PostgreSQL nextval/setval 등 시퀀스 값은 롤백되지 않는다. [PostgreSQL sequence 문서](https://www.postgresql.org/docs/current/functions-sequence.html).
- 트리거·사용자 함수·확장·외부 테이블/서비스의 외부 부작용은 DB 트랜잭션으로 복구를 보장할 수 없다. 임의의 함수를 sandbox하는 기능은 없다.
- 쓰기/DDL은 롤백까지 실제 잠금을 유지하고 전체 변경량만큼 작업한다. 샘플 한도는 실행 비용/잠금 행 수를 제한하지 않는다.
- before 조회와 실제 쓰기의 조건은 별도 평가된다. volatile 조건, 동시 쓰기, 트리거는 샘플 대응에 영향을 줄 수 있다. before/after는 행별 확정 diff나 나중 적용 결과 보증이 아니다.
- 스키마 스냅샷에는 같은 시점에 보이는 다른 세션의 DDL 변화가 섞일 수 있다. 데이터/스키마를 잠가 전체 DB를 동결하는 방식은 쓰지 않는다.
- PostgreSQL 조인/alias/CTE의 before는 제공하지 않는다. SQLite 복잡한 문장의 before도 생략한다. 기존 RETURNING은 전체 컬럼 대신 그 projection이다. Neo4j before는 제공하지 않으며 after는 명시적으로 반환한 값만 포함한다.
- Neo4j affected는 counter 합계이므로 고유 노드/관계 수와 다를 수 있다. Cypher DDL은 counter로 표시하며 SQL식 schema_diff는 제공하지 않는다.
- SQLite 복합 CREATE TRIGGER 등의 본문 내 세미콜론은 보수적 사전 검사에서 거부될 수 있다. SQL 샘플 변환이 지원하지 않는 구문은 실행 오류로 전체 롤백하며, 임의 문자열 재작성으로 실행을 강행하지 않는다.

Mongo 구현은 이 작업에서 추가하지 않았다. **Mongo는 None 또는 DryRun(explain)으로 선언 권장.**

## 호스트·에이전트·UI

`preview(session_id, run_id, req, source?)`를 Tauri에 등록했다. 기존 실행 ID/취소 토큰과 세션 오류 관찰 경로를 사용하고 타임아웃 뒤 드라이버 정리가 끝날 때까지 기다린다. 이력은 `kind: preview`, source는 기본 ui/에이전트 호출은 agent이며 쿼리/오류의 credential redaction을 유지한다. 이전 이력 JSON의 kind는 execute로 기본 설정된다.

에이전트 `preview_changes`는 열린 연결의 capability에서 조건부 등록되고 호출 시 대상 capability를 재확인한다. readOnlyHint와 untrustedContentHint를 붙이고 취소 신호를 host cancel로 전달한다. 기존 쓰기 적용 승인 흐름은 execute_agent에서 유지된다.

쿼리 탭의 capability 조건부 미리보기 버튼, 문장별 affected/경고/counter, before/after VirtualTable, schema diff 목록을 추가했다. UI에 특정 DB ID 조건을 추가하지 않았다. 브라우저 mock은 DryRun과 "illustrative samples only; no database was contacted"를 명시하며 실제 DB 검증 자료로 사용하지 않는다. 편집/적용 전용 UX는 C2 범위다.

## 검증 증거

사용자 DB 접속 없이 PostgreSQL 16.15의 `127.0.0.1:55432/axdb_test`, Neo4j `127.0.0.1:57687`, SQLite tempfile만 사용했다. Rust 실행은 모두 `CARGO_TARGET_DIR=target-c1 CARGO_BUILD_JOBS=4`로 제한했다.

- `cargo test -p dbcore`: 계약 기본값/직렬화, SQL 토큰/거부/샘플 계획 검증 통과.
- 실제 DB `change_preview` 통합 테스트: PG/SQLite의 UPDATE, DELETE, INSERT, ADD COLUMN, CREATE TABLE, DROP INDEX; Neo4j CREATE, SET, DELETE, 관계 CREATE; counter/샘플 값과 한도를 검증하고 매번 실제 상태를 다시 조회해 롤백 확인.
- PG: 열린 사용자 트랜잭션의 기존 변경 보존, 오류/취소/타임아웃 뒤 재실행. SQLite: 열린 사용자 트랜잭션 거부와 기존 변경 보존, 오류/취소/타임아웃 뒤 재실행, 장시간 INSERT 중단에 따른 엔진 자동 롤백 뒤 재사용. Neo4j: 취소/타임아웃 뒤 새 요청으로 원래 그래프 상태 확인.
- 호스트: 취소 완료 대기/credential redaction/preview 이력 종류·source와 실제 PG 임시 DDL 미리보기 후 객체 부재 확인.
- UI: `pnpm --dir ui exec tsc --noEmit`, `pnpm --dir ui test` **6파일/35테스트 통과**, `pnpm --dir ui build` 통과.
- 실제 Chromium(ego-browser TaskSpace 60)의 mock 연결에서 버튼을 눌러 미커밋 안내와 before/after grid 2개를 확인. 결과 영역 너비 1,231px 중 표 너비 1,196px, 표 높이 각각 202px 확인. 로컬 스크린샷 `/tmp/c1-preview-ui.png` 확인. 이는 mock 화면 검증이며 네이티브/DB E2E 증거가 아니다.

최종 C1 범위 게이트(코디네이터 지시로 Mongo 제외):

```sh
AXDB_TEST_PG='host=127.0.0.1 port=55432 user=postgres password=testpw dbname=axdb_test' \
AXDB_TEST_NEO4J='bolt://127.0.0.1:57687|neo4j|testpassword' \
CARGO_TARGET_DIR=target-c1 CARGO_BUILD_JOBS=4 cargo test --workspace --exclude driver-mongodb
CARGO_TARGET_DIR=target-c1 CARGO_BUILD_JOBS=4 cargo clippy --workspace --exclude driver-mongodb -- -D warnings
```

Rust는 **39 passed / 0 failed / 1 기존 ignored(OS keychain 수동 테스트)**, clippy는 경고 없이 통과했다. `git diff --check`, 문서 상대 링크 확인, 미리보기 UI/에이전트 파일 내 DB ID 조건 grep 0도 통과했다. 성공 표시를 위한 TODO/주석/빈 구현으로 테스트를 대체하지 않았다.

`cargo test --workspace`와 `cargo clippy --workspace -- -D warnings` 전체 게이트도 시도했지만, 병행 작성 중인 `driver-mongodb`의 새 계약 필드 2개 누락과 unused import 때문에 컴파일 실패했다. 코디네이터가 M1에 수정 전달했으며 **전체 workspace 재검증은 코디네이터 소유**로 명시했다. M1 파일은 수정하지 않았다. 기존 E2E 21/21(`bench/e2e-bridge.mjs`)도 명세대로 코디네이터가 확인한다.

커밋하지 않았다. 시작 시 dirty였던 W8 측정 파일과 병행 M1/P1 변경은 보존했다. 공유 `src-tauri/src/lib.rs`의 P1 viewer_bench_checkpoint 등록도 유지했다. 이 워커가 연 UI dev server(5189)는 검증 후 종료했다.
