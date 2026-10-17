# U1 네이티브 재감사

날짜 2026-10-17 · 대상 커밋 `c900231` · 실제 Tauri 디버그 앱(`CARGO_TARGET_DIR=target-c2e2e`, 격리 설정 `c2-config`), 창 1280×800, Daylight·Midnight 모두. 테스트 DB만 사용(PG `axdb-test-pg` 55432, Neo4j `axdb-test-neo4j` 57687). 제품 소스 수정 없음, 커밋 없음. 원본은 `docs/UI_AUDIT_NATIVE.md`(21건). 스크린샷은 `docs/design-shots/u1-native-<번호>-*.png`(1600px 이하)이며 번호는 원본 발견 번호다.

## 요약

- 21건 중 fixed 13, partly fixed 6, not fixed 2(#8, #11), regressed 0(툴팁 hover는 미확인). 새 회귀(경미) 3건.
- 사용자 제보 탭 바: 요구 5항목 모두 충족(양 테마).
- E2E: `e2e-bridge` 21/21, `preview-e2e` 5/5, `permissions-e2e` 13/13(첫 실행은 크래시, 아래 참고).

## 발견 항목별 상태

| # | 원 심각도 | 상태 | 근거 |
|---|---|---|---|
| 1 | blocker | fixed | 500×300으로 줄이는 시도가 960×600으로 고정됨(`osascript`로 확인). 최소 크기에서 사이드바·탭·편집기·결과·상태바·레일 설정 버튼 모두 보임. [01](design-shots/u1-native-01-min-window-midnight.png) |
| 2 | major | fixed | 토스트가 우하단에 뜨고 약 5초 뒤 자동 소멸(연결·결과 토스트 확인). 트리를 덮지 않음. [07](design-shots/u1-native-07-child-browse-daylight.png) |
| 3 | major | fixed | 설정 팝오버(좌하단)와 토스트(우하단)가 겹치지 않음. 긴 오류 토스트를 Midnight에서 만들어 보지는 않음. [03 Daylight](design-shots/u1-native-03-settings-popover-daylight.png), [03 Midnight](design-shots/u1-native-03-settings-popover-midnight.png) |
| 4 | major | fixed | 연결 실패는 탭 바 아래 배너 하나만 뜨고 토스트·탭 배너 중복 없음. 탭 바가 밀리지 않음. 닫기 X가 `Dismiss error`로 노출됨. 단, 배너가 콘텐츠를 약 48px 아래로 민다. [04](design-shots/u1-native-04-badport-error-midnight.png) |
| 5 | major | fixed | 아래 "사용자 제보 탭 바" 참고. [05 Daylight](design-shots/u1-native-05-tabs-7-daylight.png), [05 Midnight](design-shots/u1-native-05-tabs-7-midnight.png) |
| 6 | major | partly fixed | 두 모드 모두 숫자 열 머리글·값이 오른쪽 정렬, 편집 모드 Action 열은 삭제 아이콘으로 바뀌고 영어 `Action` 제거. 그러나 열 너비는 두 모드가 다르다(`amount`가 Browse에서는 말줄임, 편집에서는 전체 폭). [Browse](design-shots/u1-native-06-typed-browse-midnight.png), [편집](design-shots/u1-native-06-typed-edit-midnight.png), [Daylight](design-shots/u1-native-06-typed-browse-daylight.png) |
| 7 | major | fixed | `e2e_fk.child`의 FK ↗ 버튼이 값 오른쪽에 분리되어 `9223372036854775807` 전체가 보임. [07](design-shots/u1-native-07-child-browse-daylight.png) |
| 8 | major | not fixed | 사이드바 전체가 한 스크롤이고 Neo4j·PG 트리가 동시에 펼쳐져 길게 이어짐. 스크롤바도 그대로. U1 보고서에서도 다루지 않은 항목. [21](design-shots/u1-native-21-hub-connected-midnight.png) |
| 9 | major | fixed | 상태바에 `Neo4j 5.26.30` 표시. [18 Neo4j](design-shots/u1-native-18-neo4j-graph-midnight.png) |
| 10 | major | fixed | 연결 대화상자 높이가 MongoDB·SQLite·Redis·PostgreSQL에서 같고 내부 스크롤. SQLite는 `Database file`이 첫 `연결` 섹션에 있음. PG 고급 설정 펼침 가능. [SQLite](design-shots/u1-native-10-newconn-sqlite-midnight.png), [Redis](design-shots/u1-native-10-newconn-redis-midnight.png), [PG 고급](design-shots/u1-native-10-newconn-pg-advanced-midnight.png), [Mongo](design-shots/u1-native-10-newconn-mongo-midnight.png) |
| 11 | minor | not fixed | `User`, `Password`, `Host`, `Port`, `Database`, `TLS mode`, `prefer`, `Open read-only`, `Database file`, `host/port/db/username`(Redis), `MCP activity`, `1 databases connected`, `HISTORY`, `rows`, `Table`, 영어 오류 본문이 그대로. 메모리의 UI 언어 정책(한국어 통일, 기술 용어·DB 원본 오류 유지)과 비교하면 라벨류가 아직 영어. |
| 12 | minor | partly fixed | FK 따라가기 헤더는 `id = 9223372036854775807` 전체로 고쳐짐. 긴 SQL 결과 제목은 여전히 약 80자에서 잘려 `... '{"a":`로 끝나고 말줄임표가 없다(DOM 텍스트 자체가 80자라 백엔드 제목 절단). 호버 title은 `orca computer`에 hover가 없어 확인 못 함. [FK](design-shots/u1-native-12-fk-follow-daylight.png), [긴 SQL](design-shots/u1-native-14-long-sql-daylight.png) |
| 13 | minor | fixed | 탭 줄·툴바에 세로 스크롤바 조각 없음, 탭 가로 스크롤바 숨김. 그리드 가로 스크롤바는 마지막 행 아래 별도 줄에 있어 행을 덮지 않음(2행 표 기준). |
| 14 | minor | partly fixed | 취소 아이콘은 채운 정지 사각형, 실행 단축키 칩 판독 가능. 연결 대화상자 `저장`은 옅은 보라 tint라 주 버튼 대비가 약하고 에이전트 대화상자 `닫기`도 같음(U1 보고서가 남은 일로 명시한 부분). [10](design-shots/u1-native-10-newconn-sqlite-midnight.png), [19](design-shots/u1-native-19-agent-clients-midnight.png) |
| 15 | minor | partly fixed | 연결 폼 섹션 머리글에 텍스트 `▼`는 없음. 그러나 chevron이 두 개 겹쳐 보인다(아래 새 회귀 R1). |
| 16 | minor | fixed | 자동 수정 팝업 없음, 연결·테이블·쿼리 종류 아이콘, 중복 쿼리 없음. 목록 텍스트가 가운데 정렬로 보이는 점은 별건(R3). [16](design-shots/u1-native-16-palette-midnight.png) |
| 17 | minor | fixed | 이력 행 내용이 행 안에서 말줄임, 상자·화살표 겹침 없음, 패널 높이 제한. `HISTORY`·`rows`는 영어(#11). [17](design-shots/u1-native-17-history-midnight.png) |
| 18 | minor | partly fixed | ERD는 둥근 카드, 이름·타입 열 정렬, PK 표시, 선 라벨 칩, 검색 placeholder `테이블 이름 검색…`로 fixed. Neo4j 그래프(60행, 120노드)는 캔버스 높이는 쓰지만 폭은 약 30%만 쓰고, 초기 줌에서 노드 라벨이 보이지 않으며, `Table|Graph|JSON|ERD`의 JSON 전환은 없다. [ERD](design-shots/u1-native-18-erd-daylight.png), [그래프](design-shots/u1-native-18-neo4j-graph-midnight.png) |
| 19 | minor | partly fixed | 상태바 버전이 `PostgreSQL 16.10`, `Neo4j 5.26.30`으로 짧아짐. 에이전트 클라이언트 이름은 여전히 `기본(로컬) 클라이…`로 말줄임. `title` 툴팁은 hover 불가로 미확인. [19](design-shots/u1-native-19-agent-clients-midnight.png) |
| 20 | minor | fixed | 대화상자를 닫은 뒤 레일의 빈 포커스 링이 없고 에이전트 아이콘이 보임. [20](design-shots/u1-native-20-focus-after-dialog-midnight.png) |
| 21 | minor | fixed | 탭이 없으면 탭 줄이 없다. 단일 항목 탐색 기록 칩이 사라지고 FK 이동 뒤에만 `← → child · parent · filtered`가 나온다. [21](design-shots/u1-native-21-hub-connected-midnight.png), [12](design-shots/u1-native-12-fk-follow-daylight.png) |

## 사용자 제보 탭 바(양 테마, 7탭)

| 항목 | 결과 |
|---|---|
| 제목과 닫기 X가 하나의 탭 | 통과. 탭 전체가 한 상자. |
| 안쪽 상자 테두리 없음 | 통과(마우스 상태). 키보드 포커스 시 제목 버튼에만 링이 그려져 X가 링 밖에 남는다(R2). |
| 위쪽 그라데이션과 아래 밑줄 정렬 | 통과. 활성 탭 전체 폭에 맞음. |
| 비활성 탭 간격 균일 | 통과. 모든 탭 폭이 같고 X 자리는 비활성에서 비어 있다. |
| 6개 이상에서 활성 탭 스크롤 | 통과. 7번째 탭(`parent`)이 열릴 때 오른쪽 끝에 맞춰 보임. 가로 스크롤바 숨김. |

## 최소 창 크기

- 시도: 500×300 → 실제 960×600, 이 크기가 하한. 이 크기에서 사이드바 폭 제한, 탭, 툴바, 결과, 상태바, 레일 하단 설정 버튼이 모두 보인다. 상태바의 버전 문자열은 짧아 보이지 않는 것으로 보이며(공간 부족 시 생략) 별도 오류는 아님. [01](design-shots/u1-native-01-min-window-midnight.png)

## `:root .ax-button` 변경의 새 회귀

| ID | 심각도 | 내용 |
|---|---|---|
| R1 | minor | 연결 대화상자 섹션 머리글(`연결`, `인증`, `고급 설정`)에 chevron이 두 겹으로 그려진다(`⌄ ⌄ 연결`, `› › 고급 설정`). 네이티브 summary 마커와 CSS chevron이 함께 보이는 것으로 추정. [SQLite](design-shots/u1-native-10-newconn-sqlite-midnight.png), [Redis](design-shots/u1-native-10-newconn-redis-midnight.png) |
| R2 | minor | 키보드·접근성 클릭 후 탭 제목 버튼에만 포커스 링이 그려져 탭이 제목/X 두 상자로 보인다. [06](design-shots/u1-native-06-typed-browse-midnight.png) |
| R3 | minor | 명령 팔레트 결과 행 텍스트가 가운데 정렬로 보인다. [16](design-shots/u1-native-16-palette-midnight.png) |

테두리 있는 버튼 확인: 연결 대화상자 푸터(`연결 테스트`, `취소`), 테이블 툴바(`이전`, `다음`, `새로고침`, `편집`, `행 추가`), 탭 바 `+ 쿼리 · SQL` 모두 테두리 스타일이 유지된다. 이 부분의 회귀는 없다.

## E2E 회귀

| 스크립트 | 결과 |
|---|---|
| `bench/e2e-bridge.mjs` | 21/21 |
| `bench/preview-e2e.mjs` | 5/5 |
| `bench/permissions-e2e.mjs` | 13/13 (두 번째 실행) |

- `permissions-e2e.mjs` 첫 실행은 `permissions-e2e.mjs:77`에서 예외로 종료했다. 같은 앱·같은 상태에서 재실행하면 13/13이 나왔다. 원인은 확인하지 않았다(재현 실패, 환경 요인 의심).
- 실행 전 `e2e.typed`는 기준 상태(id 1 note NULL raw `00ffabcd`, id 2 `한글 텍스트`)였고 실행 후에도 같다.

## 확인하지 못한 것과 주의

- hover 명령이 없어 `title` 툴팁(결과 제목, 에이전트 클라이언트 이름)과 토스트 hover 정지는 확인하지 못했다.
- 긴 오류 토스트(Midnight)와 토스트 4개 이상 동시 표시는 만들어 보지 않았다.
- 실제 오류 행이 있는 이력 패널은 최상단 행만 확인했다.
- SQLite 연결은 폼만 봤고 실제 연결은 하지 않았다. Redis는 폼만(테스트 인스턴스 없음).
- 스크린샷은 Daylight와 Midnight를 섞어 찍었다. 원본이 대부분 Midnight라서 비교 항목은 Midnight를 우선했고 Daylight는 FK·탭·ERD·긴 SQL에 사용했다.
- 도중 clipboard 붙여넣기가 엉뚱한 문자열("작업은 단계별 commit 진행.")을 편집기에 넣어 Neo4j 구문 오류 쿼리가 한 번 실행됐다. 읽기 전용 구문 오류이며 DB 변경은 없다. 테스트 이력(`history.jsonl`)에 쿼리 몇 건이 더 남았다.

## 정리

- 테스트 데이터는 바꾸지 않았다(편집 모드를 열기만 하고 변경하지 않음). `profiles.json`은 시작 전 백업과 동일. 앱 테마는 System으로 되돌렸고 앱은 종료했다.
