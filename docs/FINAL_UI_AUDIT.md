# 최종 UI 품질 감사 (네이티브)

날짜 2026-10-17 · 대상 커밋 `2673cf1`(`ui/dist` 빌드 포함) · 실제 Tauri 디버그 앱(`CARGO_TARGET_DIR=target-c2e2e`로 재빌드, 격리 설정 `c2-config`) · 테스트 DB만 사용(PG `axdb-test-pg` 55432, Neo4j `axdb-test-neo4j` 57687, SQLite `e2e-sqlite` 프로필은 목록만 확인). 제품 소스 수정 없음, 커밋 없음. 스크린샷은 `docs/design-shots/final-*.png`(22장, `sips -Z 1200`). 기준 문서: `UI_AUDIT_NATIVE.md`(21건), `U1_NATIVE_REAUDIT.md`, `U2_*_REPORT.md`, `U3_COPY_REPORT.md`.

## 한 줄 결론

**아직 "겹침·가림 없이 깔끔하다"고 말할 수 없다.** 기존 21건 대부분과 새 부품(GlideSelect, BranchedMenu, 그래프 라벨)은 네이티브에서도 잘 동작한다. 그러나 이번에 새로 들어온 CallChip이 하단 상태바를 깨뜨린다. 이 문제는 960×600에서 가장 심하고 1280×800에서도 보이며, 최대 크기(1492×874)에서만 덜하다.

## 검증 범위와 한계

| 항목 | 내용 |
|---|---|
| 크기 | 960×600(최소), 1280×800, 1492×874(이 화면의 최대. 1920×1080은 화면이 작아 불가) |
| 테마 | Daylight·Midnight 모두. 모든 화면을 모든 조합으로 찍지는 못했다. 조합은 아래 표의 스크린샷 파일명 참고 |
| 입력 방식 | 접근성 클릭·키보드. 클립보드 붙여넣기는 쓰지 않았다(이름 입력은 `type-text`) |
| 미확인 | `title` 툴팁(`orca computer`에 hover 없음), 키보드 포커스 시 탭 링(R2) 재현, AI 응답이 있을 때의 CallChip 요약 행(유료 요청이라 활성화하지 않음), 마우스 휠 스크롤(합성 휠이 웹뷰에서 안 먹음), 1920×1080 |

## 1. 기존 항목 상태

범례: fixed / partly / not fixed / 미확인. 스크린샷 경로 접두사 `design-shots/final-`.

| # | 이전 상태 | 지금 | 근거 |
|---|---|---|---|
| 6 열 너비 | partly | **fixed** | `e2e.typed`·`parent` Browse 표가 말줄임 없이 `9223372036854775807` 전체를 표시. 편집 모드의 열 너비 직접 비교는 이번에 하지 않았음(U2 측정은 mock 기준). [07](design-shots/final-07-pagesize-menu-1280-midnight.png) |
| 8 사이드바 스크롤 | not fixed | **fixed(조건부)** | 연결 목록과 탐색기가 분리 스크롤. 활성 연결(PG 또는 Neo4j)의 트리만 보임. 단 960×600에서 활성 카드가 목록 밖으로 밀리는 문제는 N-5. [01](design-shots/final-01-pg-tree-1280-daylight.png), [17](design-shots/final-17-960x600-daylight-tabs-statusbar.png) |
| 11 한영 혼용 | not fixed | **fixed(잔여 소수)** | `이름·드라이버·사용자·비밀번호·데이터베이스·TLS 모드·TLS CA 파일`, 쿼리 UI, 이력, 팔레트, 에이전트 패널이 한국어. 남은 영어는 기술 용어(Host, Port, SQL, Cypher, MCP, 도구 이름, `prefer` 등 TLS 값), DB 원본 오류, 탐색기 폴더명(`Tables`·`Views`·`Columns`·`Labels` 등). 폴더명 처리는 N-8. [03](design-shots/final-03-glideselect-driver-menu-1280-daylight.png), [20](design-shots/final-20-palette-960x600-daylight.png) |
| 12 긴 SQL 제목 | partly | **미확인** | 이번에는 80자를 넘는 결과 제목을 만들지 않았다. 1행짜리 제목은 정상. 툴팁은 확인 불가 |
| 14 주 버튼 | partly | **partly** | 연결 대화상자 `저장`, AI `설정 저장`, 에이전트 `닫기`가 여전히 옅은 보라 tint(채움 아님). 실행 버튼만 채워진 파랑. 이름·필드를 채운 상태에서도 같음. [05](design-shots/final-05-glideselect-access-flip-1280-daylight.png), [09](design-shots/final-09-agent-clients-dialog-1280-midnight.png), [12](design-shots/final-12-ai-panel-logged-out-1280-midnight.png) |
| 15 `▼` 문자 | partly | **fixed** | 연결 폼 섹션 머리글에 chevron 하나만(R1도 해소). `파라미터` 접기도 chevron 아이콘. [03](design-shots/final-03-glideselect-driver-menu-1280-daylight.png) |
| 18 그래프 | partly | **fixed** | Neo4j 25행 그래프가 캔버스 폭 전체를 쓰고 노드 라벨 표시. 960×600에서 일부 라벨이 이웃 노드 점과 겹침(N-7). JSON 전환은 여전히 없음. [08](design-shots/final-08-neo4j-graph-1280-midnight.png), [14](design-shots/final-14-largest-1492x874-midnight.png) |
| 19 클라이언트 이름 말줄임 | partly | **partly** | 에이전트 대화상자 노드 이름이 `기본(로컬) 클라이…`로 여전히 말줄임. 툴팁은 미확인. 상태바 버전 짧은 형식은 유지(단 N-1로 인해 자주 잘림). [09](design-shots/final-09-agent-clients-dialog-1280-midnight.png) |
| R1 chevron 두 겹 | 회귀 | **fixed** | 한 개만 보임 |
| R2 탭 포커스 링 | 회귀 | **미확인** | 키보드 포커스 상태를 만들지 못함. 마우스 상태에서는 탭 전체가 한 상자 |
| R3 팔레트 정렬 | 회귀 | **fixed** | 결과 행이 왼쪽 정렬, 종류 아이콘과 오른쪽 라벨 정렬 정상. [20](design-shots/final-20-palette-960x600-daylight.png) |

### 고쳐졌다고 했던 항목 스팟 체크

| # | 결과 |
|---|---|
| 1 최소 창 | 960×600에서 사이드바·탭·툴바·결과·상태바·rail 설정 버튼 모두 보임. 단 상태바는 N-1 |
| 2·3 토스트 | 우하단에 뜨고 자동 소멸. 설정 팝오버와 안 겹침. [06](design-shots/final-06-settings-theme-menu-1280-daylight.png) |
| 4 배너 | 연결 실패 시 배너 하나, 닫기 버튼 `Dismiss error`. 배너가 콘텐츠를 약 60px 밀지만 탭 줄은 안 밀림(탭이 없던 상태). [22](design-shots/final-22-connect-error-banner-statusbar-1280-daylight.png) |
| 5 탭 바(사용자 제보) | 7탭(browse 5개 포함), 양 테마, 아래 표 참고 |
| 7 FK 버튼 | 이번에 `e2e_fk.child`를 열었으나 FK 셀 확대 확인은 안 함(미확인). `parent`의 큰 정수는 전체 표시 |
| 9 Neo4j 버전 | 상태바 `Neo4j 5.26.30` 정상 |
| 10 대화상자 높이 | 드라이버 전환 간 대화상자 높이 일정, 내부 스크롤 |
| 16·17·20·21 | 팔레트·이력(말줄임, 겹침 없음)·허브·탭 줄 정상. 대화상자 닫은 뒤 레일 포커스 링은 이번에 못 봄 |

## 2. 새 부품

### GlideSelect (네이티브)

| 위치 | 결과 |
|---|---|
| 연결 대화상자 드라이버 | 아래로 열림, 대화상자 안에 들어감(1280, 960×600 모두). [03](design-shots/final-03-glideselect-driver-menu-1280-daylight.png), [18](design-shots/final-18-glideselect-driver-960x600-daylight.png) |
| TLS 모드 (PG, 하단 근처) | **위로 뒤집힘**, 잘리지 않음. 열린 메뉴가 `데이터베이스`·`사용자` 입력을 덮는 것은 의도된 오버레이. [04](design-shots/final-04-glideselect-tls-flip-1280-daylight.png) |
| 접근 모드 (고급 설정, 하단) | **위로 뒤집힘**, 푸터 위에서 완전히 보임. [05](design-shots/final-05-glideselect-access-flip-1280-daylight.png) |
| 페이지 크기 | 툴바 아래로 열림, 선택 표시 정상. 항목이 `100/500/1000`(쉼표 없음)이라 결과 막대의 `1,000행`과 표기가 다름(N-9). [07](design-shots/final-07-pagesize-menu-1280-midnight.png) |
| 설정(테마) | 팝오버 오른쪽 밖으로 튀어나오며 슬라이더를 덮음. 동작은 정상(N-6). [06](design-shots/final-06-settings-theme-menu-1280-daylight.png) |
| 클라이언트 권한 | 이번에 열지 않음(권한 편집 흐름은 `permissions-e2e`가 간접 검증, 시각 확인은 못 함) |

### BranchedMenu 사이드바

| 확인 | 결과 |
|---|---|
| PG 트리 (스키마→Tables→테이블→Columns) | 접기/펼치기, 선, 깊이 4단계 모두 정상. [01](design-shots/final-01-pg-tree-1280-daylight.png), [02](design-shots/final-02-pg-columns-deep-1280-daylight.png) |
| Neo4j 트리 (Labels→레이블→속성) | 정상, 지연 로딩 중 자식 영역이 한동안 비어 있음(로딩 표시 없음, N-10). [08](design-shots/final-08-neo4j-graph-1280-midnight.png) |
| 활성 reach 선 | 활성 열(`note`)까지 accent 선이 이어지고 rail marker가 스키마 옆에 섬. 활성 행 상자의 오른쪽 테두리가 스크롤 영역 가장자리에서 잘림(N-4) |
| 연결 목록/탐색기 분리 스크롤 | 분리됨. 활성 연결만 펼침, 다른 연결의 트리는 숨김(Neo4j↔PG 전환 시 상태 유지) |
| 깊은 컬럼 행 | 컬럼 이름이 타입 문자열에 밀려 `amo…`, `creat…`로 줄어듦(N-4) |
| 필터 | 입력창·새로고침 버튼 겹침 없음. 필터 결과 동작은 이번에 시도하지 않음 |

### CallChip (에이전트 활동)

`bench/e2e-bridge.mjs`와 별도 스크립트(12초 `pg_sleep`, 존재하지 않는 테이블 조회)로 실제 MCP 이벤트를 만들었다. 칩은 **하단 상태바**에 나타난다.

| 상태 | 결과 |
|---|---|
| running | `run_query e2e-pg 3357 ms` 타이머가 올라가는 칩 확인. [10](design-shots/final-10-callchip-running-statusbar-1280-midnight.png) |
| done | `✓ open_connection e2e-pg 21 ms`, `✓ run_query e2e-pg · 1행 12.0 s` 등. 접근성 이름 `…, 완료 (45 ms)` |
| error | `run_query e2e-pg 14 ms`, 분홍 배경에 재시도 아이콘. [11](design-shots/final-11-callchip-done-error-statusbar-1280-midnight.png) |
| 문제 | 칩이 계속 쌓여 상태바 왼쪽(연결 이름·서버 버전·실행 요약)을 밀어내고, 오른쪽 끝은 창 밖으로 잘림(N-1). 에이전트 클라이언트 대화상자 안에서는 칩이 아니라 `도구 사용 중…` 접기 줄만 보임 |
| AI 패널 요약 행 | 로그아웃·꺼짐 상태에서는 설정 화면만 있고 요약 행이 없다. 응답이 있을 때의 행은 **미확인**. [12](design-shots/final-12-ai-panel-logged-out-1280-midnight.png) |

## 3. 한국어 문구 (960×600, 1280×800)

- 탭, 툴바 버튼(`실행`, `미리보기`, `취소`, `새로고침`, `편집`), 대화상자 제목·라벨, 팔레트, 이력, 설정에서 한국어 잘림·줄바꿈 문제 없음.
- 말줄임: 탭 제목(`e2e-neo4j / 다이어그램 · La…`, 정상), 에이전트 클라이언트 노드(#19), 이력의 SQL 미리보기(정상).
- 상태바는 N-1. 연결 이름·서버 버전이 `e2…`, `PostgreSQ…`로 잘리고 960×600에서는 아예 사라진다.
- 남은 영어는 위 #11 항목의 기술 용어와 폴더명 외에 `String`(타입), `run_query` 같은 도구 이름.

## 4. 사용자 제보 탭 바 (7탭, browse 5개 포함)

| 항목 | Midnight | Daylight |
|---|---|---|
| 제목+X가 한 탭 | 통과 | 통과 |
| 위 그라데이션·아래 밑줄 정렬 | 통과 | 통과 |
| 비활성 간격 균일 | 통과 | 통과 |
| 7번째 탭 활성 시 스크롤 | 통과(오른쪽 끝에 맞음) | 통과 |
| 스크롤바 | 숨김 | 숨김 |
| 참고 | 왼쪽에 일부 잘린 탭 조각(점·제목이 스크롤로 잘림)이 남는 건 스크롤 동작(N-3) | 같음 |

[15 Midnight](design-shots/final-15-tabs-7-1280-midnight.png), [16 Daylight](design-shots/final-16-tabs-7-1280-daylight.png), [17 960×600](design-shots/final-17-960x600-daylight-tabs-statusbar.png)

## 5. 새 발견 사항

심각도: major(가림·잘림·내용 소실), minor(다듬기). 모두 보고 전용.

| ID | 심각도 | 화면 | 크기 | 테마 | 스크린샷 | 문제 | 추정 파일 |
|---|---|---|---|---|---|---|---|
| N-1 | major | 하단 상태바 | 960×600 최악, 1280×800, 1492×874 일부 | 양쪽 | [13](design-shots/final-13-statusbar-chips-overflow-960x600-midnight.png), [11](design-shots/final-11-callchip-done-error-statusbar-1280-midnight.png), [22](design-shots/final-22-connect-error-banner-statusbar-1280-daylight.png), [17](design-shots/final-17-960x600-daylight-tabs-statusbar.png) | 에이전트 CallChip이 상태바에 계속 쌓여(만료·개수 제한 없음, 30건까지 확인) 연결 이름·서버 버전·실행 요약을 `e2…`로 줄이거나 960×600에서는 완전히 가린다. 맨 오른쪽 칩은 창 밖으로 잘린다. 또 `MCP 활동` 라벨이 칩 위쪽에 세로로 반쯤 잘려 보인다. | `ui/src/` 상태바 `agent-controls`·CallChip 활동 줄(`AgentPanel`/상태바 컴포넌트), `style.css` |
| N-2 | minor | 연결 대화상자, AI 패널, 에이전트 대화상자 | 전체 | 양쪽 | [05](design-shots/final-05-glideselect-access-flip-1280-daylight.png), [12](design-shots/final-12-ai-panel-logged-out-1280-midnight.png), [09](design-shots/final-09-agent-clients-dialog-1280-midnight.png) | `저장`, `설정 저장`, `닫기`가 옅은 tint라 주 버튼으로 안 보임(원 #14, U2가 brand로 바꿨다고 했으나 네이티브에서는 그대로). | `ui/src/` 대화상자 푸터 버튼 variant, `style.css` `.ax-button` |
| N-3 | minor | 탭 바 | 960×600, 1280×800(탭 6개 이상) | 양쪽 | [15](design-shots/final-15-tabs-7-1280-midnight.png), [17](design-shots/final-17-960x600-daylight-tabs-statusbar.png) | 스크롤된 탭 바 왼쪽에 점·제목이 잘린 빈 탭 조각이 남는다. 가로 스크롤바가 숨겨져 넘침 힌트가 없다. | 탭 컴포넌트, 스크롤 가장자리 페이드 |
| N-4 | minor | 사이드바 컬럼 단계 | 1280×800 | 양쪽 | [02](design-shots/final-02-pg-columns-deep-1280-daylight.png) | 컬럼 이름이 타입 문자열에 밀려 `amo…`, `creat…`로 잘림. 활성 행 상자 오른쪽 테두리가 스크롤 영역 가장자리에서 잘림. | `Connections.tsx`, `branched-menu.css` (행 detail 영역 폭 제한·`min-width`) |
| N-5 | major | 사이드바 | 960×600 | 양쪽 | [17](design-shots/final-17-960x600-daylight-tabs-statusbar.png), [19](design-shots/final-19-history-960x600-daylight.png) | 활성 연결 카드(`e2e-pg`)가 연결 목록 스크롤 밖에 있어 보이지 않고, 탐색기가 어느 연결의 것인지 알 수 없다. 클릭으로 다른 연결을 활성화하려면 목록을 스크롤해야 하는데 합성 스크롤은 안 먹어 자동화에서는 도달이 어려웠다. 탐색기 영역도 약 330px뿐. | `Connections.tsx`/`style.css` 활성 카드 `scrollIntoView`, 낮은 창에서 목록 최대 높이 |
| N-6 | minor | 설정 팝오버 | 1280×800 | 양쪽 | [06](design-shots/final-06-settings-theme-menu-1280-daylight.png) | 테마 GlideSelect 메뉴가 팝오버 폭 밖으로 튀어나오고 아래 슬라이더 레이블을 덮는다. 기능상 문제는 없음. | GlideSelect 메뉴 폭/정렬 |
| N-7 | minor | Neo4j 그래프 | 960×600 | Midnight | 미첨부(관찰, 같은 화면의 [08](design-shots/final-08-neo4j-graph-1280-midnight.png)은 1280) | 캔버스가 좁아지면 일부 라벨이 이웃 노드 점과 겹침(`LimitMovie1`, `Limit3`). | 그래프 라벨 배치(`graph` 캔버스) |
| N-8 | minor | 탐색기 폴더명 | 전체 | 양쪽 | [01](design-shots/final-01-pg-tree-1280-daylight.png), [08](design-shots/final-08-neo4j-graph-1280-midnight.png) | `Tables`, `Views`, `Columns`, `Labels`, `Relationship Types`, `Property Keys`, `Indexes & Constraints`가 영어. 기술 용어로 볼 수도 있으나 같은 화면의 다른 라벨은 모두 한국어. 정책 결정 필요. | 드라이버 `explorer.rs`(각 드라이버) 라벨 |
| N-9 | minor | 페이지 크기 | 1280×800 | 양쪽 | [07](design-shots/final-07-pagesize-menu-1280-midnight.png) | `1000`과 결과 막대의 `1,000행`이 표기 불일치. | 페이지 크기 옵션 렌더 |
| N-10 | minor | Neo4j 트리 | 1280×800 | Midnight | 관찰만 | 레이블 펼침 직후 자식이 로드될 때까지 빈 영역이 보이고 로딩 표시가 없다. | `Connections.tsx` 지연 로딩 |

## 6. E2E 회귀

앱 PID 50629(디버그 빌드, 격리 설정). 실행 전 `e2e.typed`는 기준 상태.

| 스크립트 | 기준 | 결과 |
|---|---|---|
| `bench/e2e-bridge.mjs` | 21/21 | **21/21 통과** |
| `bench/preview-e2e.mjs` | 5/5 | **5/5 통과** |
| `bench/permissions-e2e.mjs` (`AXDB_APP_PID=50629`) | 13/13 | **13/13 통과**(첫 실행). `permissions-e2e.mjs:77` 예외는 이번에 발생하지 않아 재실행하지 않았다. 따라서 "한 번 실패, 재실행 통과"는 기록할 사례가 없다. |

- 실행 후 `e2e.typed`는 기준 두 행(id 1 note NULL raw `00ffabcd`, id 2 `한글 텍스트`), `e2e.big` 250,000행 그대로.
- 권한 벤치가 `agent_clients.json`의 토큰 해시를 회전시킨다(이전 실행과 동일한 부작용). `profiles.json`은 시작 전 백업과 동일.
- 감사용으로 만든 테스트 쿼리(`SELECT pg_sleep(12)`, 오류 쿼리)와 MCP 호출이 `history.jsonl`·`agent-audit.jsonl`에 남았다.

## 정리

- 연결 대화상자에서 `final-audit-tmp`라는 이름을 입력했으나 저장하지 않고 취소했다(프로필 추가 없음).
- 앱 테마를 System으로 되돌렸고 앱 프로세스는 종료했다.
- 제품 소스·`ui/dist`는 건드리지 않았다. 생성 파일은 이 문서와 `docs/design-shots/final-*.png` 22장뿐이다.
