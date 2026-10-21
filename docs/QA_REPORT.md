# UX/UI · 기능 검증 결과 — 2026-10-21

사용자 보고의 DB 목록·자동완성, 가려진 메뉴, 연결 폼, 사용자·권한 화면, MCP 체크박스 및 연결 안내를 수정했다. [22개 코드 기반 시나리오](QA_SCENARIOS.md)를 기준으로 기존 전체 자동 테스트와 변경 영역의 브라우저 회귀, 격리 DB 통합 테스트, 실제 macOS Tauri 앱을 검증했다. 외부 계정이 필요한 기능과 모든 화면의 수동 전수 검사는 별도로 구분한다.

## 변경과 확인

| 영역 | 변경 | 검증 |
|---|---|---|
| DB 탐색·SQL 자동완성 | 연결 시 스키마/DB/폴더를 제한된 동시 요청으로 탐색한다. 세션별 테이블·뷰 메타데이터를 편집기에 전달하고 dialect와 인용 식별자를 반영한다. 이전 요청은 세션 전환 후 상태를 덮어쓰지 않는다. | explorer/completion 단위, 실제 PostgreSQL 비-public 스키마의 테이블·뷰 목록과 자동완성 |
| 연결 추가·편집 | 필수 입력이 있는 단계로 이동, 테스트 오류를 폼 안에 표시, 드라이버 전환 시 이전 초안 제거. 테스트·붙여넣기·취소 중에는 비밀값을 저장하지 않으며 새 SSH/proxy 비밀값으로 테스트한다. | 폼 browser/계약, parser, 실제 SSH·proxy·TLS 통합 |
| 우클릭·선택·설정 메뉴 | 스크롤 클리핑을 피하는 메뉴 레이어, 창 경계 보정, 키보드 이동·Escape·포커스 복원. 긴 선택 목록에서 클릭한 항목을 정확히 선택한다. 연결 안내가 설정 버튼을 덮는 레이어 오류도 수정했다. | Chromium/WebKit, 960×600·1280×800·1920×1080 × 두 테마의 좌표·hit-test, native 메뉴 캡처 |
| 사용자·권한 | 역할 선택 → 스키마/객체 필터 → 페이지별 조회. PostgreSQL의 5만 셀 초과 시 전체 교차표 생성을 피한다. 페이지별 대기 변경을 보존하고 미리보기·승인 후 적용한다. 변경 계획을 버리는 이동에는 확인을 제공한다. | PostgreSQL 800개 테이블, plugin 왕복, 실제 앱 필터 조회, browser pagination/read-only/stale response/retry/apply |
| MCP | 체크박스 표시와 클릭 영역 정렬, 한국어 도구 이름, 클라이언트마다 연결 방법. Codex TOML과 Claude/일반 JSON 설정 복사, 토큰을 포함하지 않는 명령 복사. 토큰은 클라이언트 ID로 구분한다. | browser 클릭/키보드/disabled/정렬/복사, 실제 native MCP HTTP 연결 및 조회·쓰기 거부 |

## 실행 결과

| 층 | 결과 | 증거 |
|---|---|---|
| Rust workspace | 329 통과, 0 실패, 7 ignored / 63 harness | [백엔드 보고서](verification/qa-2026-10-21/BACKEND.md), `cargo-test-final.log` |
| 실제 SSH/proxy/TLS | 8 통과, 0 실패. workspace와 일부 중복되는 별도 실환경 재실행 | `network-draft-secrets.log` |
| UI 전체 Vitest | 207 통과, 0 실패, 1 skipped / 52 files | `ui-tests.log` |
| WebKit overlay/MCP | 12 통과, 0 실패 | `webkit-overlays.log` |
| WebKit 사용자·권한 | 8 통과, 0 실패 | `webkit-roles.log` |
| 실제 macOS Tauri + PostgreSQL | 11개 단계 완료, `complete: true` | [native-results.json](verification/qa-2026-10-21/native-results.json) |
| 실제 native MCP HTTP | 12개 확인 통과 | `mcp-native-smoke.log` |
| 정적 검사·빌드 | TypeScript/Vite, Rust 앱 빌드, workspace clippy `-D warnings` 통과 | `ui-build.log`, `native-build.log`, `cargo-clippy-final.log` |

UI에서 skipped인 1개는 `--expose-gc`를 요구하는 heap 측정 테스트다. UI 브라우저 테스트는 mock DB 또는 명시적인 test-only fixture를 사용한다. WebKit 테스트는 설치된 Playwright WebKit 실행 파일을 사용했으며, macOS 앱 자체 검증은 별도로 Tauri의 시스템 WebKit에서 수행했다. 총합을 실제 DB 검증 건수로 해석하지 않는다.

네이티브 흐름은 연결 생성 → 실제 접속 테스트 → 저장·편집·취소 → 연결 → 테이블/뷰 자동 검색 → SQL 자동완성 → 결과 렌더 → 행 수정 미리보기 → 적용 취소 → 적용 후 DB 재조회 → 역할별 권한 필터 → 다른 연결 해제 시 상태 보존 순서다. 데이터는 loopback PostgreSQL fixture 안의 `qa_ui_*` 스키마에만 만들었다.

MCP 검증은 소유자 전용 discovery 파일, 인증 누락 401, 잘못된 Origin 403, initialize, 도구 목록, 연결 열기, 탐색, 정확한 int8 조회, read-only 쓰기 차단, 승인 만료 거부, DB 미변경, 에이전트 이력을 확인했다. Codex/Claude CLI의 등록 구문은 로컬 설치 CLI 도움말로 확인했다. 실제 Codex/Claude 모델 세션 호출은 수행하지 않았다. 사용자 에이전트 설정은 변경하지 않았다.

## 캡처

- [실제 앱: 메뉴와 적용된 행](verification/qa-2026-10-21/screenshots/native-menu-table.png)
- [실제 앱: MCP 연결 방법](verification/qa-2026-10-21/screenshots/native-mcp-guide.png)
- [사용자·권한: 밝은 테마](verification/qa-2026-10-21/screenshots/roles-daylight-1280.png)
- [사용자·권한: 좁은 화면](verification/qa-2026-10-21/screenshots/roles-midnight-960.png)
- [체크박스와 복사 안내](verification/qa-2026-10-21/screenshots/mcp-tools-connect.png)

검증용 앱과 이번에 생성한 DB·SSH·proxy·TLS 컨테이너는 정리했다. 수정된 앱은 기존에 저장된 프로필을 유지한 상태로 실행했다. 사용자 DB 접속은 자동으로 수행하지 않았다.

## 재실행

```sh
pnpm --dir ui install --frozen-lockfile
pnpm --dir ui test
pnpm --dir ui build
scripts/qa-backend-fixtures.sh start
eval "$(scripts/qa-backend-fixtures.sh env)"
export CARGO_TARGET_DIR=target-u4b
export AXDB_DRIVER_DIR="$PWD/target-u4b/debug"
cargo build --workspace --bins --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
TAURI_CONFIG='{"build":{"devUrl":null}}' cargo build -p axdb --bin axdb --locked
python3 scripts/qa-native.py
scripts/qa-backend-fixtures.sh stop
```

네트워크 fixture는 `scripts/test-network-setup.py`로 별도로 준비한다. `target-w1`에 driver binaries를 먼저 빌드한 후 `AXDB_TEST_FILTER=net::tests python3 scripts/test-network-run.py`로 실행한다. 실행 뒤 생성한 SSH/SOCKS/TLS 컨테이너와 임시 키를 정리한다.

WebKit 회귀는 `AXDB_QA_BROWSER=webkit`로 실행한다. 이 기기에서는 `AXDB_QA_WEBKIT_PATH=/Users/psw/Library/Caches/ms-playwright/webkit-2359/pw_run.sh`를 사용했다. 다른 기기는 설치된 Playwright 버전에 맞는 실행 파일을 사용한다. 로그 파일은 저장소의 기존 `*.log` ignore 정책에 따라 로컬 증거이며, 재실행으로 생성할 수 있다.

## 제한과 실행 중 발견한 문제

- 실제 사용자 DB의 연결 정보·데이터는 수정하지 않았다. 테스트 DB는 PostgreSQL/MySQL/MariaDB/MongoDB/Neo4j/Redis 컨테이너와 SQLite다.
- 7개 ignored 테스트와 환경 변수가 없어서 조기 반환하는 테스트를 전체 검증 성공으로 세지 않는다. 외부 OpenAI/Anthropic/OAuth/IAM 계정, MongoDB·Neo4j TLS 인증서, 파괴적 Neo4j 복원, 실제 업데이트 설치, OS keychain 실저장, 전체 화면의 수동 시각 검사는 미실행이다.
- 네트워크 초안 비밀값 테스트는 격리 secret store의 키가 생성되지 않는 것을 확인했다. 실제 사용자 키체인을 쓰지 않았다.
- 최초 native harness는 숨겨진 탭의 동명 버튼을 선택해 중단됐다. visible browse panel과 enabled 상태로 selector를 좁힌 뒤 실제 preview/cancel/apply까지 통과했다.
- 첫 네트워크 실행은 새 빌드 디렉터리에 driver binary가 없어 실패했다. binaries를 먼저 빌드한 뒤 실환경 네트워크 테스트를 재실행해 통과했다.
- 전체 UI 재실행에서 연결 진행 카드가 설정 메뉴를 가리는 실제 문제를 발견했다. 설정 레이어와 화면 높이 제약을 수정하고 같은 hit-test를 다시 실행했다.
- 현재 MCP 브리지는 다시 켤 때 주소가 달라진다. 연결 가이드에 새 주소로 설정을 다시 복사해야 한다는 안내를 표시했다.
