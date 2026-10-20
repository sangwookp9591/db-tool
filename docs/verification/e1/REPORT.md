# E1 병합 main 실제 앱 E2E 회귀

2026-10-20, main `3e169e7fa9002434f9ef145eb73f3692db675539`에서 D9·D10 병합 후 실제 debug 앱 E2E를 다시 실행했다. **bridge 21/21, preview 5/5, permissions 13/13, 합계 39/39 통과**이며 모든 스위트와 실행 하네스의 종료 코드는 0이다. 기존 세 스위트의 실패·재시도·테스트 약화·작업자 제품 코드 수정·커밋은 없다.

[집계](regressions.json), [bridge 로그](e2e-bridge.txt), [preview 로그](preview-e2e.txt), [permissions 로그](permissions-e2e.txt), [실행·정리 로그](run.txt)를 남겼다. 실제 Tauri 앱과 드라이버, 테스트 PostgreSQL·Neo4j·임시 SQLite를 사용했다. permissions는 해당 앱 PID의 네이티브 UI에서 승인·거절·Run을 조작하고, DB 상태·응답·감사 로그로 결과를 확인했다.

## 재현과 격리

[D10 보고서](../../D10_REPORT.md) 및 `bench/d10-regressions.mjs`의 프로필·fixture·debug 비밀 파일 절차를 따랐다. 기존 `bench/e2e-bridge.mjs`, `bench/preview-e2e.mjs`, `bench/permissions-e2e.mjs`는 변경하지 않고 그대로 실행했다. [재현 하네스](run.mjs)는 D10 하네스의 target·결과 경로를 E1에 맞추고 통과 수 검증과 종료 정리를 추가한 것이다.

```sh
pnpm --dir ui build
CARGO_TARGET_DIR=/Users/iron/Project/psw/db-tool/target-d7 \
AXDB_DRIVER_DIR=/Users/iron/Project/psw/db-tool/target-d7/debug \
CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 \
TAURI_CONFIG='{"build":{"devUrl":null}}' cargo build --workspace --bins
CARGO_TARGET_DIR=/Users/iron/Project/psw/db-tool/target-d7 \
AXDB_DRIVER_DIR=/Users/iron/Project/psw/db-tool/target-d7/debug \
node docs/verification/e1/run.mjs
```

[UI 빌드](ui-build.txt)와 [workspace bins 빌드](build.txt)는 exit 0이다. debug assertions를 유지했다. [검증 바이너리 해시](binaries.sha256)를 기록했다. 빌드에는 기존 vendored neo4rs deprecated 경고가 있다.

매 실행마다 새 임시 `AXDB_CONFIG_DIR`을 만들고 `AXDB_AGENT_BRIDGE=1`, `AXDB_DEV_SECRET_STORE`를 설정한다. PG는 `127.0.0.1:55432/axdb_test`, Neo4j는 `bolt://127.0.0.1:57687`, SQLite는 임시 config 아래 100행 fixture만 사용했다. 사용자 DB·설치된 `/Applications/AxDB.app`·사용자 설정 디렉터리는 대상으로 삼지 않았다. 비밀 파일은 mode 0600으로 생성하고 결과 로그에서 fixture 비밀을 가렸다.

## 정리와 변경 범위

[정리 검증](cleanup.json): 후보 PID 종료 및 임시 config 전체 삭제를 확인했다. 테스트 키체인 항목은 0개이며 OS 키체인 인증을 검증한 실행은 아니다. PG 2행·25만 행 및 원본 NULL 상태, preview 컬럼 부재, Neo4j 603명 및 임시 쓰기 라벨 0개에 대한 [실행 전](fixtures-before.txt)·[실행 후](fixtures-after.txt) 조회 출력은 동일하다. 스위트 내부에서도 UPDATE/DDL/CREATE/DELETE 미리보기 뒤 원본 상태 유지와 거절·권한 회수 뒤 쓰기 차단을 확인했다.

최초 main E2E 종료 시 시작부터 수정돼 있던 `README.md`와 `bench/results-w4b-streaming-plugin.json`의 SHA-256이 동일함을 확인했다. 추가 검증 종료 시 벤치 JSON은 여전히 동일하고, README에는 다른 작업의 테스트 수 badge 수정이 추가돼 해시가 달라졌다. 이 작업자는 README를 쓰지 않았다. 이 작업자가 추가한 파일은 `docs/verification/e1/`의 검증 증거뿐이다. 실행 도중 코디네이터가 변경한 `ui/src/App.tsx`, `ui/src/style.css` 및 버전 파일(`Cargo.toml`, `src-tauri/tauri.conf.json`, `ui/package.json`)은 그대로 보존했다.

전체 Rust/UI 테스트·clippy·TLS·OS 키체인·실 AI 공급자 인증은 이번 범위에서 실행하지 않았다. 이 결과는 세 기존 실제 앱 E2E의 증거이며 전체 기능 또는 배포 환경 검증으로 확장하지 않는다.

## 추가 요청: 끊긴 탭 재연결 화면

최초 main 기준 E2E 39/39 증거는 [main/regressions.json](main/regressions.json)과 `main/` 아래 원본 로그·바이너리 해시로 보존했다. 이후 코디네이터가 수정한 disconnected UI를 빌드한 뒤 세 스위트를 다시 실행했으며 **bridge 21/21, preview 5/5, permissions 13/13, 총 39/39**가 다시 통과했다. 위 본문의 최상위 집계·E2E 로그·바이너리 해시는 이 두 번째 후보 앱 실행이다. UI 수정 뒤 [UI 빌드](reconnect-ui-build.txt)와 [앱 빌드](reconnect-build.txt)도 exit 0이다.

[별도 네이티브 검사](disconnected.json)는 **5/5** 통과했다. 상태바의 연결 해제로 기존 쿼리 탭을 유지한 끊긴 화면을 만들고, [미드나이트](disconnected-midnight.png)·[데이라이트](disconnected-daylight.png)를 캡처했다. 두 이미지를 직접 확인했으며 로고 타일·제목·연결 안내·새로고침 아이콘 버튼이 중앙에 정렬되어 추가 제품 수정은 하지 않았다. 다시 연결 클릭 응답의 [네이티브 트리](disconnected-progress.txt)에 `실제 연결 진행`, `연결됨 ✓`, DNS·DB 인증·권한 확인 단계가 표시됐다. [복구 화면](disconnected-restored.png)의 기존 SELECT를 Run으로 실행한 뒤 UI 실행 이력에서 `SELECT 42 AS reconnect_check`, `row_count: 1`, `error: null`을 확인했다.

[캡처 하네스](disconnected.mjs)의 최초 실행은 테마 선택 항목을 option/button 역할로 찾았으나 macOS 접근성 트리는 이를 text로 노출해 15초 탐색 제한에서 종료됐다. [최초 실패 로그](disconnected-harness-first-attempt.txt)와 [상태 트리](disconnected-failure.txt)를 보존했고, 실제 역할에 맞춘 선택자만 수정한 뒤 같은 검증을 재실행해 통과했다. 타임아웃·검증 assertion·제품 코드는 바꾸지 않았다. [성공 실행 로그](disconnected-run.txt)를 별도로 남겼고, 두 시도 모두 후보 앱·임시 config·debug 비밀 파일을 정리했다.

검증 바이너리는 **0.3.0 main + 15:35:54 시점의 코디네이터 disconnected UI 수정**이다. 추가 검증 종료 시 App/CSS에도 후속 수정이 있음을 관측했으며 이 후속 수정은 이 캡처에 포함되지 않는다. 마지막 검증 중 워크트리 버전 파일이 별도로 0.3.1로 바뀐 것을 확인했으나 그 버전 변경을 포함해 다시 빌드하지 않았다. 따라서 이 증거를 0.3.1 릴리스 바이너리 검증으로 해석하면 안 된다. 진행 중 loading 버튼은 빠른 로컬 연결에서 별도 프레임으로 캡처하지 않았으며, 연결 진행 팝업과 복구 완료는 실제 네이티브 응답으로 확인했다.
