# 최종 UI 품질 재감사 3 (네이티브)

날짜 2026-10-17 · 대상 커밋 `24ef30e`(`ui/dist` 빌드 포함) · 실제 Tauri 디버그 앱(`CARGO_TARGET_DIR=target-c2e2e`로 재빌드, 격리 설정 `c2-config`, `AXDB_AGENT_BRIDGE=1`) · 테스트 DB만 사용(PG `axdb-test-pg`, Neo4j `axdb-test-neo4j`, 임시 SQLite). 제품 소스 수정 없음, 커밋 없음. 스크린샷은 `docs/design-shots/final3-*.png` 19장(`sips -Z 1200`). 기준: `FINAL_UI_AUDIT_2.md`, `U5_REPORT.md`.

## 한 줄 결론

**major(가림·잘림·내용 소실)는 이번에도 보지 못했다. F-1~F-5는 모두 의도한 시나리오에서 고쳐졌고 회귀는 없다.** 다만 새로 확인한 minor 6건(N3-1~N3-6)이 있어 "겹침·잘림이 전혀 없다"고는 말할 수 없다. 또 모든 화면×크기×테마 교차를 전수로 돌리지는 않았고(아래 "범위와 한계"), 최대 크기(1492×874)와 Redis·MongoDB 화면은 이번에 보지 않았다. 확인한 범위 안에서는 깨끗하고, E2E는 기준 건수를 모두 충족했다(21/21, 5/5, 13/13).

## 범위와 한계

| 항목 | 내용 |
|---|---|
| 크기 | 960×600, 1280×800. `System Events`로 창 크기를 맞췄다. 1492×874는 이번에 보지 않았다. |
| 테마 | Daylight·Midnight. 설정 팝오버에서 바꿨고 종료 전에 `시스템`으로 되돌렸다. |
| 입력 | 접근성 클릭·`set-value`·`type-text`·키보드. 클립보드 미사용. 이전과 같이 합성 스크롤·`PageDown`이 대화상자 스크롤 뷰포트에 먹지 않아, 960 에이전트 대화상자에서 스크롤 아래 내용은 시각으로 못 봤다. |
| 연결 수 | 이전 감사(4개)와 달리 임시 SQLite 프로필 `final3-sqlite`를 추가해 5개로 봤다. 이 차이로 N3-3·N3-4가 드러났다. |
| 못 한 것 | 1492×874, 탭 8개 Midnight, 연결 대화상자·팔레트의 이번 교차, Redis·MongoDB, 스크롤 아래 영역 시각 확인. |

## 1. F-1~F-5 재확인 (U5 보고서 대비)

스크린샷 접두사 `design-shots/final3-`.

| ID | 확인 조건 | 상태 | 근거 |
|---|---|---|---|
| F-1 탭 끝 여백 | 탭 8개, 960×600 Daylight, 마지막 탭 활성 | **fixed** | 오른쪽 빈칸 없음, 마지막 탭이 끝까지 붙음, 왼쪽에 잘린 탭 조각 없음(탭 5는 페이드만). 탭 3개가 정수 폭으로 보임. [06](design-shots/final3-06-tabs-8-960-daylight.png) · 탭 8개 Midnight는 미확인 |
| F-2 이력 패널 | Neo4j 25노드 그래프, 이력 패널 열기, 960×600 Daylight·Midnight | **fixed(원래 증상) + 새 minor** | 캔버스가 약 156px로 유지되고 `Alice`·`Limit9`·`Limit2` 등 라벨이 잘리지 않으며 이력 시간은 `오후 11:03` 형식. 그러나 같은 크기에서 패널에는 머리줄(`이력 100`)만 보이고 행이 0개다(N3-1). [04](design-shots/final3-04-neo-history-open-960-daylight.png), [28](design-shots/final3-28-midnight-neo-history-960.png). 1280 Midnight에서는 행 4개가 정상 표시 |
| F-3 AI 대화상자 | 960×600 Daylight·Midnight | **fixed** | `설정 저장`이 푸터에 고정되어 항상 보이고 brand 채움 버튼. 본문 마지막 입력(`API 키 입력`)은 페이드로 스크롤 가장자리임이 보인다. [07](design-shots/final3-07-ai-dialog-960-daylight.png) |
| F-4 에이전트 클라이언트 | 1280×800 Daylight, 960×600 Daylight·Midnight (1492는 미확인) | **partly** | 1280: 연결 DB 5개 노드와 클라이언트 2개가 한 화면에 모두 들어온다(이전 최대 크기에서도 잘렸던 것이 해결). [11](design-shots/final3-11-agent-clients-1280-daylight.png). 960: 5개 노드 중 맨 위(`e2e-neo4j`) 아이콘이 위에서, 5번째(`final3-sqlite`)가 아래에서 잘린다(N3-3, U5가 밝힌 "스크롤 필요" 한계와 연결 5개가 겹친 경우). [08](design-shots/final3-08-agent-clients-960-daylight.png), [30](design-shots/final3-30-midnight-agent-960.png). 클라이언트 이름은 전체가 보이나 줄바꿈이 어색하다(N3-2) |
| F-5 사이드바 | 960×600 Daylight·Midnight, 활성 카드 `e2e-pg` | **fixed** | 목록 높이가 카드 2장 단위이고 이웃 카드 글자 조각이 비치지 않는다. 활성 카드가 가려지지 않는다. [01](design-shots/final3-01-sidebar-active-960.png), Midnight는 [28](design-shots/final3-28-midnight-neo-history-960.png) 왼쪽. 단 1280×800에서 연결 5개일 때 마지막 카드 아래 테두리가 잘린다(N3-4) |

회귀: F-1~F-5 수정 때문에 새로 생긴 겹침·가림은 보지 못했다. N3-1·N3-3·N3-4는 U5가 정한 높이·스냅 규칙의 경계 조건이다.

## 2. 처음으로 네이티브에서 본 화면

### SQLite 연결·탐색·쿼리·ERD (`final3-sqlite`, 임시 DB)
- **연결·탐색: 정상.** 연결 직후 상태바 `final3-sqlite 3.53.2`, 트리 `테이블`(items·orders·users)·`뷰` 한국어 폴더명. 사이드바는 1280에서 5개 카드가 모두 보인다.
- **쿼리: 정상.** `SELECT sqlite_version()`, 그리고 `users`×`orders` 조인(앨리스 2·30.5, 밥 1·7.25, Carol 0·`NULL`). 한글·타입 배지(TEXT/INTEGER/REAL)·`NULL` 칩이 겹침 없이 표시된다. [17](design-shots/final3-17-sqlite-query-result-1280-daylight.png)
- **데이터 보기: 정상.** `orders` 브라우즈 3행, FK 열(`user_id`)에 이동 화살표, `편집` 버튼, 페이지 크기 `100`. [19](design-shots/final3-19-sqlite-browse-orders-1280-daylight.png)
- **ERD: 렌더 정상, 다듬기 1건.** `다이어그램 보기`로 노드 4개·관계 2개(`FK orders → users (user_id → id)`), PK 표시·컬럼 타입·범례. 1280 Daylight [20](design-shots/final3-20-sqlite-erd-1280-daylight.png), 960 Midnight(이력 패널 열림) [31](design-shots/final3-31-midnight-erd-960.png). `items → orders` 간선이 `v_totals` 노드 뒤로 지나가 `items → v_totals → orders`처럼 읽힌다(N3-5).

### 에이전트 클라이언트 권한 편집 (GlideSelect)
- `권한 편집`을 열면 이름·최대 반환 행·만료·연결별 권한 표·허용 도구가 나오고 푸터에 `취소`·`저장`이 고정된다. [12](design-shots/final3-12-permission-edit-1280-daylight.png)
- `e2e-sqlite`의 GlideSelect를 열면 `접근 없음 / 읽기 / 읽기 + 미리보기 / 쓰기 (앱 승인)` 4개가 선택 표시(체크)와 함께 팝오버로 뜨고 표 안에서 잘리지 않는다. [13](design-shots/final3-13-glideselect-open-1280-daylight.png)
- `읽기`로 바꾸고 `저장` → `agent_clients.json`에 `{"e2e-sqlite-0001":"read"}` 기록, 다시 `권한 편집`을 열면 값 `읽기` 유지. `쓰기 (앱 승인)`으로 되돌려 저장했고(`write_with_approval`), 종료 후 시작 시 백업(`connections: {}`)으로 파일을 원복해 `diff -r` 일치를 확인했다.
- 960×600 Midnight에서도 편집 폼·푸터가 겹침 없이 보였다(스크린샷 없음, 관찰).

### e2e.typed 편집 모드·변경 미리보기·적용 확인 (PG, 적용 안 함)
- 편집 모드: `편집 종료`·`행 추가`·`N개 변경 대기` 칩, PK `id` 잠금. `note`를 `Final3 preview`로 수정하고 행 2에 삭제 표시 → `2개 변경 대기`, 수정 행은 강조·말줄임(`Final3 prev…`), 삭제 행은 취소선·붉은 배경. [22](design-shots/final3-22-edit-pending-1280-daylight.png)
- `변경 미리보기`: `미리보기 · 커밋하지 않았습니다`, `수정 · 영향 1행`, UPDATE 문, `변경 전(샘플)`·`변경 후(샘플)`(NULL → `Final3 preview`), `표로 돌아가기`·`적용`. [23](design-shots/final3-23-change-preview-1280-daylight.png)
- `적용` 확인 대화상자: `변경 적용 · 2개 변경을 데이터베이스에 적용합니다. 미리보기에서 확인한 내용과 같은 변경만 적용됩니다.` `취소`/`적용`. [24](design-shots/final3-24-apply-confirm-1280-daylight.png) → **`취소`만 눌렀다.** psql로 `e2e.typed`가 시작 시 내용과 동일함을 확인(`DB_UNCHANGED`).
- 대기 중 편집이 있는 탭의 닫기(×)는 확인 없이 바로 닫히고 편집이 사라진다(N3-6).

### 쿼리 변경 미리보기
- `UPDATE e2e.typed SET note = 'q-preview' WHERE id = 1` → `미리보기`: `미리보기 · 커밋하지 않았습니다`, 롤백 안내 문장(시퀀스·트리거), `수정 · 영향 1행`, 변경 전(샘플) `NULL`. Daylight [26](design-shots/final3-26-query-preview-1280-daylight.png), Midnight [27](design-shots/final3-27-midnight-query-preview-1280.png), 960 Midnight(이력 패널 열림)도 겹침 없이 스크롤 영역으로 처리됨. DB는 변경되지 않았다. `변경 전(샘플)` 제목이 위 SQL 상자에 거의 붙어 보인다(여백, N3-7 참고용).

### Neo4j 960×600 + 이력 패널
- Daylight [04](design-shots/final3-04-neo-history-open-960-daylight.png), Midnight [28](design-shots/final3-28-midnight-neo-history-960.png): 그래프 라벨 충돌 없음, 범례·`노드와 관계 · 키보드 탐색`·시간/행 칩이 모두 보이고 이력 머리줄이 하단에 고정. 단 이력 행이 보이지 않는다(N3-1).

## 3. 회귀 스윕 (겹침·가림·잘림)

| 화면 | 960 Daylight | 960 Midnight | 1280 Daylight | 1280 Midnight |
|---|---|---|---|---|
| 사이드바 연결 목록 | OK(F-5) | OK | N3-4 | N3-4 |
| 쿼리·결과(PG/SQLite) | — | OK | OK | OK |
| 그래프 + 이력 | OK/N3-1 | OK/N3-1 | — | OK |
| 탭 바 | OK(F-1) | 탭 적어 정상 | OK | OK |
| AI 대화상자 | OK(F-3) | OK | — | — |
| 에이전트 대화상자 | N3-3 | N3-3 | OK | — |
| 권한 편집 | — | OK | OK | — |
| 편집 모드·미리보기·적용 | — | — | OK | — |
| ERD | — | N3-5 | N3-5 | — |
| 설정 팝오버 | — | OK | — | OK(사이드바 하단을 덮는 것은 설계) |

`—`는 이번에 보지 않음. 위 외 새로운 겹침 범주는 없다.

## 4. 새 발견 사항

심각도: major(가림·잘림·내용 소실), minor(다듬기). 모두 보고 전용. major 없음.

| ID | 심각도 | 화면 | 크기 | 테마 | 스크린샷 | 문제 |
|---|---|---|---|---|---|---|
| N3-1 | minor | 그래프 결과 + 실행 이력 패널 | 960×600 | 양쪽 | [04](design-shots/final3-04-neo-history-open-960-daylight.png), [28](design-shots/final3-28-midnight-neo-history-960.png) | 패널이 머리줄(`이력 100`, 새로고침·접기)만 보이고 행이 0개다. U5 보고서는 "약 1행"이라 했다. 접근성 트리에는 행 100개가 있으나 화면에 닿는 높이가 없어 캔버스 우선의 대가로 이력을 읽을 수 없다. 1280에서는 정상. |
| N3-2 | minor | 에이전트 클라이언트 다이어그램 | 960·1280 | 양쪽 | [11](design-shots/final3-11-agent-clients-1280-daylight.png), [08](design-shots/final3-08-agent-clients-960-daylight.png) | 클라이언트 노드 라벨이 `기본(로컬) 클라이언` / `트` 두 줄로 갈려 마지막 글자 하나가 고아가 된다. 전체 이름은 보이므로 #19의 잘림은 해소됐으나 줄바꿈이 부자연스럽다. |
| N3-3 | minor | 에이전트 클라이언트 다이어그램(DB 5개) | 960×600 | 양쪽 | [08](design-shots/final3-08-agent-clients-960-daylight.png), [30](design-shots/final3-30-midnight-agent-960.png) | 첫 화면에서 맨 위 DB 노드(`e2e-neo4j`)의 아이콘이 위에서 절반 잘리고 5번째 노드(`final3-sqlite`)는 보이지 않는다. 스크롤로 닿을 수는 있어 보이나(`권한 편집` 클릭 시 아래로 자동 스크롤되며 5번째 노드가 보임) 이 하네스에서는 직접 스크롤하지 못해 확정하지 못했다. |
| N3-4 | minor | 사이드바 연결 목록(연결 5개, 활성 카드 있음) | 1280×800 | 양쪽 | [27](design-shots/final3-27-midnight-query-preview-1280.png) | 마지막 카드(`final3-sqlite`) 아래 테두리가 약 8px 잘려 카드가 덜 그려진 것처럼 보인다. U5의 카드 단위 높이는 낮은 창에만 적용되고 1280에서는 연결이 5개를 넘기면 정수 배가 아니다. |
| N3-5 | minor | ERD | 960·1280 | 양쪽 | [20](design-shots/final3-20-sqlite-erd-1280-daylight.png), [31](design-shots/final3-31-midnight-erd-960.png) | `items → orders` 간선이 `v_totals` 노드 뒤로 지나가 서로 이어진 것처럼 읽히고, 그 간선의 FK 라벨은 가려져 보이지 않는다. 960에서는 노드 안 글자가 약 7px로 작다. |
| N3-6 | minor(정책 확인) | 테이블 편집 모드 탭 | 1280×800 | Daylight | 스크린샷 없음(관찰) | 대기 중인 편집 2건이 있는 `typed` 탭의 ×를 누르면 확인 없이 즉시 닫히고 편집이 사라진다. 적용 전 변경이라 데이터는 안전하지만 실수로 닫으면 되돌릴 수 없다. |
| N3-7 | 참고 | 변경 미리보기·실행 이력 | 1280 | 양쪽 | [26](design-shots/final3-26-query-preview-1280-daylight.png) | (a) `변경 전(샘플)` 제목이 위 SQL 상자에 거의 붙는다. (b) 롤백된 미리보기 실행이 이력에 일반 실행과 같은 `1행 · 3 ms`로 남아, 미리보기였다는 표시가 없다(이력 스크린샷 없음, 관찰). |

한국어 문구: 새 영어 누락은 없었다. 남은 영어는 기술 용어(SQL, Cypher, MCP, `Edit rows` 같은 접근성 이름, 타입 문자열)와 DB 원본 값뿐이다.

## 5. E2E 회귀

앱 PID 3322(`24ef30e` 디버그 빌드, 격리 설정 `c2-config`, 화면 조작 후 실행).

| 스크립트 | 기준 | 결과 |
|---|---|---|
| `bench/e2e-bridge.mjs` | 21/21 | **21/21** (2회 실행, `"ok": false` 0건) |
| `bench/preview-e2e.mjs` | 5/5 | **5/5** |
| `bench/permissions-e2e.mjs` (`AXDB_APP_PID=3322`) | 13/13 | **13/13** |

## 6. 복원

- PG `e2e.typed`: 시작 전 스냅샷과 동일(적용 확인은 취소, 쿼리 미리보기는 롤백), 종료 후 재확인 `PG_OK`.
- 격리 설정 `c2-config`: 시작 전 복사본으로 `rsync --delete` 복원 후 `diff -r` 일치. 임시 프로필 `final3-sqlite`는 `profiles.json`에서 제거됨(`grep` 0건), 임시 SQLite 파일·`sqlite/` 디렉터리 삭제.
- 앱 테마 `시스템`으로 복원 후 앱 종료(`target-c2e2e/debug/axdb` 프로세스 없음).
- 제품 소스는 수정하지 않았고, 이번에 추가한 파일은 이 문서와 `docs/design-shots/final3-*.png` 19장뿐이다.

## 7. 사용자 질문에 대한 답

"UI가 정확하고 깨끗한가?"에는 **확인한 범위에서 major 없이 깨끗하다**가 답이다. F-1~F-5는 모두 고쳐졌고 회귀는 없으며, SQLite·권한 편집·편집 모드·변경 미리보기·적용 확인·쿼리 미리보기는 처음으로 네이티브에서 정상 동작을 확인했다. 다만 새 minor 6건(N3-1~N3-6)과 참고 1건이 남아 있고, 1492×874와 Redis·MongoDB, 일부 크기×테마 교차는 보지 못했으므로 "전체가 완전하다"는 확정 답은 아니다.
