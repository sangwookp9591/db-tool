# AxDB 네이티브 UI 시각 감사

날짜 2026-10-17 · 대상 커밋 `96f0311`(실제 Tauri 디버그 앱, 격리 설정 `c2-config`) · 테스트 DB만 사용(PG `127.0.0.1:55432`, Neo4j `127.0.0.1:57687`). 제품 소스 수정 없음, 보고 전용. 기준 문서 `docs/DESIGN.md`. 스크린샷은 `docs/design-shots/ui-audit-*.png`(경로 접두 `design-shots/`).

## 범위

### 실제로 본 것

| 항목 | 크기 | 테마 | 화면 |
|---|---|---|---|
| 허브 빈 상태 | 1280×800 | Daylight(시스템 설정) | 01 |
| 연결 추가 대화상자: MongoDB, Neo4j, PostgreSQL(고급·TLS 펼침), Redis, SQLite | 1280×800 | Daylight | 02, newconn-* |
| 연결 후 탐색 트리(스키마 3개, 테이블·뷰 전체 펼침), 테이블 액션 메뉴 | 1280×800 | Daylight, Midnight | 03–05, 28 |
| 테이블 Browse(`e2e.big`, `e2e.typed`, `e2e_fk.child`), 편집 모드, 대기 변경, 변경 미리보기(INSERT NULL id 오류) | 1280×800 | Daylight, Midnight | 06, 08–13 |
| 쿼리 편집기 결과: 긴 컬럼명·긴 값 200행 Table, 1,000행 가상 스크롤(키보드 PageDown), Neo4j Graph, Neo4j 문법 오류 | 1280×800 | Daylight, Midnight | 14, 15, 32 |
| 쿼리 변경 미리보기(UPDATE, 롤백됨) | 1280×800 | Midnight | 31 |
| ERD(`e2e_fk` 다이어그램) | 1280×800 | Midnight | 27 |
| FK 따라가기, 탐색 기록 칩 | 1280×800 | Midnight | 28, 29 |
| 이력 패널, 명령 팔레트(검색), AI 도우미(로그아웃 상태), 에이전트 클라이언트 | 1280×800 | Midnight | 18–22 |
| 설정 팝오버, 테마 Daylight↔Midnight↔System | 1280×800 | 둘 다 | 16, 17, 33 |
| 토스트 중첩(연결·실행·오류 3~4개) | 1280×800 | 둘 다 | 16, 17, 30, 33 |
| 다중 탭(6개) 가로 넘침 | 1280×800, 1024×700 | Midnight | 23, 27–30 |
| 연결 실패(`e2e-pg-badport`) 오류 배너 | 1280×800 | Midnight | 30 |
| 창 크기 | 1512×881(최대), 1280×800, 1024×700, 640×480, 300×200 | Midnight | 26, 23, 25, 24 |

### 못 본 것

- 창 1920×1080: 화면이 1512×982(논리)라 macOS가 1512×881로 제한했다. 26이 실제 최대 크기다.
- 읽기 전용 탭과 충돌(1행 불일치) 배너: 프로필·데이터를 바꿔야 해서 다시 만들지 않았다. 읽기 전용은 `docs/C2_NATIVE_E2E.md`의 이전 증거가 있다.
- `Apply` 확인 대화상자: Apply는 실행하지 않았다(데이터 변경 방지).
- 툴팁 hover: `orca computer`에 hover 명령이 없어 확인하지 못했다. 말줄임 항목마다 툴팁이 있는지는 미검증이다.
- 마우스 휠 스크롤: 합성 스크롤이 웹뷰에서 먹지 않았다. 대신 키보드로 스크롤했다.
- SQLite·Redis·MongoDB 실제 결과 화면: SQLite·Redis·MongoDB는 연결 폼만 봤다(Mongo·Redis는 테스트 인스턴스 없음).
- 에이전트 활동 패널의 실제 이벤트와 빔 애니메이션: 이벤트를 만들지 않았다.
- AI 응답 화면: 유료 요청 금지라 보내지 않았다.
- 접근성 대비비 정량 측정: 눈으로만 봤다.

## 발견 사항(근본 원인 기준, 중복 제거)

심각도: blocker(작업 불가), major(기능·가독성에 직접 영향), minor(다듬기).

| # | 심각도 | 문제 | 위치·크기·테마 | 근거 | 수정 방향(추정 파일) |
|---|---|---|---|---|---|
| 1 | blocker | 창 최소 크기가 없다. 300×200까지 줄어들고, 작은 창에서 사이드바가 전부를 차지해 편집기·결과 영역이 화면 밖으로 밀린다. 640×480에서도 상태바와 rail 하단 설정 버튼이 잘리고 결과 헤더(Graph/Table)가 가로로 넘친다. | 640×480, 300×200, Midnight | [640×480](design-shots/ui-audit-25-640x480-midnight.png), [300×200](design-shots/ui-audit-24-300x200-min-midnight.png) | `src-tauri/tauri.conf.json`에 `minWidth`/`minHeight` 추가(예: 900×600). 사이드바 폭을 창 폭에 비례해 제한하고 좁을 때 접기. `ui/src/styles/*` 레이아웃 |
| 2 | major | 토스트가 닫히지 않고 쌓인다(DESIGN §5는 자동 소멸을 요구). 연결됨·실행 결과·오류 토스트 3~4개가 사이드바 하단을 덮어 탐색 트리 행("Tables")을 가린다. | 1280×800, 양 테마 | [03](design-shots/ui-audit-03-pg-connected.png), [10](design-shots/ui-audit-10-typed-edit-pending.png), [30](design-shots/ui-audit-30-badport-error.png) | `ui/src/` 토스트 컨테이너: 성공 토스트 4~5초 자동 소멸, 최대 개수 제한, 오류만 유지. 위치를 사이드바 밖(우하단)으로 |
| 3 | major | 토스트가 설정 팝오버와 같은 좌하단 자리를 쓰고 팝오버 위로 올라와 설정 컨트롤(테마, 노드 한도)을 가린다. Midnight에서는 긴 오류 토스트가 창 전체 폭으로 늘어나 하단을 덮는다. | 1280×800, Midnight 심함 | [17 Midnight](design-shots/ui-audit-17-midnight-settings-open.png), [33 Daylight](design-shots/ui-audit-33-daylight-settings-toasts.png), [16](design-shots/ui-audit-16-settings-popover.png) | 토스트 컨테이너에 `max-width`와 z-index 정리, 팝오버와 겹치지 않는 앵커. 같은 `ui/src/` 토스트·팝오버 CSS |
| 4 | major | 오류가 한 사건에 3곳에 중복 표시된다. 탭 줄 위의 전역 배너, 탭 안 배너, 토스트. 전역 배너가 탭 줄과 모든 콘텐츠를 60~200px 아래로 밀어 레이아웃이 흔들린다. 닫기 X에는 접근 가능한 이름이 없다(트리에 button으로 안 잡힘). 편집 모드를 끝내도 배너가 남는다. | 1280×800, 양 테마 | [11](design-shots/ui-audit-11-edit-preview.png), [12](design-shots/ui-audit-12-after-exit-edit.png), [15](design-shots/ui-audit-15-neo4j-result.png), [30](design-shots/ui-audit-30-badport-error.png) | 배너를 한 곳(탭 내부)으로 통일하고 전역 배너는 연결 오류에만. 긴 SQL은 접기+최대 높이. X에 `aria-label`. 전역·탭 배너 컴포넌트(`ui/src/`) |
| 5 | major | 활성 탭이 화면 밖에 있어도 탭 줄이 스크롤되지 않는다(새로 연 `child` 탭이 오른쪽 밖). 가로 스크롤바가 탭 아래쪽을 덮고, 활성 탭의 닫기 X가 가장자리에서 잘린다. 비활성 탭은 닫기 자리만큼 우측 여백이 비어 탭 간격이 들쭉날쭉하다. | 1280×800 6탭, 1024×700 | [28](design-shots/ui-audit-28-child-browse.png), [23](design-shots/ui-audit-23-1024x700-midnight.png), [27](design-shots/ui-audit-27-erd-midnight-1280.png) | 탭 활성화 시 `scrollIntoView`, 탭 폭 고정·말줄임+툴팁, 스크롤바 숨김 또는 얇은 오버레이, 닫기 자리를 항상 같은 크기로. 탭 컴포넌트 |
| 6 | major | 편집 모드 그리드가 일반 Browse 그리드와 레이아웃이 다르다. 일반 모드는 열 폭이 좁고 오른쪽에 큰 빈 공간이 생기며, 편집 모드는 전체 폭으로 늘어난다. 편집 모드에서 헤더는 왼쪽 정렬인데 숫자 값은 오른쪽 정렬이다. 열이 많으면 `Action` 열이 오른쪽 끝에서 좁아진다. 영어 `Action` 헤더. | 1280×800 | [06](design-shots/ui-audit-06-big-browse.png) vs [09](design-shots/ui-audit-09-typed-edit-mode.png), [10](design-shots/ui-audit-10-typed-edit-pending.png) | 두 모드가 같은 열 너비·정렬 로직을 쓰게 하고 `Action`을 아이콘 열(`삭제`)로. 결과 그리드 `ui/src/`(grid) |
| 7 | major | FK 이동 버튼(↗)이 셀 값 위에 겹쳐 마지막 자릿수를 가린다(`922337203685477[↗]`). 말줄임표도 없다. | `e2e_fk.child.parent_id`, 1280×800 | [28](design-shots/ui-audit-28-child-browse.png) | 값 영역에서 버튼 폭만큼 `padding-right`를 예약하고 말줄임 처리. 그리드 셀 컴포넌트 |
| 8 | major | 탐색 트리가 사이드바 전체 스크롤에 섞여 있다. 연결 카드가 위로 스크롤돼 사라지고, 연결이 여러 개면 트리가 한 스크롤 안에서 계속 길어진다(Neo4j 트리가 PG 카드 위에 붙음). 스크롤바가 콘텐츠 위에 겹친다. | 1280×800, 1024×700 | [04](design-shots/ui-audit-04-tree-expanded.png), [23](design-shots/ui-audit-23-1024x700-midnight.png) | 연결 목록과 트리를 분리된 스크롤 영역으로, 활성 연결 하나만 트리 펼침(DESIGN §4). 사이드바 컴포넌트 |
| 9 | major | 상태바에 Neo4j 서버 버전이 디버그 포맷 문자열로 노출된다: `Neo4j [[Text("Neo4j Kernel"), List([[Text("5.26.30")]])]]`. | Neo4j 연결 시 모든 크기 | [15](design-shots/ui-audit-15-neo4j-result.png) | 백엔드 버전 파싱(`src-tauri/` Neo4j 드라이버)에서 값 추출 후 `Neo4j 5.26.30`으로. UI는 방어적으로 표시 |
| 10 | major | 연결 대화상자 높이가 드라이버마다 달라 대화상자 위치가 뛴다. PostgreSQL 폼은 하단(비밀번호·키체인 체크박스)이 잘려 있고 스크롤 힌트가 없다. TLS가 담긴 `고급 설정`은 스크롤해야만 보인다. SQLite는 필수 항목 `Database file`이 `고급 설정` 안에 있고 `Open read-only` 체크박스가 `연결` 섹션에 있어 구조가 반대다. | 1280×800, 연결 대화상자 | [PostgreSQL](design-shots/ui-audit-newconn-PostgreSQL.png), [SQLite](design-shots/ui-audit-newconn-SQLite.png), [고급 스크롤](design-shots/ui-audit-newconn-PG-advanced-scrolled.png) | 대화상자 높이 고정+내부 스크롤 그림자, 필수 필드는 첫 섹션으로, 매니페스트 필드 그룹(`ui/src/` 연결 폼)과 드라이버 매니페스트의 section 지정 점검 |
| 11 | minor | 한영 혼용: `User`, `Password`, `Host`, `Port`, `Database`, `URI`, `Authentication database`, `TLS mode`, `TLS CA file`, `Open read-only`, `Database file`은 영어인데 `이름`, `드라이버`, `접근 모드`, `키체인에 저장`은 한국어다. TLS 값이 `prefer`처럼 소문자 enum 그대로다. 미리보기 패널은 제목만 한국어고 본문이 영어 문단(`All data and DDL changes were rolled back...`, `Update · affected: 2`, `Before (sample)`)이다. 에이전트 패널에 `MCP activity`, `2 databases connected`. 결과 바에 `rows`와 `행`, `1,000`과 `1000`이 섞인다. 토스트·배너 오류 본문은 영어. 테이블 보기 전환 `Table`, AI 추론 `low/medium/high/max`. | 전역 | [newconn-PostgreSQL](design-shots/ui-audit-newconn-PostgreSQL.png), [31](design-shots/ui-audit-31-change-preview.png), [22](design-shots/ui-audit-22-agent-clients.png), [32](design-shots/ui-audit-32-big-1000-keyscroll.png) | 드라이버 매니페스트 라벨과 UI 문자열의 i18n 키 통일(라벨 `label_ko`), 숫자 포맷 함수 하나로. `ui/src/i18n*`, 드라이버 매니페스트 |
| 12 | minor | 결과 제목에 실행한 SQL이 잘려서 표시된다: `SELECT id, repeat('가나다라 ', 80) AS very_long_text_value, now() AS ts, '{"a":[1,2,` 처럼 줄 중간에서 끊기고 말줄임표가 없다. FK 이동 후 헤더 SQL도 `id = 922337203685477`로 잘려 실제 값(`...5807`)과 다르게 읽힌다. | 1280×800 | [14](design-shots/ui-audit-14-query-table-1280.png), [29](design-shots/ui-audit-29-fk-follow.png) | 한 줄 말줄임(`text-overflow: ellipsis`)+title/툴팁, 잘린 값을 만들지 말고 전체 문자열을 유지. 결과 헤더 컴포넌트 |
| 13 | minor | 도구 영역 여러 줄에 세로 스크롤바 조각이 보인다(탭 줄, 툴바, 필터 줄, 그리드 오른쪽 가장자리). 해당 줄이 세로로 넘친다는 뜻이다. 시스템 오버레이 스크롤바가 콘텐츠 위에 겹친다. 그리드 하단 가로 스크롤바가 마지막 행을 덮는다. | 1280×800 | [06](design-shots/ui-audit-06-big-browse.png), [08](design-shots/ui-audit-08-typed-browse.png) | 툴바 컨테이너 `overflow: hidden`/높이 고정, 스크롤바 전역 스타일(`scrollbar-gutter`, 얇은 커스텀) |
| 14 | minor | `취소` 버튼 아이콘이 빈 체크박스처럼 보인다(정지 아이콘이 아님). 비활성 상태에서는 더 구분이 안 된다. Run 버튼 단축키 칩 `⌘↵`이 작아 판독이 어렵다. `저장`·`설정 저장`·`닫기`가 모두 옅은 soft 스타일이라 주 동작이 안 보인다(주 버튼은 허브의 `연결 추가`만 강조). | 1280×800 | [03](design-shots/ui-audit-03-pg-connected.png), [02](design-shots/ui-audit-02-newconn.png), [21](design-shots/ui-audit-21-ai-panel-midnight.png) | 취소=lucide `Square`(채운 정지), 대화상자 주 버튼은 brand 채움. 편집기 툴바·대화상자 버튼 |
| 15 | minor | 연결 대화상자·요약 제목 표식에 텍스트 문자 `▼`를 쓴다(DESIGN §6은 문자 아이콘 금지). `Parameters` 접기 표식 `▶`도 같다. | 연결 폼, Cypher 파라미터 | [02](design-shots/ui-audit-02-newconn.png), [15](design-shots/ui-audit-15-neo4j-result.png) | lucide `ChevronDown/Right`로 교체 |
| 16 | minor | 명령 팔레트 검색 입력에 macOS 자동 수정 팝업(`Typed ×`)이 떠서 입력줄을 덮는다. 목록 항목에 종류 아이콘이 없고 `Recent · e2e-pg`뿐이라 테이블/쿼리를 구분하기 어렵다. 같은 SQL이 중복해 나열된다. | 1280×800 | [20](design-shots/ui-audit-20-palette-search.png) | 입력에 `autocorrect="off" autocapitalize="off" spellcheck="false"`, 항목에 종류 아이콘·중복 제거. 팔레트 컴포넌트 |
| 17 | minor | 이력 패널의 오류 행 요약이 행 박스 밖으로 넘쳐 화살표 버튼 뒤로 흘러나온다. 하단 패널을 열면 1280×800에서 편집기·결과 영역이 크게 줄어든다. 영어 `HISTORY`, `rows`, `ui`. | 1280×800 | [18](design-shots/ui-audit-18-history-midnight.png) | 행 요약 `min-width:0`+말줄임, 이력 패널 높이 상한·리사이즈. 이력 패널 컴포넌트 |
| 18 | minor | 그래프·ERD가 작게 그려진다. Neo4j 그래프는 캔버스의 일부(약 20%)만 쓰고 초기 맞춤(fit)이 약하다. 노드 라벨이 없다. ERD는 모서리 각진 평면 상자에 `PK id: integer` 평문, 연결선 라벨이 선과 겹쳐 글자를 가른다. ERD 검색 placeholder가 그래프 문구(`레이블 또는 속성 값 검색`) 그대로다. DESIGN §4의 `Table|Graph|JSON|ERD` 전환 중 JSON이 없다. | 1280×800 | [15](design-shots/ui-audit-15-neo4j-result.png), [27](design-shots/ui-audit-27-erd-midnight-1280.png) | 로드 후 fit-to-view, 라벨 줌 임계, ERD 노드를 카드(타입 열 정렬, 둥근 모서리)로, 선 라벨 배경 칩. 그래프·ERD 캔버스 |
| 19 | minor | 에이전트 클라이언트 대화상자에서 `기본(로컬) 클라이…` 처럼 말줄임이 되고 전체 이름 확인 수단이 없다. 상태바의 SQL 서버 버전 문자열(`PostgreSQL 16.10 on aarch64-unknown-linux-musl...`)도 말줄임이고, 같은 줄 우측 요소가 `알림 N`/`에이전트 활동`처럼 서로 다른 형식이다. 말줄임 항목의 툴팁 존재는 미확인. | 1280×800 | [22](design-shots/ui-audit-22-agent-clients.png) | `title`/툴팁, 상태바 버전은 짧은 형식(`PostgreSQL 16.10`)으로. |
| 20 | minor | 대화상자를 닫은 뒤 포커스 링이 rail의 빈 자리(아이콘 없는 상자)에 떠 있다. 포커스가 상태바의 `에이전트 클라이언트` 버튼으로 돌아가는데 링이 rail 위치에 그려진다. | 1024×700, 640×480, 1280 모두 | [23](design-shots/ui-audit-23-1024x700-midnight.png), [25](design-shots/ui-audit-25-640x480-midnight.png) | 포커스 복귀 대상과 `focus-visible` 링 위치 확인. 대화상자 닫기 로직 |
| 21 | minor | 허브 빈 상태 상단에 탭 줄 자리가 빈 띠로 남는다. 연결 후에는 `Query 1` 탭 제목과 하위 breadcrumb 칩 `Query 1`이 중복이고 탭 제목 `e2e-pg / Query 1`도 긴 접두를 반복한다. | 1280×800 | [01](design-shots/ui-audit-01-hub-empty-1280-daylight.png), [03](design-shots/ui-audit-03-pg-connected.png) | 빈 상태에서는 탭 줄 숨김, breadcrumb은 이동 기록이 2개 이상일 때만. |

## 올바르게 보인 것

- 허브 빈 상태의 구성(로고, 한 줄 설명, `연결 추가`/`샘플 쿼리`, 단축키 힌트)과 연결 프로필 타일의 패러다임 색 구분(그래프 보라분홍, 관계형 파랑).
- Daylight·Midnight 둘 다 토큰 색이 일관되게 적용된다. 본문 대비는 충분하고 상태 점(연결 초록, 미연결 회색)이 명확하다.
- 데이터 타입 색(숫자 파랑, 텍스트 초록, 날짜 호박, JSON 청록, NULL pill)과 타입 배지가 DESIGN과 일치한다. NULL은 빈 문자열과 구분된다.
- 가상 스크롤: `e2e.big` 1,000행 결과를 키보드로 내려도 빈 깜박임 없이 행 74~85가 즉시 그려진다. 1,000행 한도 안내(`한도 1000에서 잘림`)가 표시된다.
- 편집 모드: 삭제 행 취소선+분홍, 추가 행 연두 구분, 기본키 잠금 표시, 미리보기 필수 안내(`적용 전 미리보기가 필요합니다`), 미리보기가 DB를 바꾸지 않음(psql 확인).
- 쿼리 변경 미리보기가 롤백됨을 제목(`미리보기 · 커밋하지 않았습니다`)과 warning 색으로 분명히 알린다.
- FK 따라가기, 탐색 기록 뒤로/앞으로 칩, 이동 후 필터된 쿼리 표시가 동작한다.
- 명령 팔레트의 키보드 안내, AI 도우미의 로그아웃 상태 안내와 데이터 전송 고지, 설정 팝오버의 테마 전환이 동작한다.
- 테마 전환 시 CodeMirror와 그래프 캔버스 색이 함께 바뀐다. 포커스 상태의 편집기 테두리 글로우가 보인다.
- 대화상자(연결, AI, 에이전트)는 배경 딤, 닫기 버튼, 제목 정렬이 일관된다.

## 데이터·환경 정리

- 테스트 DB 데이터는 바꾸지 않았다. 편집 모드에서 행 추가·삭제를 대기시키고 미리보기만 했으며(NULL id 오류로 실패, 롤백) Apply는 하지 않았다. 종료 후 `e2e.typed` 두 행(id 1 note NULL raw `00ffabcd`, id 2 `한글 텍스트`)과 `e2e.big` 250,000행을 psql로 확인했다.
- `profiles.json`은 백업과 동일하다. 앱 설정의 테마는 System으로 되돌렸다. 앱 프로세스는 종료했다.
- 감사 중 앱 이력(`history.jsonl`)에 쿼리 몇 건이 추가됐다(SELECT 2건, 의도적 오류 1건, UPDATE 미리보기 1건).
- 임시 파일과 설정 백업은 세션 scratchpad에 있다.
