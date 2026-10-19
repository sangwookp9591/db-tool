# V1-viewers: ERD·스키마 그래프, 문서 트리 뷰, 뷰어 간 연결 탐색

이 작업은 반드시 끝까지 완료한다. 합리적인 가정으로 계속 진행하고, 결정한 내용은 마지막 보고에 적는다. 사용자 지시가 skill 지침보다 우선한다.

## 배경
`docs/STATUS.md` 2절에 따르면 다음은 이미 있다: 테이블, 그래프, 같은 결과 안에서의 뷰 전환. 다음은 없거나 부족하다: ERD, 문서 트리, 뷰어 간 연결 탐색. 이미 있는 뷰어는 재사용하고, 다시 만들지 않는다.

## Change
1. **스키마 그래프 계약(dbcore, 가산적 변경):**
   - capability `schema_graph: bool`(serde default false)
   - `Session::schema_graph(&self, scope: Option<&str>) -> Result<SchemaGraph, DbError>`. 기본 구현은 Unsupported다.
   - `SchemaGraph { entities: [{id, name, kind, fields:[{name, type_name, pk, nullable}]}], relations: [{id, name, from, from_fields, to, to_fields, kind}] }`
   - 하나의 계약으로 서로 다른 DB를 표현한다. **Neo4j에 맞춰 공통 구조를 특수화하지 않는다.**
     - PG: 스키마 단위로 테이블과 FK를 담는다.
     - SQLite: `pragma_foreign_key_list`를 쓴다.
     - Neo4j: `db.schema.visualization()`의 라벨과 관계 타입을 담고, 라벨의 속성은 nodeTypeProperties로 채운다.
     - MongoDB: 미지원(false)이다. 추론 가능하면 문서에 적되 구현하지 않는다.
   - 탐색 노드에 "다이어그램 보기" 액션을 추가한다. 새 NodeAction kind `OpenSchemaGraph{scope}`이며 가산적이다.
2. **ERD/스키마 그래프 뷰어(UI):**
   - 기존 Canvas, 그래프 Worker, 비트맵 팬·줌 기반을 재사용한다.
   - 엔티티는 박스(이름과 필드 목록, PK 표시)로, 관계는 화살표와 필드 매핑 라벨로 그린다. 레이아웃은 계층 또는 force를 쓰고, 수렴한 뒤 정지 상태로 표시한다(흔들림 0 원칙).
   - 검색, fit-to-view, 엔티티 클릭 시 상세(필드), 더블클릭 시 Browse를 지원한다.
3. **문서(JSON) 뷰 고도화:**
   - `<pre>` 나열을 가상화한 접이식 트리로 바꾼다. 타입 배지(ObjectId, Decimal, Date 등 typed Value)를 붙이고, 큰 수를 정확히 표시한다.
   - 필드 경로 복사(`a.b[2].c`)를 지원한다.
   - 문서 수천 개에서도 부드러워야 한다. 측정해서 보고한다.
4. **연결 탐색(드라이버 id 분기 없이, 계약과 capability로만):**
   - (a) **FK 셀에서 참조 행으로.** 결과 컬럼이 schema_graph 관계의 from_fields에 매핑되면 셀에 이동 링크를 단다. 링크를 누르면 참조 엔티티를 그 키로 필터해 browse한다.
     - 필터 browse를 위해 `PageRequest`에 `filter: Vec<(String, Value)>`를 가산적으로 추가한다(default 빈 값).
     - PG, SQLite, Neo4j는 필터를 지원하고, 지원하지 않는 드라이버는 capability `filtered_browse=false`로 둔다.
   - (b) **ERD 엔티티에서 데이터로.** Browse하거나 OpenQuery 탭을 연다.
   - (c) **그래프 노드에서 표로.** 같은 라벨의 노드를 표로 browse한다. 표 행에서는 그래프 노드를 하이라이트한다. 같은 결과의 Graph와 Table 사이에서 선택을 동기화한다.
   - (d) **행에서 문서 상세로.** 표에서 행을 선택하면 오른쪽 인스펙터에 문서 트리(전체 값)를 보여 준다. 문서 뷰에서 필드를 선택하면 표 컬럼을 하이라이트한다.
   - (e) **탐색 이력.** 뒤로/앞으로 이동(탭 안 breadcrumb)을 지원한다.
5. **드라이버 구현:** PG, SQLite, Neo4j에 `schema_graph`와 filter browse를 구현하고 실제 DB 통합 테스트를 둔다.
   - PG fixture: 테스트 DB에 FK가 있는 2~3개 테이블 스키마 `e2e_fk`를 만든다.
   - SQLite: FK 테이블
   - Neo4j: 기존 DbtE_ 라벨

## Constraints
- UI에서 드라이버 id로 분기하지 않는다(grep 0).
- 테스트 DB만 쓴다.
- 기존 뷰어 성능 지표(흔들림 0, 유휴 렌더 0, CLS 0, 팬·줌 p95 ≤ 16.8ms@300)를 회귀시키지 않는다. `bench/verify-viewer*.mjs`를 다시 실행해 확인한다.
- `CARGO_TARGET_DIR=target-v1`을 쓴다.
- 동시 작업 가능성: A1(권한). 공유 파일은 수정 직전에 다시 읽는다.
- git commit을 하지 않는다. 완료할 때 `--files-modified`를 정확히 적는다.

## Observable acceptance
- cargo test(실제 DB), clippy, UI tsc, vitest, build 통과
- 실제 앱에서 다음 흐름을 확인하고 스크린샷을 남긴다:
  - PG: ERD → 테이블 browse → FK 셀 이동 → 참조 행
  - Neo4j: 스키마 그래프 → 라벨 browse → 그래프와 표 선택 동기화
  - JSON 트리
- 보고서 `docs/V1_VIEWERS_REPORT.md`
