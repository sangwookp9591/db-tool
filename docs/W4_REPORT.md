# W4 관리 2 보고서

ADR-0004 C절, ADR-0003, ARCHITECTURE 및 W3_MANAGEMENT_REPORT를 읽고 W3의 가져오기·미리보기 영수증·권한·취소·플러그인 경로를 확장했다. 공유 main은 시작부터 여러 작업자의 tracked/untracked 변경이 있었으며 기존 변경을 되돌리지 않았다. commit/push, 사용자 앱·GUI, 사용자 DB·SSH, dist/·target/·target-rel은 건드리지 않았다. Rust 산출물은 target-w4에만 만들었다.

## 구현과 계약

`Capabilities.backup/roles`는 누락 시 false이고 protocol 1의 선택 메서드로 확장했다. 전송 payload는 기존 JSON 중간 디코더와 호환되는 base64 문자열을 사용한다. 최초 plugin 검증에서 MessagePack binary 요청 디코딩이 끊기는 문제를 발견했으며, 최대 8 MiB Restore 요청의 실제 프레임 encode/decode 회귀검사로 수정했다. PostgreSQL은 repeatable-read 안에서 스키마·시퀀스·제약·인덱스·뷰와 COPY TO STDOUT 데이터를 읽고 COPY FROM STDIN으로 복원한다. SQLite는 online backup API를 페이지 단위로 수행하고 복원 전 integrity_check를 한다. Mongo는 컬렉션 옵션·canonical Extended JSON NDJSON·인덱스, Neo4j는 노드·관계·제약·인덱스 Cypher와 원본 바이너리 파라미터를 보존한다. 화면 표본용 바이너리 절단값을 백업에 쓰지 않는다. `.axbackup`은 버전 magic, 길이가 있는 JSON manifest, zstd payload이며 SHA-256과 원본 길이를 복원 전에 확인한다. manifest 512 KiB, 해제 payload 8 MiB 상한을 강제하고 초과 시 실패한다.

복원 기본은 현재 연결된 새 DB/빈 대상이다. PostgreSQL·SQLite 덮어쓰기는 destructive이며 동일 바이트·세션·overwrite에 묶인 60초 이내 일회용 미리보기, 사람 승인, 정확한 연결 이름을 모두 요구한다. PostgreSQL은 실패 시 트랜잭션 롤백, SQLite는 online backup 정리, Mongo는 새로 만든 컬렉션 정리를 수행한다. 취소는 기존 run/cancel 경로이며 백업·복원은 `backup-progress`와 `backup_status`를 제공한다. 백업·복원·권한 적용 이력에는 행 값, 파일 payload, 비밀을 쓰지 않는다.

권한은 PostgreSQL 역할·멤버십·실효 테이블 권한 행렬, Mongo 사용자·역할·상속 권한, Redis ACL 사용자·명령·키 범위를 조회한다. Mongo 사용자 상세는 서버가 요구하는 exact-match usersInfo를 사용하고 credentials는 요청하지 않는다. Redis GETUSER의 password hashes는 버린다. PostgreSQL GRANT/REVOKE는 실트랜잭션 미리보기 후 롤백하고 일회용 승인 후 적용한다. Mongo는 현재 DB 사용자에 대한 역할 부여/회수 한 건을 계획·승인 적용하고 Redis는 ACL 계획만 제공한다. Neo4j Community는 비활성 이유를 반환한다. 권한 요약은 상속·PUBLIC·소유권·행 정책 등 실효 권한의 해석 한계를 명시한다.

W3 잔여는 Redis key/value 가져오기를 MSETNX 한 명령으로 원자적으로 추가하고 기존 키 충돌 시 전체를 거부한다. PostgreSQL 호스트 가져오기는 COPY 경로와 100행 단위 전송 진행률을 사용한다. 진행률은 applying이며 커밋 확인 전 완료로 표시하지 않는다. 새 progress 선택 메서드를 모르는 드라이버는 Unsupported일 때만 기존 import_rows로 폴백한다.

호스트 명령은 `backup_create({sessionId,runId}) → {manifest,bytes}`, `backup_status({sessionId})`, `restore_preview({sessionId,bytes,overwrite})`, `restore_apply({sessionId,runId,bytes,overwrite,approved,confirmation?})`, `roles({sessionId})`, `plan_grants/grants_preview({sessionId,changes})`, `grants_apply({sessionId,runId,changes,approved})`다. `GrantChange={role,namespace,object,privilege,revoke}`이며 PostgreSQL은 테이블 권한, Mongo는 `privilege=ROLE, namespace=역할 DB, object=역할 이름`, Redis는 `namespace=command, object="", privilege=명령 또는 @범주`다. W6 화면이 API 없이 대기 mock 상태여서 코디네이터 지시에 따라 BackupRoles·계약·mock 연결도 추가했다. 브라우저 mock은 실제 백업 파일·복원 성공을 가장하지 않고 거부한다.

MCP `backup_status`는 read, `plan_grants`는 preview 권한이다. 도구에는 적용 명령을 노출하지 않으며 AI 출력·계획 생성으로 실행 승인이나 영수증을 만들지 않는다.

## 검증

최종 native 계약 5/5와 workspace 바이너리 재빌드 뒤 `AXDB_DRIVER_DIR=target-w4/debug` plugin 계약 5/5가 통과했다. UI 전체 29 files/126 tests 실행과 tsc 통과, 호스트 archive 변조 거부·승인 누락·미리보기 재사용·연결 이름 확인 2/2 통과, 호스트 포함 전체 변경 crate의 clippy --all-targets -- -D warnings 통과다. PostgreSQL·Mongo·SQLite의 백업 → 빈 대상 복원 → 행 수와 SHA-256 payload 비교가 통과했고 Redis 실제 키 충돌 원자성·ACL 비밀 제외도 통과했다. Neo4j 초기 전체 검증은 ConnectionTimedOut으로 실패했다. 원인은 neo4rs가 연결 제한을 Bolt 읽기에도 재사용해 5초 연결 제한이 60초 복원 작업을 먼저 끊은 것이었다. Config에 operation-owned clone timeout 설정을 추가하고 일반 실행·미리보기·explain·백업·복원의 실제 작업 제한과 맞췄다. 연결 제한을 늘린 테스트나 retry-to-pass로 숨기지 않았으며 수정 후 최종 native 계약 5/5가 통과했다. `cargo test --manifest-path vendor/neo4rs/Cargo.toml --lib operation_timeout_clone_preserves_connection_settings`도 1/1 통과했다.

`cargo test -p dbcore -p axdb-driver-protocol -p axdb-driver-sdk -p driver-postgres -p driver-sqlite -p driver-mongodb -p driver-redis -p driver-neo4j`는 지정 실DB 환경에서 79개 reported tests가 통과했다. TLS 환경변수가 없는 내부 생략은 TLS 증거로 세지 않는다. 최대 크기 복원 프레임 회귀검사 1/1도 통과했다. 마지막 timeout/전송 수정 후 Neo4j·프로토콜 crate 재검증은 18개 reported tests가 통과했다.

지정 테스트 포트 55432/57687/57017/56379만 원본으로 사용하고, Neo4j 복원 대상은 worker 소유 `axdb-test-w4-neo4j-restore`의 127.0.0.1:57688이었다. 검증 후 worker 소유 컨테이너는 제거했다. Neo4j는 전송 실패 후에도 새 세션으로 W4 fixture 및 전용 복원 대상만 정리하도록 검증 코드도 보강했다. PG 왕복 테스트는 별도 axdb_test_w4_* DB와 axdb_test_role_w4_* 역할을 만들고 닫은 후 제거한다. Mongo 별도 테스트 DB도 제거하고 Redis는 자기 prefix 키만 제거한다. GUI나 운영 검증으로 주장하지 않는다. 이후 병렬 UI 작업이 추가된 최신 전체 실행은 33 files 중 1 failed, 131/132 tests 통과였으며 branched-menu.test.ts:139의 active-row 기대 1/실제 0 실패를 코디네이터에게 보고했다. W4 계약·에이전트 registry focused 실행은 2 files/19 tests 통과했다. 집중 검증 통과로 전체 UI 실패를 지우지 않는다.

## 잔여와 제한

전체 요청을 완료로 판정하지 않는다. Mongo·Neo4j의 기존 대상 덮어쓰기는 원자적 DB 교체를 보장하지 못해 명시적으로 거부하며, 8 MiB 초과 백업의 청크 전송·파일 스트리밍은 남아 있다. 백업 진행률은 시작/종료 중심이고 행 단위 중간 진행률은 PostgreSQL COPY 가져오기 경로에서 제공한다. PostgreSQL 백업은 일반 테이블·시퀀스·제약·인덱스·뷰 범위이며 사용자 함수/타입/트리거/파티션/foreign table/identity/generated 등은 불완전한 파일을 만들지 않고 거부한다. 모든 PG 객체 종류의 권한 행렬·GRANT와 Neo4j Enterprise 권한 적용은 구현 범위 밖에 남아 있다. Mongo·Neo4j의 백업은 동시 쓰기를 차단하지 않으므로 일관된 백업을 위해 원본 쓰기를 중지해야 한다. 외부에서 받은 백업의 DB 정의는 신뢰할 수 있는 출처인지 사람이 확인해야 하며 UI와 미리보기에서 이를 안내한다. 대용량 성능·실제 앱·네이티브 파일 대화상자·운영 DB·Enterprise·TLS는 검증하지 않았다.
