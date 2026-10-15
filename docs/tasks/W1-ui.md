# W1-ui: 경량 프런트엔드 (Preact + CodeMirror 6 + 가상 테이블 + 캔버스 그래프)

이 작업은 반드시 끝까지 완료한다. 합리적인 가정으로 계속 진행하고, 결정한 내용은 마지막 보고에 적는다. 사용자 지시가 skill 지침보다 우선한다.

## Target
저장소는 `/Users/iron/Project/psw/db-tool`이고 이 작업의 범위는 `ui/` 디렉터리만이다. 계약 명세는 `docs/ARCHITECTURE.md`이며 **먼저 전부 읽는다.** 백엔드(Tauri 호스트, Rust)는 다른 워커가 동시에 만들고 있다.

## Change
Vite + Preact + TypeScript(strict) 앱을 `ui/`에 만든다. 패키지 매니저는 pnpm이다.

1. `ui/src/contract.ts`: ARCHITECTURE.md의 직렬화 형태를 그대로 TS 타입으로 옮긴다.
   - 필드는 snake_case로 둔다.
   - `Value`는 `{t:'Int', v:'123'}`처럼 판별 유니온으로 정의한다.
   - `Value::Null`의 직렬화는 `{t:'Null'}`이다.
   - `Map`과 `props`는 `[string, Value][]` 형태다.
2. `ui/src/api.ts`: `@tauri-apps/api/core`의 `invoke`를 감싼다. command 이름과 인자는 ARCHITECTURE.md의 호스트 명령 표를 따른다. 인자는 camelCase(Tauri 기본)로 넘긴다. 예: `sessionId`, `runId`, `nodeId`.
   - `window.__TAURI_INTERNALS__`가 없으면, 즉 브라우저 단독 개발 환경이면 `ui/src/mock.ts`의 목 백엔드를 쓴다.
   - 목 백엔드에는 테스트 데이터를 넣는다:
     - PG 비슷한 relational 드라이버 1개
     - graph 드라이버 1개
     - 큰 결과(10만 행)
     - 그래프 결과
     - 오류
     - 지연 실행(취소 확인용)
3. **화면** (장식은 최소화하고 정보 밀도는 높게, 다크/라이트 시스템 테마):
   - **왼쪽: 연결 목록.**
     - 추가, 편집, 삭제, 연결 테스트, 연결/해제를 할 수 있다.
     - 연결 폼은 `manifest.connection_fields`로 **자동 생성**한다. secret 필드는 password input으로 만들고 "키체인에 저장" 체크를 둔다.
     - 연결된 항목 아래에는 탐색 트리를 둔다. 펼칠 때 `explorer_children`를 호출하고, 새로고침 버튼과 검색 필터(로드된 노드 대상)를 제공한다.
     - 노드의 `actions`는 컨텍스트 메뉴와 더블클릭으로 실행한다.
       - Browse는 페이지 조회 탭을 연다.
       - OpenQuery는 새 쿼리 탭에 텍스트를 넣는다.
       - Insert는 현재 에디터 커서 위치에 텍스트를 넣는다.
   - **가운데: 탭.**
     - 쿼리 탭(연결별)과 browse 탭이 있다. 탭 헤더에 연결 이름과 실행 중 표시를 보여 준다.
     - 실행 중인 탭을 닫으면 `cancel(runId)`를 호출한다.
   - **쿼리 탭:**
     - CodeMirror 6 에디터를 쓴다. 언어는 `manifest.query_language.editor_mode`에 따라 고른다: `"sql"`이면 `@codemirror/lang-sql`, 그 외에는 plain이다. Cypher는 간단한 StreamLanguage 키워드 하이라이트를 직접 만든다. 이것도 editor_mode 값으로 고르며 **드라이버 id로 분기하지 않는다.**
     - 실행: Cmd/Ctrl+Enter. 선택 영역이 있으면 선택 영역만 실행한다.
     - 취소 버튼은 `capabilities.cancel !== 'None'`일 때만 보인다.
     - 툴바에 timeout(초)과 row limit 입력을 둔다. timeout 입력은 `capabilities.timeout`일 때만 보인다.
     - `capabilities.params`가 true일 때만 파라미터 패널(JSON 객체 편집, 유효성 표시)을 보여 준다.
   - **결과 영역:**
     - 여러 `ResultSet`이 오면 결과 탭으로 나눠 보여 준다.
     - 각 ResultSet 안에서는 **들어 있는 뷰만** 전환 버튼으로 보여 준다: Graph / Table / JSON(documents) / Message.
     - 상태줄에 elapsed_ms, row_count, truncated 경고("한도 N에서 잘림"), affected를 표시한다.
   - **가상 테이블** (직접 구현, 라이브러리 없이):
     - 고정 행 높이, 보이는 행만 DOM에 둔다. 10만 행에서도 스크롤이 부드러워야 한다.
     - 헤더 고정, 컬럼 너비 조절, 컬럼명 아래에 type_name을 작게 표시한다.
     - 셀을 선택하면 하단 상세 패널에 전체 값을 보여 준다(JSON은 들여쓰기 적용).
   - **값 렌더러** (`ui/src/values.ts` 한 곳에서 처리):
     - Null은 회색 이탤릭 `NULL`
     - Int와 Decimal은 우측 정렬, 문자열 그대로(정밀도 유지)
     - Date/DateTime/DateTimeTz는 서버 원문 그대로
     - Json은 한 줄 축약, 상세는 들여쓰기. 큰 수를 보존하기 위해 JSON.parse/stringify를 거치지 말고 원문을 쓰는 들여쓰기 포매터를 직접 구현한다.
     - Bytes는 `<N bytes> 0x..`
     - Node는 `(:Label {k: v})`, Rel은 `[:TYPE]`, Path는 `(a)-[:R]->(b)` 요약
     - Point, Other(type_name 툴팁), List, Map
   - **그래프 뷰** (Canvas 2D + `d3-force`):
     - 노드는 라벨별 색 원, 캡션은 `name`/`title`/첫 번째 문자열 속성 순으로 고른다. 관계는 화살표와 타입 텍스트로 그린다.
     - 휠 확대·축소, 드래그로 이동, 노드 드래그.
     - 노드나 관계를 클릭하면 오른쪽 속성 패널에 id, labels/type, props를 보여 준다.
     - 노드를 더블클릭하면 `capabilities.graph_expand`일 때 `expand(sessionId, elementId, limit)`를 호출하고, 받은 그래프를 현재 그래프에 **병합**한다. 노드와 관계는 id로 중복을 제거한다.
     - 검색창에 입력하면 라벨이나 속성값이 일치하는 노드를 하이라이트하고 그 노드로 이동한다.
     - 표시 한도: 노드가 `graph_node_limit`(UI 설정, 기본 300)을 넘으면 넘는 만큼은 그리지 않고 "N개 중 M개 표시" 배너를 띄운다. 레이아웃 시뮬레이션은 틱 수를 제한(예: 300틱)한 뒤 정지한다.
     - 그래프 뷰 범례에 라벨별 개수를 보여 준다.
   - **browse 탭:** 가상 테이블과 페이지 이동(이전/다음, 페이지 크기 100/500/1000, `has_more`에 따라 활성화)을 둔다.
   - **하단 패널:** 실행 이력. `history_list`를 사용하며 클릭하면 에디터에 넣는다.
   - **오류 표시:** `DbError`의 kind별 배지(Connection/Auth/Query/Timeout/Cancelled)와 message, detail, position을 보여 준다. Connection 오류가 나면 해당 연결에 "재연결" 버튼을 띄운다.
4. 번들 크기 정책:
   - CodeMirror 언어 팩과 d3-force는 dynamic import로 분리한다. 그래프 뷰를 처음 열 때 로드한다.
   - 의존성은 preact, @preact/signals(선택), @codemirror/*, d3-force, @tauri-apps/api만 쓴다.
   - Monaco, React, UI 프레임워크, 아이콘 패키지는 쓰지 않는다. 아이콘은 인라인 SVG 몇 개로 충분하다.

## Constraints
- **UI 코드 어디에도 드라이버 id 문자열(`'postgres'`, `'neo4j'`, `'sqlite'` 등) 비교를 쓰지 않는다.** 모든 분기는 manifest의 capabilities, query_language, connection_fields와 결과에 들어 있는 뷰로 한다. 목 데이터 파일(`mock.ts`)은 예외다.
- `ui/` 밖의 파일은 수정하지 않는다.
- git commit을 하지 않는다.
- AI 기능과 장식용 애니메이션은 넣지 않는다.

## Observable acceptance
- `pnpm --dir ui install && pnpm --dir ui build` 성공, `pnpm --dir ui exec tsc --noEmit` 성공
- `grep -rnE "'(postgres|neo4j|sqlite|mysql|mongodb|redis)'" ui/src --include=*.ts --include=*.tsx | grep -v mock.ts`의 결과가 비어 있어야 한다
- `ui/src/values.test.ts` 같은 vitest 단위 테스트가 통과해야 한다. 대상: 값 렌더러(큰 정수, Decimal, Json 원문 들여쓰기에서 큰 수 보존), 그래프 병합 중복 제거, 가상 테이블 범위 계산
- 브라우저(`pnpm --dir ui dev`, mock 백엔드)에서 다음 흐름이 동작해야 한다. 가능하면 Playwright나 브라우저로 직접 확인한다:
  - 연결 → 트리 펼치기
  - 쿼리 실행 → 표
  - 10만 행 스크롤
  - 그래프 결과 → 그래프/표 전환
  - 노드 클릭 → 속성
  - 더블클릭 → 확장
  - 오류 표시
  - 지연 실행 취소
- 보고에 다음을 담는다: 빌드 산출물 크기(`ui/dist` 전체와 초기 로드 JS의 gzip 크기), 결정한 내용, 남은 한계
