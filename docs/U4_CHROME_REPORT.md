# U4 CHROME 보고서

범위: 최종 감사 N-1(상태바 CallChip 누적), N-2/#14(주 버튼 tint), N-3(탭 바), N-6(설정 팝오버), N-9(페이지 크기 표기), #19(에이전트 클라이언트 이름 말줄임). 기준: `docs/FINAL_UI_AUDIT.md`, `docs/DESIGN.md`.
검증은 브라우저 모의(ego-browser)에서 960×600 · 1280×800 · 1440×900, 양쪽 테마로 했다. 네이티브 WebView 재촬영은 하지 않았다.

## 변경 요약

| 항목 | 원인 | 수정 |
|---|---|---|
| N-1 상태바 | `AgentPanel`이 최근 8개 CallChip을 상태바(`footer.statusbar .agent-controls`)에 그대로 렌더하고 만료가 없었다. 라벨 `MCP 활동`이 세로로 잘렸다. | 상태바에는 **최근 호출 칩 1개**만 둔다. 고정 최대 폭 260px·말줄임, 호출이 끝나면 4초 뒤 페이드아웃 후 제거, ≤1040px에서는 숨김. 칩을 누르면 에이전트 클라이언트 패널이 열린다. 전체 CallChip 이력(최신순, 50개 상한, 스크롤)은 에이전트 클라이언트 대화상자의 `MCP 호출 이력`에 둔다. 컨테이너의 `aria-label="MCP activity"`는 유지했고 `role="group"`만 추가했다. |
| 상태바 우선순위 | 모든 항목이 같이 줄어들었고 ≤1000px에서는 서버 버전을 숨겼다. | 연결 이름(`.status-name`, 최대 22ch)과 서버 버전(`.status-driver`, 최대 30ch)은 줄어들지 않는다. 실행 요약·에이전트 컨트롤이 먼저 줄어든다. ≤1000px의 버전 숨김 규칙을 제거했다. 에이전트 컨트롤 안의 체크박스가 28px 높이로 상태바(24px)를 넘던 것을 20px로 맞췄다. |
| N-2/#14 주 버튼 | `variant="brand"`는 이미 전달되고 있었다. 원인은 **CSS 특이도**: `.dialog-footer>button:last-child`(0,2,1)와 `.profile-form .toolbar>button:first-child`(0,2,1)가 `:root .ax-button--brand`(0,2,0)의 배경·글자색을 덮었다. AI 설정 저장은 variant 자체가 없었다. | 두 규칙에 `:not(.ax-button--brand)`를 붙였다. AI `설정 저장`에 `variant="brand"`를 줬다. 연결 저장·에이전트 클라이언트 저장/닫기·AI 설정 저장이 양쪽 테마에서 채워진 brand 버튼이다. |
| N-3 탭 바 | 가로 스크롤이 탭 경계와 무관하게 멈춰 잘린 탭 조각이 왼쪽에 남았다. 스크롤바도 숨겨져 있다. | 스크롤이 멈추면(140ms 디바운스, 스크롤 이벤트에서만 동작) 가장 가까운 탭 경계로 부드럽게 맞춘다. 활성 탭이 보이는 상태면 그것이 잘리지 않는 경계만 쓴다. 끝 위치도 탭 경계에 오도록 오른쪽 여백을 계산한다. 양 끝에 28px 페이드(`data-edge="start end"`, mask)로 더 있음을 표시한다. 유휴 시 타이머·rAF 없음. |
| N-6 설정 팝오버 | GlideSelect 메뉴 최소 폭이 176px라 130px 트리거 기준으로 팝오버 밖으로 나갔다. | `GlideSelect`에 `fitTrigger` 옵션을 추가하고 테마 선택에 썼다. 메뉴가 트리거 폭(130px)이라 팝오버(56–336) 안(189–319)에 들어온다. |
| N-9 페이지 크기 | 옵션을 문자열 그대로 렌더했다. | 옵션 라벨을 `toLocaleString()`으로 만들었다(`1,000`). 값은 그대로 `'1000'`이다. |
| #19 클라이언트 이름 | 대화상자 노드(`integration-beam__label`)가 한 줄 말줄임이었다. 클라이언트 목록 이름도 말줄임이었다. | 에이전트 클라이언트 안에서 노드 라벨을 2줄까지 줄바꿈하고, 라벨 `title`에 전체 이름을 넣었다(`IntegrationBeam`에 `title` 한 줄 추가). 목록의 이름은 줄바꿈 허용(`title` 유지). `기본(로컬) 클라이언트`를 넣어 보니 2줄(28px)로 완전히 읽힌다. |

## 모의 전용 도구
`ui/src/mock.ts`의 `simulateAgentCalls(count, emit)`과 `AgentPanel`의 `window.__axdbSimulateAgentCalls(n)`는 `isMock`일 때만 등록된다. 콘솔에서 `__axdbSimulateAgentCalls(30)`을 실행하면 MCP 호출 30건(running → success/error)이 흐른다. 네이티브 경로·번들 동작에는 영향이 없다.

## 검증
- `pnpm --dir ui exec tsc --noEmit` 통과, `pnpm --dir ui test` 19 파일 90 테스트 통과(신규 `agent/callChipProps.test.ts`).
- 빌드 확인은 `vite build --outDir <scratchpad>/u4-chrome`로만 했다(`ui/dist`에는 쓰지 않음).
- overlap-probe: 960×600(결과 24행, 알림, 호출 30건)에서 상태바 관련 `viewport-overflow`·`text-clipping`이 모두 사라졌다. 모달이 열려 있을 때 나오는 레일 버튼 occlusion, 첫 입력의 `focus-ring-clipped`, 열린 GlideSelect 메뉴가 슬라이더 위에 겹치는 항목은 기존과 같은 성격이라 이번 범위에서 손대지 않았다.
- 상태바(호출 30건): 960×600에서 이름 28–109, 버전 119–198로 유지, 칩은 숨김. 1280×800에서 이름·버전 유지, 칩 1개가 975–1223로 창 안에 있다.
- 탭 8개 스크롤: 960/1280/1440에서 모든 탭 왼쪽 경계가 0 또는 정수 탭 폭 배수 위치에 놓인다.

## 스크린샷 (이전 → 이후)

| 항목 | 이전 | 이후 |
|---|---|---|
| 상태바 960×600 | [final-13](design-shots/final-13-statusbar-chips-overflow-960x600-midnight.png), [final-17](design-shots/final-17-960x600-daylight-tabs-statusbar.png) | [midnight](design-shots/u4-chrome-status-960-midnight.png), [daylight](design-shots/u4-chrome-status-960-daylight.png) |
| 상태바 1280×800 | [final-11](design-shots/final-11-callchip-done-error-statusbar-1280-midnight.png) | [midnight](design-shots/u4-chrome-status-1280-midnight.png), [daylight](design-shots/u4-chrome-status-1280-daylight.png) |
| 호출 이력 패널 | [final-09](design-shots/final-09-agent-clients-dialog-1280-midnight.png) | [daylight](design-shots/u4-chrome-agent-history-1280-daylight.png), [midnight](design-shots/u4-chrome-agent-history-1280-midnight.png) |
| 연결 대화상자 저장 | [final-05](design-shots/final-05-glideselect-access-flip-1280-daylight.png) | [midnight](design-shots/u4-chrome-connection-dialog-1280-midnight.png) |
| AI 설정 저장 | [final-12](design-shots/final-12-ai-panel-logged-out-1280-midnight.png) | [daylight](design-shots/u4-chrome-ai-settings-1280-daylight.png) |
| 탭 8개 | [final-15](design-shots/final-15-tabs-7-1280-midnight.png) | [1280](design-shots/u4-chrome-tabs-8-1280-midnight.png), [960](design-shots/u4-chrome-tabs-8-960-daylight.png) |
| 설정 테마 메뉴 | [final-06](design-shots/final-06-settings-theme-menu-1280-daylight.png) | [daylight](design-shots/u4-chrome-settings-theme-menu-1280-daylight.png) |

## 남은 것
- 네이티브 WebView(WKWebView)에서 brand 버튼·탭 마스크·`-webkit-line-clamp`가 같게 보이는지 재촬영이 필요하다. 원인이 CSS 특이도라 같은 결과가 예상되지만 확인하지 못했다.
- 탭 끝 여백(최대 탭 한 칸 폭)은 탭이 넘칠 때만 생기며, 끝 스크롤에서 마지막 탭 오른쪽이 비어 보일 수 있다.
