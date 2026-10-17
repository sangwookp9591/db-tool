# U2 BranchedMenu 보고서 (사이드바 탐색기)

날짜 2026-10-17 · 영역 BRANCHEDMENU · 커밋 없음 · 확인 환경은 브라우저 mock(ego-browser)이며 네이티브 Tauri 앱에서는 보지 않았다.

## 변경

1. **`ui/src/ui/BranchedMenu.tsx` · `branched-menu.css`** (React Bits BranchedMenu를 참고한 독립 Preact+CSS 구현, 원본 소스 복사 없음, 새 의존성 없음)
   - 공개 부품: `BranchGroup`(grid-rows 접기, 닫힐 때 애니메이션이 끝난 뒤 언마운트, reduced-motion이면 즉시), `BranchItem`(행 하나와 중첩 그룹), 두 단계 섹션 메뉴 `BranchedMenu`(갤러리·재사용용), 순수 함수 `toggleOpen` · `activePath` · `reachIndex` · `reachState` · `treeKey`.
   - 선은 행마다 CSS 상자 둘(줄기 `.bm-trunk`, 곡선 가지 `.bm-elbow`)이다. SVG path가 없다. accent "reach"는 줄기를 위에서 아래로 채운 뒤(transform scaleY) 가지가 꺾여 들어오는(clip-path) 방식이고, 활성 경로 위의 행에만 걸리므로 평소 애니메이션되는 요소가 없다. rAF·타이머 기반 프레임 갱신 없음.
   - 색은 토큰만 쓴다: 선 `--border-default`, accent `--accent-primary`, 글자 `--text-primary/secondary`. rail marker는 활성 노드가 속한 **펼쳐진** 최상위 섹션 헤더 옆에만 선다.
   - 깊이: 각 그룹이 자기 줄기를 가지므로 컬럼 같은 3단계 이상도 같은 규칙으로 그려진다(깊이당 20px).
2. **`Connections.tsx` 탐색 트리**
   - 드라이버가 주는 `ExplorerNode` 트리를 그대로 `BranchGroup/BranchItem`으로 그린다(드라이버 id 분기 없음). 지연 로딩·필터·컨텍스트 메뉴·더블클릭 기본 동작·Enter·`Expand/Collapse/Actions for …` aria-label·`tree-item`·`role=menu(item)`은 그대로다.
   - 추가: 활성 노드(`aria-current=true`, 라벨 클릭·더블클릭·Enter·동작 실행·컨텍스트 메뉴로 지정), 방향키 이동(↑↓ Home End, → 펼치기/자식으로, ← 접기/부모로), 그룹당 300행씩 그리는 "+N 더 보기"(`Show more in <그룹>`). 450개 테이블 그룹도 DOM은 300행이다.
3. **사이드바 #8** (`Connections.tsx`, `style.css` 끝에 국소 추가)
   - 사이드바는 flex 열이다: 고정 머리(제목·검색·연결 헤더) → `.connection-list`(자체 스크롤, 최대 높이 42%, 낮은 창 32%) → 활성 연결의 `.explorer-region`(쿼리 버튼·필터 고정, `.explorer-scroll`만 스크롤). 전체 aside는 스크롤하지 않는다.
   - 활성 연결은 하나다. 연결 직후·카드 클릭·카드 포커스로 바뀌고, 나머지 연결은 카드로 접힌다. 비활성 탐색기는 `hidden`으로 마운트를 유지해 펼침 상태와 로딩한 노드를 잃지 않는다(`onNodes`도 유지).
   - 두 스크롤 영역 모두 `scrollbar-gutter: stable` + 얇은 스크롤바라 내용을 덮지 않는다. 목록이 스크롤되므로 `…` 연결 메뉴는 `position: fixed`로 트리거 옆에 열고 화면 아래쪽이면 위로 뒤집는다(목록에 잘리던 문제를 확인 후 수정). 목록을 스크롤하면 메뉴를 닫는다.
   - 낮은 창(≤720px)에서는 머리 설명 문구를 숨기고 여백을 줄여 트리 영역을 확보했다.
4. **mock 픽스처** (`ui/src/mock.ts`, 추가만): `public` 아래에 people·orders(컬럼 하위 노드)·Views·Indexes·Archive(450개 테이블 그룹)를 둔다. `people`의 id·Browse·Open query·Insert 동작은 그대로다.
5. **갤러리·문서**: `ExtraGallery.tsx`에 `21 · BranchedMenu` 데모, `docs/DESIGN.md` React Bits 표 행 추가와 사이드바 규칙 한 줄 갱신.

## 검증

- `pnpm --dir ui exec tsc --noEmit` 통과. `pnpm --dir ui test` 17개 파일 87개 통과(새 `ui/src/ui/branched-menu.test.ts` 10개).
- scratch 빌드 `vite build --outDir …/scratchpad/u2-branchedmenu-build` 통과. `ui/dist`는 건드리지 않았다.
- 새 테스트: 접기 상태(`toggleOpen`이 이전 집합을 바꾸지 않음), 활성 경로·reach 판정, 키보드 모델(이동·끝 클램프·펼침·접기·부모로), 갤러리 DOM(접기 후 언마운트, accent 줄과 marker 이동, 접힌 섹션은 marker 없음, SVG 없음), mock 앱 DOM(활성 탐색기 하나·세 스크롤 영역의 overflow, 지연 로딩, 깊은 컬럼에서의 reach, 방향키, Browse/Open query 메뉴, 300행 제한과 더 보기, SVG 없음).
- 브라우저 mock: PG 탐색기에서 더블클릭·Enter → `Analytics sandbox / people`·`/ orders` Browse 탭, `Actions for people` → Browse·Open query·Insert name, `Open query` → 새 쿼리 탭 확인.
- 프로브 `overlap-probe.js`를 1280×800 · 1024×700 · 960×600 × Midnight·Daylight에서 사이드바 상태 5종마다 실행했다. 원본 프로브는 스크롤 영역에 잘려 보이지 않는 행(중심점이 스크롤 컨테이너 밖)까지 `occlusion`으로 센다. 새 목록은 스크롤 영역이라 이 오탐이 생기므로, 중심점이 스크롤 조상 밖인 요소는 가림 판정에서 빼는 사본 `u2-bm/probe-scroll-aware.js`를 만들어 최종 측정에 썼다(원본은 수정하지 않음).
  - 전(HEAD 사이드바, 같은 mock 픽스처): 총 findings 41(1280)·37(960)·36~38(deep·filter), 사이드바 항목 30~32. 트리 행이 하단 상태바에 가려지는 occlusion, 필터 입력·버튼 가림, 연결 카드가 트리와 겹침 포함.
  - 후: 총 22~27, 사이드바 항목 16~17 전부 기존 의도 겹침(검색 아이콘↔입력, SpotlightCard 광원↔카드 내용) 기준선과 같고 occlusion·text-clipping·viewport-overflow는 사이드바에서 0이다. 960×600에서 연결 카드 한 장이 목록의 스크롤 영역에 걸쳐 `explorer-region`과 `overlap` 1건으로 잡히지만 목록 스크롤로 잘린 부분의 박스 교차이며 화면에서는 겹치지 않는다.
  - 이 측정은 상태바 `agent-controls`의 viewport-overflow 2건(사이드바 밖, 기존)을 포함하지 않았다.

## 스크린샷 (`docs/design-shots/`, `sips -Z 1600`)

크기 `1280x800`·`1024x700`·`960x600`, 테마 `midnight`·`daylight`.

- 전(HEAD 사이드바 + 같은 mock): `u2-branchedmenu-before-two-connections-…`, `before-deep-columns-…`, `before-filter-…`
- 후: `u2-branchedmenu-two-connections-…`(연결 2개, public 펼침, Tables·Views, 활성 뷰), `deep-columns-…`(people 컬럼 단계, 활성 `created_at`), `filter-…`(필터 `ord`), `archive-scrolled-…`(450개 그룹의 마지막과 "+150 더 보기"), `connection-menu-…`(스크롤된 목록 위의 `…` 메뉴)
- 갤러리 데모: `u2-branchedmenu-gallery-1280x800-midnight/daylight.png`

## 한계와 메모

- 확인은 mock 브라우저뿐이다. 실제 Tauri 앱·PG 트리는 열어 보지 않았다.
- 전 스크린샷은 HEAD의 Connections·style.css에 같은 mock 픽스처를 얹은 사본에서 찍었다. 다른 워커의 미커밋 변경은 전 쪽에 없다.
- 방향키 탐색은 기존처럼 모든 라벨이 Tab 정지점인 상태 위에 더한 것이고 roving tabindex로 바꾸지 않았다(E2E·접근성 이름 보존).
- 필터가 켜져 있으면 기존처럼 모든 그룹이 열린다(chevron 표시는 사용자가 연 상태 기준). 기존 동작을 바꾸지 않았다.
- 접기 애니메이션 중(300ms)에는 닫히는 행이 `inert`로 DOM에 남는다. 자동화에서 접기 직후 행 수를 세면 한 박자 기다려야 한다.
- 그룹 안 행이 수천 개면 "더 보기" 단위(300)로만 늘어난다. 진짜 가상 스크롤은 아니다.
- 활성 연결에서 연결 해제 시 `activeId`는 남은 첫 연결로 넘어간다. 연결 검색은 카드만 거르고 탐색기는 거르지 않는다(기존과 같음).
