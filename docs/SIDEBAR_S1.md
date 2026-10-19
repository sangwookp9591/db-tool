# S1 사이드바와 MCP 스크롤 수정

사이드바를 연결·탐색기 두 섹션으로 나누고 각각 접기 버튼(`aria-expanded`), 포인터·↑/↓ 키로 조절하는 수평 separator, 컴팩트 보기와 활성 연결 상단 고정을 추가했다. 접힘·분할 비율·컴팩트 설정만 `axdb.sidebar` localStorage에 저장하며 읽기·쓰기 오류와 손상된 값은 기본값으로 처리한다. 새 스타일은 `ui/src/sidebar.css`에 격리했고 `style.css`, App, onboarding 및 다른 워커 담당 파일은 수정하지 않았다. 새 컨트롤은 애니메이션 없이 즉시 상태가 바뀌며 reduced-motion에서도 동작한다.

MCP 문제는 대화상자 본문의 proximity scroll snap과 상단 맵의 snap 지점 때문에 재현됐다. 수정 전 960×600 Chromium mock에서 콘텐츠 높이 809px / 본문 높이 386px 상태로 60px 아래 wheel을 네 번 보내도 `scrollTop`은 0, 0, 0, 0이었다. ClientsPanel 전용 Dialog 클래스에서 본문의 snap을 끄고 overscroll을 contain으로 설정했으며 호출 이력의 높이 제한과 중첩 스크롤을 제거했다. Dialog의 열림 효과는 `open`이 바뀔 때만 실행되어 스크롤 중 재렌더링으로 재개방되지 않는다.

`pnpm --dir ui exec tsc --noEmit` 통과. 요청한 `pnpm --dir ui test`는 실행 당시 100개 통과·2개 실패였고 실패는 병렬 O1 소유 `src/onboarding/onboarding.test.ts`의 timeout과 닫은 후 animation 개수 검사였다. 코디네이터에게 전달했으며 S1 완료 판단에서 제외하도록 회신받았다. onboarding을 제외한 별도 검사는 `pnpm --dir ui exec vitest run --exclude 'src/onboarding/**'`로 21개 파일·99개 테스트가 통과했다. 이 별도 결과는 전체 테스트 통과를 의미하지 않는다.

별도 mock Vite 서버에서 `cd ui && node sidebar-browser.mjs http://127.0.0.1:5191`로 24개 연결, 960×600 및 1280×800, 섹션 접기·펼치기, 포인터·키보드 분할, 컴팩트 행 높이, 활성 연결 sticky 고정, 설정 복원, 손상 설정 및 storage 쓰기 실패, 유휴 애니메이션 0을 확인했다. MCP의 동일한 60px wheel 네 번은 60, 120, 180, 240px로 전진했고 calls 재렌더링 뒤에도 240px을 유지했다. 호출 이력은 별도 스크롤 영역이 아니며 본문 끝에서 wheel을 보내도 문서가 스크롤되지 않았다. 페이지 JavaScript 오류는 없었다.

스크린샷: [확장](design-shots/sidebar-960-expanded.png), [컴팩트](design-shots/sidebar-960-compact.png), [연결 접힘](design-shots/sidebar-960-connections-collapsed.png), [1280](design-shots/sidebar-1280-compact.png), [MCP 스크롤](design-shots/sidebar-mcp-scroll.png).

수정 파일은 `ui/src/Connections.tsx`, `ui/src/Dialog.tsx`, `ui/src/agent/ClientsPanel.tsx`, `ui/src/sidebar.css`, `ui/src/sidebar.ts`, `ui/src/sidebar.test.ts`, `ui/sidebar-browser.mjs`, 이 보고서 및 위 5개 이미지다. 앱 GUI·실제 DB·Tauri WebKit의 물리 트랙패드 탄성 스크롤은 확인하지 않았고 commit/push는 하지 않았다. Chromium mock 검증이 네이티브 WebKit 검증을 대신하지 않는다.
