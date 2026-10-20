# D9 — AI 다중 파일 검토와 도구 실행 승인

2026-10-20, main `2faf7a6`의 깨끗한 작업 트리에서 시작했다. README·DESIGN·ADR-0004와 D7·D8의 미구현 범위를 확인했다. 본 worker는 commit/push하지 않았고, 공유 HEAD가 다른 작업으로 `8eab089`까지 전진한 변경과 코디네이터의 헤더 CSS도 보존했다. 사용자 DB와 `/Applications/AxDB.app` 파일은 변경하지 않았다. 빌드는 `target-d7`, 실행 프로브는 임시 설정과 127.0.0.1:55432/axdb_test만 사용했다.

여러 SQL 블록·파일명 fence·파일명 제목을 파일 카드로 묶었다. 변경 수·추가/삭제 수, 40px 파일 행, CodeMirror 하이라이트·줄 번호를 쓰는 통합 diff, 파일별/전체 수락·거절과 직후 undo를 제공한다. 열린 탭이나 실제 저장 시트에서 대상을 고르며, 미개방 파일은 선택 경로를 읽고 수락할 때 탭을 연다. 파일별 한 트랜잭션으로 dirty만 남기고 자동 저장·실행하지 않는다. 원본·연결이 달라지거나 수락 후 편집되면 덮어쓰기·카드 undo를 거부한다. macOS Undo/Redo 메뉴는 쿼리 편집기에서만 CodeMirror 이력으로 연결하며, 네이티브 메뉴가 잠시 창 포커스를 가져가도 실제 편집 DOM으로 대상을 판별한다.

쿼리와 tool/mcp JSON 제안은 명령·이유·연결·권한을 승인 카드로 검토한다. 호스트는 기존 `permissions::check`를 재사용하고 MCP 도구는 기존 브리지 dispatch를 거친다. 읽기 전용 요청과 에이전트의 행·그래프·표본 상한을 보존하며, 미리보기 전용 에이전트의 적용은 UI와 서버 모두 차단한다. 읽기만 세션 재확인을 생략할 수 있다. 쓰기는 기존 롤백 미리보기·fingerprint·TTL·일회용 승인 후 실행한다. 결정된 호출은 ToolCallGroup과 행 수·소요 시간으로 접고, 감사 로그에는 결정·도구·연결 ID·SQL 해시만 남긴다.

최신 사용자 지시에 따라 파괴적 작업의 1.2초 hold를 React Bits FuseButton 포트로 교체했다. `허용 → 4초 fuseEnd → 미리보기`이며 버튼·Esc 취소, 호버/숨김 일시 정지, reduced-motion 초 카운트다운과 유휴 애니메이션 0을 제공한다. 승인 UI가 숨겨지거나 언마운트되면 예약을 취소한다. 기존 controlled FuseButton API와 삭제 호출부는 유지했다. Hugeicons·React·motion·Tailwind 의존성은 추가하지 않았고 lucide·앱 토큰을 사용했다. [공식 라이선스](https://github.com/DavidHDev/react-bits/blob/main/LICENSE.md)의 MIT + Commons Clause를 확인해 THIRD_PARTY_NOTICES에 출처를 기록했다.

후속 모델 UI는 최신 ChatGPT 레퍼런스의 모델·Effort 두 pill로 구현했다. 검색·공급자 서브메뉴·연결되지 않은 공급자의 계정 연결, 지원 Effort 목록과 ⇧⌘M·⇧⌘.를 제공한다. 헤더 상태에서도 같은 메뉴를 연다. 기본값 저장은 기존 저장소를 공유하고, 끄면 현재 연결 세션에만 적용한다. 요청마다 모델·effort를 호스트에서 검증하며 저장된 AI 비활성 상태를 우회할 수 없다. 헤더 높이·줄 높이 28px, 긴 제목의 자연 폭 228.93px / 표시 폭 162.89px에서 말줄임도 확인했다.

검증은 tsc 통과, 전체 UI `pnpm --dir ui test --maxWorkers=1` 160 passed / 1 skipped, 변경한 `axdb` 크레이트 전체 Rust 204 passed / 7 ignored다. 정상 병렬 UI 시도는 다른 저장소 Rust 빌드와 경합해 기존 테스트를 포함한 4개 파일이 timeout했고, timeout 증가 없이 워커 수를 제한해 전부 실행했다. 첫 Rust 통합 시도의 디스크 부족도 캐시 정리 후 전체 재검증했다. 헤더/서브메뉴의 지연 포커스와 네이티브 Undo의 포커스 경계는 구현과 회귀 테스트로 고쳤다.

최종 [WKWebView 검증 JSON](design-shots/d9-native-result.json)은 9개 시나리오 모두 통과했다. 실제 저장 시트로 3개 제안 중 1개 거절·2개 수락, dirty·실제 ⌘Z·일괄 undo와 기존 탭 재사용을 확인했다. 읽기 승인·재확인 생략, Fuse arm·호버 이벤트 정지·실제 Esc 취소 시 미리보기 없음, 재arm·4초 fuseEnd·롤백 미리보기·추가 승인·DELETE 실행, 저장된 읽기 MCP 에이전트의 lint_query, 실제 연결의 읽기 전용 허용 비활성도 통과했다. 별도 psql로 미리보기 중 2행 유지·적용 뒤 0행을 확인하고 만든 테이블만 삭제했다. SQL 파일 자동 쓰기 없음과 감사 로그의 SQL 원문·비밀번호·토큰 부재도 확인했다. 모델 선택과 기본값 저장 후 실제 요청 metadata의 `gpt-6.1-sol` / `xhigh`도 확인했다. 호버는 Chromium 실제 포인터와 WKWebView 입력 이벤트로 검증했다.

캡처: [파일 검토](design-shots/d9-files-pending-midnight.png), [수락·undo](design-shots/d9-files-accepted-daylight.png), [읽기 승인](design-shots/d9-read-approval-daylight.png), [Fuse Midnight](design-shots/d9-fuse-armed-midnight.png) / [Daylight](design-shots/d9-fuse-armed-daylight.png), [Fuse 취소](design-shots/d9-fuse-cancelled-daylight.png), [DELETE 미리보기](design-shots/d9-delete-preview-midnight.png), [읽기 전용 차단](design-shots/d9-readonly-disabled-daylight.png), [모델 메뉴](design-shots/d9-model-menu-midnight.png), [Effort 메뉴](design-shots/d9-effort-menu-daylight.png). 각 화면은 두 테마 파일을 남겼다.

재현은 `vite build --emptyOutDir --outDir ../target-d7/ui`, `CARGO_TARGET_DIR=target-d7 CARGO_INCREMENTAL=0`과 devUrl=null·frontendDist=target-d7/ui·identifier=dev.axdb.d9probe의 TAURI_CONFIG로 `cargo build -p axdb`, `swiftc bench/d9_input.swift -o target-d7/d9-input`, `python3 bench/d9_native.py` 순서다. 프로브는 자신의 PID와 테스트 DB만 사용하고 종료한다. 공급자는 debug fixture이므로 실제 계정·과금·OAuth·공급자 서버 검증이 아니며 Windows 앱도 검증하지 않았다.
