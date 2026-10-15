# V1 뷰어 구현·검증 보고

2026-10-15. 시작 기준 커밋 `568f43e`; 작업 중 W8 커밋으로 HEAD가 `c56feae`로 이동했다. V1은 커밋하지 않았다.

## 구현 범위

- dbcore에 `schema_graph`/`filtered_browse`(serde 기본 false), `Session::schema_graph`(기본 Unsupported), 공통 `SchemaGraph`, `OpenSchemaGraph { scope }`, `PageRequest.filter`(기본 빈 목록)를 가산적으로 추가했다. Capabilities와 PageRequest는 Default도 지원한다.
- PG는 스키마 테이블·컬럼·PK·nullable 및 FK의 양쪽 필드를 제약 순서대로 조회한다. SQLite는 `pragma_foreign_key_list`를 사용하고 생략된 참조 컬럼은 부모 PK 순서로 해석한다. SQLite의 일반 복합 PK가 NULL을 허용하는 특성도 nullable에 반영한다.
- Neo4j는 `db.schema.visualization()`의 라벨·관계 타입과 `db.schema.nodeTypeProperties()`의 속성을 동일 계약으로 반환한다. 가상 스키마 노드는 실제 DB에서 음수 ID를 사용하고 `n.name`이 NULL이므로 `id(n)`/`labels(n)`과 관계 끝점을 스칼라 맵으로 투영해 Bolt 디코딩 문제를 피했다.
- 세 드라이버의 Browse는 AND 동등 필터를 적용한다. PG/SQLite는 식별자와 검증한 스칼라 값을 quote하고, PG 문자열은 E 리터럴로 역슬래시까지 escape한다. Neo4j는 식별자 quote와 파라미터 바인딩을 사용한다. 호스트는 filtered_browse=false인 드라이버의 비어 있지 않은 필터를 거부한다.
- ERD는 기존 GraphView Canvas·Worker·비트맵 팬/줌을 재사용한다. 엔티티 박스, PK·타입·nullable, 화살표·필드 대응 라벨, 검색·fit·상세·더블클릭 Browse, 키보드용 엔티티 목록을 제공한다. Worker가 수렴한 위치만 표시하고 유휴에 다시 tick하지 않는다.
- JSON은 기존 `<pre>` 목록을 접이식 가상 트리로 교체했다. typed Value 배지, 원문 정수·decimal의 정밀도, 경로 복사(`a.b[2].c`; 특수 필드명은 bracket quoting)를 보존한다. 단일 문서는 루트를 열어 행 인스펙터의 필드를 바로 보여 준다.
- FK 링크는 복합 키의 모든 필드가 있고 NULL이 아닌 경우에 참조 엔티티를 필터로 Browse한다. ERD→Browse와 그래프 노드→같은 라벨 Browse는 불투명 엔티티 ID를 사용한다. Browse의 첫 뷰는 Table이다.
- 같은 결과의 Graph/Table 사이에서 선택을 공유하고, 표 행은 전체 typed 값을 오른쪽 문서 인스펙터로 전달한다. JSON/인스펙터 필드 선택은 대응 표 컬럼을 강조한다. 탭 내 breadcrumb·뒤로·앞으로를 제공하고 조회 중에는 이력 이동을 막는다. 쿼리에서 탐색 후 돌아오면 편집 중인 쿼리 텍스트를 보존한다.
- UI의 드라이버 ID별 분기는 추가하지 않았다. 기능은 capability, 액션, 결과 뷰, schema entity ID에 따라 결정한다. SchemaView/GraphView는 지연 로딩을 보존하며 일반 빈 쿼리 탭에서는 스키마를 선조회하지 않는다.

## 검증 결과

모든 Rust 명령은 `CARGO_TARGET_DIR=target-v1 CARGO_BUILD_JOBS=3`으로 실행했다. PG는 `127.0.0.1:55432/axdb_test`, Neo4j는 `bolt://127.0.0.1:57687`, SQLite는 임시 파일을 사용했다. PG fixture는 `e2e_fk.parent`와 `e2e_fk.child`; Neo4j fixture는 `DbtE_V1`/`DbtE_V1_LINK`다. 사용자 DB에는 접근하지 않았다.

| 검사 | 결과·근거 |
|---|---|
| V1+호스트 cargo test | dbcore, PG, SQLite, Neo4j, axdb: **50 passed, 1 ignored**. ignored는 기존 OS 키체인 테스트다. 세 드라이버의 schema_graph_and_filtered_browse가 실제 DB에서 실행됐다. |
| cargo clippy --workspace -- -D warnings | **통과**. MongoDB를 포함한 workspace 검사. |
| 앱 cargo build -p axdb | **통과**. 최신 UI 자산을 다시 임베드한 target-v1 바이너리도 빌드했다. |
| UI tsc --noEmit / vitest / build | **통과**. Vitest **8 files, 41 tests**. |
| bench/verify-viewer.mjs | **통과**. 기존 가상 테이블, 정확한 값, 정렬·필터, 뷰 유지, 유휴 크기 안정성 및 CLS 검사. |
| bench/verify-viewer-w4b.mjs | **통과**. 실제 포인터 팬·노드 드래그·확장, DPR 2 비트맵 복원, 테마·resize, 유휴 렌더/좌표 고정. |
| bench/v1-viewers.mjs | **통과**, [원자료](../bench/results-v1-viewers.json). 문서 5,000개, 정확한 값·ObjectId, 경로 복사의 Clipboard API 인자, Graph→Table 행 선택, Table→Graph 속성, JSON→컬럼 강조를 assert했다. |
| 드라이버 ID 분기 grep / git diff --check | **통과**. |
| cargo test --workspace | **전체 통과 아님**. M1의 real_mongodb_contract에서 `tests/mongodb.rs:204`, 실제 Query / 기대 Timeout이 실패해 뒤 crate 실행이 중단됐다. V1/호스트는 위의 별도 실DB 검사로 확인했다. 실패를 전달한 뒤 코디네이터가 M1 제외 V1 범위로 마무리하도록 지시했다. |

PG·SQLite 테스트는 복합 FK 필드 순서, i64::MAX 키의 정확한 행, 따옴표가 들어 있는 공격 문자열의 0행 결과를 검사한다. Neo4j 테스트는 실제 라벨·관계·속성과 큰 정수 키 및 주입 문자열을 확인한다. 같은 라벨의 여러 nodeType 속성 타입을 합치는 최종 수정도 혼합 String/정수 fixture로 별도 재검증했다. dbcore는 기존 JSON의 기본값 호환과 SQL 리터럴 검증을, UI는 lossless JSON 파서·특수 경로·복합 FK·NULL/누락 키를 검사한다.

### 측정

[최종 브라우저 측정](../bench/results-v1-viewers.json)은 Chromium headless, 1440×1000, 기존 W4의 60 rAF 표본 방식이다. 문서 first 값은 이미 JS에 있는 fixture를 마운트한 시간이며 DB 전송이나 native 시간은 아니다.

| 항목 | 결과 |
|---|---|
| Graph 300 팬·줌 p95 | **16.8 ms**, 기준 ≤16.8 ms |
| Graph 유휴 렌더 / 평균 좌표 이동 | **0 frames / 0 px** |
| 문서 5,000개 첫 표시 | **33.5 ms** |
| 문서 스크롤 p95 / 최대 DOM 행 | **16.7 ms / 37행** |

[별도 headless 측정](../bench/results-v1-panzoom-headless.json)도 Graph 300 p95 16.8ms, 유휴·이동 0이다. [초기 visible Ego 측정](../bench/results-v1-panzoom.json)은 다른 앱 포커스와 동시 빌드 중 p95 34.3ms였으므로 통과 증거로 사용하지 않았다. 네이티브 팬·줌 p95를 이 브라우저 수치로 대체하지 않는다.

## 실제 앱 흐름과 스크린샷

별도 `AXDB_CONFIG_DIR=/tmp/axdb-v1-ui`, MCP bridge OFF, `target-v1/debug/axdb`의 실제 WKWebView에서 테스트 DB에 연결했다. mock 표시가 없는 앱에서 확인했다. 최종 UI 자산을 포함한 바이너리의 ERD는 별도로 다시 캡처했다. 최종 바이너리의 전체 PG 흐름 반복은 native 자동화의 창 포커스/접근성 인덱스 경합으로 완료하지 못했으며, 아래 참조 행·Neo4j·JSON 증거는 앞서 완료한 실제 앱 실행에서 얻었다.

- PG: 탐색기의 e2e_fk 다이어그램 액션 → [최종 바이너리 ERD](../bench/v1-screenshots/pg-erd-final.png) → child 엔티티 상세의 Browse → [FK가 있는 표](../bench/v1-screenshots/pg-browse-fk.png) → parent_id 링크 → [tenant=a, id=9223372036854775807인 참조 행](../bench/v1-screenshots/pg-reference-row.png).
- Neo4j: Labels 다이어그램 액션 → [실제 스키마](../bench/v1-screenshots/neo-schema.png) → DbtE_V1 Browse → [표 행 선택](../bench/v1-screenshots/neo-table-selection.png) → [Graph 전환 시 exact 노드의 노란 선택 테두리](../bench/v1-screenshots/neo-graph-selection.png).
- JSON: 같은 실제 행의 인스펙터에서 루트·n을 펼쳐 [큰 정수와 n.id/n.name 경로](../bench/v1-screenshots/native-json-tree.png)를 확인했다. [5,000문서 트리](../bench/v1-screenshots/documents-5000.png)는 브라우저 fixture 증거다.

## 합리적인 가정과 한계

- `scope`는 드라이버가 해석하는 불투명 값이다. PG는 스키마명 또는 탐색기 엔티티 ID에서 스키마를, Neo4j는 라벨명 또는 엔티티 ID에서 라벨을 해석한다. SQLite는 단일 main 스키마를 표시한다.
- FK의 원본은 Browse 또는 탐색기에서 열어 수정하지 않은 OpenQuery로 확정한다. 임의 SQL의 alias/join이나 편집 후 쿼리에 원본 테이블을 추측하지 않는다. 해당 쿼리까지 FK 링크를 확장하려면 드라이버가 컬럼 provenance를 반환해야 한다.
- 같은 결과의 그래프 선택은 현재 결과에 실제로 포함된 노드에 적용한다. 다중 라벨 노드는 현재 스키마에 있는 첫 대응 엔티티로 Browse한다. 복잡한 값/지원하지 않는 키 타입은 필터 오류로 거부하며 전체 Browse로 조용히 대체하지 않는다.
- MongoDB의 schema_graph/filtered_browse는 false다. 샘플 문서의 필드 구조·중첩 및 참조 패턴을 추론할 가능성은 있지만 스키마 관계 계약으로 추론하는 기능은 구현하지 않았다.
- 브라우저 경로 복사는 Clipboard API를 stub해 전달 값을 assert했다. 네이티브 OS 클립보드의 실제 쓰기는 검증하지 않았다.
- 깊이 128을 넘는 raw JSON은 원문 표시로 유지한다. 더 깊은 트리가 필요하면 반복형 파서로 전환할 수 있다. 키체인·공급자·운영 DB 검증은 이번 범위에 넣지 않았다.
- 탐색 뒤로/앞으로는 대상·필터를 복원해 다시 Browse하며, 이전 DB 응답 스냅샷을 영구 저장하지 않는다.

## 공유 작업 보존

시작 시 Cargo.toml/Cargo.lock, W8 문서, resource.ts가 수정돼 있었고 W8 산출물과 MongoDB crate가 untracked였다. 이후 A1·P1·M1 변경과 W8 커밋이 같은 작업 디렉터리에 들어왔다. V1은 공유 파일을 수정 직전에 다시 읽었으며 다른 lane의 변경을 되돌리거나 커밋하지 않았다. MongoDB에는 새 계약으로 깨진 Capabilities/PageRequest 생성자의 기본값 호환 hunk만 보정했고, 중복된 ExecRequest prefix는 담당자의 최신 필드를 유지하며 제거했다. MongoDB 기능·오류 분류는 M1 소유다.

기준 커밋에 대한 `git diff` 전체를 V1 단독 diff로 해석하면 안 된다. V1 변경 목록은 worker_done의 files-modified와 아래 파일 범위를 함께 본다.

- 계약·드라이버: dbcore lib/filter/contract test, example-kv manifest, PG lib/schema/test, SQLite lib/schema/test, Neo4j lib/explorer/test, MongoDB의 호환 생성자 hunk.
- 호스트·UI: src-tauri/lib의 schema_graph와 browse filter; UI contract/api/App/QueryTab/Results/VirtualTable/GraphView/NavigationTab/SchemaView/DocumentTree/documents/navigation/style/graph.worker 및 관련 테스트·perf harness.
- 문서·증거: ARCHITECTURE, PLUGIN_GUIDE, 이 보고서, bench/v1-viewers.mjs와 results-v1-*·v1-screenshots.

카탈로그 조회 근거: [PostgreSQL pg_constraint](https://www.postgresql.org/docs/17/catalog-pg-constraint.html), [SQLite foreign_key_list](https://www.sqlite.org/pragma.html#pragma_foreign_key_list). Neo4j 가상 노드 투영은 테스트 DB의 직접 실행 결과로 확인했다.

검증용 native 앱 프로세스는 종료했다. 기존 실행 앱과 다른 lane의 서버는 보존했고, 이 작업에서 사용한 Vite 1424/1426 서버는 코디네이터의 증거 재현을 위해 유지했다.
