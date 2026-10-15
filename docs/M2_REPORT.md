# M2: MongoDB schema graph / TLS 실제 서버 검증

기준: main `2a9d69b`, 2026-10-16 KST. 앱 실행, commit/push, 사용자 DB 접근 없이 지정된 테스트 DB와 신규 loopback TLS 컨테이너에서 검증했다. 시작 시 작업 트리는 깨끗했으며, 이후 다른 작업자가 만든 Redis·등록·PG preview 변경은 수정하지 않았다.

## MongoDB 스키마 그래프

`SchemaGraph` 계약을 그대로 구현하고 `schema_graph=true`와 DB 노드의 `OpenSchemaGraph` 액션을 추가했다. 엔티티 ID는 기존 browse의 `["collection", database, collection]`와 같아 컬렉션과 뷰를 기존 ERD에서 열고 browse할 수 있다. `scope=None`은 연결 DB, DB/컬렉션 노드 scope는 해당 DB 전체를 반환한다. `system.*` 메타데이터 컬렉션은 제외한다.

컬렉션·뷰 각각 `$sample`로 최대 200문서를 읽어 BSON 타입 합집합과 중첩 점 경로를 만든다. 예: `profile.name: null|string`, `items.price: int32|string`. 배열 안 문서도 같은 점 경로로 합치며, 필드 누락 또는 명시적 null이 샘플에 있으면 nullable이다. `_id`는 PK로 표시한다. 빈 컬렉션은 필드가 비어 있는 엔티티로 남는다.

ObjectId 필드 `<x>Id` / `<x>_id`는 같은 DB의 `<x>` / `<x>s` 컬렉션 `_id`를 후보별 최대 5개 값의 `$in`으로 확인한다. 같은 DB DBRef의 `$ref` / `$id`도 확인하며 다른 DB를 지정한 DBRef는 제외한다. 실제 값이 하나 이상 존재한 경우만 `kind=inferred_reference` 관계를 만든다. 이는 FK 보증이 아니라 샘플 근거의 추론이다.

뷰의 직접 `$lookup {from,localField,foreignField}`는 뷰에서 대상 엔티티로 `kind=view_lookup` 관계를 만든다. pipeline-only lookup의 `$expr`, 중첩 facet/lookup pipeline, 동적 컬렉션명은 추측하지 않는다. `ui/src/SchemaView.tsx`는 relation.kind를 분기하지 않고 relation.name과 필드 목록을 표시하며, `ui/src/navigation.ts`도 kind 분기를 하지 않는다. 따라서 관계 name에 `inferred_reference` / `view_lookup`를 넣었고 UI 변경과 DB ID 조건문은 추가하지 않았다. `filtered_browse`는 기존 false를 유지하며 관계 셀 필터 이동 기능은 선언하지 않는다.

성능 한도는 DB당 컬렉션·뷰 합계 100개, 각각 200문서, 필드 2,000개, 깊이 32, 배열당 첫 200개 문서 원소, 관계 후보 100개, 후보당 ObjectId 5개다. 101개 이상이면 조용히 일부만 반환하지 않고 Unsupported 오류를 반환한다. 그래프 호출의 응답 제한은 15초이며 sample와 참조 확인에는 남은 시간/3초 중 작은 `maxTimeMS`를 적용한다. SDK future를 중간에 버리지 않도록 이미 전송된 작업은 detached task에서 응답을 drain하고, 다음 작업 전에 전체 deadline을 확인한다. listCollections 자체는 SDK의 연결/서버 선택 제한을 사용하므로 응답 deadline 이후에도 이미 전송된 메타데이터 요청이 끝날 때까지 drain이 남을 수 있다.

실제 fixture는 `AXDB_TEST_MONGO`의 `axdb_test` DB에 `axdbt_<ObjectId>_users`, orders, lookup 뷰, empty, sampled를 생성하고 정리한다. orders에는 요청한 `userId:ObjectId` 및 정확한 후보 이름 규칙을 검사하는 `<prefix>userId` / `<prefix>user_id`, 같은/다른 DB DBRef, 존재하지 않는 참조가 있다. lookup은 `userId → _id`로 join한다. 201개 문서에서 정확히 200개 필드가 관측되는지 검증하고, 별도 생성한 `axdbt_schema_limit_<ObjectId>` 테스트 DB의 101개 컬렉션 거부 후 해당 DB를 삭제한다. 기존 테스트 fixture나 사용자 컬렉션은 정리 대상으로 삼지 않는다.

## TLS 동작 및 수정

PostgreSQL 기존 코드는 native_tls 기본 검증 때문에 자체 서명 서버에 `sslmode=require`로 접속하지 못했다. 수정 전 동일한 `real_postgres_tls_modes` 테스트에서 require 성공 기대가 실패했고, 수정 후 모드별 실제 연결과 `pg_stat_ssl` 검증이 통과했다. `require`는 CA가 없으면 암호화만 하며, 명시한 CA가 있으면 libpq 호환으로 CA를 검증한다. `prefer`는 암호화를 선호하되 서버가 지원하지 않으면 평문도 허용한다. 암호화와 인증서를 모두 강제하려면 `verify-full`을 선택해야 한다.

연결 폼에 선택적 `sslrootcert` PEM CA 파일과 `verify-ca` / `verify-full` 모드를 추가했다. CA 파일은 비동기로 읽고 PEM 단일/번들 인증서를 등록한다. 명시한 CA가 있으면 기본 신뢰 루트를 끄며, 읽기 오류·잘못된 PEM·빈 인증서 묶음을 오류로 처리한다. verify-ca는 인증서 체인만, verify-full은 체인과 호스트명을 검사한다. CA를 지정하지 않은 verify 모드는 native_tls 플랫폼 신뢰 루트를 사용한다. libpq의 `~/.postgresql/root.crt` 자동 탐색은 구현하지 않는다. TLS 취소 커넥터도 같은 설정을 재사용한다.

| PostgreSQL 서버 인증서: DNS localhost, IP SAN 없음 | 기대 / 실제 |
| --- | --- |
| require / prefer, 자체 서명 CA 미지정 | 암호화 연결 성공 |
| disable, hostssl-only 서버 | 실패 |
| verify-ca + 올바른 CA, localhost / 127.0.0.1 | 성공 |
| verify-full + 올바른 CA, localhost | 성공 |
| verify-full + 올바른 CA, 127.0.0.1 | 호스트명 검증 실패 |
| verify-ca / verify-full + 잘못된 CA 또는 CA 미지정 | 실패 |
| require + 명시적으로 잘못된 CA | 실패 |

모든 성공 연결에서 드라이버 `Session::execute`로 `SELECT ssl FROM pg_stat_ssl WHERE pid=pg_backend_pid()`를 실행해 Bool(true)를 확인한다. 서버 psql의 별도 점검도 `t|TLSv1.3|TLS_AES_256_GCM_SHA384`였다.

Mongo는 URI의 `tls` / `tlsCAFile`을 이미 지원했다. 선택적 폼 필드 `tls`(uri/true/false)와 `tlsCAFile`을 추가해 URI 기본 동작을 보존하며 CA 경로를 직접 지정할 수 있게 했다. `tls=true`는 기본적으로 CA/호스트명을 검증하며 PostgreSQL require처럼 검증을 생략하지 않는다. 자체 서명 CA 미지정, 잘못된 CA, 호스트명 불일치, requireTLS 서버에 tls=false는 모두 실패했다. 올바른 CA는 폼과 URI 양쪽에서 성공했다. `tlsAllowInvalidCertificates=true`는 사용자가 URI에 명시한 경우에만 검증 없는 TLS로 연결됨을 확인했다. TLS 비활성화와 폼 CA 동시 지정은 드라이버에서 거부한다.

Neo4j 라이브러리는 +ssc를 지원하지만 기존 드라이버 allowlist가 거부했다. `bolt+ssc` / `neo4j+ssc`를 허용하고, `bolt+s` / `neo4j+s`에만 선택적 `tlsCAFile`을 전달한다. TLS REQUIRED 서버에서 bolt는 실패, bolt+ssc는 검증 없이 성공, bolt+s는 CA 없으면 실패/올바른 CA면 성공, 잘못된 CA와 호스트명 불일치는 실패했다. CA를 +ssc 또는 평문 모드에 지정하면 모순된 검증 기대를 막기 위해 거부한다. 클러스터 routing URI는 실제 클러스터로 검증하지 않았다.

단독 드라이버 테스트 후 최초 전체 workspace 실행에서는 Rustls의 `ring` / `aws-lc-rs` feature 통합으로 Neo4j TLS가 CryptoProvider 선택 panic을 냈다. 기존 transitive 의존성인 rustls 0.23을 Neo4j 드라이버의 직접 의존성으로 명시하고, TLS 연결 전에 ring provider를 설치하도록 수정했다. 이미 다른 provider가 설치된 경우 기존 선택을 보존한다. 새 crate 버전은 추가하지 않았으며 Cargo.lock에는 driver-neo4j의 rustls 의존성 항목만 Cargo가 자동으로 추가했다. 수정 후 workspace 전체를 다시 실행해 세 드라이버 TLS 매트릭스를 포함하여 통과했다. 최초 실패 로그도 `/tmp/axdb-m2-workspace.log`에 남겼다.

Neo4j 신규 서버의 첫 실행에서는 3초 연결 제한 동안 첫 쿼리가 완료되지 않아 +ssc 테스트가 실패했다. 서버 로그의 실행 중 클라이언트 연결 해제와 컨테이너 내부 `cypher-shell RETURN 1` 완료를 확인한 후, 같은 3초 제한의 전체 TLS 매트릭스가 통과했다. 제한 시간 증가는 하지 않았다. 첫 실패 로그도 보존했다.

## 격리 환경과 재실행

테스트 CA·서버 인증서는 openssl로 scratch에 생성했다. CA 유효 기간은 2일이고 서버 SAN은 의도적으로 `DNS:localhost`만 가진다. 서버 개인키는 저장소에 넣지 않았으며 상위 scratch 디렉터리는 0700이다. Docker 내부에서 읽기 위해 만든 Mongo/Neo4j 키 복사본은 scratch 내부에 있다.

| 신규 컨테이너 | loopback 포트 | 서버 설정 |
| --- | --- | --- |
| axdb-test-pg-tls, postgres:16-alpine | 127.0.0.1:55433 | ssl=on; hostssl SCRAM 허용; hostnossl reject |
| axdb-test-mongo-tls, mongo:7 | 127.0.0.1:57018 | requireTLS; 테스트 CA; client certificate 선택 |
| axdb-test-neo4j-tls, neo4j:5-community | 127.0.0.1:57688 | Bolt TLS REQUIRED; 테스트 CA; client_auth NONE |

신규 Mongo TLS 컨테이너는 인증 없는 TLS 전송 검증 전용이며 loopback에만 노출했다. 기존 55432/57017/57687 테스트 서버 설정은 변경하지 않았다. 세 TLS 컨테이너는 후속 검증을 위해 실행 상태로 남겨 두었다.

Scratch: `/var/folders/_p/fy8jj5b95c38w41rttwnnll40000gn/T/axdb-m2-tls-92x_2b8h`.
환경 설정: `source /var/folders/_p/fy8jj5b95c38w41rttwnnll40000gn/T/axdb-m2-tls-92x_2b8h/env.sh`.
생성 명령 기록: `/tmp/axdb-m2-setup.py` (PG/Mongo), Neo4j 설정은 위 표와 동일하다.

환경 변수로 활성화되는 신규 실제 테스트는 다음과 같다. 변수 미지정 시 서버에 연결하지 않고 그 사실을 출력한다. URI/DSN은 테스트 전용 loopback 포트와 자격증명인지 assert한다.

| 테스트 | 활성화 변수 |
| --- | --- |
| real_mongodb_schema_graph / real_mongodb_schema_collection_limit | AXDB_TEST_MONGO |
| real_postgres_tls_modes | AXDB_TEST_PG_TLS |
| real_mongodb_tls_modes | AXDB_TEST_MONGO_TLS |
| real_neo4j_tls_modes | AXDB_TEST_NEO4J_TLS |

TLS 테스트 공통 변수: `AXDB_TEST_TLS_CA`, `AXDB_TEST_TLS_WRONG_CA`.
현재 TLS 연결 값: `AXDB_TEST_PG_TLS='host=localhost port=55433 user=postgres password=testpw dbname=axdb_test'`, `AXDB_TEST_MONGO_TLS='mongodb://localhost:57018'`, `AXDB_TEST_NEO4J_TLS='bolt://localhost:57688|neo4j|testpassword'`.

```sh
source /var/folders/_p/fy8jj5b95c38w41rttwnnll40000gn/T/axdb-m2-tls-92x_2b8h/env.sh
export AXDB_TEST_PG='host=127.0.0.1 port=55432 user=postgres password=testpw dbname=axdb_test'
export AXDB_TEST_NEO4J='bolt://127.0.0.1:57687|neo4j|testpassword'
export AXDB_TEST_MONGO='mongodb://axdb:testmongo@127.0.0.1:57017/?authSource=admin'
CARGO_TARGET_DIR=target-m2 cargo test --workspace
CARGO_TARGET_DIR=target-m2 cargo clippy --workspace --all-targets -- -D warnings
pnpm --dir ui test
```

## 검증 결과

- PostgreSQL TLS: 수정 전 실패 재현, 수정 후 실제 TLS 모드 매트릭스 통과.
- Mongo: 단위 6개, 기존 실제 계약, 실제 스키마 그래프, 실제 TLS 매트릭스 통과; 경계 확장 후 전체 결과는 아래 최종 기록 참조.
- Neo4j TLS: 초기 서버 첫 쿼리 지연 실패 기록 후 서버 준비 확인; 동일 제한 시간의 실제 TLS 매트릭스 통과.
- UI: 9개 파일 / 45개 테스트 통과.
- 최종 `CARGO_TARGET_DIR=target-m2 cargo test --workspace`: 종료 코드 0, harness 기준 80 passed / 0 failed / 0 ignored. PG·Neo4j·Mongo 기본 변수와 세 TLS 변수 및 두 CA 경로 변수를 모두 설정했다. `real_mongodb_schema_graph`, `real_mongodb_schema_collection_limit`, 세 DB의 `real_*_tls_modes`가 각각 실제 실행되어 통과했다.
- Redis 실제 연결 테스트는 이 M2 실행에 `AXDB_TEST_REDIS`가 없어 함수 내부에서 건너뛰었으며, harness 통과 수에 포함된다. Redis 실서버 검증 완료로 해석하면 안 된다.
- 최종 `CARGO_TARGET_DIR=target-m2 cargo clippy --workspace --all-targets -- -D warnings`: 종료 코드 0, 경고 없음.
- 확장한 Mongo 200문서 / 101컬렉션 경계 검사: 실제 서버 2개 테스트 통과. 생성한 fixture 및 전용 한도 검사 DB 정리 완료.
- `git diff --check`: 통과. 앱 실행·commit/push 없음.

로그는 `/tmp/axdb-m2-{pg-red,pg-green,pg-final,mongo-unit,mongo-live,mongo-bounds,neo-tls,neo-tls-warm,workspace,workspace-final,clippy,clippy-final,ui}.log`에 있다. source-only 확인과 실제 DB 증거를 혼동하지 않았고, browser/native 앱·실사용 서버·원격 배포는 검사하지 않았다. 샘플링은 드문 필드·관계를 놓칠 수 있으며 추론 관계는 무결성 제약을 의미하지 않는다.

## 변경 범위

직접 변경한 파일은 Mongo `src/lib.rs`, 신규 `src/schema.rs`, 신규 `tests/schema.rs` / `tests/tls.rs`; PG의 TLS 연결/manifest 부분과 신규 `tests/tls.rs`; Neo4j의 TLS 연결/manifest 부분, Cargo.toml의 기존 rustls 직접 의존성 및 신규 `tests/tls.rs`; 이 보고서다. 루트 Cargo.toml, src-tauri 등록, Redis, PG preview 및 기존 postgres.rs 테스트의 다른 작업자 변경은 보존했다. Cargo.lock의 다른 작업자 변경도 보존하며 Neo4j 의존성 1줄만 Cargo가 자동 추가했다. 코디네이터가 알린 PG cancellation drain 변경도 수정하지 않았다. 주석이나 capability 선언만 추가한 완료가 아니라 실제 `Session` 계약 호출로 엔티티·관계·browse와 실제 TLS 연결을 확인했다.

공식 동작 근거: [PostgreSQL libpq SSL 모드](https://www.postgresql.org/docs/18/libpq-ssl.html), [Mongo Rust 연결 옵션](https://www.mongodb.com/docs/drivers/rust/current/connect/connection-options/). 실제 사용한 구현은 설치된 native-tls 0.2, MongoDB 3.9.1 및 vendor/neo4rs의 TLS 경로를 확인했다.
