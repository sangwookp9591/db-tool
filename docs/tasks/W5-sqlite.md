# W5-sqlite: 세 번째 DB(SQLite) 드라이버로 확장 구조 검증 + 플러그인 가이드

이 작업은 반드시 끝까지 완료한다. 합리적인 가정으로 계속 진행하고, 결정한 내용은 마지막 보고에 적는다. 사용자 지시가 skill 지침보다 우선한다.

## 목적
"새 DB는 드라이버 crate와 등록 한 줄로 추가되고, UI는 수정하지 않는다"는 주장을 실제로 증명한다.

## Target
- 저장소: `/Users/iron/Project/psw/db-tool`
- 먼저 읽을 것: `docs/ARCHITECTURE.md`, `crates/dbcore`, `crates/driver-postgres`(패턴 참고)

## Change
1. **`crates/driver-sqlite`(rusqlite, `bundled` feature):**
   - 연결 필드: `path`(FilePath, 필수). 비밀번호는 없다. `read_only_open`(Bool)은 켜면 `SQLITE_OPEN_READ_ONLY`로 연다.
   - paradigm: Relational. query_language: `{id:"sql", editor_mode:"sql"}`.
   - capabilities:
     - `cancel: Server`. `InterruptHandle`로 실행 중인 문장을 실제로 중단한다.
     - `timeout`, `multi_statement`, `paged_browse`, `read_only_exec`(`PRAGMA query_only=ON`으로 실행하고 원복), `transactions`
   - rusqlite는 동기 API이므로 연결을 전용 스레드에 둔다(`spawn_blocking` 또는 actor 스레드). async 런타임을 막지 않아야 한다.
   - 탐색: "Tables"/"Views" → 테이블 → Columns(`pragma_table_info`, PK 표시) / Indexes(`pragma_index_list`, 정의는 `sqlite_master.sql`). Browse 액션과 OpenQuery 액션을 둔다.
   - 값 매핑:
     - INTEGER → Int(문자열)
     - REAL → Float
     - TEXT → Text. 단 선언 타입이 DATE/DATETIME/TIMESTAMP/JSON이면 각각 Date/DateTime/Json 원문으로 매핑한다.
     - BLOB → Bytes
     - NULL → Null
     - `column.type_name`은 선언 타입이고, 없으면 첫 값의 storage class를 쓴다.
   - row_limit: 스텝을 진행하다 한도에 도달하면 멈추고 truncated로 표시한다. 메모리에 전부 적재하지 않는다.
   - 오류 매핑: SQL 오류 → Query(메시지 포함), 파일 없음·권한 문제 → Connection, interrupt → Cancelled, 시간 초과 → Timeout.
   - 테스트(임시 파일 DB를 직접 생성):
     - 타입(int64 최댓값, NULL, BLOB, JSON 텍스트)
     - truncated
     - 문법 오류
     - 없는 경로 + read_only_open → Connection
     - 재귀 CTE 무한 루프 + timeout 300ms → Timeout, 이후 같은 세션 `SELECT 1` 성공
     - cancel
     - read_only에서 INSERT 거부
     - browse 페이징
     - 탐색
2. **등록:** `src-tauri/src/drivers.rs`에 한 줄을 추가한다. `src-tauri/Cargo.toml`에 의존성 한 줄을 추가한다.
3. **증명** (주의: W3/W4가 ui/와 src-tauri/lib.rs를 동시에 수정 중이므로 전체 git diff는 오염된다. 대신 **이 워커가 만든/수정한 파일 목록**을 worker_done의 --files-modified에 정확히 넣어라. 코디네이터가 그 파일만 별도 커밋해 diff로 증명한다. 등록 줄과 Cargo 의존성은 공유 파일이므로 수정 직전 다시 읽고 한 줄만 추가):
   - 작업 시작 시점의 `git rev-parse HEAD`를 기록해 둔다.
   - 완료 후 `git diff --stat <그 커밋>`을 실행해 다음을 보여 준다:
     - `ui/` 변경 0
     - `src-tauri` 변경은 등록 줄과 Cargo 의존성뿐
     - `crates/dbcore` 변경 0. 꼭 필요하면 이유를 적는다. 그것은 계약이 불충분하다는 발견이므로 그 자체로 보고 가치가 있다.
   - 실제 앱(debug)에서 SQLite 파일 연결 → 트리 → 쿼리 → browse가 동작하는지 확인한다. UI 자동화는 `window.dbtool.agent`나 MCP 브리지가 있으면 그것을 쓰고, 없으면 수동 절차와 스크린샷을 남긴다.
4. **`docs/PLUGIN_GUIDE.md`:**
   - 새 DB 드라이버를 만드는 방법을 설명한다: 계약 요약, capabilities를 선언하는 기준, Value 매핑 규칙(정확성 원칙), 탐색 트리 설계, 취소·타임아웃·한도 구현 요령, 오류 분류, 테스트 패턴, 등록 방법
   - **최소 예제:** 메모리 키-값 드라이버 `crates/driver-example-kv`를 둔다. 실제로 컴파일되고 테스트되어야 하며 약 150줄이다.
     - paradigm: KeyValue
     - 탐색: 키 목록
     - 명령: `GET k` / `SET k v` / `KEYS pattern`. 결과는 table 또는 documents.
     - capabilities는 대부분 false로 둬서 "선언하지 않으면 UI가 해당 기능을 숨기는" 예시가 되게 한다.
     - **앱에 등록하지 않는다.** 가이드에 등록 한 줄만 보여 준다. 컴파일과 테스트는 워크스페이스에서 한다.
   - "외부 프로세스 플러그인(JSON-RPC)으로 옮기는 방법" 절을 짧게 둔다. 계약 타입은 serde 직렬화가 가능하므로 경계를 옮길 수 있다는 점을 설명한다.

## Constraints
- `ui/`를 수정하지 않는다.
- `crates/driver-postgres`, `crates/driver-neo4j`를 수정하지 않는다.
- git commit을 하지 않는다.

## Observable acceptance
- `cargo test --workspace` 통과(PG/Neo4j 환경변수 포함), `cargo clippy --workspace -- -D warnings` 통과
- `git diff --stat` 증명 결과를 보고에 붙인다
- `docs/PLUGIN_GUIDE.md`와 예제 crate의 테스트가 통과한다
