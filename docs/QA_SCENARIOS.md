# UX/UI 및 기능 시나리오 목록

코드베이스를 기준으로 작성한 22개 시나리오(UX 3개, 기능 19개)다. 기존 전체 자동 테스트, 변경 영역의 브라우저 회귀, 격리 DB와 실제 macOS 앱 검증을 실행했다. 결과·제한·재실행 명령은 [QA_REPORT.md](QA_REPORT.md)에 기록했다.

## 실행 상태와 범위

| 구분 | 이번 결과 |
|---|---|
| UI 단위·계약·Chromium browser | 207 통과, 1 skipped |
| Rust workspace | 329 통과, 7 ignored; 환경 의존 조기 반환은 별도 구분 |
| 실제 SSH·proxy·TLS 재실행 | 8 통과 |
| WebKit UI 회귀 | overlay/MCP 12 + 사용자·권한 8 통과 |
| 실제 macOS Tauri + PostgreSQL | 11단계 완료; 연결·목록·자동완성·SQL 결과·행 편집·권한 조회 |
| 실제 native MCP HTTP | 12개 확인 통과; 인증·탐색·조회·쓰기 거부·감사 이력 |
| 수동/외부 계정 | 아래표의 잔여 범위 참고; 모든 화면·DB·외부 서비스 전수 검증은 아님 |

과거 `FINAL_UI_AUDIT*.md`의 결과를 이번 결과로 승계하지 않았다. 현재 증거는 [검증 디렉터리](verification/qa-2026-10-21/)와 아래 시나리오별 검증 층을 참고한다.

## 시나리오

상태 표기: 첫 표는 시나리오와 코드의 매핑이며, 실행 결과는 아래 검증 층 표에 기록한다. “부분”은 데이터/헬퍼만 검증하고 전체 화면 흐름은 다루지 않는다는 뜻이다. 시나리오는 총 22개(UX 3개, 기능 19개)다. 우선순위 P0은 데이터 손실·무단 쓰기·접속 불능, P1은 핵심 기능 실패 또는 심한 가림, P2는 보조 기능·표시 결함이다.

| ID / 우선순위 | 기능과 구체적 시나리오 | 자동 검증 및 코드 포인터 | 사전 조건 | 기대 결과 | 전수 수동·외부 검증의 잔여 범위 |
|---|---|---|---|---|---|
| UX-01 / P1 | 창을 960×600으로 줄이고 사이드바 연결 메뉴, 드라이버 선택, 실행 옵션, 테마 설정 메뉴를 위·아래·좌·우 가장자리에서 연다. 스크롤 목록 안의 연결 메뉴도 연다. | `ui/src/ui/FloatingMenu.tsx`, `floating-menu.css`, `ui/src/Connections.tsx`; `ui/src/ui/GlideSelect.tsx`; `ui/src/QueryTab.tsx`; `ui/src/App.tsx`; 스타일 `ui/src/style.css`, `ui/src/sidebar.css`. `overlays.browser.test.ts`가 viewport/theme별 메뉴 bounds와 hit-test를 검증한다. 과거 N-6: `docs/FINAL_UI_AUDIT_2.md`. | 데스크톱 앱, 960×600 및 1280×800 | 메뉴가 창/대화상자 안에 완전히 보이고 트리거와 연결되며 잘리거나 다른 컨트롤을 가리지 않는다. 클릭/바깥 클릭/Escape로 닫힌다. | 화면 크기·테마별 수동 확인 필요. 미실행. |
| UX-02 / P1 | 빈 상태에서 연결 추가를 누르고 드라이버를 선택한 뒤 기본/네트워크/인증/고급을 오가며 필수값 오류, 이전/다음, 테스트, 저장을 수행한다. 기존 연결 편집과 저장된 암호 유지도 확인한다. | `ui/src/ProfileForm.tsx`, `ui/src/Connections.tsx`; `ui/src/ax/connectionContract.test.ts`, `ui/src/ax/accountsMock.test.ts`, `ui/src/api.test.ts`; backend 프로필 명령 `src-tauri/src/lib.rs`. `ProfileForm.browser.test.ts`가 폼 단계·검증·비밀값 전달을 검증한다. | 설치 드라이버 또는 mock | 추가/편집 제목과 폼 상태가 정확하고, 필수값 오류로 해당 단계가 열리며, 입력 수정 시 이전 테스트 결과가 무효화된다. 암호 비밀값은 화면·저장소에 노출되지 않고 저장/취소 동작이 일관된다. | 실제 앱 수동 확인. 미실행. 사용자 보고의 추가/편집 UX 장애 우선 재현. |
| UX-03 / P1 | 연결 문자열/JDBC/psql/SSH 정보를 스마트 붙여넣기로 넣고 파싱된 필드와 비밀값 처리 결과를 검토한다. 잘못된 형식과 미설치 드라이버도 시도한다. | `ui/src/ProfileForm.tsx`; `src-tauri/src/net/parse.rs`; `src-tauri/src/net/tests.rs`. | 합성 입력 예시, mock 또는 실제 앱 | 지원 형식은 안전하게 필드화되고 secret은 초안 메모리로 분리되고 저장 전에는 키체인을 변경하지 않는다. 실패 시 기존 폼 값이 조용히 손실되지 않고 이유가 표시된다. | UI 수동 및 parser 단위 확인. 미실행. |
| FUNC-01 / P1 | 연결 테스트/접속/해제를 수행한다. 잘못된 암호, 서버 거부, 연결 중 취소/중복 클릭, 저장된 암호가 없는 연결을 포함한다. | `ui/src/Connections.tsx`, `ui/src/ProfileForm.tsx`, `ui/src/disconnect.test.ts`; `src-tauri/src/lib.rs`의 `session_open`, `session_close`; `src-tauri/src/net/tests.rs`; `docs/verification/e1/` 과거 E2E 자산. | 테스트 DB/실패 endpoint 또는 mock | 로딩/성공/오류 상태가 명확하고, 실패해도 프로필과 탭이 보존된다. disconnect는 해당 연결의 run만 취소하고 다른 세션은 유지한다. | 테스트 DB 및 네이티브 앱 필요. 미실행. |
| FUNC-02 / P1 | SSH 터널, TLS, proxy, IAM/클라우드 인증을 조합해 연결하고 각 인증 만료/취소/잘못된 인증서를 확인한다. | `ui/src/ax/ConnectionNetwork.tsx`; `src-tauri/src/net/{ssh,tls,proxy,iam}.rs`; `src-tauri/src/net/tests.rs`; driver TLS 통합 테스트 `crates/driver-{postgres,mongodb,neo4j}/tests/tls.rs`. | SSH/TLS/IAM 제공 테스트 환경과 자격 증명 | 설정값과 인증 단계가 연결 요약에 반영되고, 오류 종류가 구분되며, secret이 로그/이력에 남지 않는다. | 외부 계정/서버 필요. 미실행. |
| FUNC-03 / P1 | 여러 프로필에서 세션을 열고 닫으며, 연결 검색·접기·정렬·탐색기 선택·창 재시작 후 저장 상태를 확인한다. | `ui/src/Connections.tsx`, `ui/src/sidebar.ts`, `ui/src/sidebar.test.ts`; `src-tauri/src/storage.rs`, `src-tauri/src/lib.rs`. | 2개 이상 합성 프로필 | 현재 연결과 탐색기 대상이 일치하고, 검색/접기 상태가 합리적으로 유지되며, 비밀값을 제외한 프로필만 영속화된다. | 앱 재시작 수동 확인. 미실행. |
| FUNC-04 / P1 | 테이블/뷰/스키마/레이블을 탐색하고 새로고침·검색·확장, 컬럼 액션, FK 이동 및 노드 선택을 수행한다. | `ui/src/Connections.tsx`, `ui/src/explorer.ts`, `ui/src/explorer.test.ts`, `ui/src/navigation.ts`, `ui/src/navigation.test.ts`; backend `schema_graph` 등록 `src-tauri/src/lib.rs`. | PostgreSQL 관계 fixture와 Neo4j fixture 권장 | 스키마 계층이 중복 없이 나타나고 복합 FK가 올바른 키 순서로 이동하며 누락/null 키 이동은 거부된다. 빈/권한 제한 스키마는 설명된다. | 2종 이상 DB 수동/E2E. 미실행. |
| FUNC-05 / P1 | 새 쿼리 탭, 여러 탭, 탭 전환/닫기, 저장되지 않은 편집 텍스트 보존과 닫은 탭 리소스 정리를 확인한다. | `ui/src/App.tsx`, `ui/src/QueryTab.tsx`, `ui/src/disconnect.ts`, `ui/src/disconnect.test.ts`; `ui/src/perf/tabRetainers.test.ts`, `ui/src/perf/graphLifecycle.test.ts`. | 앱 및 mock 가능 | 탭별 연결/텍스트/결과가 섞이지 않고 연결 해제 뒤 편집 텍스트가 보존된다. 닫힌 탭의 그래프/worker 리소스가 해제된다. | 앱 전환 시각·동작 확인. 미실행. |
| FUNC-06 / P1 | SQL/Cypher 편집기에서 자동완성, 파라미터 JSON, 문법 오류, 포맷/리팩터/진단, OS 단축키, 편집기 우클릭 메뉴를 확인한다. | `ui/src/Editor.tsx`, `ui/src/QueryTab.tsx`, `ui/src/sqlCompletion.ts`, `sqlCompletion.test.ts`, `params.test.ts`, `ax/editorEdits.test.ts`; backend `src-tauri/src/editor.rs`. | 지원 드라이버/언어 | 자동완성은 문자열/주석을 오염시키지 않고 인용 식별자와 dialect에 맞는다. 잘못된 파라미터는 실행 전에 설명된다. 텍스트 변경은 무관한 범위를 보존한다. | 편집기 우클릭 메뉴 및 플랫폼 단축키 수동 확인. 미실행. |
| FUNC-07 / P1 | 읽기 쿼리 실행, 다중 결과, 긴 쿼리, timeout, 취소, 오류 후 재실행을 수행한다. | `ui/src/QueryTab.tsx`, `ui/src/mock.test.ts`, `ui/src/api.test.ts`; `src-tauri/src/lib.rs` execute/cancel/history; `src-tauri/tests/streaming_compat.rs`. | mock 및 테스트 DB | 실행 중 중복 실행 방지와 취소가 동작하고, 완료/오류/timeout 결과와 이력이 연결된다. 오류 후 편집 내용/탭은 보존된다. | mock 자동 + 테스트 DB 네이티브. 미실행. |
| FUNC-08 / P1 | 결과 표에서 대량 행 스크롤·정렬·필터·열 폭 조절, NULL/빈 문자열/큰 정수/JSON/바이너리를 확인한다. | `ui/src/VirtualTable.tsx`, `ui/src/virtual.test.ts`, `ui/src/values.test.ts`, `ui/src/perf/viewer.test.ts`; backend `src-tauri/src/stream.rs`, `src-tauri/tests/streaming_memory.rs`. | 100k fixture 또는 synthetic stream | viewport 규모만 렌더하고 정확한 원본 값이 표시된다. 정렬/필터는 원본 데이터를 변경하지 않고 큰 정수를 반올림하지 않는다. | 큰 데이터 성능 측정은 지정 환경 필요. 미실행. |
| FUNC-09 / P1 | 결과 그래프에서 노드/관계, 중복·자기 루프, 선택, 확대/이동, fit, 노드 확장 및 표/그래프 전환을 수행한다. | `ui/src/GraphView.tsx`, `ui/src/graph.ts`, `ui/src/perf/viewer.test.ts`, `ui/src/perf/graphLifecycle.test.ts`; `ui/src/mock.test.ts`. | Neo4j 샘플 그래프 또는 mock | 노드/엣지가 식별자 기준으로 병합되고 반복/역방향 관계가 읽힌다. 확장 결과에 중복 seed가 있어도 안정적이며 탭 종료 시 리소스가 해제된다. | 좁은 캔버스·작은 창의 라벨/조작은 수동. 미실행. |
| FUNC-10 / P1 | 지원 driver에서 행 추가/수정/삭제 후 미리보기, 충돌 변경, 적용 확인, 취소, 재조회를 수행한다. | `ui/src/EditableTable.tsx`, `ui/src/QueryResultEditor.tsx`, `ui/src/edits.ts`, `ui/src/edits.test.ts`, `ui/src/values.test.ts`; `src-tauri/src/edits.rs`; `docs/C2_NATIVE_E2E.md` 과거 증거. | rollback 가능한 테스트 DB 및 편집 지원 capability | 키 컬럼/원본값을 기준으로 preview가 생성되고 승인 전에는 변경되지 않는다. 변경된 원본 행은 충돌 처리되고 취소는 DB에 쓰지 않는다. | 수동 네이티브 DB E2E. 미실행. |
| FUNC-11 / P1 | 쓰기 쿼리의 미리보기에서 여러 statement, 영향 행, 전/후 표본, schema diff, 경고를 확인하고 적용/취소한다. preview 도중 취소 및 재실행도 포함한다. | `ui/src/ChangePreviewView.tsx`, `ui/src/QueryTab.tsx`, `ui/src/previewCopy.test.ts`; `src-tauri/src/lib.rs` preview; `docs/verification/e1/preview-e2e.txt`. | rollback 가능한 전용 테스트 DB | preview는 명시적으로 미커밋 상태이고 변경 지문이 달라지면 적용 버튼이 비활성화된다. 취소는 commit하지 않고 적용 후 결과를 다시 조회한다. | 네이티브 E2E 및 DB 필요. 미실행. |
| FUNC-12 / P2 | import/migration/backup/restore/role-grant 관리 흐름에서 계획, preview, 확인, 취소를 수행한다. 잘못된 파일과 권한 부족도 포함한다. | `ui/src/ax/ImportWizard.tsx`, `Migrations.tsx`, `BackupRoles.tsx`; `src-tauri/src/management/`, `backup_file.rs`; `src-tauri/tests/administration.rs`. | 각 기능 지원 DB 및 별도 fixture 파일 | 계획/preview와 실제 apply 단계가 분리되고, 승인 전 DB 변경이 없다. 잘못된 파일·지원되지 않는 기능은 오류를 보인다. | DB별 통합 테스트 필요. 미실행. |
| FUNC-13 / P1 | 필터·정렬된 표와 그래프 결과를 CSV/NDJSON/Markdown/이미지/ERD로 저장하고, 쿼리 파일 불러오기와 취소를 확인한다. | `ui/src/ExportButton.tsx`, `ui/src/export.ts`, `ui/src/export.test.ts`, `ui/src/exportImage.ts`; backend `src-tauri/src/export.rs`. | 쓰기 가능한 임시 디렉터리 | 인코딩/NULL/빈값/정확 숫자/중첩 JSON이 손실되지 않고 현재 정렬·필터 행과 일치한다. 파일 경로와 덮어쓰기/취소가 안전하다. | 실제 파일 대화상자/앱 수동 필요. 미실행. |
| FUNC-14 / P1 | 쿼리 이력 검색·재실행·삭제와 UI/MCP 출처 확인, 오류·preview 기록 및 민감값 redaction을 확인한다. | `ui/src/mock.test.ts`, `ui/src/api.test.ts`; `src-tauri/src/storage.rs`; host tests `src-tauri/src/lib.rs`; UI `App.tsx`/`QueryTab.tsx`. | 합성 secret을 포함한 테스트 쿼리 | 쿼리 텍스트는 이력 정책에 따라 남고 파라미터/비밀값은 남지 않는다. 재실행은 새 run으로 기록되고 삭제 대상이 정확하다. | 실제 앱 및 격리 설정에서 확인. 미실행. |
| FUNC-15 / P1 | AI 계정 연결/해제, 모델·노력도 설정, 대화, 선택한 결과 행 컨텍스트 전달, 코드 검토/승인/거부를 수행한다. 네트워크 실패/로그아웃도 포함한다. | `ui/src/ai/` 및 `loginFlow.test.ts`, `chat.test.ts`, `modelPicker.test.ts`, `types.test.ts`, `review.test.ts`; backend `src-tauri/src/assistant/`. | 공급자 계정과 네트워크, 또는 mock | 선택하지 않은 행/자격증명은 전송하지 않고, 실패 응답은 대화 상태를 망가뜨리지 않는다. AI 제안 SQL은 사용자 확인 전 실행되지 않는다. | 실제 공급자 계정 수동 확인. 미실행. |
| FUNC-16 / P1 | MCP/WebMCP 클라이언트 등록, 토큰 회전·폐기, 도구 허용 목록의 클릭/키보드 전환, Codex·Claude Code·일반 JSON 연결 방법 복사, 읽기 호출, 쓰기 preview/승인/거부, 결과 페이지 및 감사 이력을 확인한다. | `ui/src/agent/registry.ts`, `registry.test.ts`, `ClientsPanel.tsx`, `ToolAccessList.tsx`, `ClientConnectGuide.tsx`, `clientsTools.browser.test.ts`; backend `src-tauri/src/{agent_bridge,agent_dispatch,permissions,agent_results}.rs`; `src-tauri/tests/plugin_contracts.rs`. | 격리 앱 설정 및 MCP 클라이언트 | 토큰 원문/프로필 secret/행 제한 초과 결과가 노출되지 않는다. 권한을 재확인하며 쓰기는 일치하는 preview와 사용자 승인 없이는 적용되지 않는다. 연결 가이드는 준비/대기 상태와 실제 요청 기록을 구분한다. | 도구 목록·복사 가이드는 Chromium 자동 회귀로 확인. 실제 native MCP HTTP 검증 결과는 아래표 참고. Codex/Claude 모델 세션은 미실행. |
| FUNC-17 / P2 | 드라이버 카탈로그 열기, 설치/취소/업데이트/삭제, 서명 또는 손상 패키지 오류를 확인한다. | `ui/src/drivers/DriversPanel.tsx`, `DriverPicker.tsx`; `src-tauri/src/driver_install.rs`, `driver_install/tests.rs`; `crates/driver-package/src/tests.rs`. | 네트워크/패키지 카탈로그, 설치 디렉터리 | 작업 상태와 진행이 표시되고 취소/실패 후 설치 목록이 일관된다. 사용 중 드라이버 삭제는 안전하게 거부되거나 연결 정리 후 처리된다. | 패키지 공급망 환경 필요. 미실행. |
| FUNC-18 / P2 | 앱 업데이트 확인, 다운로드/설치/취소, 릴리스 노트 링크, 새 기능 안내의 재표시/숨김을 확인한다. | `ui/src/update/update.test.ts`, `schedule.ts`, `Updater.tsx`, `WhatsNew.tsx`; backend `src-tauri/src/updater.rs`, `src-tauri/tests/updater_feed.rs`. | 업데이트 feed 또는 mock | 비활성/대기/실패 상태가 구분되고 허용되지 않은 링크를 열지 않는다. 다시 알림 설정이 유지된다. | 서명된 업데이트 feed 수동 확인. 미실행. |
| FUNC-19 / P2 | 테마, 글자/사이드바 설정, 키보드 접근, 명령 팔레트와 온보딩 다시 열기를 조합한다. 최소 크기와 reduced motion도 확인한다. | `ui/src/theme.ts`, `sidebar.test.ts`, `ui/src/ui/branched-menu.test.ts`, `onboarding/onboarding.test.ts`; `App.tsx`, `ui/src/ui/CommandPalette.tsx`. | Daylight/Midnight 및 reduced-motion 설정 | 설정이 재시작 후 저장되고 키보드 탐색 후 focus가 복원된다. 온보딩은 설정/팔레트에서 재진입 가능하고 작은 화면에서 내용/버튼이 접근 가능하다. | viewport/키보드 수동 점검. 미실행. |

## 실행 증거와 시나리오별 검증 층

다음은 이번 검증에서 확인된 자동/네이티브 층이다. “단위/mock”은 화면 전체 흐름이나 실제 서비스 연결을 뜻하지 않는다. 상세 요약은 [QA_REPORT.md](QA_REPORT.md), 원본은 [Rust 로그](verification/qa-2026-10-21/cargo-test-final.log), [UI 로그](verification/qa-2026-10-21/ui-tests.log), [WebKit overlay 로그](verification/qa-2026-10-21/webkit-overlays.log), [네이티브 결과](verification/qa-2026-10-21/native-results.json)다.

| 시나리오 | 이번 실행에서 실제로 확인한 층 | 실행 범위와 제한 |
|---|---|---|
| UX-01 | WebKit overlays 8/8, 순수 위치 계산, 긴 선택 목록; `overlays.browser.test.ts` | viewport/theme 조합 6개 포함. 앱 전체 모든 팝오버 전수 확인은 아님. |
| UX-02 | `ProfileForm.browser.test.ts` 및 UI 테스트; 제한적 네이티브 연결 추가/테스트/편집/취소 단계 | 네이티브 연결은 테스트 DB에 한정. 전체 UI 207 통과·1 skipped에 포함. |
| UX-03 | parser Rust 단위 테스트, ProfileForm browser/단위 흐름 | 외부 자격증명은 사용하지 않음. |
| FUNC-01 | Rust 네트워크/host 테스트, UI mock·`connectionError.test.ts`; 네이티브 실제 PostgreSQL 연결·연결 테스트 단계 | 인증·권한 오류의 실제 서버 조합은 제한적. |
| FUNC-02 | Rust 단위 + 실제 SSH/password/key/agent/jump/proxy/TLS 8/8 | IAM 외부 계정·인증 만료와 MongoDB/Neo4j TLS는 미실행. |
| FUNC-03 | Rust host/profile tests, UI sidebar/disconnect unit tests | 여러 실사용 프로필을 이용한 창 재시작 시각 흐름은 미실행. |
| FUNC-04 | UI explorer/navigation tests; 네이티브 PostgreSQL 스키마·테이블/뷰 검색 단계 | PostgreSQL 테스트 fixture에 한정. Neo4j 전체 탐색은 이번 네이티브 런에서 미확인. |
| FUNC-05 | UI disconnect/tab-retainer/graph lifecycle unit tests | 전체 앱 다중 탭 전환 수동 시나리오는 미실행. |
| FUNC-06 | SQL completion/parameter/editor-edit unit tests; 네이티브 SQL 자동완성 확인 | 편집기 native context menu, 모든 OS 단축키 수동 확인은 미실행. |
| FUNC-07 | UI mock cancel/timeout/history tests, Rust execute/stream integration tests; 네이티브 실제 SQL 결과 렌더 완료 | PostgreSQL fixture에서 확인; 전체 DB UI 전수는 아님. |
| FUNC-08 | UI exact-value/virtualization unit tests, Rust streaming/row-limit tests | 대량 UI 성능 측정은 생략. |
| FUNC-09 | UI graph algorithm/mock/perf unit tests | 실제 그래프 화면의 작은 viewport·라벨 배치는 이번 실행에서 미검증. |
| FUNC-10 | UI edits/value unit tests; Rust PostgreSQL/SQLite/MongoDB/Neo4j edit preview/apply/conflict 통합 테스트 | 네이티브 PostgreSQL 행 수정 preview → 취소 → 적용 → DB 재조회까지 완료. |
| FUNC-11 | UI preview-copy/mock tests; Rust change-preview rollback integration tests | 행 수정 대화상자의 적용·취소 native 확인 완료. 모든 임의 SQL 미리보기 조합은 전수 미실행. |
| FUNC-12 | Rust administration/import/backup 통합 + PG 800개 테이블 권한 페이지 회귀; roles browser 8/8 + native 역할·필터 조회 | 파일 import/backup/restore의 모든 OS 대화상자 수동 흐름은 미실행. |
| FUNC-13 | UI export serialization/unit tests, Rust export/backup 명령 테스트 | 실제 OS 파일 선택/저장 대화상자 미실행. |
| FUNC-14 | UI mock history/API tests, Rust storage/history/redaction host tests | 사용자 데이터 설정을 읽지 않음; 앱 재시작 후 이력 확인 미실행. |
| FUNC-15 | UI AI flow/model/chat/review tests 및 Rust AI policy/operation tests | 실제 OAuth 로그인과 외부 공급자 호출은 미실행. |
| FUNC-16 | UI agent registry tests, Rust permission/dispatch/plugin contract tests; `clientsTools.browser.test.ts` Chromium/WebKit 4/4 (checkbox label/keyboard/disabled/alignment, 960×600 compact list, safe Codex/Claude setup copy) | 실제 native MCP HTTP 12개 확인 완료. Codex/Claude 모델 세션과 운영 자격 증명은 미실행. |
| FUNC-17 | Rust driver package/install 단위 테스트 | 실제 외부 카탈로그 다운로드·서명된 업데이트 설치는 미실행. |
| FUNC-18 | UI updater schedule/feed parsing tests, Rust updater feed tests | 실제 업데이트 다운로드·설치는 미실행. |
| FUNC-19 | UI onboarding/theme/sidebar/keyboard unit/browser tests | 모든 화면×테마×크기 조합의 수동 시각 검사는 미실행. |

## 재현하여 수정한 장애

- 연결 후 비-public 스키마 테이블·뷰와 SQL 자동완성 목록이 누락되는 문제.
- 연결 폼의 숨겨진 필수 항목 검증, 드라이버 변경 시 초안 잔존, SSH/proxy 테스트에서 새 비밀값이 누락되는 문제.
- 스크롤 컨테이너의 메뉴 잘림, WebKit에서 삭제 예약 후 Escape가 전달되지 않는 문제, 긴 선택 목록에서 다른 항목이 선택되는 문제.
- 연결 진행 카드가 설정 메뉴를 가리는 문제.
- PostgreSQL 사용자·권한 전체 교차표가 50,000셀을 넘으면 실패하는 문제. 역할 선택 및 제한된 페이지 조회로 변경.
- MCP 체크박스 정렬과 연결 설정 복사 진입점의 부재.

## 테스트 실행 매핑의 한계

UI 자동 테스트는 `ui/src/**/*.test.ts` 및 `ui/src/**/*.browser.test.ts`에 분산되어 있다. Rust driver 통합 테스트 중 일부는 실제 DB/Docker fixture를 쓰고 일부는 필요한 환경이 없으면 조기 반환한다. 따라서 workspace 집계만으로 모든 DB 동작을 주장하지 않는다. 실제 OAuth/IAM 외부 계정, Codex/Claude 모델 세션, 업데이트 설치, 대량 UI 성능은 이번 실행에서 검증하지 않았다. MCP HTTP bridge 자체는 격리 앱에서 확인했다.
