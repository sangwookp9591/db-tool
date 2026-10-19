# W6 UI 보고서

2026-10-19 · 계약: [ADR-0004](adr/0004-ax-native-workbench.md), [ADR-0003](adr/0003-installable-drivers.md), [아키텍처](ARCHITECTURE.md). 이전 워커의 `ui/src/ax/` 구현을 이어서 완성했다. 공통 계획 카드와 승인 대화상자, capability 기반 워크벤치·⌘K, 단계형 연결·선택 리전의 IAM 프로필·스마트 붙여넣기·SSH 지문 확인·연결 타임라인, 계획 트리·비용·린트·quick fix·리팩터링 검토, 파일 드롭·새/기존 테이블 가져오기, 스키마 diff·DDL·스냅숏·마이그레이션, 공급자 로그인·모델·사용량·MCP 안내를 연결했다. 백업·복원과 권한 행렬은 W4 엔진 연결 대기를 명시한 화면/mock이다.

AI 응답은 `ai_operation`의 계획 카드로만 표시하며 직접 실행하지 않는다. 쿼리 검토는 기존 `aiSuggested` 경로의 새 탭으로 보내고 기본 실행은 읽기 전용이다. 적용은 같은 계획의 미리보기와 별도 승인 뒤에만 가능하며 파괴적 작업은 연결 이름 입력이 필요하다. 중복 적용 클릭, 미리보기 이후 계획 변경, 초기 effect가 사용자 입력을 지우는 경쟁 상태를 방어했다. 연결 진단에는 단계·시간·허용된 오류 코드만 보낸다. W1의 선택 network.code 계약을 받아 SSH_HOST_KEY_CHANGED는 재연결 표시 대신 연결 차단 alert로 안내하며, 이전 이벤트 호환성 단위 테스트와 mock 브라우저의 차단 경고·지문 거부 검사를 통과했다. 탐색 대상은 드라이버가 반환한 ID로 찾고 모호하거나 찾지 못한 이름은 추정하지 않는다.

검증: `pnpm --dir ui exec tsc --noEmit` 통과, `pnpm --dir ui test` 최종 26파일·122테스트 통과(33.60초), `node ui/ax-e2e.mjs` 통과, `git diff --check -- ui src-tauri/tauri.conf.json` 통과. 후속 W1 계약에 맞춘 IAM 리전 필수 표시 제거는 tsc와 확장 E2E로 재확인했다. Playwright는 격리 Chromium·mock·reduced-motion에서 두 테마, 최소 960×600 가로 넘침 없음, SSH 확인, 린트 quick fix·⌘., 탐색기 파일 드롭, 새 테이블 생성 계획의 승인, 스키마 diff, Claude/OpenAI 모의 로그인, AI 제안의 무실행/읽기 전용 새 탭을 확인했다. 주요 20화면 × 2테마 = 40장 모두 존재하며 [실행 계획](design-shots/ax-query-plan-midnight.png), [SSH 지문](design-shots/ax-host-key-midnight.png), [스키마 diff](design-shots/ax-schema-diff-daylight.png), [최소 창](design-shots/ax-minimum-960-daylight.png) 등을 직접 확인했다.

중간 실패도 남긴다. 전체 UI 검사에서 온보딩 테스트의 5초 timeout이 발생했으며, 화면 갱신 직후 Enter가 이전 step closure를 사용해 2단계를 건너뛰는 것을 별도 재현했다. 키 리스너를 `useLayoutEffect`로 갱신하고 렌더 경계 회귀 테스트를 추가해 해당 11테스트와 최종 전체 검사가 통과했다. 다음 전체 검사에서는 탐색기 페이지 추가 직후의 즉시 count가 300행을 읽었으므로 화면 밖 마지막 행의 노출 대기는 별도 timeout을 만들었으므로 실제 450번째 행의 DOM 생성(attached)을 기다리도록 기존 테스트를 고쳤고 해당 10테스트와 최종 전체 검사가 통과했다. timeout 증대, retry-to-pass, forceExit는 사용하지 않았다. E2E의 중복 진행 순서와 전역 option 선택도 수정했으며 최종 실행은 모든 검증을 끝까지 통과했다.

변경 경계: 시작부터 추적 파일 40개와 여러 미추적 파일이 dirty였다. 재시작 전 워커의 동시 UI 편집으로 생긴 중복 선언을 정리한 뒤 코디네이터가 구 W6 종료를 확인했고, 다른 lane의 Rust·공유 변경은 보존했다. 보고 시점 전체 상태는 추적 변경 52개·미추적 항목 76개이며 동시 작업 결과도 포함하므로 전부 W6 변경은 아니다. W6 추가 연결은 AX 폴더, App/Connections/QueryTab 등 UI 접점, 온보딩 키보드 수정·관련 테스트와 `ui/ax-e2e.mjs`다. Windows HTML5 파일 드롭에 필요하다는 설치된 Tauri 공식 소스에 따라 `src-tauri/tauri.conf.json`에는 `dragDropEnabled:false` 한 필드를 추가했다. 새 의존성·commit·push는 없다.

미검증: 이 워커는 Rust cargo test/clippy, 실제 DB/SSH/IAM, 네이티브 WebView/Windows 드롭, 실제 OAuth/API 공급자·OS 키체인, 설치된 사용자 앱을 실행하거나 검증하지 않았다. `dist/`, `target/`, `target-rel/`과 사용자 DB·SSH 호스트·GUI는 건드리지 않았다. 브라우저 mock은 실제 인증·DB 트랜잭션·백업/복원/권한 적용의 증거가 아니며, 해당 엔진과 네이티브 통합은 각 담당 lane 및 독립 검증에서 확인해야 한다. 계획 대상이 중복되거나 인덱스만의 이름으로 반환되어 안전한 탐색 대상을 찾을 수 없으면 탐색기 직접 선택 안내를 표시한다.

검증용으로 이 워커가 실행한 Vite 서버(51987)는 검증 종료 후 정리했다. 기존 서버와 코디네이터 빌드는 건드리지 않았다.
