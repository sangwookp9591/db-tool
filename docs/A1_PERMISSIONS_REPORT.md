# A1 권한과 범위 MCP 검증 보고

기준: `568f43e`; 작업 중 M1/P1/V1/W8 변경이 공존한다. A1은 커밋하지 않는다. 빌드 대상은 `target-a1`, Cargo 병렬도는 3으로 제한한다.

## 구현

- 연결의 `access_mode`는 기존 프로필에서 `read_write`로 기본 해석한다. 호스트의 실행·스트리밍 실행은 매번 저장된 최신 모드를 읽어 DB 수준 읽기 전용을 강제한다. 기존 열린 세션에도 적용한다. `read_only_exec` 없는 드라이버에서는 UI가 선택을 숨기고 호스트도 저장/실행을 거부한다. 읽기 전용 연결의 변경 미리보기는 거부한다.
- PostgreSQL browse가 만드는 실행 요청도 `read_only: true`로 바꿨다. SQLite/Neo4j의 browse는 기존 DB 읽기 전용 경로를 재사용한다. 드라이버 id로 UI를 분기하지 않는다.
- `agent_clients.json`은 0600이며 UUID id/name, 연결별 none/read/preview/write_with_approval, allowed_tools, max_rows, expires_at, revoked와 토큰의 salt/hash를 저장한다. 토큰은 UUIDv4 난수 2개, salt는 별도 난수이며 SHA-256(salt + ':' + token)을 저장한다. 새 클라이언트/회전 토큰은 명령 응답과 설정 패널에서 한 번만 표시한다. 목록에서는 salt/hash도 반환하지 않는다.
- 기존 브리지 bearer는 `local` 기본 클라이언트로 연결된다. 기존 도구·연결 접근은 유지한다. **하위 호환 예외:** 기존 `agent-bridge.json`의 0600 discovery 파일은 로컬 기본 토큰 원문을 계속 담는다. 별도 생성한 범위 토큰 원문은 디스크에 저장하지 않는다. 기본 토큰 회전 시 discovery도 갱신된다.
- HTTP 요청마다 해시로 클라이언트를 판별하고 만료/폐기를 확인한다. 도구 목록·연결 목록을 필터링하고, 열기/탐색/닫기/쿼리/미리보기/탭 열기에 연결 권한을 적용한다. none/알 수 없는 세션은 NotFound, 허용되지 않은 도구/쓰기/미리보기는 Forbidden이다. 유효하지 않거나 만료·폐기된 토큰은 HTTP 401이다.
- run_query/preview/browse/expand/history는 Rust 호스트가 실행한다. 결과 표/그래프/문서와 미리보기 샘플은 호스트가 제한한다. 기존 최대 200개 응답 제한을 유지하므로 실제 반환 한도는 `min(max_rows, 200)`이다. 여러 결과의 합계도 제한하고 제외된 그래프 끝점을 참조하는 간선은 제거한다. 행 수 메타데이터는 실제 조회 수를 보존하고 `truncated`를 표시한다.
- 범위 클라이언트의 이력은 `profile_id`로 필터링한다. id 없는 과거 이력은 기본 무제한 로컬 클라이언트에만 보인다. 범위 클라이언트의 UI 상태는 허용된 연결 목록만 제공하고 다른 연결의 에디터·결과 요약을 내보내지 않는다.
- HTTP 쓰기는 호스트에서 미리보기를 수행한 뒤 실제 앱의 승인 창에 미리보기와 문장을 표시한다. 승인이 돌아오면 토큰 해시/폐기/만료/도구/연결 권한과 사용자 모드를 다시 검사한 뒤 실행한다. 웹뷰의 로컬 에이전트 경로도 미리보기 지문(세션·문장·파라미터·실행 옵션)과 일회성 승인 지문을 호스트가 확인한다. 거부/닫기/중복/시간 만료는 실행하지 않는다. 미리보기 기능이 없는 드라이버의 쓰기는 거부한다.
- 설정 패널에서 클라이언트 목록·생성·권한 편집·만료/행 제한·허용 도구·토큰 회전/폐기와 최근 50개 감사 이벤트를 제공한다. 연결 목록/탭/에디터에 읽기 전용 표시를 붙인다.
- `agent-audit.jsonl`은 0600이며 시각, 클라이언트 id, 도구, 연결 id, allowed/denied/approved/rejected 결정과 문장 SHA-256만 기록한다. 문장 원문·파라미터·토큰은 기록하지 않는다. 알 수 없는 도구명은 `unknown`, 저장된 프로필이 아닌 연결 id는 null로 기록해 잘못된 인자 위치로 들어온 토큰도 남기지 않는다. 허용/승인 감사 기록에 실패하면 실행하지 않는다.

## 위협 모델과 한계

신뢰하지 않는 로컬 MCP 클라이언트가 다른 연결의 존재·내용을 보거나, 허용 도구를 우회하거나, 읽기 권한으로 쓰거나, 승인 대기 중 철회된 권한을 사용하는 경우를 방어한다. 보안 경계는 Rust 호스트의 bearer 인증·권한 검사와 DB 읽기 전용 트랜잭션이며 UI 배지나 도구 힌트는 경계가 아니다. 웹뷰/Tauri IPC와 설정 파일을 수정할 수 있는 같은 OS 사용자 자체는 신뢰 경계 안이다. 기존 loopback 바인딩/Origin 거부/1MiB body 제한/동시 64개 요청 제한을 유지한다.

미리보기는 실행 후 롤백하므로 잠금·시퀀스·트리거/함수의 외부 효과까지 되돌린다는 보장은 없다. PostgreSQL 읽기 전용 모드에도 임시 객체나 비영속 외부 효과 같은 DB 자체 의미의 한계가 있다. 앱 권한은 DB 최소 권한 계정을 대신하지 않는다. 승인 시점의 데이터가 실행 때 동일하다는 보장은 하지 않는다(C1 계약 유지).

감사 로그는 로컬 파일이며 변조 방지/원격 전송/자동 회전을 추가하지 않았다. 기본 토큰은 재시작 때 새로 발급되며 별도 클라이언트의 해시/폐기 상태는 유지된다. 클라이언트별 세션 소유권은 따로 두지 않고 연결 권한을 공유 열린 세션에도 적용한다. 이미 시작된 DB 작업의 취소는 기존 실행 timeout/cancel 경로를 사용한다.

## 검증

실행 결과를 아래 표에 최종 기록한다. live 표시는 실제 테스트 DB 또는 실제 앱에서 관찰한 증거에만 쓴다.

| 검증 | 결과 |
|---|---|
| Host 단위 테스트 | 21 passed / 1 ignored(OS keychain 수동 테스트); live PG 포함 |
| 권한 매트릭스 | 832조합: none/read/preview/write_with_approval × 사용자 모드 × 도구 허용 × 요청 read flag × driver capability × 모든 도구 |
| 실제 SQLite 호스트 테스트 | 열린 세션의 최신 read_only, UI 경로 쓰기 거부, 미리보기·변조·재사용 승인 거부 확인 |
| UI vitest | 8개 파일, 41/41 |
| Cargo (M1 제외) | `cargo test --workspace --exclude driver-mongodb`: 51 passed / 1 ignored; PG/Neo4j live 환경 변수 지정 |
| clippy (M1 제외) | `cargo clippy --workspace --exclude driver-mongodb -- -D warnings`: 통과 |
| UI tsc / build | `pnpm --dir ui build` (tsc --noEmit 포함): 통과 |
| 앱 build | `cargo build -p axdb`: 통과 |
| 실제 앱 permissions E2E | 13/13 (네이티브 UI Run/승인/거부 포함) |
| 실제 앱 기존 bridge E2E | 21/21; 최신 기본 클라이언트 조회 row_count=1000/반환 표 200행 유지 |
| 최종 ready / 선택 인자 호환성 | 새 후보 앱 ready 직후 인자 없는 list_connections/list_drivers 요청 통과 |
| 기존 approval smoke | 12개 검사 통과; 실제 승인 만료 후 UserDenied와 미커밋 확인 |
| 네이티브 설정 UI | 클라이언트 생성, max_rows=3/허용 도구 저장, 토큰 1회 표시·숨기기, 회전 시 hash 변경, 폐기, 최근 50개 감사 로그 표시 확인 |

`bench/permissions-e2e.mjs`는 실제 실행 중인 후보 앱과 격리된 기존 e2e 프로필을 사용한다. 임시 해시 클라이언트 파일 fixture를 만들며 앱 HTTP 인증/권한/미리보기와 네이티브 Run/승인/거부 버튼을 실제로 거친다. 클라이언트 fixture는 끝나면 제거하고 프로필 모드를 복원한다. 토큰이나 비밀번호는 출력하지 않는다. UI CRUD는 실제 앱에서 별도 네이티브 조작과 `agent_clients.json`의 저장 상태를 함께 확인했다. 클립보드 복사 자체는 실행하지 않았다.


초기 전체 `cargo test --workspace`와 `cargo clippy --workspace -- -D warnings`는 동시 작업 중인 M1 MongoDB crate의 최신 계약 필드 누락과 미사용 import로 실패했다. 이후 M1의 호스트 등록/의존성 반영 중에도 일시적인 컴파일 실패가 있었으며, A1은 해당 파일을 수정하지 않고 통합이 반영된 시점에 재검증했다. 코디네이터의 2026-10-15 회신에 따라 A1은 M1 제외 검증으로 종료하며, M1/V1/P1을 포함한 최종 통합 게이트는 코디네이터가 수행한다. 실패 원인을 숨기기 위한 코드/timeout/재시도 변경은 하지 않았다. 최초 키체인 연결은 새 바이너리의 OS 접근 확인에서 멈춰 중단했으며, 실제 앱 E2E는 테스트 비밀번호를 앱의 일회성 자격 증명 창에 넣어 수행했다(기존 E2E 항목명에 남은 keychain 문구는 키체인 증거로 사용하지 않는다).

실제 데이터 대상은 `127.0.0.1:55432/dbtool_test`의 `e2e.typed/e2e.big`와 `bolt://127.0.0.1:57687/neo4j`의 DbtE fixture뿐이다. Cargo의 별도 호스트 통합 테스트는 `axdb_test`를 사용했다. 기존 테스트 DB/기존 세션을 임의로 종료하거나 복원하지 않았으며, A1이 띄운 후보 앱 프로세스만 정리한다.

최종 증거 파일(공유 작업 환경):

- `/tmp/axdb-a1-cargo-scoped-final.log`: live 환경을 지정한 51개 테스트 및 무시된 OS 키체인 테스트 1개.
- `/tmp/axdb-a1-clippy-scoped-final.log`, `/tmp/axdb-a1-clippy-host-final.log`, `/tmp/axdb-a1-ui-build-final.log`, `/tmp/axdb-a1-vitest-final.log`, `/tmp/axdb-a1-build-final.log`.
- `/tmp/axdb-a1-permissions-e2e-final.log`: 실제 MCP/네이티브 UI 13개 시나리오. 도구명·연결 id 위치에 토큰을 넣어도 감사 로그에 원문이 남지 않는 검사 포함.
- `/tmp/axdb-a1-legacy-e2e-final.log`: 기존 21/21. 자격 증명 입력 보조는 앱의 실제 Password/Connect 컨트롤만 조작했다.
- `/tmp/axdb-a1-dispatch-final.log`: 최종 직접 실행 입력/집계 제한 2개 테스트 통과(201개 result set 제한 포함).
- `/tmp/axdb-a1-startup-final.log`: 최종 바이너리 ready 직후 선택 인자 생략 호환성.
- `/tmp/axdb-a1-smoke-final.log`: 기존 12개 보안/승인 만료 smoke 검사. A1의 필수 미리보기가 끝난 뒤 승인 만료를 측정하도록 실행 예산을 1ms에서 1초로 구분했다(실제 승인 대기 약 31초).
- `/tmp/axdb-a1-ui-check.log`: 실제 클라이언트 설정 생성/회전/폐기와 저장 검증. 토큰은 출력하지 않았다.

추가로 `bench/agent-bridge-smoke.mjs`의 승인 만료 fixture를 새 미리보기 단계에 맞췄다. A1 소유 변경은 권한 모듈·브리지·호스트/저장소·설정/배지·관련 테스트/문서이다. `Cargo.lock`은 axdb의 sha2 의존 항목만 A1이며 나머지 신규 MongoDB 의존성 변경을 보존했다. `src-tauri/src/lib.rs`, `ui/src/{App,QueryTab,api,contract}`, `crates/driver-postgres/src/lib.rs`의 V1 변경을 보존했고, V1의 새 `NavigationTab.tsx`에는 readOnly prop 전달만 추가했다. 시작 시 이미 있던 M1/P1/W8 변경과 작업 중 추가된 다른 파일은 A1 완료에 포함하지 않는다. git commit은 하지 않았다.


시작 상태는 HEAD `568f43e`에 이미 `Cargo.lock`, `Cargo.toml`, `docs/tasks/W8-compare-bench.md`, `ui/src/perf/resource.ts` 수정과 비교 측정/MongoDB 미추적 파일이 있는 상태였다. 작업 중 코디네이터가 W8을 `c56feae`로 커밋했고, A1 자체는 커밋하지 않았다. A1 변경 22개 경로 외의 M1/P1/V1 변경은 그대로 남아 있다.
