# U5 보고서 — 최종 감사 2의 F-1~F-5 수정

기준 커밋 `89bfc9a` · 대상 `docs/FINAL_UI_AUDIT_2.md` §2 · 브라우저 모의(ego-browser)에서 960×600·1280×800·1440×900(F-4는 1492×874 추가) × Midnight/Daylight 검증. 커밋 없음.
게이트: `tsc --noEmit` 통과, `vitest` 19파일 90테스트 통과, `pnpm --dir ui build` 통과. aria-label·role·data-testid·단축키·id 분기 변경 없음.

| ID | 상태 | 변경 | 증거(이전 → 이후) |
|---|---|---|---|
| F-1 탭 끝 여백 | 수정 | `App.tsx` 탭 정렬: 끝 패딩 대신 탭 폭을 `--tab-w`로 조정해 스트립 폭을 탭 수로 나눠 떨어지게 함(탭 폭 190~285px). 모든 스크롤 정지점이 탭 경계라 마지막 탭에서도 오른쪽 빈칸·왼쪽 조각이 없다. 폭이 바뀌면 활성 탭을 다시 보이게 한다. `style.css` `.workspace-tab` 폭이 `var(--tab-w,224px)` | 끝 여백 190/62/222px → 0px, 왼쪽 조각 0px 유지. [이전 final2-05](design-shots/final2-05-tabs-8-960-daylight.png) → [이후](design-shots/u5-f1-tabs-960-daylight.png) |
| F-2 이력 패널 | 수정 | 이력 패널 `flex-shrink:1`·최소 72px, 그래프 결과가 있는 탭 본문 최소 462px, 편집기 비율의 결과 영역 예약을 240→300px(`QueryTab.tsx`), 이력 시간은 `오후 10:27` 형식(전체는 title), 시간 열 84px | 960×600 캔버스 84px → 156px, 시간 말줄임 없음. 캔버스 크기 변화는 기존 ResizeObserver로 재맞춤. 1280×800 190px, 1440×900 262px(변화 없음). [이전 final2-11](design-shots/final2-11-history-open-graph-squeezed-960-midnight.png) → [이후](design-shots/u5-f2-history-960-midnight.png) |
| F-3 AI 대화상자 | 수정 | `설정 저장`·`저장된 키 삭제`를 대화상자 푸터(고정)로 이동(`AssistantPanel.tsx`), 본문 스크롤 가장자리 페이드 18→28px | 960×600에서 `설정 저장`이 푸터에 항상 보임(전 크기·두 테마). [이후](design-shots/u5-f3-ai-960-midnight.png) (이전 960 컷은 감사에 없음, 최대 크기 [final2-12](design-shots/final2-12-ai-dialog-primary-largest-midnight.png)) |
| F-4 에이전트 클라이언트 | 수정 | `ClientsPanel.tsx`에 `agent-overview` 영역 추가: 1100px 이상에서 호출 이력(왼쪽)·다이어그램(오른쪽) 2열, 좁으면 세로. 호출 이력 목록은 6행 높이(180px)로 제한하고 행 단위 스냅 | 이력 목록 잘린 행 1개 → 0개(전 크기). 다이어그램이 본문 안에 완전히 들어옴: 1280·1440·1492에서 예(이전 전부 아니오). 960×600은 높이가 낮아 다이어그램이 스크롤 아래에 있으나 스냅으로 한 덩어리로 도달. [이전 final2-13](design-shots/final2-13-agent-clients-dialog-largest-midnight.png) → [1492](design-shots/u5-f4-agent-1492-midnight.png), [960](design-shots/u5-f4-agent-960-daylight.png) |
| F-5 사이드바 | 수정 | 낮은 창에서 연결 목록 높이 130px(카드 2장 정수 배), 위/아래 12px 페이드(스크롤 위치별 `data-edge`), 카드 단위 y 스냅(`Connections.tsx`, `style.css`) | 960×600 이웃 카드 조각 18px → 숨김(목록 높이 130 < 세 번째 카드 시작 132). 활성 카드가 세 번째여도 위쪽 2px만 페이드로 가려짐. [이전 final2-01](design-shots/final2-01-sidebar-active-card-960-daylight.png) → [이후](design-shots/u5-f5-sidebar-960-daylight.png), [활성 3번째](design-shots/u5-f5-sidebar-active3-960-midnight.png) |

## 겹침 프로브
`overlap-probe.js`를 F-1~F-5 화면 × 3크기 × 2테마(30회)에서 변경 전(stash)·후로 비교. 새 겹침 범주는 없다. 건수는 대부분 같거나 감소(F-4 1280: 56→49, 1440: 56→54, F-3 960: 31→30). 증가·신규로 보인 항목은 모두 스크롤로 가려진 요소(왼쪽으로 밀려난 탭, 대화상자 스크롤 밖 행, 토스트 아래 상태 줄)를 프로브가 가시로 오판한 것이며 이전에도 같은 종류가 있었다. 기준선의 기존 항목(검색 아이콘·입력, 레일 에이전트 슬롯 등)은 이번 범위 밖이라 그대로다.

## 한계
- 네이티브 Tauri 앱이 아니라 브라우저 모의에서 확인했다. 네이티브 재측정은 하지 않았다.
- F-1 탭 폭이 190~285px 사이에서 바뀐다(이전 고정 224px). 탭이 스트립에 다 들어가면 224px 그대로다.
- F-2 960×600에서 이력 패널이 열린 상태의 목록은 약 1행만 보인다(캔버스 우선). 패널은 사용자가 크기를 조절할 수 있다.
- F-4 960×600은 낮은 창이라 다이어그램까지 스크롤이 필요하다. 2열은 폭 1100px 이상에서만 적용된다.
- 퍼프: 유휴 타이머·rAF 추가 없음(ResizeObserver·스크롤 이벤트만). 그래프 팬/줌 경로는 변경하지 않았다.
