# W1 UI 구현 보고

작업 범위는 `ui/`뿐이며 git commit은 하지 않았다. README, W1 명세, ARCHITECTURE 전체와 병행 생성된 dbcore/호스트 직렬화 타입을 확인했다. 주석이나 모형 화면으로 완료를 대체하지 않고, 실제 invoke 경계와 동작하는 브라우저 mock을 구현했다.

## 구현

- strict TypeScript 계약, camelCase Tauri command 인자, snake_case 요청 본문, 브라우저용 지연 import mock.
- manifest로 생성하는 연결 폼과 비밀번호/키체인 체크, 추가·편집·삭제·테스트·연결/해제·재연결.
- 세션별 lazy explorer, 새로고침·로드된 노드 필터, 컨텍스트 메뉴/더블클릭 Browse·OpenQuery·Insert.
- 연결별 query/browse 탭, 실행 표시, 탭 닫기 취소, CodeMirror 6, 선택 실행, capability 기반 timeout/cancel/params.
- 결과셋 탭과 실제 존재하는 Graph/Table/JSON/Message 뷰만 표시, 실행 시간·행 수·affected·truncated 표시.
- 고정 28px 행 가상화, sticky 헤더, 포인터/키보드 컬럼 리사이즈, 셀 상세 패널, 100/500/1000 browse pagination.
- Int/Decimal 문자열, 시간 원문, 원문 JSON 토큰 포맷팅, 모든 Value variant 렌더링을 values.ts에 집중.
- lazy Canvas+d3-force 그래프, 라벨 색/캡션/화살표, 줌·팬·노드 드래그, 노드/관계 속성, id 중복 제거 확장, 검색 이동/하이라이트, 한도 배너/범례, 최대 300틱 종료. 키보드용 노드/관계 선택도 제공.
- 실행 이력과 editor 복원, DbError 배지/detail/position. 시스템 light/dark, native modal dialog의 포커스/ESC 처리.

## 확인한 결과

환경: macOS, Node 25.8.1, pnpm 10.33.0, Preact 11.0.1, Vite 8.3.4, TypeScript 7.0.2, Vitest 5.0.4. 정확한 의존 버전은 pnpm-lock.yaml에 고정했다.

| 확인 | 결과 |
| --- | --- |
| `pnpm --dir ui install` | 성공 |
| `pnpm --dir ui build` | 성공 |
| `pnpm --dir ui exec tsc --noEmit` | 성공 |
| `pnpm --dir ui test` | 4개 파일, 10개 테스트 통과 |
| 지정 드라이버 문자열 grep (mock.ts 제외) | 결과 없음 |
| 단위 검증 | 큰 Int/Decimal, JSON 원문 큰 수/escape 보존, Bytes/Map/시간, 그래프 id 병합, 가상 범위, params 검증, mock 10만 행·한도·취소·timeout·오류·그래프·browse, native command camelCase 경계 |

Playwright로 개발 서버와 실제 `dist` preview(`127.0.0.1:4174`)를 직접 조작했다. 모든 데이터 검증은 브라우저 mock 범위이다.

| 브라우저 흐름 | 관찰 |
| --- | --- |
| 연결 → public 펼치기 → people 액션 | 자식 lazy 로드와 액션 표시 |
| 기본 쿼리 → 표 | 24행, 첫 정수 `9007199254740993` 원문 |
| 10만 행 → 스크롤 | `aria-rowcount=100001`(헤더 포함), 중간 스크롤에서 DOM 23행 |
| 프로덕션 10만 행 마지막 구간 | scrollTop=2799724, DOM 16행, 마지막 aria-rowindex=100001 |
| 60 requestAnimationFrame 표본, wheel 8회 | p95=17.6ms, max=17.7ms; 짧은 로컬 표본이며 범용 성능 보장은 아님 |
| Graph → Table → Graph | 동일 결과의 Node 표 요약, 그래프 전환 |
| Canvas 노드 클릭 | alice / Person / name / score 속성 표시 |
| Canvas 더블클릭 확장 | 3→5노드, 2→4관계; Table 전환 후에도 병합 상태 유지 |
| graph_node_limit=1 | `3개 중 1개 표시` 배너 |
| `SELECT error;` | Query 배지, message/detail/position=8 |
| `SELECT slow;` → Cancel | Cancelled 표시와 이력 |
| 실행 중 탭 닫기 | 탭 제거와 cancelled 이력 |
| timeout=1초 → slow | Timeout 배지 |
| 선택 영역 + Cmd/Ctrl+Enter | 전체 텍스트의 error 문장 제외, 선택한 SELECT만 이력에 기록 |
| params=[] | Invalid와 Run 비활성화 |
| Browse Next / 크기500 | 첫 정수 +100 이동, 500행 및 상세 JSON 큰 수 보존 |
| 연결 폼 Test / Save / Edit / Delete | mock 테스트 성공, 이름 수정과 삭제 확인 |
| `SELECT disconnect;` → 재연결 | 재연결 버튼, 새 세션에서 다시 실행 성공 |
| 다중 결과셋 / documents | 결과 탭 2개, 큰 수가 보존된 JSON 뷰 |
| light / dark | 실제 화면 검사, dark syntax 색 대비 보정 |
| 프로덕션 console | 오류 0, 경고 0 |
| 지연 모듈 | 초기에는 entry+mock만 요청; 연결 후 SQL, 그래프 쿼리 시 Cypher/GraphView 요청 |

스크린샷과 일부 snapshot/console은 로컬 `ui/.playwright/`에 보관했으며 git에서는 제외했다. 이 폴더나 node_modules/dist가 없어도 빌드와 단위 테스트는 재현된다.

## 번들 크기

아래는 최종 `dist` 전체 파일의 실제 길이 및 Node `gzipSync` 기본 압축 결과다. 바이트 단위이며 파일시스템 할당량은 제외한다. Vite 출력의 gzip 수치와는 압축 구현 차이가 있을 수 있다.

| 파일 | 원문 bytes | gzip bytes |
| --- | ---: | ---: |
| GraphView-D6hdsLTl.js | 20771 | 8228 |
| cypher-B8znICFa.js | 637 | 471 |
| dist-BH2RD74n.js | 41231 | 14556 |
| index-BTJYCqDl.js | 335446 | 109549 |
| index-Cn6fJ190.css | 11117 | 3061 |
| mock-C35AdHze.js | 6287 | 2725 |

- dist 전체: **415917 bytes**.
- Tauri 초기 JS(entry): **109549 bytes gzip**.
- 브라우저 mock 초기 JS(entry + mock): **112274 bytes gzip**.
- HTML/CSS는 초기 JS 합계에 포함하지 않았다. SQL 언어 chunk와 GraphView(d3-force 포함)는 초기 진입 요청에서 제외됨을 프로덕션 네트워크에서도 확인했다.

## 결정과 한계

- 별도 UI/아이콘/가상화 라이브러리, AI 기능, 장식 애니메이션을 추가하지 않았다. 의존성은 지정 런타임 패키지와 빌드/테스트용 TypeScript/Vite/Vitest/타입 패키지뿐이다.
- 새 호스트 실제 타입에 맞춰 `HistoryEntry.timestamp`는 epoch milliseconds, params는 `serde_json::Value`에 대응하는 일반 JSON 객체로 정했다. 결과의 태그 Value와 params는 구분한다.
- `session_open`은 현재 호스트의 optional secret 인자를 지원한다. 저장되지 않은 자격증명은 HTML `<dialog>` password 폼에서 세션용으로 입력하며 프로필 필드/브라우저 저장소에는 쓰지 않는다. 저장은 활성 세션을 자동으로 바꾸지 않는다.
- 호스트 ConnectConfig에 secret 슬롯이 하나여서 현재 드라이버의 단일 secret 필드를 지원한다. 여러 독립 secret이 필요한 새 manifest는 호스트 계약 확장이 먼저 필요하다.
- 안전하지 않은 정수 파라미터는 거절한다. 임의 정밀도 decimal 파라미터는 문자열로 넣고 쿼리에서 캐스팅해야 한다. 결과 JSON은 parse/stringify를 전혀 거치지 않는 포매터로 큰 숫자와 문자열 escape를 보존한다.
- graph_node_limit은 기본 300, UI 설정 범위 1–5000이다. 기존 결과에도 표시 제한은 즉시 반영하고, 원래 실행/확장 API에서 잘려 반환되지 않은 데이터는 새 실행이나 추가 확장이 필요하다.
- mock은 fixture command(`SELECT large/error/slow/disconnect/documents`)와 기본 SQL/그래프 결과를 제공하는 개발 도구이며 SQL/Cypher 엔진이나 키체인을 흉내 내어 보안 보장을 주장하지 않는다. mock elapsed_ms는 예시 수치다.
- 10만 행 데이터 자체는 탭 메모리에 남는다. 가상화는 DOM 수를 제한하며, 여러 큰 결과 탭의 메모리를 자동으로 내보내는 기능은 범위에 넣지 않았다.
- native Tauri WebView, 실제 DB/서버 취소, OS 키체인 저장/재시작 복구, 패키징 및 장시간 메모리/하드웨어 성능 측정은 이번 UI 검증에서 실행하지 않았다. 코디네이터의 백엔드·통합 검증이 남아 있다.
- 구현 상태: 완료 / UI 단위·브라우저 mock 검증: 통과 / commit·push·배포: 수행하지 않음.

공식 참고: [CodeMirror API](https://codemirror.net/docs/ref/), [Tauri command 인자](https://v2.tauri.app/develop/calling-rust/), [d3-force](https://d3js.org/d3-force/simulation), [Preact 11 px 단위 변경](https://preactjs.com/guide/v11/upgrade-guide/).

로컬 확인용 dev 서버(127.0.0.1:5173)와 dist preview(127.0.0.1:4174)는 코디네이터가 바로 확인할 수 있도록 실행 상태로 두었다. 다른 포트의 기존 서버는 건드리지 않았다.
