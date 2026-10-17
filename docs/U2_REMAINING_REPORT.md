# U2 REMAINING 보고서

대상: `docs/U1_NATIVE_REAUDIT.md`에서 partly fixed/회귀로 남은 항목 중 다른 작업자가 소유하지 않은 파일. 커밋 없음. 검증은 vite dev + 브라우저 mock(ego-browser), 1280×800 / 1024×700 / 960×600, Midnight·Daylight. 이전(before) 증거는 재감사 스크린샷(`u1-native-*`, 실제 앱)을 그대로 사용한다.

## 항목별 결과

| 항목 | 변경 | 이후(after) 스크린샷 | 이전(before) |
|---|---|---|---|
| #6 열 너비 | `virtual.ts`에 `columnWidths(table)` 하나를 두고 `VirtualTable`(읽기 전용 경로)·`EditableTable`이 같이 사용. 머리글·타입·앞 50행 샘플로 열마다 110~340px(숫자 280px). 원인은 편집 표의 `table-layout:fixed` + `min-width:100%`가 남는 폭으로 열을 늘린 것이라 편집 표에 명시 합계 너비를 주고 `min-width:0`으로 덮음(style.css 끝). | `u2-remaining-table-browse-*`, `u2-remaining-table-edit-*` (3 해상도 × 2 테마) | `u1-native-06-typed-browse-*`, `u1-native-06-typed-edit-*` |
| #12 제목 | 세 드라이버 `result_title()`: 80자(char 단위) 초과 시 `…` 추가, 단위 테스트 3건. UI는 한 줄 CSS 말줄임이며 `Results`에 `queryText` 선택 prop을 추가해 잘린 제목이면 `title=`에 전체 쿼리를 표시. | (mock은 제목이 고정이라 시각 검증 불가) | `u1-native-14-long-sql-daylight.png` |
| #18 그래프 | 워커가 첫 배치 후 노드 배치를 캔버스(패딩 반영) 비율로 넓힘(`fitAspect`), 캔버스 비율이 바뀌면(숨김 패널·창 크기) `aspect` 메시지로 다시 맞추고 fit 재적용. 노드 라벨은 ≤300노드면 항상 표시(화면 고정 크기, 줌아웃 시 18자 축약, 겹치는 라벨은 그리디로 생략), >300은 scale≥0.6에서만. 프레임당 DOM 작업 없음, 상호작용 중에는 기존 비트맵 경로 유지. mock에 `LIMIT n`(n>3) 합성 그래프 추가. | `u2-remaining-graph-120-*` (120노드, 3 해상도 × 2 테마) | `u1-native-18-neo4j-graph-midnight.png` |
| R2 탭 포커스 | `.workspace-tab:has(>.ax-button:focus-visible)`에 링, 개별 버튼 링 제거. 탭 전체(제목+X)를 감쌈. | `u2-remaining-tab-focus-{midnight,daylight}.png` | `u1-native-06-typed-browse-midnight.png` |
| R3 팔레트 | 결과 행 `text-align:left; justify-content:flex-start`, 라벨 `flex:1`. | `u2-remaining-palette-{midnight,daylight}.png` | `u1-native-16-palette-midnight.png` |

## 측정

- `people`(mock) 열 너비: 읽기 전용 `[192,110,249,340,233,110]` = 편집 `[192,110,249,340,233,110]` (완전 일치). `balance`(숫자 27자)는 읽기 전용에서도 말줄임 없이 전체 표시.
- 그래프 120노드(캔버스 958×333): 가로 사용 약 30%(U1) → 약 87%, 세로 약 74%; 라벨 판독 가능.
- 계약 외: 읽기 전용 표 렌더 경로는 `defaultWidths` 내용만 바뀜(50행 샘플, 열 수만큼). 가상화·셀 캐시 변경 없음.

## 검증

- `pnpm --dir ui exec tsc --noEmit` 통과, `pnpm --dir ui test` 17 파일 87 테스트 통과(`virtual.test.ts` 신규 2건 포함, 기존 perf 테스트 포함).
- `cargo test -p driver-postgres -p driver-sqlite -p driver-neo4j`(PG 55432, Neo4j 57687 실 DB) 모두 통과, `cargo clippy --all-targets -D warnings` 경고 없음(`CARGO_TARGET_DIR=target-u2r`).
- overlap-probe: 그래프 화면의 그래프·캔버스·결과 영역 소견 0건(6조합). 표·팔레트 화면은 사이드바 검색 아이콘·spotlight, 레일 뒤의 모달 오클루전, 행 아래 상태줄 가림 등 이번 변경과 무관한 기존 소견만 있고 편집 표/행 영역에서 새 유형은 없음.

## 한계 / 남은 일

- `Results`의 `queryText`는 `QueryTab.tsx`(타 작업자 소유)가 넘겨야 `title=`에 전체 쿼리가 표시된다. 지금은 prop이 없으면 이전과 같이 잘린 제목이 툴팁에 나온다.
- 실제 Tauri 앱·pan/zoom p95(~17ms)·100k행 첫 페인트는 실행하지 못했다. 그래프 변경은 비상호작용 프레임의 라벨 그리기만 늘렸고, 비트맵 경로는 그대로다.
- 그래프 JSON 전환(재감사에서 언급)은 이번 범위 밖.
- 사용한 임시 dev 서버는 종료했으며 `ui/dist`는 건드리지 않았다.
