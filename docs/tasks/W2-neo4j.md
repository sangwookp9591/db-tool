# W2-neo4j: Neo4j 드라이버 crate

이 작업은 반드시 끝까지 완료한다. 합리적인 가정으로 계속 진행하고, 결정한 내용은 마지막 보고에 적는다. 사용자 지시가 skill 지침보다 우선한다.

## Target
- 저장소: `/Users/iron/Project/psw/db-tool`
- 계약: `docs/ARCHITECTURE.md`, `crates/dbcore`(구현 완료). PostgreSQL 드라이버(`crates/driver-postgres`)를 패턴 참고로 삼는다.

## Change
`crates/driver-neo4j`를 새로 만들고 워크스페이스에 추가한다. 등록은 `src-tauri/src/drivers.rs`에 **한 줄** 추가하는 것으로 끝낸다.

1. **Bolt 클라이언트:** neo4rs(최신 0.8.x 이상)를 쓴다. 이 작업에 필요한데 neo4rs에 없는 기능(예: 레코드 키를 순회하며 임의 BoltType 얻기, Path, Bolt 5 element_id, 시간·공간 타입)이 있으면 먼저 neo4rs 소스에서 지원 여부를 확인한다. 정말 지원하지 않으면 대안을 고르고 그 근거를 보고에 쓴다. 대안은 다음 중 하나다.
   - neo4rs의 `BoltType`을 직접 매칭한다.
   - `elementId()`를 Cypher로 함께 반환하도록 재작성하지 않는 방식을 쓴다.
   - 최소 범위의 Bolt 클라이언트를 직접 구현한다.
2. **manifest:**
   - paradigm: Graph
   - query_language: `{id:"cypher", display:"Cypher", editor_mode:"cypher", supports_params:true}`
   - 연결 필드: uri(기본 `bolt://localhost:7687`, `neo4j://`·`neo4j+s://`·`bolt+s://` 허용), database(선택, 비우면 기본 DB), user(기본 `neo4j`), password(secret)
   - capabilities:
     - true: `params`, `graph_expand`, `paged_browse`, `timeout`
     - `cancel: Abort`. 정직하게 선언한다: Bolt에는 서버 취소가 없으므로 진행 중인 스트림을 끊고 연결을 정리한다. 가능하면 RESET 또는 연결 폐기를 쓴다.
     - `multi_statement`: 세미콜론으로 구분된 여러 문장을 지원하면 true. 지원할 경우 문자열과 주석을 인식하는 분리기를 쓴다.
     - `transactions`는 1차에서 false여도 된다. 근거를 보고한다.
   - default_query: `MATCH (n) RETURN n LIMIT 25`
3. **탐색:** 필요할 때 조회한다.
   - 루트: "Labels"(`CALL db.labels()`, 라벨별 개수는 보이는 것만 `MATCH (n:\`L\`) RETURN count(n)` — 비용이 크면 생략 가능), "Relationship Types"(`CALL db.relationshipTypes()`), "Property Keys"(`CALL db.propertyKeys()`), "Indexes & Constraints"(`SHOW INDEXES`, `SHOW CONSTRAINTS`)
   - 라벨 노드의 하위: 그 라벨의 속성 키와 타입. `db.schema.nodeTypeProperties()`로 가져오며, 실패하면 샘플 100개로 추론한다.
   - 라벨 노드 actions:
     - Browse(`MATCH (n:\`L\`) RETURN n SKIP $o LIMIT $l` 페이징)
     - OpenQuery(`MATCH (n:\`L\`) RETURN n LIMIT 25`)
   - 관계 타입 노드 actions:
     - Browse
     - OpenQuery(`MATCH p=()-[r:\`T\`]->() RETURN p LIMIT 25`)
   - 식별자는 백틱으로 escape한다.
4. **execute:**
   - 파라미터: `ExecRequest.params`(JSON)를 BoltType으로 바꾼다. 정수는 문자열 Int 표기도 받는다.
   - 레코드 스트림을 읽다가 행 수가 `row_limit`에 도달하면 멈추고 `truncated=true`로 표시한다. 이때 나머지 스트림은 버리고 연결 상태를 정리한다.
   - ResultSet 구성:
     - **table:** 컬럼 = 레코드 키. 값은 Value로 매핑하며, `column.type_name`은 첫 non-null 값의 타입(Node/Relationship/Path/Integer/…)이다.
     - **graph:** 모든 셀(List·Map·Path 내부 포함)에서 Node와 Relationship을 재귀적으로 수집하고 id로 중복을 제거한다.
       - 노드가 `graph_node_limit`을 넘으면 넘는 노드는 수집하지 않고 `truncated=true`로 표시한다.
       - 관계는 양 끝 노드가 모두 포함된 것만 남긴다.
       - 그래프 요소가 하나도 없으면 `graph=None`이다.
     - 쓰기 쿼리는 summary counters(nodes created 등)를 `message`에, 영향 수를 `affected`에 담는다. 가능한 범위에서 처리한다.
   - **Value 매핑:**
     - Integer → Int(문자열)
     - Float → Float
     - String → Text
     - Boolean
     - Null
     - List
     - Map
     - Bytes
     - Date → Date
     - LocalTime → Time
     - Time(offset) → Time
     - LocalDateTime → DateTime
     - DateTime(zone/offset) → DateTimeTz, ISO-8601 원문 형식
     - Duration → Duration(ISO `P..T..`)
     - Point2D/3D → Point
     - Node → Node, Relationship → Rel, Path → Path
     - GNode/GRel의 id는 Bolt 5의 element_id 문자열을 쓴다. 없으면 legacy 정수 id 문자열을 쓰고, 그 경우 expand 쿼리도 그에 맞춘다.
   - **오류:**
     - Neo4j 오류 code(`Neo.ClientError.Statement.SyntaxError` 등)를 detail에, 메시지를 message에 넣는다.
     - 인증 오류 → Auth
     - 연결 끊김 → Connection
     - Neo.ClientError.Transaction.TransactionTimedOut 등 → Timeout
5. **expand(element_id, limit):**
   - `MATCH (n) WHERE elementId(n) = $id MATCH (n)-[r]-(m) RETURN n, r, m LIMIT $limit`를 실행해 graph 결과를 돌려준다. legacy id라면 `id(n) = $id`를 쓴다.
   - 결과에는 table도 포함한다.
6. **browse:** 그래프와 표를 모두 담은 ResultSet을 돌려준다. `page.has_more`는 limit+1개를 가져와 판정한다.
7. **통합 테스트:** `DBTOOL_TEST_NEO4J="bolt://127.0.0.1:57687|neo4j|testpassword"`가 있을 때만 실행한다. 테스트 DB는 `dbtool-test-neo4j` 컨테이너이며 이미 떠 있다. 테스트 전에 직접 fixture를 만든다: 테스트 전용 라벨 접두사 `DbtT_`를 쓰고, 정리 시 그 라벨만 삭제한다.
   - 파라미터 쿼리
   - 노드·관계·경로가 섞인 결과(`RETURN n, r, p, [n, m], {a: n}`)의 graph 중복 제거와 table
   - 큰 정수 9007199254740993의 정확성
   - temporal 타입 전부와 point
   - `row_limit`과 `graph_node_limit`의 truncated
   - 문법 오류 → Query + code
   - 잘못된 비밀번호 → Auth
   - 닫힌 포트 → Connection
   - 긴 쿼리(`UNWIND range(1, 100000000) AS x WITH x WHERE x < 0 RETURN x` 등)에 대해:
     - timeout 500ms 후 Timeout
     - cancel 후 Cancelled
     - 이후 같은 세션에서 `RETURN 1` 성공
   - expand
   - 탐색(labels, rel types, property keys, indexes)
   - browse 페이징

8. **읽기 전용 실행(W3-agent 워커와 연계):** W3 워커가 `dbcore`에 `ExecRequest.read_only`와 `Capabilities.read_only_exec`를 추가한다. 이 필드가 생기면 다음을 한다.
   - `read_only_exec: true`를 선언한다.
   - `read_only=true`이면 READ 접근 모드(neo4rs의 read operation/routing, 또는 `BEGIN` 시 mode `r`)로 실행해, 서버가 쓰기를 거부하도록 한다.
   - 테스트: read_only로 `CREATE (:DbtT_X)`가 거부되는지 확인한다.
   - 필드가 아직 없으면 잠시 다른 부분을 먼저 진행하고, 직접 추가하지 않는다.

## Constraints
- 사용자 데이터가 있는 DB에는 접속하지 않는다. 테스트 Neo4j는 `127.0.0.1:57687`만 쓴다.
- `ui/` 디렉터리는 **수정하지 않는다.** Neo4j 지원은 UI 변경 없이 동작해야 한다. UI에서 무언가 바꿔야 한다고 판단되면 고치지 말고 보고에 적는다.
- `crates/dbcore` 계약 변경은 꼭 필요할 때만, 가산적으로(필드 추가, 기본값 보존) 한다. 변경했다면 그 이유를 보고한다. PG 드라이버 테스트는 계속 통과해야 한다.
- 비밀번호를 로그와 오류에 넣지 않는다.
- git commit을 하지 않는다.

## Observable acceptance
- `cargo test --workspace` 통과. 두 환경 변수를 모두 설정하고 실행한다:
  - `DBTOOL_TEST_PG='host=127.0.0.1 port=55432 user=postgres password=testpw dbname=dbtool_test'`
  - `DBTOOL_TEST_NEO4J='bolt://127.0.0.1:57687|neo4j|testpassword'`
- `cargo clippy --workspace -- -D warnings` 통과
- `git diff --stat -- ui/`가 비어 있어야 한다
- 보고: 테스트 요약, neo4rs 한계와 대응, dbcore 변경 여부, 남은 한계
