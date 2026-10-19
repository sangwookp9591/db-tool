# W6b AX 워크벤치 디자인 다듬기

2026-10-19. [W6 보고서](W6_REPORT.md), [ADR-0004의 UX 언어](adr/0004-ax-native-workbench.md#ux-언어-ax-느낌), [디자인 시스템](DESIGN.md)을 기준으로 기존 기능과 승인 경계를 유지하며 시각 구성을 다듬었다. 워크벤치와 결과/계획 탭은 드라이버 화면의 `JellyRadio segmented`를 재사용해 방향키 이동까지 통일했다. 연결/가져오기 단계에는 번호 원·연결선·완료 상태를 넣었고, 연결 폼의 중복 접이식 제목을 숨겼다. 테스트 버튼은 다시 실행할 수 있는 “연결 테스트”로 유지하고 결과는 별도 “테스트 통과/실패” 상태로 표시한다. 기존 SSH 인증 방식별 조건부 입력은 보존하고 키·비밀번호·agent 모드의 표시를 확인했다.

실행 계획은 총비용(루트의 추정값), 최대 비용 노드, 경고 수를 요약하고 실제 PlanTree의 들여쓰기·연결선·최대 대비 비용 막대·예상/실제 행 수 칩을 표시한다. 부모에서 비용이 가장 큰 자식으로 이어지는 경로를 강조하며 선택한 노드의 상세, 10배 이상 행 수 오차 경고, 탐색기 이동을 제공한다. 비용이 없는 노드는 값을 만들어 넣지 않는다. 기존 비용 aria-label과 progressbar 의미를 유지했다. 데모 문구는 `isMock`일 때만 한국어로 표시하고, mock 예시는 두 단계 트리로 확장했다.

AI 제안은 입력과 검토 카드를 나란히 배치하고 좁은 창에서는 한 열로 내려간다. 카드에 AI 생성 아이콘, 한국어 작업 제목, 읽기/쓰기/파괴적 변경의 서로 다른 위험 배지, 근거 칩, 영향 영역과 주/보조 동작을 넣었다. 행·객체 수를 제공하지 않는 AI 계약에서는 수치를 꾸며내지 않고 미리보기에서 확인하도록 안내하며, 실제로 아는 제안 문장 수만 표시한다. 연결 테스트·마이그레이션 타임라인, 스키마 변경 트리, 가져오기 표, 명령 팔레트, AI 계정 카드도 같은 토큰·간격·상태 색을 쓴다. 백업/복원·권한 TSX 로직은 수정하지 않았고 W4의 동시 계약 갱신을 보존했다. AiAccounts/App/AssistantPanel의 W7 변경도 보존했다.

검증: `pnpm --dir ui exec tsc --noEmit`, 임시 경로 Vite production build, `AX_UI_URL=http://127.0.0.1:51988 node ui/ax-e2e.mjs`, `git diff --check -- ui` 통과. AX E2E는 두 테마, 승인 게이트, 노드 선택·비용 요약·행 수 오차 경고, 960×600, 캡처별 문서 가로 넘침 없음과 반복 유휴 애니메이션 0을 검사했다. 별도 정상 모션 Playwright 점검은 960×600 연결 폼/계획, SSH 키·비밀번호 조건부 입력, 방향키 탭 이동, 비용 막대 1800/2100 비율, 유휴 running animation 0을 확인했다. 이는 mock 브라우저 증거이며 OS 프로세스 CPU 0 측정이나 실제 네이티브 성능 증거는 아니다.

전체 UI 테스트 결과는 구분해야 한다. 초기 `pnpm --dir ui test`는 26파일·122테스트 통과(24.90초)했다. 병행 W7 테스트가 추가된 다음 실행은 `modelPicker.test.ts`의 `window is not defined` 수집 오류가 있었고, W7이 순수 함수를 분리해 수정했다. 그 수정 후 이 워커의 최신 전체 실행은 **125개 테스트 본문 통과, 27개 파일 통과, 온보딩 1개 파일의 afterAll 종료 훅 실패**(69.09초)다. 실패 위치는 `ui/src/onboarding/onboarding.test.ts:21`의 browser/server close, 오류는 `Hook timed out in 30000ms`이며 어느 close가 멈췄는지는 아직 분리하지 않았다. 전체 suite 통과로 간주하지 않는다. timeout 증가·무변경 retry-to-pass·forceExit·온보딩 코드 변경은 하지 않았다. 코디네이터와 W7에 전달했다.

캡처 20화면 × 2테마 = 40장을 같은 `ax-*.png` 파일명으로 갱신했고 기존 40장은 [before-ax](design-shots/before-ax/)에 보존했다. [실행 계획](design-shots/ax-query-plan-midnight.png), [AI 계획 카드](design-shots/ax-intent-plan-midnight.png), [연결 네트워크](design-shots/ax-connection-network-daylight.png), [테스트 타임라인](design-shots/ax-connection-test-daylight.png), [스키마 diff](design-shots/ax-schema-diff-daylight.png), [명령 팔레트](design-shots/ax-command-daylight.png), [최소 창](design-shots/ax-minimum-960-daylight.png)을 포함해 직접 확인했다. 개발 서버 실행 중 W4의 권한 aria-label 계약 변경과 W7의 HMR page reload로 검증이 중단된 이력이 있어, 권한 선택자를 최신 계약에 맞추고 `/tmp/ax-w6b-preview-20261019`의 고정 빌드로 최종 검증했다. 일반 모션 보조 점검의 첫 시도는 인증 방식 select의 부정확한 exact-label 선택자로 중단됐으며 실제 label/select 구조로 고쳐 확인했다.

소유 변경은 `ui/src/ax/{QueryPlan,PlanCard,Workbench,IntentPlans,ImportWizard,ConnectionStatus,Migrations,SchemaDiffPanel}.tsx`, `ui/src/ax/ax.css`, `Connections.tsx`의 ProfileForm, `QueryTab.tsx`의 출력 탭, `mock.ts`의 explain 예시, `ui/ax-e2e.mjs`, 이 보고서와 캡처다. 시작 상태는 추적 변경 53개·미추적 항목 80개였고 보고 직전에는 추적 변경 55개·미추적 항목 101개였으며 병행 작업이 포함된 수치다. 작업 전 복사본과 상태는 `/tmp/ax-w6b-before`에 보관했다. 새 의존성·Rust·사용자 앱/GUI/DB·commit/push는 없다. 실제 DB/SSH/IAM/OAuth, 네이티브 WebView, 설치 산출물 크기와 전체 자원 벤치마크는 검증하지 않았다. 전체 UI 테스트 종료 훅 실패는 남은 검증 항목이다.

검증용으로 이 워커가 실행한 Vite 서버(51987)와 고정 preview 서버(51988)는 검증 종료 후 정리했다. 기존 서버는 건드리지 않았다.
