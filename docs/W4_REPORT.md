# W4 관리 2 보고서

최신 후속 결과는 문서 하단의 W4b 절에 기록했다. 앞부분은 최초 W4 종료 시점의 기록이다.

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

이 절은 최초 W4 종료 시점의 잔여이며, 후속 보완 결과는 아래 W4b 절에 있다.

전체 요청을 완료로 판정하지 않는다. Mongo·Neo4j의 기존 대상 덮어쓰기는 원자적 DB 교체를 보장하지 못해 명시적으로 거부하며, 8 MiB 초과 백업의 청크 전송·파일 스트리밍은 남아 있다. 백업 진행률은 시작/종료 중심이고 행 단위 중간 진행률은 PostgreSQL COPY 가져오기 경로에서 제공한다. PostgreSQL 백업은 일반 테이블·시퀀스·제약·인덱스·뷰 범위이며 사용자 함수/타입/트리거/파티션/foreign table/identity/generated 등은 불완전한 파일을 만들지 않고 거부한다. 모든 PG 객체 종류의 권한 행렬·GRANT와 Neo4j Enterprise 권한 적용은 구현 범위 밖에 남아 있다. Mongo·Neo4j의 백업은 동시 쓰기를 차단하지 않으므로 일관된 백업을 위해 원본 쓰기를 중지해야 한다. 외부에서 받은 백업의 DB 정의는 신뢰할 수 있는 출처인지 사람이 확인해야 하며 UI와 미리보기에서 이를 안내한다. 대용량 성능·실제 앱·네이티브 파일 대화상자·운영 DB·Enterprise·TLS는 검증하지 않았다.


## W4b — 파일 스트리밍과 실사용 복원

W4의 8 MiB 제한은 새 파일 경로에서 제거했다. PG COPY, Mongo canonical Extended JSON, Neo4j 노드·관계 레코드, SQLite online backup을 `Session::backup_stream`과 protocol 1의 기존 chunk envelope로 전송한다. 청크는 64 KiB, SDK 출력 큐는 8칸이며 호스트는 즉시 zstd 파일에 쓰고 SHA-256을 누적한다. `.axbackup` v2는 payload 뒤에 manifest·길이·magic trailer를 기록한다. 복원은 파일 전체를 스트리밍 검증한 뒤 익명 임시 파일에 고정하고, SDK의 익명 staging 파일로 청크 요청을 보낸다. 취소·실패 시 부분 출력과 업로드를 정리하며, 백업은 기존 목적 파일을 성공 전까지 교체하지 않는다. 일반 응답의 256 MiB 한도와 16 MiB 프레임 한도는 유지하고, 형식·길이를 검증한 백업 청크만 누적 응답 한도에서 제외한다. manifest 512 KiB와 단일 레코드 128 MiB 한도는 남는다. 전체 아카이브 크기 한도는 아니다.

UI는 `backup_create_file`에 저장 대화상자 path/token을 전달하고 manifest만 받는다. `restore_pick_file`은 `{token,name,manifest}`를 반환하며 `restore_file_preview/apply/discard`는 토큰을 사용한다. UI에서 File.arrayBuffer와 number[] 백업 전송을 제거했다. 선택 토큰은 세션에 묶이고 5분 동안 유효하며, 미리보기 영수증은 기존 60초 일회용 정책과 동일하다. 승인에는 payload SHA-256·형식·길이·overwrite·새 DB 이름을 묶는다. read-only 차단, 사람 승인, 덮어쓰기의 정확한 연결 이름 확인을 유지한다. v1 파일과 기존 bounded 명령은 호환 경로로 남겼으며 AI/MCP에는 복원 실행 도구를 추가하지 않았다.

PG는 카탈로그로 함수·프로시저, enum·composite·domain, 트리거와 활성 상태, identity·generated 컬럼, 다단계 파티션을 재구성한다. SECURITY DEFINER 루틴은 복원 계정으로 권한이 상승하지 않도록 원래 소유자 역할을 유지하며, 그 역할이 필요하다는 경고를 남긴다. COPY의 날짜·interval·float·bytea·timezone·문자열 형식은 작업 트랜잭션에서 고정하고 원래 세션 설정을 보존한다. [PostgreSQL COPY 문서](https://www.postgresql.org/docs/current/sql-copy.html)의 DateStyle/IntervalStyle 권고를 따른다. 생성 의존성은 `pg_depend`, 파티션 부모, column default와 view rewrite 의존성을 함께 정렬한다. COPY는 leaf/ordinary table만 읽고 generated 컬럼을 제외하며, 제약·인덱스·트리거는 데이터 뒤에 만든다. foreign table과 extension은 백업 실패 대신 manifest의 재생성 경고에 기록한다. RLS, 임시 테이블, legacy 테이블 상속 및 실제 순환 생성 의존성은 여전히 명시적으로 거부한다. foreign server·extension의 재설치나 소유권·모든 객체 권한 이전을 자동으로 수행하지 않는다.

Mongo는 기존 DB를 보존한 채 승인된 새 이름에 복원하는 한 흐름을 제공한다. Neo4j Enterprise도 새 DB 생성 후 복원하는 경로를 제공하며 기존 대상 덮어쓰기는 둘 다 계속 거부한다. Neo4j Community는 추가 표준 DB를 만들 수 없으므로 `restore_options`가 그 이유와 별도 서버의 빈 대상에 복원한 뒤 검증·연결 교체 안내를 반환한다. 이 차이는 [Neo4j 공식 DB 관리 문서](https://neo4j.com/docs/operations-manual/current/database-administration/)에 따른다. Enterprise 실서버 검증은 하지 않았다.

branched-menu의 active-row 문제는 렌더를 지연하면 클릭 직후 0개, commit 뒤 1개가 되는 경계로 재현했다. 클릭 완료를 렌더 완료로 가정했던 테스트를 실제 active-row DOM 조건 대기로 바꿨다. 60ms 지연 scheduler를 회귀 조건으로 유지하며 timeout 증가·retry-to-pass는 사용하지 않았다. 브라우저를 닫기 전 페이지를 about:blank로 이동해 Vite 요청도 정리한다.

### 검증

`CARGO_BUILD_JOBS=4`, `CARGO_TARGET_DIR=target-w4b`로 workspace 바이너리를 빌드한 뒤 streaming native 4/4와 `AXDB_DRIVER_DIR=target-w4b/debug` plugin 4/4를 통과했다. 이후 COPY 형식 고정을 추가한 PG 집중 native 1/1·plugin 1/1도 통과했다. 기존 W4 계약은 native 5/5·plugin 5/5, core/protocol/SDK 및 5개 드라이버의 crate 테스트는 80 reported tests 통과다. 환경변수가 없어 내부에서 생략한 TLS 테스트는 TLS 증거로 세지 않는다. 호스트 archive·변조·일회용 승인·대상 변경·연결 이름 검사는 2/2, 백업 청크만 누적 응답 한도를 우회하는 경계는 1/1, 새 DB용 config clone 보존은 vendor 단위 1/1 통과다. 관련 crate와 호스트의 `clippy --all-targets -- -D warnings`, tsc도 통과했다.

공유 UI의 최종 실행은 35 files passed/1 skipped, 138 tests passed/1 skipped를 3회 연속 통과했다. 생략된 columnar 수동 성능 테스트를 기능 검증으로 주장하지 않는다. 이전 단독 재현 과정의 paging 5초 테스트와 종료 hook 30초 실패도 기록해 두며, 그 실행은 통과로 세지 않는다. 현재 UI에서는 60ms 지연 scheduler를 둔 active-row 검증을 포함한 전체 suite의 3회 결과를 사용한다. L1의 MessagePack 채널 변경에 맞춰 기존 `plugin_stream_bench.rs` 수신부만 `Channel<Response>`·Raw·rmp-serde로 갱신했고, 실제 Raw 채널 순서·셀 보존 경계 2/2도 통과했다. 수동 10만 행 채널 성능 수치는 재측정하지 않았다.

지정 PG의 `dbtool_test`를 worker 전용 `axdb_test_w4b_*` DB로 복제했다. `e2e.big` 250,000행과 새 `w4b.large` 120,000행(열 payload 153,600,000 B, 실제 relation 크기 164,110,336 B)을 백업하고 빈 DB에 복원하여 테이블별 행 수와 순서 고정 행 체크섬을 비교했다. 함수·프로시저, enum·composite·domain, 트리거, identity 상태, SERIAL 소유권, generated 전용 테이블, 다단계 파티션과 상속 CHECK, 의존 뷰·materialized view, SECURITY DEFINER 원래 소유자 및 foreign table·extension 경고가 통과했다. 독일식 DateStyle, sql_standard interval, float 출력 정밀도 0에서도 왕복하며 백업 후 원래 세션 설정이 보존됨을 확인했다.

원자료는 [native RSS](../bench/results-w4b-streaming-native.json), [plugin RSS](../bench/results-w4b-streaming-plugin.json)다. 각 small/large는 같은 테스트 프로세스에서 11,140,134/166,193,029 B payload의 백업·파일 검증·복원을 약 100ms 간격으로 샘플링한 피크 RSS다. native는 호스트와 드라이버가 같은 프로세스다.

| 경로 | 작은 payload RSS | 큰 payload RSS |
|---|---:|---:|
| native 공통 프로세스 | 19,251,200 B | 19,693,568 B |
| plugin 호스트 | 23,822,336 B | 21,807,104 B |
| plugin PG 드라이버 | 17,498,112 B | 19,054,592 B |

payload는 약 15배였고 RSS는 데이터 크기에 비례해 증가하지 않았다. 이 수치는 지정 fixture·환경의 결과이며, 테스트는 RSS 측정값이 0이거나 추가 RSS가 64 MiB를 넘으면 실패한다. SQLite 12 MB 파일, 취소 시 기존 파일 보존·부분 파일 삭제, archive 변조 거부, Mongo 새 DB의 인덱스·바이너리·SHA-256 왕복과 기존 sentinel 보존, Neo4j 빈 대상과 Community 제약도 native/plugin에서 확인했다.

worker DB·역할·컬렉션은 테스트 종료 시 제거했고, worker 소유 `axdb-test-w4b-neo4j-restore`(57688) 컨테이너를 제거했다. 사용자 앱·GUI·운영 DB·사용자 SSH·실제 네이티브 파일 대화상자·Enterprise·TLS는 검증하지 않았다. Mongo·Neo4j의 동시 쓰기 중지, 신뢰하는 백업만 복원, 필요한 extension·foreign 정의·소유자 역할의 별도 준비는 계속 필요하다. commit/push하지 않았고 dist/·target/·target-rel을 사용하지 않았다. 시작 시 기존 dirty는 R2 환경 JSON 한 파일이었으며, 종료 시 다른 lane의 UI·스트림·문서 변경을 그대로 보존했다.
