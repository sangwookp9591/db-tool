# DB 드라이버 추가 가이드

드라이버는 `dbcore`에만 의존하고 호스트와 UI를 참조하지 않는다. 실제 예제는
`crates/driver-example-kv`이며 앱에는 등록하지 않는다. SQLite 구현은
`crates/driver-sqlite`에서 파일 DB, blocking 실행, 취소와 읽기 전용 실행을 보여 준다.

Redis 실제 서버 드라이버는 `crates/driver-redis`이며 SCAN 탐색과 자료형별 표, 드라이버 허용 목록 기반 read-only 실행을 제공한다.

## 계약과 등록

1. `crates/driver-<name>`에 workspace package 설정과 `dbcore`, `async-trait` 의존성을 만든다.
2. workspace `Cargo.toml`의 `members`에 crate를 추가한다.
3. `Driver::manifest`에 연결 폼, 언어, paradigm과 실제 구현한 capabilities를 반환한다.
4. `Driver::connect`에서 연결을 검증하고 `Box<dyn Session>`을 반환한다.
5. `Session`의 ping, server_info, list_children, execute, close를 구현한다.
   지원하지 않는 browse/expand는 `DbError::unsupported`로 반환한다.
6. 앱에서 사용하려면 `src-tauri/Cargo.toml`에 path 의존성을 추가하고
   `src-tauri/src/drivers.rs`의 registry에 한 줄을 추가한다. UI 변경은 필요 없다.

예제 KV를 앱에 추가할 경우:

```toml
# src-tauri/Cargo.toml
 driver-example-kv = { path = "../crates/driver-example-kv" }
```

```rust,ignore
registry.register(Arc::new(driver_example_kv::KvDriver::new()));
```

`ConnectConfig.fields`의 값은 문자열이다. Bool도 `"true"`/`"false"`이다.
필수 필드와 허용 값은 드라이버에서 다시 검증한다. secret 필드는 manifest에서
`secret: true`로 선언하고 연결 시 `cfg.secret`을 사용한다. 로그와 오류에 비밀번호를 넣지 않는다.
SQLite는 `path: FilePath`와 `read_only_open: Bool`만 선언하며 비밀번호가 없다.

## Capabilities는 동작 보증이다

| 필드 | true로 선언하기 위한 조건 |
|---|---|
| cancel | None: 미지원, Abort: 클라이언트 중단, Server: DB 실행 자체 중단 |
| timeout | 시간 경과 후 DB 작업과 리소스 정리를 실제로 완료 |
| read_only_exec | SQL 문자열 추측이 아닌 DB 수준의 쓰기 거부, 실행 후 상태 원복 |
| params | JSON 입력의 타입과 바인딩 규칙을 검증하고 파라미터 바인딩 구현 |
| transactions | 같은 세션에서 BEGIN/COMMIT/ROLLBACK의 상태 유지 |
| paged_browse | offset/limit과 has_more를 정확히 반환 |
| schema_graph | `Session::schema_graph(scope)`에서 엔티티·필드·관계를 공통 계약으로 반환 |
| filtered_browse | `PageRequest.filter`의 모든 `(필드, typed Value)`를 AND 동등 조건으로 적용 |
| multi_statement | 문자열 내부 세미콜론을 보존하며 문장별 결과 반환 |
| graph_expand | 노드 식별자 검증, 이웃 한도와 그래프 정합성 보장 |
| edit_rows | 실제 편집 계약과 저장 기능이 있을 때만 선언 |

KV 예제는 대부분 false다. 따라서 UI는 paging, graph 확장, timeout, cancel, parameters 등의
선택 기능을 표시하지 않는다. 지원하지 않는 기능을 호출하면 드라이버도 거부한다.
읽기 전용을 보증하지 못하는 드라이버의 agent 실행에는 호스트의 승인 절차가 적용된다.

## 값의 정확성

`Value` 태그를 사용하며 정수와 decimal은 문자열로 보낸다. 예를 들어
`9223372036854775807`은 `Value::Int("9223372036854775807".into())`다.
JSON을 파싱 후 float로 다시 직렬화하지 않는다. 큰 수를 포함하는 JSON은
`Value::Json`에 원문으로 담는다. NULL은 빈 문자열이 아닌 `Value::Null`이다.

SQLite는 storage class에 따라 Integer/Real/Text/Blob/Null을 매핑한다. TEXT의 선언 타입이
DATE이면 Date, DATETIME/TIMESTAMP이면 DateTime, JSON이면 Json으로 원문을 보낸다.
선언 타입이 없으면 첫 행의 storage class를 `Column.type_name`으로 사용한다.
BLOB은 전체 바이트 수와 첫 64바이트 hex를 반환하는 공통 계약을 따른다.
유효하지 않은 UTF-8은 손실 변환하지 않고 Query 오류로 반환한다.

## 탐색과 browse

`list_children`은 한 단계만 조회한다. SQLite는 Tables/Views → 테이블 → Columns/Indexes
순서다. 컬럼은 PK를 Key 아이콘과 detail로 표시하고, 인덱스는 정의 SQL을 detail로 반환한다.
노드 ID는 드라이버 전용의 불투명 식별자다. 테이블 이름에 따옴표나 세미콜론이 있어도
식별자로 quote해야 한다. 문자열 연결로 SQL 식별자를 그대로 넣지 않는다.
노드의 `Browse`, `OpenQuery` 액션을 선언하면 UI가 기존 동작으로 처리한다.
`OpenSchemaGraph { scope }`는 다이어그램 탭을 연다. `scope`와 엔티티 `id`는 드라이버가
해석하는 불투명 값이며 UI는 이름이나 드라이버 ID로 조회 문장을 생성하지 않는다.
`schema_graph`·`filtered_browse`는 serde 기본값 false이고 `PageRequest.filter`의 기본값은 빈 목록이다.
필터를 지원하지 않는 드라이버에 빈 목록 이외의 필터를 보내면 호스트가 Unsupported로 거부한다.
FK 관계의 `from_fields`와 `to_fields`는 복합 키 순서대로 대응한다. NULL 키는 이동 링크를 만들지 않는다.

browse는 `limit + 1`개까지만 확인하고 결과는 limit개만 반환해 `has_more`를 계산한다.
SQLite OFFSET paging은 동시 쓰기 사이에 스냅샷을 유지하지 않으며, 정렬 없는 SQL의
행 순서는 보장되지 않는다. 안정적인 업무용 목록은 명시적 ORDER BY/트랜잭션을 사용한다.

## 취소, 시간 초과, 행 한도

동기 DB API는 async 런타임에서 직접 실행하지 않는다. SQLite는 `spawn_blocking`과
세션별 mutex를 사용한다. 연결 생성/조회/종료도 blocking 작업으로 실행한다.
`InterruptHandle` watcher가 취소 토큰과 deadline을 지켜본다. progress handler도
같은 조건을 확인해 interrupt가 문장 시작 전에 도착한 경우를 처리한다.
완료와 interrupt를 동기화해서 이전 실행의 취소가 다음 실행에 전달되지 않게 한다.
호스트가 execute future를 버려도 blocking 작업은 deadline 안에 정리된다.

SQLite `read_only`는 기존 query_only를 저장하고 ON으로 실행한 후 성공/오류/취소
모두에서 원복한다. authorizer는 PRAGMA, ATTACH, DETACH, transaction/savepoint 변경을
거부한다. 따라서 `PRAGMA query_only=OFF; INSERT ...` 우회가 불가능하다.
모든 PRAGMA를 거부하므로 read-only PRAGMA도 사용할 수 없다. 읽기 전용 SQL은 SELECT를 사용하고,
스키마 메타데이터는 list_children으로 조회한다. PRAGMA 기반 table-valued 함수도
내부 PRAGMA 권한 검사로 거부될 수 있다. 읽기 전용 실행은 미신뢰 SQL의 완전한
sandbox가 아니므로 신뢰할 수 없는 DB 파일이나 확장을 자동 로드하지 않는다.

여러 문장의 쓰기를 원자적으로 처리하려면 명시적인 BEGIN/COMMIT을 사용한다.
취소·타임아웃이 이전 문장에서 이미 commit된 쓰기까지 되돌리지는 않는다.

행은 step하면서 `row_limit`까지만 모은다. 다음 행이 있으면 truncated=true로 표시하고
커서를 버린다. 이것은 결과 행 수의 상한이며 단일 셀의 크기나 DB 엔진 자체의 sort/aggregate
메모리 상한은 아니다. 여러 문장은 DB 엔진의 parser로 분리해야 trigger/string/comment의
세미콜론이 보존된다. SQLite는 rusqlite Batch를 사용한다.

## 오류와 테스트

SQL/타입 변환 실패는 Query, 파일 열기/권한 실패는 Connection, 취소는 Cancelled,
실행 deadline은 Timeout, 미지원 기능은 Unsupported다. 드라이버 오류를 빈 성공 결과로
바꾸지 않는다. 취소 직후 같은 세션의 SELECT가 성공하는지 확인한다.

```sh
CARGO_TARGET_DIR=target-w5 cargo test -p driver-sqlite -p driver-example-kv
CARGO_TARGET_DIR=target-w5 cargo test --workspace
CARGO_TARGET_DIR=target-w5 cargo clippy --workspace -- -D warnings
```

SQLite 테스트는 임시 디렉터리의 실제 파일 DB를 생성한다. 정확한 값, NULL/BLOB/JSON,
잘린 결과, 문법 오류, read-only 파일 열기, timeout/cancel 및 세션 재사용, 읽기 전용
우회, 탐색, 특수 문자가 포함된 이름, 마지막 browse 페이지를 검증한다.
KV는 GET/SET/KEYS, limit, 탐색과 미지원 기능 거부를 검증한다.
KV의 `KEYS pattern`은 정확한 키 또는 하나의 후행 `*`만 지원하며 다른 wildcard는 거부한다.
메모리는 Driver 인스턴스 내 세션끼리 공유하고 프로세스 재시작 시 사라진다.

## 외부 프로세스 플러그인(JSON-RPC)으로 옮기기

Manifest, 요청, 결과, 탐색과 오류 계약은 serde 직렬화가 가능하다. `Driver/Session` trait
자체를 직렬화하는 대신 호스트에 proxy Driver/Session을 두고 connect/execute/list_children
등의 메서드를 JSON-RPC 요청으로 바꾼다. 외부 프로세스는 실제 연결을 소유한다.
세션 ID와 실행 ID를 별도로 전달하고 cancel/close 메시지, 요청 크기 상한, 프로세스 종료 시
pending 요청 실패, timeout과 인증 경계를 구현해야 한다. DB 비밀과 결과는 로그에 남기지
않는다. 현재 저장소는 in-process 등록만 구현하며 외부 프로세스 로더를 제공하지 않는다.

구현 API 참고: [rusqlite Connection](https://docs.rs/rusqlite/0.40.2/rusqlite/struct.Connection.html),
[SQLite query_only](https://sqlite.org/pragma.html#pragma_query_only),
[SQLite interrupt](https://sqlite.org/c3ref/interrupt.html).
