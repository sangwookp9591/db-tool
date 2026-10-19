# W3 관리 기능 보고서

ADR-0004 C절, ADR-0003과 ARCHITECTURE를 기준으로 이전 워커의 부분 변경을 이어서 구현했다. 시작 시 공유 트리에 이미 다른 배치와 W3의 tracked/untracked 변경이 있었으며 이를 보존했다. UI 파일은 이번 재시작 워커가 수정하지 않았고 W6에 커맨드·타입을 Orca 메시지로 전달했다. commit/push, 사용자 DB·SSH, 설치된 앱·GUI, dist/target/target-rel 변경은 하지 않았다.

완료 판정은 **부분 구현 / ADR 전체 충족 아님**이다. 아래 핵심 경로와 집중 검증은 통과했으나 Redis 가져오기, PG COPY 경로, 중간 행 진행률이 남아 있어 전체 요청 완료로 처리하지 않는다. 호스트 포함 clippy와 UI 전체 테스트도 통과하지 않았다.

## 구현

가져오기는 CSV/TSV 레코드, JSON 배열 원소, NDJSON 행, XLSX 셀을 순차적으로 파싱한다. 입력 파일 8 MiB, 10,000행, 512열, XLSX 압축 해제 32 MiB/2,048항목으로 제한한다. 파일 바이트와 변환된 편집 요청은 상한 안에서 메모리에 보관하며 무제한 파일을 DB에 직접 스트리밍하는 구현은 아니다. 정수·불리언·실수·텍스트를 결정적으로 추론하고 JSON 명시 매핑을 지원한다. 숫자 식별자의 앞자리 0과 큰 정수·중첩 JSON 원문을 보존하며, 변환 오류의 원본 행 번호와 최대 100개 오류를 반환한다. 오류가 하나라도 있으면 적용용 편집 목록을 비워 부분 가져오기를 막는다. 처음 100개 변환 행을 보여 주고 DB 미리보기는 실제 제약 검사 후 롤백한다.

`import_plan` → `import_preview` → `import_apply`는 기존 edits를 재사용한다. 적용에는 동일 세션·요청에 묶인 60초 이내 일회용 미리보기 영수증이 필요하다. PG/SQLite는 새 테이블 생성과 삽입도 같은 트랜잭션에 넣고, 생성 SQL을 미리보기에 포함한다. 임의 타입 SQL 대신 매핑 타입 허용 목록을 쓰고 기존 테이블은 덮어쓰지 않는다. PG 가져오기는 세션 잠금을 유지한 채 열린 사용자 트랜잭션을 거부해 실제 커밋과 SAVEPOINT 해제를 혼동하지 않는다. 취소는 기존 run/cancel 경로를 사용하며 `import-progress`는 시작·미리보기 완료·커밋·실패 단계와 0/전체 행 수를 알린다. 행별 중간 진행 이벤트는 없다. 이력에는 입력값을 기록하지 않는다.

`schema_snapshot`/`ddl_render` capabilities와 protocol 1 선택 메서드, 버전 1 정규화 스냅숏, 호스트 diff를 연결했다. 추가는 write, 삭제·변경은 보수적으로 destructive로 분류한다. PG는 범위 미지정 시 일반 스키마들을 조회하며 테이블·컬럼·뷰·인덱스·제약을 수집한다. SQLite는 sqlite_master와 컬럼 메타데이터를 사용한다. PG 컬럼/기본값/NULL 변경·제약 삭제, SQLite 컬럼 추가·삭제, 인덱스·뷰 변경의 DDL을 만든다. Neo4j는 라벨·관계 타입·제약·인덱스를 비교하고 제약·인덱스 Cypher를 생성한다. Mongo는 컬렉션 옵션/검증과 인덱스를 비교하고 인덱스 명령을 생성한다. `snapshot_encode`/`snapshot_decode`와 기존 안전한 export 경로로 파일 왕복을 제공한다. 불러온 정의는 신뢰하지 않으며 생성 DDL도 미리보기·승인을 거친다.

마이그레이션은 Flyway `V*__*.sql`, Prisma `*/migration.sql`, plain `NNN_*.sql`을 제한된 폴더 탐색으로 찾고 숫자 버전 순서로 정렬한다. Flyway/Prisma/Rails/axdb 이력 테이블을 읽어 적용·대기를 구분한다. SQL 파일 1 MiB/폴더 SQL 8 MiB/탐색 항목 5,000개를 제한하고 symlink는 읽지 않는다. 미리보기 후 동일 SQL의 일회용 영수증과 쓰기 권한을 확인하며, 파괴적 SQL에는 연결 이름 확인 문구를 요구한다. PG/SQLite의 SQL 적용과 axdb 이력 기록은 한 트랜잭션이다. 트랜잭션 탈출 제어문은 호스트와 드라이버가 거부한다. 기존 Flyway/Prisma 이력은 변경하지 않는다.

MCP `schema_diff`는 read, `plan_import`/`plan_migration`은 preview 권한으로만 제공하고 적용 도구는 노출하지 않는다. AI가 만든 계획 자체는 커밋이나 승인으로 취급하지 않는다.

## W6 계약

- `import_plan({filename, bytes, nodeId, mapping?, createTable?}) → ImportPlan`
- `import_target({sessionId, namespace?, name}) → nodeId`: 스냅숏 dialect로 PG/SQLite 노드 ID 생성. PG namespace 기본 public.
- `EditRequest.create_table?: Column[]`: 새 테이블의 매핑 타입. 생략 시 기존 동작 그대로.
- `import_pick_file`, `import_preview`, `import_apply`, `schema_snapshot`, `schema_diff`, `ddl_render`, `snapshot_encode`, `snapshot_decode`, `migration_scan`, `migration_preview`, `migration_apply`는 기존 W3 계약 유지.
- 진행 이벤트 `import-progress`: `{run_id, phase, completed, total}`. 취소는 `cancel({runId})`.

## 검증

검증은 전용 `target-w3`와 지정된 로컬 테스트 DB만 사용했다. 디스크 부족 시 자기 `target-w3/debug/incremental`만 비우고 이후 `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=4`를 사용했다.

- `cargo test -p dbcore -p driver-postgres -p driver-sqlite -p driver-neo4j -p driver-mongodb`: 지정 실DB 환경변수로 전체 통과. TLS 환경변수는 없으므로 TLS 테스트 내부 생략을 TLS 검증으로 세지 않는다.
- 마지막 PG 사용자 트랜잭션 경계 보완 후 `cargo test -p driver-postgres`도 지정 실DB에서 전체 통과했다.
- `cargo test -p axdb --lib management::`: 11/11 통과. 다섯 파서, 희소 XLSX 상한, 숫자 정밀도, 매핑/원본 오류 행, 승인 재사용 방지, 이력 비공개, 폴더 순서/중복, 실제 SQLite 마이그레이션 미리보기·승인·이력·파괴적 확인 검증.
- `cargo build --workspace --bins`: 통과.
- `AXDB_DRIVER_DIR=target-w3/debug cargo test -p axdb --test plugin_contracts management`: 최종 workspace 바이너리 재빌드 후 실DB/SQLite 플러그인 계약 4/4 통과. 생성 SQL/영향 행 수 배열 길이 회귀 검사에서 오래된 SQLite 바이너리를 검출했으며 소스 변경 완료 후 순차 재빌드로 해결했다. 새 테이블 미리보기 롤백/적용, 제약 오류 전체 롤백, 스냅숏, 생성 DDL 미리보기/적용, 마이그레이션 실패 롤백 확인.
- `cargo clippy -p dbcore -p driver-postgres -p driver-sqlite -p driver-neo4j -p driver-mongodb --all-targets -- -D warnings`: 통과.
- 마지막 PG/SQLite 경계 보완 후 두 crate의 `clippy --all-targets -- -D warnings`도 재검사하여 통과했다.
- 호스트 포함 clippy는 W1 `net/tls.rs` nonminimal_bool, `net/ssh.rs` items_after_test_module로 실패했다. W3 진단은 없지만 호스트 전체 통과로 보고하지 않는다.
- `pnpm --dir ui exec tsc --noEmit`: 통과.
- `pnpm --dir ui test`: 118개 테스트는 통과했지만 온보딩 afterAll 정리 훅의 30초 시간 초과로 24개 파일 통과/1개 파일 실패였다. 시간 제한을 늘리거나 강제 종료로 숨기지 않았다.
- `git diff --check`: 통과.

## 제한과 미검증

Mongo 단독 서버는 트랜잭션을 지원하지 않아 원자적 가져오기를 명시적으로 거부한다. 실제 replica set에서 insertMany의 적용·취소는 이번 환경에서 검증하지 못했다. Redis는 import/schema capabilities를 켜지 않았고 가져오기를 제공하지 않는다. SQLite 타입·기본값·NULL 속성 변경이나 테이블 옵션 변경은 자동 재구성하지 않고 명시적 마이그레이션을 요구한다. 모든 DB 확장 객체나 복잡한 의존 관계의 DDL 복원까지 보장하지 않는다. PostgreSQL 가져오기는 기존 바인딩 INSERT 경로이며 COPY 고속 경로를 추가하지 않았다. 미리보기의 시퀀스 소비·트리거 외부 부작용은 기존 preview와 같은 한계다. UI/실제 앱, 드래그앤드롭, 실제 키체인, 운영 DB, TLS 실연결, 대규모 성능 측정은 수행하지 않았다.
