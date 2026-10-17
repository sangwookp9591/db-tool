# U4 사이드바 보고서

대상: N-4, N-5, N-7, N-8, N-10 (`docs/FINAL_UI_AUDIT.md`). 커밋 없음.

## 변경
| 항목 | 변경 | 파일 |
|---|---|---|
| N-5 | 활성 연결 카드를 `position:sticky; top:0; bottom:0`으로 목록 스크롤 영역 안에 고정(어느 쪽으로 스크롤해도 보임). 낮은 창(≤720px)에서 목록 높이 `min(32%,150px)`로 제한해 탐색기에 남은 공간을 배분. 활성화·연결 수 변경 시 `scrollIntoView` 유지 | `style.css`, `Connections.tsx` |
| N-4 | 컬럼 행: 이름 `flex:0 1 auto`(우선), 타입 `small`이 남은 폭만 사용·먼저 줄어듦. `.bm-row`에 2px 오른쪽 여백을 두어 활성 행 상자 테두리가 `overflow:hidden`에서 잘리지 않음 | `style.css` |
| N-8 | 탐색기 폴더 표시 라벨 한국어화: 테이블·뷰·컬럼·인덱스(PG/SQLite), 레이블·관계 유형·속성 키·인덱스·제약 조건(Neo4j), 필드·인덱스(MongoDB). 노드 id/kind/action은 불변. 에이전트 컨텍스트(`src-tauri/src/context.rs`)는 포커스를 라벨뿐 아니라 노드 id로도 매칭해 `labels` 등 기존 포커스가 계속 동작. 드라이버 테스트·mock·UI 테스트 라벨 갱신 | `crates/driver-*`, `context.rs`, `mock.ts`, `branched-menu.test.ts` |
| N-10 | 지연 로딩 중인 접힌 그룹 안에 기존 `Skeleton`(2행, `role=status`, aria-label `Loading children`)을 표시. 정적 요소라 타이머·rAF 없음 | `Connections.tsx`, `style.css` |
| N-7 | 라벨 패스(≤300 노드)에서 라벨 사각형이 다른 노드 점과 겹치면 노드 아래→위 순으로 재배치, 둘 다 안 되면 생략. 프레임당 DOM 없음, O(n²)이나 n≤300(≈9만 비교) | `GraphView.tsx` |

mock에 테스트용 `__mockExplorerDelay` 훅(기본 꺼짐)을 추가해 로딩 상태를 검사할 수 있게 했다.

## 검증
- `pnpm --dir ui exec tsc --noEmit` 통과, `pnpm --dir ui test` 18파일 88건 통과(사이드바 테스트에 sticky·이름 잘림 검사 추가).
- `cargo test --workspace`(PG 55432·Neo4j 57687 실DB 포함, `target-u4b`) 전부 통과, `cargo clippy --workspace --all-targets -D warnings` 경고 0.
- 브라우저 mock(ego-browser) 960×600·1280×800·1440×900 × 양쪽 테마에서 overlap-probe 실행. 사이드바의 신규 결함 없음(남은 항목은 이미 있던 spotlight/검색 아이콘 의도적 겹침, 스크롤로 잘린 행·가려진 탭에 대한 probe 오탐).
- 빌드는 `vite build --outDir`(scratchpad)로만 확인, `ui/dist`는 건드리지 않음.

## 스크린샷 (before = `final-*.png`)
- before: `final-01-pg-tree-1280-daylight.png`, `final-02-pg-columns-deep-1280-daylight.png`, `final-17-960x600-daylight-tabs-statusbar.png`, `final-08-neo4j-graph-1280-midnight.png`
- after 사이드바(연결 3개, 활성 카드 보임, 한국어 폴더명): `u4-sidebar-960-{daylight,midnight}.png`, `u4-sidebar-1280-{daylight,midnight}.png`
- after 깊은 컬럼: `u4-sidebar-deep-columns-1280-daylight.png`, `u4-sidebar-deep-columns-960-midnight.png`
- after 로딩: `u4-sidebar-loading-1280-midnight.png`, `u4-sidebar-loading-960-daylight.png`
- after 그래프: `u4-graph-labels-960-midnight.png`, `u4-graph-labels-1280-daylight.png`

## 한계
- mock은 짧은 타입 문자열(`timestamptz`)만 있어 `timestamp with time zone · NOT NULL` 같은 긴 타입의 네이티브 확인은 하지 못했다(CSS상 타입이 먼저 잘림).
- 960×600 mock 그래프(100노드)는 점과 겹치는 라벨이 없어 재배치 경로는 코드 검토로만 확인했다. 네이티브 `LimitMovie1` 사례는 미재현.
- 작업 중 `git stash/pop`을 한 번 사용해 공유 트리를 잠깐 되돌렸다가 복원했다(다른 작업자 변경 손실 없음, 상태 확인함).
