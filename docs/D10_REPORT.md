# D10 쿼리 결과 편집

작업 브랜치는 `d10-result-edit`, 시작 커밋은 `0c6c75d`이며 시작 작업 트리는 깨끗했다. 메인 작업 트리와 설치된 `/Applications/AxDB.app`은 수정하지 않았다. 검증 대상은 지정된 루프백 테스트 DB와 임시 SQLite 파일이다.

PostgreSQL은 prepared RowDescription의 table OID/attnum, MySQL·MariaDB는 column definition의 schema/org_table/org_name, SQLite는 `SQLITE_ENABLE_COLUMN_METADATA`의 database/table/origin 정보를 사용한다. SQL AST는 단일 직접 테이블 SELECT인지와 JOIN·집계·DISTINCT·GROUP BY·집합 연산 등의 제외 조건만 검사하며 원본 컬럼을 추측하지 않는다. 실제 카탈로그에서 뷰/생성 컬럼/기본 키를 확인하고, 기본 키 전체가 결과에 있어야 한다. 결과 컬럼은 원본 이름과 인덱스로 매핑하며 계산 컬럼과 중복 컬럼 이름은 안전하게 제한한다.

QueryTab은 [ADR-0004](adr/0004-ax-native-workbench.md)의 기존 EditableTable, 변경 미리보기, 승인 Dialog, host의 60초·요청 지문·일회성 승인 계약을 사용한다. 편집 표의 전체 행 렌더링을 고려해 쿼리 결과 편집은 1,000행까지로 제한하며 상한 초과 시 이유를 표시한다. 결과에서는 셀 수정만 제공하며 추가/삭제는 기존 browse 기능에 남긴다. 변경 대기 중 새 쿼리 실행을 막고 변경 취소를 제공한다. 적용 뒤에는 결과를 만든 SELECT와 파라미터를 다시 실행한다. 읽기 전용 연결/권한과 AI·MCP 경로는 편집을 허용하지 않는다.

SQL WHERE는 키와 원래 조회 값을 NULL-safe 비교한다. 영향 행 수가 1이 아니면 기존 트랜잭션/세이브포인트 경로가 전체 편집을 롤백한다. 잘린 binary 표시는 원래 값 비교에서 제외하며, DB가 equality를 지원하지 않는 타입은 미리보기에서 오류가 날 수 있다. 키·바인딩·컬럼 검증과 rollback 실패 시 세션 폐기는 유지한다.

MongoDB는 최상위 원본 필드 inclusion/exclusion projection 또는 projection 없는 find와 실제 컬렉션, `_id`를 확인한다. 미리보기는 쓰기 없이 현재 문서와 원래 값을 비교하여 updateOne 계획 및 변경 전·후를 계산한다. 적용은 `_id`와 원래 값의 원자적 조건부 updateOne이다. 단독 서버에서도 부분 적용이 없도록 한 승인에서 문서 하나만 수정한다. 여러 문서 트랜잭션, 집계/계산·부분 projection/returnKey 결과, 뷰, 지원하지 않는 BSON 형식은 범위 밖이며 거부한다. 필드명에 점/달러/널을 허용하지 않고 비교 값을 `$eq`로 감싸 연산자로 해석되지 않게 한다. 문서 값은 일시적인 변경 전·후 샘플에서만 보여 주며 변경 이력에는 바인딩 자리표시자를 기록한다. projection에서 제외한 필드는 미리보기 샘플에도 추가하지 않는다.

전체 검증에서 드러난 기존 문제도 수정했다. SQLite는 취소 감시의 interrupt gate를 정리 작업 시작 전에 닫아 늦은 interrupt가 롤백을 방해하지 못하게 한다. MongoDB 통합 테스트는 각 fixture의 독립 테스트 DB를 사용하고, snapshot은 뷰의 정의를 보존하면서 뷰에 listIndexes를 요청하지 않는다. MySQL 미리보기는 같은 트랜잭션 안에서 변경 후 값을 다시 읽어 보여 준다. 메타데이터 추가로 커진 스트리밍 이벤트는 Box로 저장하며 wire 형식은 동일하다.

브라우저 검증은 타임아웃을 늘리지 않고 Chromium 동시 실행 수를 2로 제한했다. 지연된 toggle 이벤트가 권한 메뉴의 키보드 포커스를 되돌리는 문제와 여러 모델 선택기가 단축키에 동시에 반응하는 문제를 회귀 검사로 보강했다. 기존 E2E는 현재 로그인 모달·도킹 패널·릴리스 타임라인을 검증한다. 권한 E2E는 명시적 stale-element 오류에 한해 새 화면 상태에서 요소를 다시 찾으며 실행 결과가 불명확한 클릭은 재시도하지 않는다.

## 검증 상태

| 검사 | 결과 | 근거 |
|---|---|---|
| `cargo test --workspace` | 322 passed, 0 failed, 기존 ignored 7 | [전체 로그](verification/d10/rust-workspace.txt) |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | [로그](verification/d10/clippy.txt) |
| `pnpm --dir ui exec tsc --noEmit`, UI build | exit 0 | [tsc 포함 빌드 로그](verification/d10/ui-build.txt) |
| `NODE_OPTIONS='--expose-gc --no-experimental-webstorage' pnpm --dir ui test` | 156 passed, 0 skipped | [로그](verification/d10/ui-tests.txt) |
| 기존 실제 앱 E2E | bridge 21/21, preview 5/5, permissions 13/13 | [집계](verification/d10/regressions.json), [bridge](verification/d10/e2e-bridge.txt), [preview](verification/d10/preview-e2e.txt), [permissions](verification/d10/permissions-e2e.txt) |
| 기존 브라우저 E2E | 4개 스위트 통과, 두 테마 | [AX](verification/d10/ui-ax-e2e.txt), [W7](verification/d10/ui-w7-e2e.txt), [connect](verification/d10/ui-w7-connect-e2e.txt), [updater](verification/d10/ui-update-e2e.txt) |
| 실제 debug 앱 결과 편집 | 15개 검사, 19개 PNG | [실행 로그](verification/d10/native.txt), [런타임 집계](design-shots/d10-native.json) |

Rust 실행에는 요청한 PG/MySQL/MariaDB/Mongo/Redis/Neo4j 테스트 DB 환경 변수를 모두 설정했다. `CARGO_TARGET_DIR`은 이 워크트리의 `target`, `AXDB_DRIVER_DIR`은 `target/debug`이다. 자원 경합을 줄이기 위해 build jobs 2, dev/test debug symbols 0, incremental off를 사용했으며 debug assertions는 유지했다. 기존 vendor Neo4j 의존성의 deprecated 경고는 출력되지만 엄격 clippy는 exit 0이다.

정규 Rust 실행의 기존 ignored 7개 중 로컬 패키지 설치/서명/롤백 검사는 별도 실행해 통과했다([로그](verification/d10/package-install.txt)). 나머지는 실 Anthropic 유료 호출, 실 OS 키체인 왕복, 수동 성능/자원 벤치마크 4개이며 실행하지 않았다. TLS 전용 추가 환경 변수도 설정하지 않아 실 TLS 경로 검증은 포함하지 않는다. 테스트를 새로 skip/삭제하거나 타임아웃을 늘리지 않았다.

실제 앱 E2E의 자격 증명은 격리된 debug fixture 파일을 사용한다. 기존 스위트의 keychain 명칭과 달리 실제 OS 키체인 인증 증거는 아니다. 브라우저 4개 스위트는 명시적 mock 백엔드이며 실 공급자 로그인 증거가 아니다. 네이티브 편집 검사는 실제 Tauri/WKWebView와 이 워크트리의 드라이버 프로세스 및 테스트 DB를 사용한다.

네이티브 검사는 네 엔진 모두 셀 수정 → 미리보기에서 DB 값 유지 → 확인 대화상자 → 적용 → 원래 쿼리 재조회, 실패 쿼리 뒤 오래된 결과 제거, 읽기 전용/에이전트 탭의 편집 차단을 확인했다. SQL 세 엔진은 JOIN 비활성 사유와 계산 컬럼 잠금도 확인했다. 미리보기·승인·재조회 장면은 `design-shots/d10-{postgres,mysql,sqlite,mongodb}-*.png`, JOIN 장면은 SQL 엔진별 `d10-*-join-disabled.png`에 있다.

전체 게이트 통과 뒤 MongoDB의 부분 projection/returnKey 원본성 경계와 fixture 종료 정리를 보강했다. 관련 실제 MongoDB 집중 검사 6개와 엄격 clippy가 다시 통과했다([검사 로그](verification/d10/mongo-boundary-tests.txt)). 새로 생성한 UUID 테스트 DB의 내부 뷰 카탈로그까지 정리하고, 앞선 실행에서 남은 이 작업의 UUID DB 16개를 정리했으며 잔여 0개를 확인했다([정리 결과](verification/d10/mongo-fixture-cleanup.json)).

재현: README의 테스트 DB 설정 뒤 UI build와 workspace bins build를 실행한다. `node bench/d10-native.mjs`는 실제 앱 검사/캡처를, `node bench/d10-regressions.mjs`는 기존 실제 앱 E2E 세 스위트를 격리 설정으로 실행한다. 사용자 DB/설치 앱/OS 키체인 공급자 계정을 검증 대상으로 삼지 않는다.

로컬 단계 커밋: `2175de6` 권한 메뉴 회귀, `f5f4700` 드라이버/계약, `31330be` 결과 편집 UI, `2d65211` 안전한 미리보기 및 전체 검사 실패 수정, `4723dcf` 특수 find 읽기 전용/fixture 정리. 마지막 커밋에 이 보고서와 네이티브 하네스/캡처를 포함한다. push와 main merge는 수행하지 않았다.

## 남은 범위

Redis와 Neo4j 결과 편집은 이번 범위에서 제외했다. SQL 함수는 사용자 정의 집계와 스칼라를 카탈로그로 안전하게 구분할 때까지 결과 전체를 읽기 전용으로 둔다. CTE·하위 쿼리·여러 문장·JOIN의 부분 편집도 지원하지 않는다. SQLite는 main 데이터베이스의 테이블만 편집한다. MongoDB 여러 문서 일괄 편집은 replica-set 트랜잭션 지원이 필요하다.

메타데이터 API 근거: [tokio-postgres Column](https://docs.rs/tokio-postgres/0.7.18/tokio_postgres/struct.Column.html), [rusqlite columns_with_metadata](https://docs.rs/rusqlite/0.40.2/rusqlite/struct.Statement.html#method.columns_with_metadata), [SQLite 원본 컬럼 API](https://www.sqlite.org/c3ref/column_database_name.html). SQLite column_metadata 빌드 기능을 켜서 원본 출처를 얻는다.
