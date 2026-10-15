# W4-viewer-perf: 결과 뷰어 고속화 + 성능 계측·벤치마크

이 작업은 반드시 끝까지 완료한다. 합리적인 가정으로 계속 진행하고, 결정한 내용은 마지막 보고에 적는다. 사용자 지시가 skill 지침보다 우선한다.

사용자 요구: "Viewer 속도가 빨라야 한다. 다양한 개념을 도입하고, 전환·표시·생성·추가 등의 속도를 다 테스트하라."
**측정하지 않은 성능을 빠르다고 쓰지 않는다.** 모든 주장은 수치로 뒷받침한다.

## Target
- 저장소: `/Users/iron/Project/psw/db-tool`
- 먼저 읽을 것: `docs/ARCHITECTURE.md`, `ui/src/`(특히 `VirtualTable.tsx`, `virtual.ts`, `GraphView.tsx`, `graph.ts`, `Results.tsx`, `QueryTab.tsx`, `App.tsx`), `src-tauri/src/lib.rs`의 execute와 browse
- 동시 작업 워커:
  - W3-agent: `ui/src/agent/`, 승인 모달, 상태줄, MCP 브리지, history source
  - W2-neo4j: `crates/driver-neo4j`
- **파일 소유권:**
  - 이 워커 소유: `ui/src/VirtualTable.tsx`, `virtual.ts`, `GraphView.tsx`, `graph.ts`, `Results.tsx`, 새 `ui/src/perf/`, `ui/src/workers/`, 그리고 `src-tauri`의 결과 전송 경로
  - `App.tsx`, `QueryTab.tsx`, `lib.rs`는 W3도 수정한다. 작은 diff로 고치고, 고치기 직전에 파일을 다시 읽어 남의 변경을 덮어쓰지 않는다.

## Change — 적용할 기법
각 기법을 적용하기 **전후**로 측정한다. 효과가 없거나 오히려 나빠진 기법은 되돌리고 보고서에 기록한다.

**A. 전송(IPC)**
1. 큰 결과는 Tauri `ipc::Channel`로 **청크 스트리밍**한다(예: 첫 청크 200행, 이후 2,000행 단위). UI는 첫 청크를 받는 즉시 표를 그린다.
   - 계약은 가산적으로 유지한다. 기존 `execute`는 그대로 두고 `execute_stream` 같은 command를 새로 추가하거나 옵션으로 둔다.
   - 드라이버가 아직 전체를 메모리에 모은 뒤 반환하는 구조라면 호스트에서 청크로 나눠 보내는 것으로 시작한다. 드라이버 스트리밍이 필요하다고 판단되면 dbcore에 가산적인 `execute_stream`/sink trait 메서드를 추가하고 기본 구현은 기존 execute로 둔다. PG에는 FETCH 단위 스트리밍을 구현한다.
2. 행 데이터는 **컬럼형 또는 압축 표현**으로 보낸다. 예: 컬럼별 태그 한 번 + 값 배열로 `{t,v}` 반복 오버헤드를 줄인다. JSON 크기와 파싱 시간을 전후로 측정한다.

**B. 표**
3. 행과 열을 **모두 가상화**한다(넓은 테이블 200컬럼). 스크롤은 transform 기반으로 하고 `contain: strict`, `will-change`를 쓴다. 가능한 범위에서 DOM 노드 풀을 재사용한다.
4. 셀 포맷 결과는 **보이는 범위만 계산하고 캐시**한다. 리렌더는 rAF로 묶는다.
5. 정렬과 필터(클라이언트, 로드된 행 대상)는 Web Worker에서 하거나 인덱스 배열로 처리한다. 원본은 복사하지 않는다.

**C. 그래프**
6. **d3-force 레이아웃을 Web Worker로 옮긴다.** 위치는 Float32Array로 transfer한다. 메인 스레드는 렌더만 한다.
7. **쿼드트리로 클릭 대상을 찾는다**(호버·클릭 O(log n)). 화면 밖 요소는 그리지 않는다. **LOD**: 줌이 낮으면 캡션과 화살표를 생략한다. 같은 색은 한 번의 path로 일괄 그린다.
8. **증분 확장:** expand나 추가로 새 노드가 생기면 부모 근처에 배치한다. 기존 노드는 고정에 가깝게 두고(alpha를 낮게 재가열) 전체 재배치를 하지 않는다.
9. **표시 한도는 유지한다**(기본 300, 설정 가능). 측정은 100 / 300 / 1,000 / 5,000 노드로 한다. 5,000은 한도를 올려서 측정한다.
10. 필요하면 대규모에서 Canvas 2D 대신 WebGL을 쓴다. 기본값은 측정 결과로 정한다.

**D. 전환**
11. 결과 뷰(Table/Graph/JSON)는 처음 열 때 마운트하고 이후 **keep-alive**한다. 숨길 때는 `display:none` 또는 `content-visibility:hidden`을 쓴다. 그래프 레이아웃 위치는 캐시해 다시 열 때 재계산하지 않는다.
12. 탭 전환도 keep-alive한다. 비활성 탭의 그래프 애니메이션 루프는 정지한다.

## Change — 계측과 벤치마크
13. `ui/src/perf/`:
    - `performance.mark`/`measure` 래퍼를 만들고 `window.dbtool.perf = { marks(), clear(), run(scenario) }`로 노출한다.
    - W3 레지스트리가 있으면 `get_perf_metrics` 에이전트 도구로도 노출한다. 이 경우 W3와 협의해 `ui/src/agent/registry.ts`에 한 항목만 추가한다.
    - 측정할 지점: 실행 클릭 → 첫 행 페인트, 마지막 청크 도착, 뷰 전환 요청 → 다음 프레임 페인트(double rAF), 탭 전환, 그래프 첫 프레임, 레이아웃 안정화, expand 요청 → 병합 후 첫 프레임
    - 스크롤과 팬·줌 중 프레임 시간 p50, p95, max를 rAF 샘플링으로 구한다.
14. 벤치 시나리오:
    - 목 백엔드: 브라우저에서 결정적 데이터로 실행한다. Playwright + Chromium, headless.
    - 실제 앱: 테스트 PG와 테스트 Neo4j 대상.
    - **표:** 1k / 10k / 100k행 × 10컬럼, 그리고 10k행 × 200컬럼에 대해 측정한다:
      - 첫 행 표시
      - 전체 로드
      - 끝까지 프로그램 스크롤할 때의 프레임 시간
      - JS heap(가능한 환경에서)
    - **그래프:** 100 / 300 / 1,000 / 5,000 노드(관계 약 2배)에 대해 측정한다:
      - 첫 프레임
      - 레이아웃 안정화
      - 팬·줌 프레임 시간
      - 노드 클릭 → 속성 패널 표시
      - expand(+50 노드) 병합 시간
    - **전환:**
      - Table ↔ Graph 전환 10회의 중앙값(첫 진입과 재진입을 구분)
      - 탭 전환
      - 결과 집합 탭 전환
      - 새 쿼리 탭 생성
      - 연결 추가 후 트리 표시
    - 스크립트: `bench/viewer-bench.mjs`(Playwright, mock dev 서버). 결과는 `bench/results-viewer-*.json`과 사람이 읽을 요약 `bench/VIEWER_PERF.md`에 남긴다.
    - 실제 앱 측정 방법은 둘 중 하나다:
      - W3의 MCP 브리지가 준비되면 `run_query`/`get_perf_metrics`로 구동한다(`bench/viewer-bench-app.mjs`).
      - 준비되지 않았으면 `DBTOOL_BENCH=1`로 앱이 시작 시 시나리오를 자동 실행하고 결과를 `{config_dir}/perf.json`에 쓰게 한다.
    - 실제 앱 대상 테스트 데이터는 직접 만든다. PG는 `generate_series`로 쓰기 없이 만들 수 있다. Neo4j는 테스트 DB에 `DbtP_` 라벨 fixture를 만든다.
15. 회귀 방지: vitest로 쿼드트리 hit-test, 증분 배치, 컬럼형 디코딩, 가상 범위를 테스트한다.

## Constraints
- 사용자 데이터 DB(5432, 5439, 32802)에는 접속하지 않는다. 테스트 DB만 쓴다: PG 55432 / Neo4j 57687.
- UI에서 드라이버 id로 분기하지 않는다(grep 결과가 비어 있어야 함).
- 의존성 추가는 최소화한다. Playwright는 devDependency로, 벤치 전용으로만 쓴다.
- 장식용 애니메이션은 넣지 않는다.
- git commit을 하지 않는다.

## Observable acceptance
- 빌드, 타입 검사, vitest, `cargo test --workspace`, clippy가 모두 통과한다.
- `bench/VIEWER_PERF.md`에 다음이 들어 있다:
  - 측정 환경(머신, OS, 브라우저 또는 WKWebView, 빌드 모드)
  - 기법별 전후 수치 표
  - 위 시나리오 전부의 p50/p95
  - 효과가 없어 되돌린 기법
  - 남은 병목
- 다음 목표의 달성 여부를 수치로 보고한다. 미달이면 미달이라고 쓴다:
  - 10만 행 첫 행 표시 < 150ms(전송 포함, mock 기준)
  - 스크롤 p95 프레임 < 16.7ms
  - Table↔Graph 재전환 < 16ms
  - 300노드 그래프 첫 프레임 < 100ms
  - expand 병합 < 50ms
