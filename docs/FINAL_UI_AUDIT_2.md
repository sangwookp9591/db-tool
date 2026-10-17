# 최종 UI 품질 재감사 (네이티브)

날짜 2026-10-17 · 대상 커밋 `c1e3020`(`ui/dist` 빌드 포함) · 실제 Tauri 디버그 앱(`CARGO_TARGET_DIR=target-c2e2e`로 재빌드, 격리 설정 `c2-config`, `AXDB_AGENT_BRIDGE=1`) · 테스트 DB만 사용(PG `axdb-test-pg`, Neo4j `axdb-test-neo4j`, SQLite는 벤치로만). 제품 소스 수정 없음, 커밋 없음. 스크린샷은 `docs/design-shots/final2-*.png` 14장(`sips -Z 1200`). 기준: `FINAL_UI_AUDIT.md`, `U4_CHROME_REPORT.md`, `U4_SIDEBAR_REPORT.md`.

## 한 줄 결론

**major(가림·잘림·내용 소실)는 이번에 보지 못했다. 이전 감사의 N-1~N-9, #12, #14, R2는 모두 고쳐졌거나 재현되지 않았다.** 다만 minor 5건(F-1~F-5)이 남아 있어 "겹침·잘림이 전혀 없다"고는 말할 수 없다. 또 모든 화면×크기×테마 조합을 전수로 돌리지는 못했으므로(아래 "범위와 한계") "전체 UI가 정확하고 깨끗하다"는 확정 답은 아니다. 확인한 범위 안에서는 깨끗하다.

## 범위와 한계

| 항목 | 내용 |
|---|---|
| 크기 | 960×600, 1280×800, 1492×874(이 화면의 최대). `System Events`로 창 크기를 맞췄다. |
| 테마 | Daylight·Midnight. 항목마다 한 테마씩만 찍은 것이 많다(스크린샷 파일명 참고). |
| 입력 | 접근성 클릭·`set-value`·키보드. 클립보드 미사용. 합성 마우스 휠 스크롤과 사이드바 분할선 드래그는 웹뷰에서 먹지 않았다. |
| 못 한 것 | SQLite 화면, 클라이언트 권한 편집, 편집 모드/미리보기 대화상자의 시각 확인(E2E로만 검증), 모든 화면의 1280·최대 Midnight/Daylight 교차, N-10 로딩 스켈레톤(펼침이 즉시 끝나 포착 못 함), 분할선을 좁혀 만든 "좁은 캔버스" 그래프. |

## 1. 항목별 상태

범례: fixed / partly / not fixed / 미확인. 스크린샷 접두사 `design-shots/final2-`.

| ID | 확인 조건 | 상태 | 근거 |
|---|---|---|---|
| N-1 상태바 | 1280 Daylight, `stream.mjs`로 MCP 40건(오류 포함) 연속 호출. 960 Daylight 동일 | **fixed** | 1280: 칩 정확히 1개(`run_query e2e-pg 249 ms`), 연결 이름 `e2e-pg`·버전 `PostgreSQL 16.10` 유지, 잘림 없음. 960: 칩은 설계대로 숨김(≤1040px), 이름·버전 유지. `MCP 활동` 세로 잘림 사라짐. [03](design-shots/final2-03-statusbar-one-chip-1280-daylight.png) |
| N-1 전체 이력 | 에이전트 클라이언트 대화상자 1280 Daylight | **fixed** | `도구 50개 사용 · 25.7s 실패 7개`, `MCP 호출 이력 50`(상한 50, 76건 이상 호출했으므로 이전 호출은 밀려난다). 오류 항목은 재시도 아이콘. [04](design-shots/final2-04-agent-history-primary-button-1280-daylight.png) |
| N-2/#14 주 버튼 | 연결 `저장`(960 Midnight), AI `설정 저장`(최대 Midnight), 에이전트 `닫기`(1280 Daylight·최대 Midnight) | **fixed** | 모두 채워진 brand 그라데이션. [10](design-shots/final2-10-connection-dialog-primary-960-midnight.png), [12](design-shots/final2-12-ai-dialog-primary-largest-midnight.png), [04](design-shots/final2-04-agent-history-primary-button-1280-daylight.png) |
| N-3 탭 바 | 탭 8개, 960 Daylight(마지막 탭 활성). 최대 Midnight 탭 5개 | **partly** | 잘린 탭 조각 없음, 왼쪽에 페이드, 탭 경계에 정렬. 그러나 끝 정렬 때문에 오른쪽이 약 1.7탭 폭 비어 보인다(F-1). 최대 크기에서는 5탭이 모두 보임. [05](design-shots/final2-05-tabs-8-960-daylight.png) |
| N-4 컬럼 행 | `e2e.typed` 컬럼, 960 Daylight, 4단계 깊이 | **fixed** | 컬럼 이름 `id`가 먼저 전체 표시, 타입 문자열 `integer · NOT NULL …`이 먼저 줄어든다. [02](design-shots/final2-02-sidebar-deep-columns-korean-960-daylight.png) |
| N-5 활성 카드 | 960×600 Daylight, 연결 4개 | **fixed** | 활성 `e2e-pg` 카드가 목록 영역 안에 고정되어 보이고 탐색기에 약 330px 이상 남는다. 단 주변 카드가 조각으로 비친다(F-5). [01](design-shots/final2-01-sidebar-active-card-960-daylight.png) |
| N-6 설정 팝오버 | 960 Daylight | **fixed** | 테마 메뉴가 트리거 폭이고 팝오버 안에 들어간다. [08](design-shots/final2-08-settings-theme-menu-960-daylight.png) |
| N-7 그래프 라벨 | Neo4j 25노드, 960 Daylight·Midnight, 1280, 최대 Midnight | **fixed(재현 안 됨)** | 라벨이 이웃 점과 겹치지 않는다(`LimitMovie10`/`Alice`, `Limit7`/`LimitMovie1` 모두 분리). 분할선이 안 움직여 더 좁은 캔버스는 못 만들었다. 이력 패널을 열어 캔버스가 줄어든 경우는 F-2. [09](design-shots/final2-09-graph-labels-960-midnight.png), [14](design-shots/final2-14-error-banner-graph-largest-midnight.png) |
| N-8 폴더명 | PG·Neo4j 트리, 960·1280·최대 | **fixed** | `테이블`·`뷰`·`컬럼`·`인덱스`, `레이블`·`관계 유형`·`속성 키`·`인덱스·제약 조건`. 팔레트의 종류 라벨도 한국어. [02](design-shots/final2-02-sidebar-deep-columns-korean-960-daylight.png) |
| N-9 페이지 크기 | 1280 Midnight, 메뉴 열기 | **fixed** | `100 / 500 / 1,000`. |
| N-10 지연 로딩 | PG·Neo4j 펼침 | **미확인** | 펼침이 즉시 끝나 스켈레톤을 포착하지 못했다. 빈 영역이 남지는 않았다. |
| #12 긴 제목 | 80자 넘는 SQL 결과, 960 Daylight | **fixed** | 결과 제목이 `SELECT 'aaaa…`로 말줄임. 셀 값도 `aaa…`. [07](design-shots/final2-07-long-title-ellipsis-960-daylight.png) |
| #19 클라이언트 이름 | 에이전트 대화상자, 1280·최대 | **partly(시각 미확인)** | 접근성 트리에서는 `기본(로컬) 클라이언트` 전체가 노출된다. 시각으로는 다이어그램 아래 라벨이 대화상자 스크롤 뷰포트 밖이라 확인하지 못했다(F-4). [13](design-shots/final2-13-agent-clients-dialog-largest-midnight.png) |
| R2 탭 포커스 링 | 960 Daylight, `Shift+Tab`으로 키보드 포커스 이동 | **fixed** | 비활성 탭(`쿼리 7`)과 활성 탭(`쿼리 8`) 모두 링이 닫기 버튼까지 포함해 탭 전체를 감싼다. 주 버튼·`미리보기`·설정 버튼 링도 정상. [06](design-shots/final2-06-tab-focus-ring-960-daylight.png) |

## 2. 새 발견 사항

심각도: major(가림·잘림·내용 소실), minor(다듬기). 모두 보고 전용. major 없음.

| ID | 심각도 | 화면 | 크기 | 테마 | 스크린샷 | 문제 |
|---|---|---|---|---|---|---|
| F-1 | minor | 탭 바(탭 8개) | 960×600 | Daylight | [05](design-shots/final2-05-tabs-8-960-daylight.png) | 마지막 탭을 활성으로 두면 탭 7이 왼쪽 끝에 놓이고 오른쪽 약 380px이 비어, 탭 1~6이 모두 밀려난 것처럼 보인다. 탭 5~8을 보여 줄 수 있는데 경계 정렬 때문에 여백이 남는다. U4 보고서의 "끝 여백" 한계가 실제로 눈에 띈다. |
| F-2 | minor | 실행 이력 패널을 연 결과 화면 | 960×600 | Midnight | [11](design-shots/final2-11-history-open-graph-squeezed-960-midnight.png) | 이력 패널을 열면 그래프 캔버스가 약 120px로 줄어 점이 한 덩어리로 모이고 라벨(`Alice`, `Limit9`, `Limit2`)이 아래에서 잘린다. 이력 행의 시간이 `오후 10:27:…`로 말줄임. 패널을 열 때 결과 영역의 최소 높이가 없다. |
| F-3 | minor | AI 도우미 대화상자 | 960×600 | Midnight | 960 컷은 미첨부(관찰). 최대 크기는 [12](design-shots/final2-12-ai-dialog-primary-largest-midnight.png) | 낮은 창에서 `설정 저장` 버튼이 스크롤 영역 아래에 있고, `Anthropic API 키` 입력 상자가 경계에서 반쯤 잘린 채 끝난다. 푸터에는 안내 문구만 있어 버튼이 있다는 단서가 없다. |
| F-4 | minor | 에이전트 클라이언트 대화상자 | 1492×874 | 양쪽(Midnight 촬영) | [13](design-shots/final2-13-agent-clients-dialog-largest-midnight.png) | 호출 이력 목록(내부 스크롤)과 다이어그램이 한 스크롤 안에 있어, 최대 크기에서도 세 번째 DB 노드 라벨과 클라이언트 노드 라벨(`기본(로컬) 클라…`)이 뷰포트 아래에서 잘린다. 이력 목록의 일곱 번째 행도 중간에서 잘려 있다. |
| F-5 | minor | 사이드바 연결 목록 | 960×600 | 양쪽 | [01](design-shots/final2-01-sidebar-active-card-960-daylight.png) | 목록 높이를 제한하면서 고정된 활성 카드 위아래로 이웃 카드(`e2e-sqlite`, `e2e-pg-badport`)가 글자 조각만 비친다. 겹침으로 읽힐 수 있다. 활성 카드가 가려지지는 않는다. |

한국어 문구: 새 영어 누락은 없었다. 남은 영어는 기술 용어(Host, SQL, Cypher, MCP, 도구 이름, `String`·`int4` 같은 타입), DB 원본 오류(`PostgreSQL connection failed or was closed`), 모델 ID뿐이다.

## 3. E2E 회귀

앱 PID 24214(`c1e3020` 디버그 빌드, 격리 설정).

| 스크립트 | 기준 | 결과 |
|---|---|---|
| `bench/e2e-bridge.mjs` | 21/21 | **21/21**(2회 실행 모두) |
| `bench/preview-e2e.mjs` | 5/5 | **5/5** |
| `bench/permissions-e2e.mjs` (`AXDB_APP_PID=24214`) | 13/13 | **13/13** |
| `bench/w5-sqlite-smoke.mjs` | — | **건너뜀**: 스크립트가 `w5-sqlite` 프로필 ID를 요구하는데 `c2-config`에는 `e2e-sqlite-0001`만 있다. 돌리려면 프로필을 추가해 설정을 바꿔야 하므로 하지 않았다. SQLite는 `e2e-bridge`·`preview-e2e`에서 이미 검증된다. |

## 4. 정리

- 실행 후 `e2e.typed`는 기준 두 행(id 1 note NULL raw `00ffabcd`, id 2 `한글 텍스트`), `e2e.big` 250,000행 그대로. `profiles.json`은 시작 전 사본과 내용 동일.
- 감사용 호출(`stream.mjs` 40건 ×2, 오류 쿼리, 긴 제목 쿼리)이 `history.jsonl`·`agent-audit.jsonl`에 남았다. 권한 벤치가 `agent_clients.json` 토큰 해시를 회전시킨다(기존과 동일한 부작용).
- 테마는 System으로 되돌렸다. 앱은 종료했다. 제품 소스·`ui/dist` 미수정, 커밋 없음. 생성 파일은 이 문서와 `final2-*.png` 14장뿐이다.
