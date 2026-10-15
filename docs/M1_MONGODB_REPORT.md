# M1 MongoDB 드라이버 검증 보고

검증일: 2026-10-15. 대상: `127.0.0.1:57017`, MongoDB 7.0.45, 테스트 DB `axdb_test`.

## 구현

- 공식 Rust 드라이버 `mongodb 3.9.1`: `cargo search mongodb --limit 1`, `cargo info mongodb`로 crates.io 최신 안정판을 확인했다. [패키지](https://crates.io/crates/mongodb/3.9.1), [공식 API](https://docs.rs/mongodb/3.9.1/mongodb/).
- 새 `driver-mongodb` crate를 workspace와 앱 registry에 등록했다. Document 패러다임, `mongo` 명령 언어, `json` 에디터 모드를 선언했다. URI, database, user, password(secret), authSource를 제공한다.
- Extended JSON 문서 하나를 BSON으로 읽고 `runCommand`로 실행한다. 셸 문법, 빈 명령, 중복 최상위 키, 다중 문서는 오류로 처리한다. 기본 쿼리와 오류 메시지에 명령 예시를 제공한다.
- 커서의 `firstBatch`와 `getMore`를 같은 세션으로 이어 읽는다. 한도 판별용 문서 하나를 추가로 확인하고 `truncated`를 표시한다. 남은 커서는 `killCursors`로 정리한다. 공식 드라이버 규칙에 따라 `killCursors`에는 명시적 세션을 넘기지 않는다.
- 문서 배열이 전체 필드를 보존한다. 처음 100개 문서의 최상위 필드 합집합으로 표를 만들고 없는 필드는 Null로 채운다. 필드 순서를 보존하며 Int64·Decimal128은 문자열로 전달한다.
- ObjectId, 정수, Double/NaN/Infinity, Decimal128, String, Bool, Null, DateTime, Timestamp, Binary, Array, Document, Regex, JavaScript, MinKey, MaxKey를 기존 typed Value로 변환한다. Bytes는 기존 드라이버와 같이 길이와 제한된 hex 미리보기를 제공한다.
- databases → collections → Fields/Indexes를 탐색한다. Fields는 문서 100개의 관측 타입을 합치고, 컬렉션에 추정 문서 수·Browse·OpenQuery를 제공한다. Browse는 `_id` 순으로 skip/limit 페이징한다.
- 명령 오류의 서버 code/codeName을 detail에 포함한다. 인증/연결/시간 초과를 구분하고 세션 비밀번호를 오류에서 가린다. 쓰기 명령의 writeErrors/writeConcernError를 성공으로 표시하지 않는다.

## 보안과 결정

- 읽기 전용은 명령 허용 목록과 집계 단계 허용 목록으로 강제한다. `$out`, `$merge`, 알 수 없는 단계, `$lookup`/`$unionWith`/`$facet`의 중첩 쓰기 단계도 거부한다. `$db`, lsid, txnNumber, autocommit 등 실행 세션 재지정을 거부한다.
- URI에 자격 증명을 넣으면 호스트의 secret 필드 저장·마스킹 경계를 우회하므로 별도 User/Password 필드를 요구한다. 테스트 환경 변수의 인증 URI는 fixture 클라이언트만 사용하고 실제 드라이버 연결에는 분리한 필드를 전달한다.
- 취소는 `Abort`로 선언했다. `killOp` 권한을 요구하지 않는다. 취소/시간 초과는 호출자 대기를 중단하며, 공식 드라이버의 cancellation safety 지침에 따라 별도 task가 응답을 끝까지 처리한다. 이후 추가 getMore를 멈추고 커서를 정리하며 읽기 명령의 서버 작업에는 `maxTimeMS`를 적용한다. 드라이버 future를 직접 버리지 않는다. 이미 전송된 쓰기는 취소만으로 롤백되지 않으며 결과가 불확실할 수 있다.
- preview_changes=None, schema_graph=false, filtered_browse=false다. 추가된 dbcore 계약 필드는 MongoDB 리터럴에 반영했으며 지원하지 않는 필터 탐색은 거부한다.
- 테스트 fixture는 실행마다 고유 ObjectId 접미사가 있는 `axdb_test.m1_types_*`, `axdb_test.m1_pages_*`를 만들고 정리한다. 앱 검증 fixture는 별도 `m1_bridge_types`, `m1_bridge_pages`다. 사용자 DB 포트에는 접속하지 않았다. 커밋하지 않았다.

## 검증

| 검증 | 결과 |
|---|---|
| 환경 변수 설정 후 `cargo test -p driver-mongodb` | PASS: 단위 4개 + 실DB 통합 1개 |
| Int64 최댓값, Decimal128 정확성, ObjectId, DateTime, 중첩 순서, 배열 등 전체 지정 타입 | PASS |
| 여러 배치 getMore, row_limit 잘림, 정확히 한도인 경우 | PASS |
| 문법 오류, 서버 code/codeName, 잘못된 비밀번호, 닫힌 테스트 포트 | PASS |
| 긴 `$range` aggregate timeout·취소 후 같은 세션 재사용 | PASS |
| 읽기 전용 insert/집계 쓰기 차단과 비영속성 | PASS |
| 탐색·필드 타입·인덱스·browse 두 페이지 | PASS |
| 쓰기 affected/message, 중복 키 쓰기 실패 | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS (최종 코드, 27.75s) |
| 실DB 계약 테스트 두 프로세스 동시 실행 | PASS: 1.69s / 1.70s, 실행별 고유 fixture |
| `pnpm --dir ui build` | PASS |
| `cargo build -p axdb` | PASS (debug=0, 최종 빌드 2m58s) |
| 실제 앱 MCP 브리지 | PASS: 8/8 |
| 네이티브 Table/JSON 화면 | PASS: 실제 Run 후 두 뷰 전환 및 화면 확인 |

명령은 모두 `CARGO_TARGET_DIR=target-m1 CARGO_BUILD_JOBS=4`로 실행했다. 앱 빌드는 공유 디스크 여유를 고려해 `CARGO_PROFILE_DEV_DEBUG=0`을 추가해 디버그 심볼을 제외하며 저장소 설정은 바꾸지 않았다. 실DB 테스트 환경 변수는 작업 명세의 격리 MongoDB URI만 허용한다. 환경 변수가 없으면 통합 테스트는 서버에 접속하지 않는다.

동시 실행 진단에서 서버 JavaScript가 `code=11601/codeName=Interrupted`를 반환하는 경우를 확인했다. 이를 Timeout으로 덮어쓰지 않고 Cancelled로 분류한다. 시간 제한 증거는 명세가 허용한 대량 `$range` 집계를 사용하며, 서버 `maxTimeMS=1`의 `MaxTimeMSExpired` detail을 검사한다. 시간 제한 증가나 실패 후 통과까지 재시도하는 방식은 사용하지 않았다.

공식 드라이버 README의 [cancellation 주의사항](https://github.com/mongodb/mongo-rust-driver#warning-about-timeouts--cancellation)에 따라 실행 task는 응답을 끝까지 처리한다. 읽기 전용 insert 차단과 별개로 이미 취소된 토큰의 insert 요청도 서버에 전송되지 않는지 검사했다.

## 계약 충분성

새 드라이버의 연결·manifest·탐색·실행·페이징·문서 및 표 결과는 기존 Driver/Session/ResultSet/Value 계약으로 표현할 수 있다. M1은 `ui/`, `crates/dbcore`, 기존 드라이버를 수정하지 않았다. 공유 작업 공간에서는 다른 워커의 C1/V1/A1 변경이 해당 경로에 존재하므로 저장소 전체 `git diff --stat -- ui crates/dbcore ...`가 비어 있다고 주장하지 않는다.

문서 배열은 전체 구조를 보존하지만 공통 UI는 결과 뷰를 Graph → Table → JSON 순서로 구성하므로 첫 화면은 Table이다. `ResultSet`에는 기본 뷰 선호를 전달하는 필드가 없다. 문서 기본 선택·JSON 구문 강조·에디터 placeholder는 UI의 일반 개선 사항이며 M1이 계약이나 UI를 고치지 않았다. 기본 쿼리와 파싱 오류에는 요구된 Extended JSON 예시를 제공한다.

명세 작성 시 JSON 뷰는 `<pre>` 목록이었다. 현재 공유 작업 공간에는 V1의 DocumentTree 변경이 있으므로 실제 앱 검증은 빌드 시점 UI 상태를 별도로 기록한다. 이 변경을 M1의 구현 성과로 계산하지 않는다.

## 실제 앱 검증

격리 설정 경로 `/tmp/axdb-m1-native`와 `AXDB_AGENT_BRIDGE=1`로 `target-m1/debug/axdb`를 실행했다. `save_secret=false` 프로필에 기존 Session credential 다이얼로그의 AX set-value로 테스트 비밀번호를 전달해 연결했다. 그 뒤 MCP `open_connection`이 기존 세션을 재사용하는 것과 explore → run_query → browse를 확인했다. UI/브리지에 자격 증명 인자를 추가하지 않았다.

**명세 대비 검증 예외:** 다이얼로그 없는 연결을 위해 코디네이터가 지정한 임시 키체인 bootstrap을 먼저 시도했지만 macOS SecurityAgent가 로그인 키체인 암호를 요구했고 연결은 `Cannot read keychain credential`로 실패했다. 로그인 암호는 입력하지 않았다. 거부 버튼 조작은 window_not_found로 성공 여부를 확인하지 못했다. 코디네이터 승인 후 기존 세션 다이얼로그를 사용하는 경로로 검증을 완료했으므로, 다이얼로그 없는 fresh MCP 연결은 검증하지 못했다.

브리지 검증은 manifest, open_connection, databases/collections/Fields/Indexes 탐색, Int64·Decimal128·문서/표 정확성, row_limit 잘림, 읽기 전용 insert 거부, browse offset/limit, 쿼리 탭 열기의 8개 항목이다. 브리지 결과 수신과 실제 탭 렌더링은 구분했다. 네이티브 화면에서 Run을 눌러 결과를 표시하고 Table/JSON 탭을 선택해 스크린샷을 확인했다. 현재 UI의 JSON 트리는 동시 V1 작업 결과이며 M1이 구현한 기능으로 계산하지 않는다.

- [브리지 검증 원자료](../bench/results-m1-mongodb-bridge.json)
- [네이티브 Table](../bench/m1-mongodb-table.png)
- [네이티브 JSON](../bench/m1-mongodb-json.png)

테스트 세션을 MCP로 닫고 이 워커의 앱 프로세스만 종료했다. 임시 키체인 항목 `profile:m1-mongodb`를 삭제했고, 최종 설정의 두 프로필 모두 save_secret=false다. profiles.json/axdb.log/agent-audit.jsonl에 테스트 비밀번호가 남지 않았음을 확인했다.

## 범위와 남은 한계

서버 측 killOp 취소, 변경 미리보기, 필터 탐색, 스키마 그래프, 셸 문법, 트랜잭션, JSON 전용 UI는 추가하지 않았다. 단위/실DB 테스트와 별도로 실제 앱 브리지·렌더링을 확인했다. 다이얼로그 없는 fresh MCP 연결, 서버 측 killOp, 다른 OS, TLS·SRV·replica set·sharded cluster는 검증하지 않았다.
