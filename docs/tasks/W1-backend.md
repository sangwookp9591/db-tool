# W1-backend: Rust 워크스페이스 + dbcore 계약 + PostgreSQL 드라이버 + Tauri 호스트

이 작업은 반드시 끝까지 완료한다. 합리적인 가정으로 계속 진행하고, 결정한 내용은 마지막 보고에 적는다. 사용자 지시가 skill 지침보다 우선한다.

## Target
저장소: `/Users/iron/Project/psw/db-tool`. 계약 명세는 `docs/ARCHITECTURE.md`에 있으니 **먼저 전부 읽는다.**

## Change
1. 루트 `Cargo.toml` 워크스페이스를 만든다. 멤버는 `crates/dbcore`, `crates/driver-postgres`, `src-tauri`이다.
2. `crates/dbcore`: ARCHITECTURE.md의 타입과 trait을 그대로 구현한다.
   - serde로 직렬화한다. 필드 이름은 snake_case이고 `Value`는 `{"t":..,"v":..}` 형태다.
   - `Registry`는 `register(Arc<dyn Driver>)`, `get(id)`, `manifests()`를 제공한다.
   - `ConnectConfig`의 Debug는 secret을 가린다.
   - `DbError` 생성 헬퍼를 둔다.
   - 결과 한도 헬퍼를 둔다: `row_limit` 클램프(1..=100000).
   - 단위 테스트: Value 직렬화 형태, 큰 정수가 문자열로 직렬화되는지, Debug 마스킹.
3. `crates/driver-postgres` (tokio-postgres):
   - **연결 필드:** host, port(5432), database, user, password(secret), sslmode(Select: disable/prefer/require). TLS는 `tokio-postgres-rustls` 또는 `postgres-native-tls`로 처리하고, prefer와 require를 지원한다.
   - **capabilities:** `cancel: Server`, `timeout`, `multi_statement`, `paged_browse`, `transactions` = true. `params` = false, `graph_expand` = false.
   - **탐색:** schemas(시스템 스키마 숨김: pg_catalog, information_schema, pg_toast*) → 테이블/뷰 폴더 → 테이블 → "Columns"(타입·nullable·PK 표시) 그리고 "Indexes"(정의 포함). 각 단계는 필요할 때 조회한다. 테이블 노드에는 Browse 액션과 `OpenQuery`(`SELECT * FROM "s"."t" LIMIT 100`) 액션을 단다.
   - **browse:** `SELECT * FROM "s"."t" ORDER BY <pk 또는 ctid> LIMIT n OFFSET m`. n+1개를 가져와 `has_more`를 판정한다. 식별자는 반드시 quote한다.
   - **execute:**
     - 문장 분리기를 직접 구현한다. 작은따옴표, `"..."`, `$tag$...$tag$`, `--`, `/* */` 안의 `;`를 무시해야 하고 단위 테스트를 둔다.
     - 문장마다 다음을 수행한다:
       - 결과 행을 반환하는 문장(prepare 결과의 `columns()`가 비어 있지 않음)이면 `prepare`로 컬럼 타입 OID와 이름을 얻는다.
       - 실행은 **텍스트 프로토콜로 서버 측 한도를 걸어** 한다. 트랜잭션 밖이면 `BEGIN; DECLARE c NO SCROLL CURSOR FOR <stmt>; FETCH <row_limit+1> FROM c; CLOSE c; COMMIT`처럼 `simple_query`를 쓴다. 이렇게 하면 서버가 텍스트로 렌더한 정확한 값이 그대로 온다(numeric, int8, timestamptz, json, 배열, bytea hex).
       - 사용자가 이미 BEGIN한 트랜잭션 안이면 그 트랜잭션을 유지해야 하므로 DECLARE와 FETCH만 한다. 세션별로 트랜잭션 상태를 추적한다.
       - 행을 반환하지 않는 문장은 `simple_query`로 실행하고 `CommandComplete`의 영향 행 수를 `affected`와 `message`에 담는다.
     - OID별 값 매핑:
       - bool → Bool
       - int2/4/8 → Int(text)
       - numeric → Decimal
       - float4/8 → Float(NaN/Infinity 처리)
       - text/varchar/char/name → Text
       - date → Date, time/timetz → Time, timestamp → DateTime, timestamptz → DateTimeTz, interval → Duration
       - uuid → Uuid, json/jsonb → Json(원문)
       - bytea → Bytes(`\x` hex를 파싱해 len과 앞 64바이트 hex)
       - 배열 → Other{type_name:"int4[]", text}
       - 그 외 → Other{type_name, text}
       - NULL → Null
     - `column.type_name`은 pg_type의 typname이다. 세션 연결 시 캐시하거나 `format_type`으로 얻는다.
   - **취소:** 토큰이 발동하면 `client.cancel_token().cancel_query(tls)`를 보내고 `ErrorKind::Cancelled`를 반환한다.
   - **오류:** PG `DbError`의 message, detail, position을 옮긴다. 네트워크 오류나 연결 종료는 `ErrorKind::Connection`, 인증 실패(SQLSTATE 28P01/28000)는 `Auth`로 분류한다.
   - **통합 테스트:** `DBTOOL_TEST_PG` 환경변수(예: `host=127.0.0.1 port=55432 user=postgres password=testpw dbname=dbtool_test`)가 있을 때만 실행한다. 테스트 DB(`127.0.0.1:55432`, 이미 떠 있음)에서 직접 테이블을 만들고 검증한다:
     - 타입 정확성: int8 최댓값 9223372036854775807, numeric 12345678901234567890.123456789, NULL, timestamptz, jsonb 안의 큰 수, bytea, uuid, 배열
     - row_limit truncated
     - 쿼리 오류 메시지와 위치
     - `pg_sleep(10)` + timeout 500ms
     - `pg_sleep(10)` + cancel 후 서버에서 실제로 중단되는지(이후 같은 세션으로 `SELECT 1` 성공)
     - 잘못된 비밀번호 → Auth
     - 닫힌 포트 → Connection
     - browse 페이징
     - 탐색(스키마/테이블/컬럼/인덱스)
4. `src-tauri` (Tauri 2) 호스트:
   - ARCHITECTURE.md의 command를 전부 구현한다. `drivers.rs`에 `pub fn registry() -> Registry`를 두고 드라이버 등록은 이곳에서만 한다.
   - **프로필:** `app_config_dir/profiles.json`. manifest에서 `secret=true`인 필드는 파일에 저장하지 않고 `keyring` crate(apple-native 등 플랫폼 feature)에 저장한다. 서비스는 `db-tool`, 계정은 `profile:{id}`이다. 프로필에 `save_secret: bool` 옵션을 두고, false면 연결할 때마다 UI가 secret을 입력받는다.
   - **세션 맵:** `session_id → Arc<dyn Session>`.
   - **실행 맵:** `run_id → CancellationToken`. `execute`는 `tokio::time::timeout(req.timeout_ms)`을 적용하고, 타임아웃이 나면 토큰을 발동해 서버 취소까지 보낸 뒤 `Timeout`을 반환한다.
   - **이력:** `app_data_dir/history.jsonl`, 최대 1,000줄. 파라미터 값과 secret은 기록하지 않는다.
   - **로그:** `tracing` + 파일 로그(`app_log_dir/db-tool.log`). 연결 필드는 secret이 아닌 것만 기록한다.
   - **환경 변수 `DBTOOL_CONFIG_DIR`:** 지정하면 설정, 데이터, 로그 디렉터리를 모두 이 경로로 바꾼다. 검증할 때 격리하는 데 쓴다.
   - **tauri.conf.json:** `frontendDist: "../ui/dist"`, `devUrl: "http://localhost:5173"`, `beforeDevCommand: "pnpm --dir ../ui dev"`, `beforeBuildCommand: "pnpm --dir ../ui build"`. 창 제목은 `db-tool`, productName도 `db-tool`이며 identifier는 `dev.dbtool.app`이다.
   - `ui/`는 다른 워커가 만든다. 빌드 확인용으로 `ui/dist/index.html`이 없을 때만 임시 파일을 만들고, `ui/`의 다른 파일은 건드리지 않는다.
   - release 프로필: `opt-level=3`, `lto=true`, `codegen-units=1`, `strip=true`.
5. `crates/dbcore`에 TS 타입 미러가 필요하다. `ui/src/contract.ts`는 UI 워커가 ARCHITECTURE.md를 보고 작성하므로 이 워커는 만들지 않는다. 대신 serde 직렬화 결과의 실제 JSON 예시를 `docs/contract-examples.json`에 남긴다. 예시에는 manifest 1개, PG 결과 1개, 오류 1개를 넣는다.

## Constraints
- 사용자 데이터가 있는 DB(5432, 5439, 32802 포트의 기존 컨테이너)에는 **절대 접속하지 않는다.** 테스트는 `127.0.0.1:55432`(dbtool-test-pg, 비밀번호 `testpw`, DB `dbtool_test`)만 쓴다.
- 드라이버 crate가 Tauri에 의존하면 안 된다.
- 비밀번호를 로그, 오류 메시지, 이력에 넣지 않는다.
- `ui/` 디렉터리는 수정하지 않는다(위의 임시 index.html만 예외).
- git commit을 하지 않는다. 커밋은 코디네이터가 검증 후에 한다.
- AI 기능은 넣지 않는다.

## Observable acceptance
- `cargo build --workspace` 성공
- `cargo test --workspace` 성공. 통합 테스트는 `DBTOOL_TEST_PG='host=127.0.0.1 port=55432 user=postgres password=testpw dbname=dbtool_test'`로 실행하고 모두 통과해야 한다.
- `cargo clippy --workspace -- -D warnings` 성공
- `cargo build -p <tauri crate> --release` 성공
- 보고에 다음을 담는다: 테스트 출력 요약, 결정한 내용, 남은 한계
