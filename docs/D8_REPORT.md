# D8 — AI 권한 메뉴와 맥락 줄 마무리

2026-10-20, main의 기존 D7 미커밋 변경 위에서 작업했다. D7 보고서와 네 캡처, README·디자인 규칙·호출부를 확인했다. 권한 옵션을 16px 아이콘 / 제목 / 12px muted 설명 / 선택 체크의 동일한 행으로 통일했고, 8px 12px 패딩과 8px radius, 같은 hover·선택 배경을 적용했다. 공통 안내는 구분선 아래 footnote이며 메뉴는 280px 폭으로 칩 위 8px에 정렬된다. 읽기 전용 연결의 쓰기 옵션은 비활성·opacity·이유를 표시한다. menu/menuitemradio, ↑↓ 이동, Enter 선택, Esc 닫기 및 포커스 복귀, 바깥 클릭 닫기를 지원한다.

행 포함 토글은 맥락 줄 맨 앞에 고정하고 나머지 칩만 가로 스크롤한다. 스크롤바는 숨기고 양 끝 fade mask를 적용했다. 맥락 줄의 불투명 패널 배경·하단 hairline과 메시지 영역의 overflow 격리로 이전 메시지 잔상을 제거했다. 캡처 검토에서 확인한 토글 내부 중복 테두리와 권한 설명 줄바꿈도 수정했다.

`pnpm --dir ui exec tsc --noEmit` 통과, 전체 `pnpm --dir ui test`는 150 passed / 1 skipped다. 추가 Chromium 테스트는 실제 키보드 Enter·화살표·Esc와 선택 상태·포커스 복귀를 검증한다. `target-d7` UI 빌드와 debug 호스트 빌드가 통과했고, 별도 임시 설정·debug 앱/WKWebView·127.0.0.1:55432 테스트 DB에서 기존 D7 검증에 메뉴 의미·키보드·280px/8px 실측·고정 토글·메시지 경계·실제 읽기 전용 연결 검증을 추가했다. 초기 하네스의 API 프로필 변경은 화면 상태를 갱신하지 않아 실패했으며 실제 연결 편집·접근 모드 변경·재연결 경로로 고쳤다. 실패를 숨기는 retry나 timeout 증가는 없다.

[Midnight](design-shots/d7-chat-midnight.png), [Daylight](design-shots/d7-chat-daylight.png), [권한 Midnight](design-shots/d7-chat-permission-midnight.png), [권한 Daylight](design-shots/d7-chat-permission-daylight.png)를 덮어썼고 [읽기 전용 Midnight](design-shots/d8-permission-readonly-midnight.png)을 추가했다. 네이티브 실행 증거는 [검증 JSON](design-shots/d7-native-result.json)이다. D8 소유 변경은 PermissionMode.tsx, AssistantPanel.tsx·css, permissionMode.test.ts, perf/d7.ts, bench/d7_native.py, 본 보고서와 캡처·검증 JSON이다. 기존 다른 D7 변경을 되돌리지 않았으며 commit/push하지 않았다.

코디네이터 후속 요청으로 ConnectSteps.css·ConnectionLive.css 수정본을 보존한 채 추가 검증했다. 실제 debug 앱의 연결 완료 팝업은 중첩 테두리 없이 16px 안쪽 여백과 1열 단계·오른쪽 시간 정렬을 확인했다. 실제 연결 편집의 연결 테스트 성공과 AI 계정 연결 모달의 초기 단계도 두 테마에서 캡처했다: `design-shots/d8-connect-live-*.png`, `d8-connect-test-*.png`, `d8-connect-account-*.png`. AI 로그인은 시작하지 않았다. 추가 하네스에서 비밀번호 입력 직후 렌더 전 테스트 버튼을 눌러 생긴 인증 실패는 다음 프레임에서 입력 상태 반영 후 실행하도록 고쳤다.

사용자 DB와 /Applications/AxDB.app은 건드리지 않았다. 실제 AI 공급자 호출과 전체 Rust 테스트 재실행은 이번 UI 범위에서 제외했다. 네이티브 AI 응답은 debug fixture이므로 공급자 검증 증거가 아니다.
