# U1 chrome 수정 보고

범위: 앱 크롬(탭 바, 토스트, 배너, 이력, 상태바, 창 크기). 근거 `docs/tasks/U1-ui-fixes.md`, `docs/UI_AUDIT_NATIVE.md`. 검증은 브라우저 mock(Vite, playwright chromium)에서 했고 네이티브 WKWebView는 확인하지 않았다. 커밋하지 않았다.

전후 스크린샷(mock): 경로 `docs/design-shots/u1-chrome-*.png`(대표본) 및 scratch `…/scratchpad/u1-chrome/{before,after}/`에 1280×800·1024×700·800×600·960×600 × Midnight·Daylight 전체(허브, 탭·토스트, 설정+토스트, 오류, 이력, 토스트 지속, 전역 배너, 탭 hover, 대화상자 포커스).

## 1. `:root .ax-button` 우선순위 회귀

원인: `Button`(bare가 아님)은 `.ax-button`을 붙이고, `:root .ax-button`(0,2,0)이 `.rail>button`(0,1,1) 같은 보정 규칙의 테두리·배경·radius·padding을 이긴다. 대부분의 위치가 `<Button morph={false}>`라서 모두 해당한다.

방법: (b) 보정 규칙 앞에 `:root `를 붙여 명시도를 (0,2,1) 이상으로 올렸다(`style.css`의 기존 규칙을 제자리에서 수정, 21개). 호버는 `:root .ax-button:hover`(0,4,0)가 배경을 덮으므로 납작한 컨트롤 묶음에 `--ax-button-bg:transparent`를 줘서 호버가 미세한 오버레이만 되게 했다(U1 블록 첫 줄). `.ax-button--sm`의 32px가 메뉴·트리 행을 키우지 않도록 compact 위치에 `min-height:26px`를 복원했다.

위치별 판정(합성 DOM으로 `.ax-button` 버튼과 같은 자리의 bare 버튼의 계산 스타일을 비교, 이전=원본 style.css, 이후=현재. 테두리·배경·radius·padding·min-height·색 비교):

| 위치 | 이전 | 이후 | 비고 |
|---|---|---|---|
| `.rail>button` | min-h만 차이 | 정상 | 34px는 의도(`:root .rail .ax-button`) |
| `.section-title>button` | 배경 깨짐 | 정상 | |
| `.connection-actions>button` | 정상 | 정상 | 이미 `:root … .ax-button`로 보정돼 있었음 |
| `.connection-menu button` | 테두리·배경·padding·높이 깨짐 | 정상 | |
| `.explorer .toolbar button` | 높이(32px) | 정상 | 26px 복원 |
| `.tree-node>button:last-child` | 높이(32px)로 트리 행이 커짐 | 정상 | 26px 복원 |
| `.context-menu button` | 테두리·배경·padding·높이 | 정상 | |
| `.notice button` | 테두리·배경·padding·높이 | 정상 | |
| `.workspace-tab>button`(제목) | 테두리·배경·radius·padding | 재설계 | 아래 2절. padding·min-h·색 차이는 의도 |
| `.workspace-tab>button`(닫기) | 테두리·배경·radius | 재설계 | 〃 |
| `.view-switch button` | 테두리·배경·padding·색 | 정상 | 선택 상태는 `--ax-button-bg` |
| `.result-tabs button` | 〃 | 정상 | |
| `.column-header button` | 배경·padding·높이 | 배경 정상, padding 9→12px | 실제 헤더는 bare 버튼이라 영향 없음 |
| `.workspace-error>button` | 높이 | 정상 | 전역 배너가 `ErrorBanner`(bare X)로 바뀌어 사실상 미사용 |
| `.history-list>button` | 깨짐 | 깨짐(대상 없음) | 죽은 선택자. 이력 행은 `SwipeRow` 안의 bare `.history-entry`뿐이라 `.history-list>button`은 어떤 요소에도 안 붙는다. 건드리지 않음 |
| `.agent-controls>button` | 테두리·배경·padding | 정상 | |
| `.agent-rail-button` | 정상 | 정상 | 아이콘 소실은 별건(7절) |
| `.document-row button` | 배경·padding | 정상 | |
| `.row-inspector>button` | 테두리·배경 | 배경 정상, padding 9→12px | 닫기 X, 영향 미미 |
| `.history .section-title button` | 정상 | 정상 | |
| `.rail>.micro-tooltip>button` | min-h만 차이 | 정상 | 34px는 의도 |
| `.dialog-header button` | 테두리·배경·색 | 정상 | 28px는 의도 |

재현: `scratchpad/u1-chrome/verdict.mjs`.

## 2. 워크스페이스 탭 바(사용자 제보, #5)

- 제목과 닫기 X가 한 탭: 안쪽 테두리·radius 제거, 탭 전체가 `.workspace-tab` 한 상자(배경, hover, 위쪽 그라데이션 선).
- 폭 고정 224px, 제목은 `.tab-title`로 말줄임 + `title` 툴팁(aria-label은 그대로).
- 닫기 칸 28px를 모든 탭이 같게 예약. 비활성은 투명, hover·포커스·활성에서 보인다.
- 아래 슬라이딩 선: `SlidingIndicator`가 `data-indicator-scope`가 붙은 탭 전체(제목+X)를 기준으로 x·폭을 잰다(기존엔 제목 버튼만 재서 X 영역을 빠뜨렸다). 스코프가 없는 사용처(JellyRadio 등)는 동작 그대로.
- 활성 탭 스크롤: 활성 탭·탭 개수·창 폭이 바뀔 때 탭 바를 스크롤해 활성 탭이 보이게 한다(`App.tsx`). 세로 휠은 가로 스크롤로 변환.
- 가로 스크롤바는 숨겼다(`scrollbar-width:none` + `::-webkit-scrollbar{display:none}`). 탭을 덮지 않는다. `overflow-y:hidden`으로 세로 조각도 없앴다(#13).
- 허브(탭 없음)에서는 탭 바를 렌더하지 않는다(#21).

## 3. 창 최소 크기와 사이드바 (#1)

`tauri.conf.json`에 `minWidth 960 / minHeight 600`. 근거: 960에서 레일 48 + 사이드바(최대 40% = 384) + 스플리터 4를 빼도 편집기·결과에 약 520px이 남고, 600 높이에서 탭 34 + 툴바 36 + 편집기 80 + 결과 240(`results-wrap` 최소) + 상태바 24가 들어간다. `.app{min-height:500px}`는 그대로 둔다.
사이드바는 CSS `max-width:min(420px,40vw)`와 JS(`sidebarMax`, 창 resize 반영, 스플리터 aria-valuemax·키보드·드래그)로 창의 40%까지만 커진다. 960×600·800×600에서 상태바와 레일 하단 설정 버튼이 잘리지 않음을 스크린샷으로 확인했다.

## 4. 토스트 (#2 #3)

- `ToastStack`에 선택 prop `autoDismissMs`(갤러리 호환을 위해 기본 꺼짐). `App`이 4500ms로 켠다. success/info만 사라지고 error는 닫을 때까지 남는다. 호버·포커스 중에는 타이머 정지.
- 최대 4개: 넘치면 오래된 success/info부터 제거, error는 마지막에.
- 위치: 창 우하단(`right:16px; bottom:32px`, 폭 `min(360px, 100vw-32px)`). 사이드바 트리와 좌하단 설정 팝오버를 덮지 않는다. 사이드바를 접어도 보인다(기존 `display:none` 제거).
- 긴 메시지는 4줄 말줄임 + `title`.
- 확인: 연결·결과 토스트가 5초 뒤 사라지고 클립보드 실패 error 토스트만 남음(`toasts.mjs`).

## 5. 오류 표면 (#4)

- 새 `ErrorBanner.tsx`: 배지·메시지·위치, 긴 상세(140자 초과 또는 줄바꿈)는 `<details>`로 접고 `pre`는 `max-height:120px`, 닫기 X에 aria-label(전역 `Dismiss error`, 탭 `Dismiss query error`/`Dismiss browse error`/`Dismiss edit error`/`Dismiss schema error`). 기존 `ErrorBox`(Results.tsx)는 건드리지 않았고 이 영역에서는 더 쓰지 않는다.
- 한 사건 한 표면: 탭 안의 오류(실행·편집기·결과 뷰·Browse·스키마)는 그 탭의 배너 하나만. 전역 배너+토스트는 더 이상 겹쳐 뜨지 않는다. `Connection` 종류만 전역 배너 하나(탭 배너는 숨김)로 올라가고 프로필을 dead로 표시. 탭 밖 오류(이력 로드, 복사 실패 등)는 지속 토스트 하나.
- 전역 배너는 탭 바 아래에 둬서 탭 바가 밀리지 않는다(탭이 없을 때만 맨 위).
- 컨텍스트 종료: 편집 모드를 끝내면 편집 오류 배너를 지운다. 전역 배너는 해당 프로필 연결 해제·재연결·다음 실행 시 지운다. 스키마 배너는 다음 로드 성공 시 지운다.
- `NavigationTab`의 탐색 기록 줄은 항목이 2개 이상일 때만 표시(#21).

## 6. 이력 패널 (#17)

- 원인 1: `.micro button{border;background}`(0,1,1)가 `.history-entry{border:0}`(0,1,0)를 이겨 행 안에 상자가 생겼다. `.history-list .history-entry`로 올림.
- 원인 2: 셀에 `min-width:0`·말줄임이 없어 오류 요약이 행 밖으로 흘렀다. `.history-entry>*`에 적용.
- 원인 3(탐지기의 `div#P0-*`): 닫힌 `SwipeRow`의 액션 레일이 표면 뒤에 그대로 렌더돼 행과 겹쳤다. `visibility:hidden`(닫힘 상태, 0.35s 지연으로 닫힘 애니메이션 유지). 빠른 실행 버튼도 호버·포커스 전에는 `visibility:hidden`(Tab 경로는 행 포커스가 `:focus-within`을 켜서 유지).
- 높이: `clamp(96px,26vh,200px)`, 최대 36vh. 빠른 실행 버튼에 아이콘(`SquarePlus`/`Copy`/`Trash2`)을 줘서 셋이 같은 `>`로 보이던 문제도 해결.

## 7. 포커스 링 (#20)

- #20의 원인은 포커스 링 위치가 아니라 레일의 에이전트 아이콘 소실이다. `.agent-rail-button span{display:none}`이 라벨뿐 아니라 `Button`의 내부 `.ax-button__content/__label` span까지 숨겨 아이콘이 0×0이 됐고, 포커스 링만 빈 칸에 그려졌다. 내부 span만 복원했다. 버튼 `top`도 슬롯 위치(192px)에 맞췄다(186px였음).
- F001(대화상자 닫기 X): 스크린샷상 링은 잘리지 않는다(`dialog-close-focus.png`). 탐지기는 에이전트 대화상자가 `footer.statusbar`(overflow:hidden) 안에서 렌더돼서 상위 overflow를 오탐한다. 모달은 top layer라 실제로는 잘리지 않으므로 코드는 안 바꿨다. 대화상자 컨텍스트에서 링을 `--bg-overlay` 간격으로 그리도록 한 줄 보정만 했다.

## 8. 그 외

- #14: 취소 아이콘 `Square` 채움(`fill=currentColor`), `⌘↵` 칩 11px·대비 높임. 대화상자 주 버튼 brand: 이 영역의 `App.tsx`(연결 인증)와 `QueryTab`(변경 적용, 이미 brand)만. Connections·AI·에이전트 대화상자는 담당 외 파일이라 그대로(`variant="brand"` 추가 필요, 아래 남은 것).
- #15: QueryTab Parameters와 오류 상세는 lucide `ChevronRight`(회전). 텍스트 문자는 없었고 네이티브 `<summary>` 마커를 썼다. 연결 폼 `.field-group`·그래프 접근성 `summary`는 마크업이 다른 담당 파일이라 CSS로 그린 chevron으로 통일(CSP 때문에 data-URI 마스크는 쓰지 않음).
- #19/#9 UI: 상태바 서버 버전을 `<Product> <version>`(`serverVersion.ts`, 테스트 추가)으로 줄이고 전체 문자열은 `title`. Neo4j 디버그 포맷 문자열도 `Neo4j 5.26.30`으로 줄어든다(백엔드 파싱은 별건).
- 상태바 넘침(F203/F204, 코디네이터 전달): 왼쪽 라벨(프로필명·버전·결과)에 `min-width:0`+말줄임, 에이전트 컨트롤·점·읽기 전용 배지는 줄지 않게 했다. 긴 프로필명(`Production 분석용 …`)+읽기 전용+결과 칩으로 1280/960/800/560폭에서 `scrollWidth==clientWidth`, 뷰포트 밖 요소 0 확인. 기본 상태(짧은 이름)에서는 960 이하에서도 원래 넘침이 재현되지 않았다.
- #13: 고정 높이 줄(`table-area/graph-main/browse-tab` toolbar, 탐색 nav, 탭 바)의 `overflow-y:hidden`.

## 검증

- `pnpm --dir ui exec tsc --noEmit` 통과(작업 중 다른 워커의 `Results.tsx` 편집 중 일시 오류가 한 번 있었고 이후 사라짐), `pnpm --dir ui test` 15 파일 73 통과, `vite build --outDir <scratch>` 통과. `ui/dist`는 쓰지 않음. 드라이버 id 분기 추가 없음. 기존 aria-label·role·단축키 변경 없음(탐색 기록 줄은 항목 1개일 때 숨김).
- 탐지기 재실행(`overlap-probe.js`, 8개 크기·테마 × 허브/탭·토스트/설정/오류/이력, 관련 항목만): 전 `overlap` 52→0(워크스페이스 탭 내부 겹침 F190–F192류, 이력 `div#P0-*`·빠른 실행 겹침 모두 사라짐). `occlusion` 12→56은 활성 탭 스크롤로 왼쪽에 밀려난(스트립 밖에서 클리핑된) 탭을 탐지기가 가시 요소로 세는 거짓 양성이다. `text-clipping`은 탭 스트립 자체(의도된 스크롤러).

## 남은 것 / 한계

- 네이티브(WKWebView)에서의 시각 확인은 하지 않았다. 탭 바 스크롤바 숨김과 `line-clamp`는 확인 필요.
- 그리드 하단 가로 오버레이 스크롤바가 마지막 행을 덮는 문제(#13 후반)는 `VirtualTable`/`EditableTable` 영역이라 손대지 않았다.
- Connections·AssistantPanel·ClientsPanel 대화상자 푸터의 주 버튼에 `variant="brand"` 필요(담당 워커).
- 상태바 `알림 N` 버튼은 여전히 사이드바를 연다(토스트 위치가 바뀌어 의미가 약해짐, 동작은 유지).
- `.history-list>button` 규칙은 죽은 선택자라 남겨 뒀다.
