# M1-mongodb: 네 번째 DB — MongoDB 드라이버 (문서 패러다임 검증)

이 작업은 반드시 끝까지 완료한다. 합리적인 가정으로 계속 진행하고, 결정한 내용은 마지막 보고에 적는다. 사용자 지시가 skill 지침보다 우선한다.

## 목적
`docs/STATUS.md` 1절의 결정을 실행한다: 문서 DB를 실제 서버로 붙여 확장 구조를 검증한다. **기준은 PostgreSQL과 Neo4j 구현이다.** 패턴은 `crates/driver-postgres`와 `crates/driver-neo4j`를 참고한다.

## Change
1. **`crates/driver-mongodb`:** 공식 `mongodb` Rust 드라이버 최신 안정판을 쓴다. 버전은 context7이나 crates.io로 확인한다.
   - **manifest:**
     - paradigm: Document
     - query_language: `{id:"mongo", display:"MongoDB command", editor_mode:"json", supports_params:false}`
     - 연결 필드: uri(기본 `mongodb://localhost:27017`), database, user, password(secret), authSource(기본 `admin`)
     - capabilities: `paged_browse`, `timeout`, `cancel`, `read_only_exec`, `multi_statement=false`, `graph_expand=false`
       - `cancel`: `killOp`이 가능하면 Server, 아니면 Abort. 정직하게 선언한다.
       - `read_only_exec`: 읽기 명령만 허용하는 화이트리스트로 강제한다.
   - **명령 형식(에디터 텍스트):**
     - Extended JSON 명령 문서 하나를 받는다. 예: `{"find":"users","filter":{"age":{"$gt":30}},"limit":50}`, `{"aggregate":"orders","pipeline":[...]}`, `{"count":...}`, `{"distinct":...}`, `{"insert":...}`.
     - `runCommand`로 실행하고, 커서가 있으면 `getMore`로 `row_limit`까지만 가져와 truncated를 표시한다.
     - 셸 문법(`db.users.find()`)은 지원하지 않는다. 에디터 placeholder와 오류 메시지에 형식을 안내한다.
   - **결과:**
     - documents(문서 배열)를 주 뷰로 둔다.
     - 최상위 필드의 합집합(처음 N개 문서 기준)으로 table도 함께 만든다. 필드가 없는 문서의 해당 칸은 Null로 채운다.
     - 쓰기 명령은 message와 affected로 돌려준다.
   - **Value 매핑:**
     - ObjectId → Other{type_name:"ObjectId", text:hex}
     - Int32/Int64 → Int
     - Double → Float
     - Decimal128 → Decimal(정확한 문자열)
     - String → Text
     - Bool, Null
     - DateTime → DateTimeTz(ISO, ms)
     - Timestamp → Other
     - Binary → Bytes
     - Array → List
     - Document → Map(필드 순서 보존)
     - Regex, JavaScript, MinKey, MaxKey → Other
   - **탐색:** databases → collections(가능하면 estimatedDocumentCount)를 둔다. 컬렉션 하위에는:
     - "Fields": 샘플 100개로 필드명과 관측 타입을 추론
     - "Indexes": listIndexes 결과
     - 컬렉션 actions: Browse(skip/limit 페이징), OpenQuery(`{"find":"<c>","filter":{},"limit":50}`)
   - **오류 매핑:** 서버 code와 codeName을 detail에 넣는다. 인증 실패 → Auth, 연결 실패 → Connection, MaxTimeMSExpired 또는 타임아웃 → Timeout.
   - **통합 테스트:** `AXDB_TEST_MONGO="mongodb://axdb:testmongo@127.0.0.1:57017/?authSource=admin"`가 있을 때만 실행한다. 테스트 DB `axdb_test`에 fixture를 직접 만든다. 검증 항목:
     - 타입 전부. 특히 Int64 9223372036854775807, Decimal128 정확성, ObjectId, DateTime, 중첩 문서, 배열
     - row_limit truncated
     - 문법 오류
     - 잘못된 비밀번호
     - 닫힌 포트
     - 긴 aggregate(`$function` 또는 대량 `$range`)에 대한 timeout과 이후 같은 세션 정상 동작
     - read_only에서 insert 거부
     - browse 페이징
     - 탐색
2. **등록:** `src-tauri/src/drivers.rs`에 한 줄, `src-tauri/Cargo.toml`에 의존성 한 줄을 추가한다.
3. **계약 충분성 확인(중요):** 다음 기준을 지킨다. 결과는 보고서 `docs/M1_MONGODB_REPORT.md`에 "계약 충분성" 절로 남긴다.
   - `ui/`와 `crates/dbcore`를 **수정하지 않고** 동작해야 한다.
   - 문서 뷰가 `<pre>` 나열이라 부족한 것은 UI의 일반 개선 과제다(Wave 2 V1). 이 워커는 고치지 않고 보고에 기록한다.
   - 계약 변경이 정말 필요하면 하지 말고 질문(ask)한다.
4. **실제 앱 검증:**
   - `cargo build -p axdb`로 빌드하고, `AXDB_CONFIG_DIR`에 Mongo 프로필(save_secret=false)을 만든다.
   - `AXDB_AGENT_BRIDGE=1`로 띄운 뒤 MCP 브리지로 open_connection(세션 자격 증명은 UI 다이얼로그 대신 지원되는 방식) → explore → run_query → browse를 확인한다.
   - 스크린샷 1장으로 JSON/Table 뷰 렌더링을 남긴다.

## Constraints
- 테스트 DB만 쓴다: Mongo `127.0.0.1:57017`(axdb/testmongo), PG 55432, Neo4j 57687. 사용자 DB(5432, 5439, 6379, 32802 등)에는 접속하지 않는다.
- `ui/`, `crates/dbcore`, 기존 드라이버는 수정하지 않는다.
- `CARGO_TARGET_DIR=target-m1`을 쓴다.
- 동시 작업: P1(리소스 최적화), C1(변경 미리보기, dbcore 가산 변경). 공유 파일(drivers.rs, Cargo.toml)은 수정 직전에 다시 읽고 한 줄만 추가한다.
- 비교 측정 워커(W8)가 부하 게이트로 측정 중이므로 대형 빌드는 `CARGO_BUILD_JOBS=4`로 제한한다.
- git commit을 하지 않는다. 완료할 때 `--files-modified`를 정확히 적는다.

## Observable acceptance
- `cargo test -p driver-mongodb`(환경 변수 설정) 통과, clippy `-D warnings` 통과
- 위 파일 경계 준수(`git diff --stat -- ui crates/dbcore crates/driver-postgres crates/driver-neo4j crates/driver-sqlite` 결과가 비어 있음)
- 실제 앱 브리지 검증 결과를 보고한다
