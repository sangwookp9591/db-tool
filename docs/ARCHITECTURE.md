# 아키텍처와 드라이버 계약

## 레이아웃

```
Cargo workspace
  crates/dbcore            계약: Driver/Session trait, Manifest, Value, ResultSet, Explorer, Error, Registry
  crates/driver-postgres   tokio-postgres
  crates/driver-neo4j      neo4rs (Bolt)
  crates/driver-sqlite     rusqlite (세 번째 DB: 확장성 검증용)
  src-tauri                호스트: 프로필/키체인/세션/실행/이력/로그, Tauri commands
ui/                        Vite + Preact + TypeScript, CodeMirror 6, 가상 테이블, 캔버스 그래프
bench/                     측정 스크립트와 결과
```

의존 방향은 `driver-* → dbcore ← src-tauri`이다. 호스트와 UI는 개별 드라이버 crate를 알지 못한다. 유일한 예외는 `src-tauri/src/drivers.rs`의 등록 목록이다.

## 원칙

1. **UI는 드라이버 id로 분기하지 않는다.** UI가 분기에 쓰는 정보는 manifest의 `capabilities`, `connection_fields`, `query_language`와 결과에 실제로 들어 있는 뷰(`table`/`graph`/`documents`/`message`)뿐이다.
2. **결과는 다형이다.** 한 `ResultSet`에 표·그래프·문서 뷰가 동시에 있을 수 있다. 예를 들어 Cypher 결과는 graph와 table을 함께 담고, UI는 있는 뷰만 탭으로 보여 준다.
3. **값은 정확하게 전달한다.** 모든 셀 값은 타입 태그가 붙은 `Value`이다. 정수·decimal은 문자열로 보내 JS의 2^53 정밀도 손실을 피한다. JSON도 원문 텍스트로 보내 내부의 큰 수가 손상되지 않게 한다.
4. **메모리 상한을 둔다.** 모든 실행에 `row_limit`(기본 1,000, 상한 100,000)을 적용하고, 드라이버는 서버 쪽에서 한도를 걸거나 스트림을 끊는다. 한도를 넘으면 `truncated=true`로 표시한다. 그래프 노드는 `graph_node_limit`(기본 300)으로 제한한다.
5. **필요한 시점에 조회한다.** 탐색 트리는 `list_children(parent)`로 한 단계씩 가져온다.
6. **UI가 멈추지 않는다.** 모든 DB 호출은 async Tauri command로 처리하며 tokio에서 실행된다. 실행마다 `run_id`를 두어 취소, 타임아웃, 탭 닫힘에 대응한다.

## 계약 (crates/dbcore)

```rust
pub struct DriverManifest {
    pub id: String,                       // "postgres"
    pub display_name: String,             // "PostgreSQL"
    pub paradigm: Paradigm,               // Relational | Graph | Document | KeyValue
    pub query_language: QueryLanguage,    // { id: "sql"|"cypher"|…, display, editor_mode, supports_params }
    pub connection_fields: Vec<FieldSpec>,// 연결 폼. secret=true 필드는 키체인에만 저장
    pub capabilities: Capabilities,
    pub default_query: String,            // 새 탭 템플릿
}
pub struct FieldSpec { key, label, kind: FieldKind /*Text|Number|Password|FilePath|Bool|Select{options}*/, required, default: Option<String>, secret: bool, placeholder }
pub struct Capabilities {
    pub cancel: CancelSupport,     // None | Abort(클라이언트만 중단) | Server(서버에 취소 요청)
    pub timeout: bool,
    pub params: bool,              // 쿼리 파라미터 (JSON 객체)
    pub transactions: bool,        // 명시적 BEGIN/COMMIT 세션 유지 가능 여부
    pub paged_browse: bool,        // 탐색 노드에서 페이지 조회
    pub graph_expand: bool,        // 그래프 노드 이웃 확장
    pub multi_statement: bool,     // 한 번에 여러 문장 → 여러 ResultSet
    pub edit_rows: bool,           // (1차 미구현, false)
}

#[async_trait] pub trait Driver: Send + Sync {
    fn manifest(&self) -> &DriverManifest;
    async fn connect(&self, cfg: &ConnectConfig) -> Result<Box<dyn Session>, DbError>;
}
pub struct ConnectConfig { pub fields: BTreeMap<String,String>, pub secret: Option<String>, pub connect_timeout_ms: u64 }
// Debug 구현에서 secret은 "***"로 출력한다.

#[async_trait] pub trait Session: Send + Sync {
    async fn ping(&self) -> Result<(), DbError>;
    async fn server_info(&self) -> Result<String, DbError>;
    async fn list_children(&self, parent: Option<&str>) -> Result<Vec<ExplorerNode>, DbError>;
    async fn browse(&self, node_id: &str, page: PageRequest) -> Result<ResultSet, DbError>;      // capabilities.paged_browse
    async fn execute(&self, req: &ExecRequest, cancel: CancelToken) -> Result<Vec<ResultSet>, DbError>;
    async fn expand(&self, element_id: &str, limit: u32) -> Result<ResultSet, DbError>;          // capabilities.graph_expand
    async fn close(&self);
}

pub struct ExplorerNode {
    pub id: String,            // 드라이버가 해석하는 불투명 경로
    pub label: String, pub detail: Option<String>,  // detail: 컬럼 타입, 행 수 등
    pub kind: String,          // "schema"|"table"|"column"|"label"|"rel_type"|"property"|… UI는 아이콘 힌트로만 사용
    pub icon: Icon,            // Folder|Table|Column|Key|Index|Label|Relationship|Property|Collection|Database|View|Function
    pub has_children: bool,
    pub actions: Vec<NodeAction>, // { id, label, kind: Browse | OpenQuery{text} | Insert{text} }
}
pub struct PageRequest { pub offset: u64, pub limit: u32 }
pub struct ExecRequest { pub text: String, pub params: serde_json::Map<String,Value>, pub row_limit: u32, pub graph_node_limit: u32, pub timeout_ms: u64 }

pub struct ResultSet {
    pub title: String,                  // 문장 요약
    pub table: Option<Table>,
    pub graph: Option<Graph>,
    pub documents: Option<Vec<Value>>,
    pub message: Option<String>,        // "UPDATE 3", "Created 2 nodes"
    pub affected: Option<u64>,
    pub row_count: u64,                 // 반환된 행 수
    pub truncated: bool,                // row_limit / graph_node_limit 초과 여부
    pub page: Option<PageInfo>,         // { offset, limit, has_more } browse 결과일 때
    pub elapsed_ms: u64,
}
pub struct Table { pub columns: Vec<Column>, pub rows: Vec<Vec<Value>> }
pub struct Column { pub name: String, pub type_name: String }
pub struct Graph { pub nodes: Vec<GNode>, pub rels: Vec<GRel> }   // 중복 제거됨
pub struct GNode { pub id: String, pub labels: Vec<String>, pub props: Vec<(String, Value)> }
pub struct GRel  { pub id: String, pub rel_type: String, pub start: String, pub end: String, pub props: Vec<(String, Value)> }

#[serde(tag = "t", content = "v")]
pub enum Value {
    Null, Bool(bool),
    Int(String), Float(f64 | "NaN"/"Infinity" 문자열), Decimal(String),
    Text(String), Bytes { len: u64, hex: String /*앞 64바이트*/ },
    Date(String), Time(String), DateTime(String), DateTimeTz(String), Duration(String), Uuid(String),
    Json(String /*원문*/), List(Vec<Value>), Map(Vec<(String, Value)>),
    Node(GNode), Rel(GRel), Path { nodes: Vec<GNode>, rels: Vec<GRel> },
    Point { srid: i32, x: f64, y: f64, z: Option<f64> },
    Other { type_name: String, text: String },
}

pub struct DbError { pub kind: ErrorKind, pub message: String, pub detail: Option<String>, pub position: Option<u32> }
pub enum ErrorKind { Connection, Auth, Query, Timeout, Cancelled, Unsupported, Internal }
```

`CancelToken`은 `tokio_util::sync::CancellationToken`이다. 드라이버는 `select!`로 토큰을 지켜본다. `CancelSupport::Server` 드라이버(PostgreSQL)는 토큰이 발동하면 서버에 취소 요청을 보낸다.

## 호스트 (src-tauri) 명령

| command | 역할 |
|---|---|
| `drivers_list` | 등록된 manifest 목록 |
| `profiles_list / profile_save / profile_delete` | `profiles.json`에는 secret 필드를 **제외**하고 저장한다. secret은 OS 키체인(`keyring`, service=`db-tool`, account=`profile:{id}`)에 둔다 |
| `connection_test(profile, secret?)` | 연결 → `ping` → `server_info` → 닫기 |
| `session_open(profile_id) → session_id` / `session_close` | 여러 연결을 동시에 연다 |
| `explorer_children(session_id, parent?)` | |
| `browse(session_id, node_id, offset, limit, run_id)` | |
| `execute(session_id, run_id, req)` | 타임아웃(`tokio::time::timeout`)과 취소를 모두 적용한다. 결과를 이력에 기록한다 |
| `cancel(run_id)` | 탭을 닫거나 사용자가 취소할 때 호출 |
| `expand(session_id, element_id, limit)` | |
| `history_list(limit)` | `history.jsonl`: 시각, 프로필명, 쿼리 텍스트, 소요 시간, 행 수, 오류 요약. 파라미터 값과 secret은 기록하지 않는다 |

세션 오류 중 `ErrorKind::Connection`(네트워크 끊김 등)이 나면 호스트는 세션을 dead로 표시한다. UI는 재연결 버튼을 보여 준다.

로그는 `tracing`을 쓴다. 연결 필드 중 secret인 것, 연결 URI, 파라미터 값은 로그에 남기지 않는다. 검증 스크립트가 로그와 설정 파일을 grep해 이를 확인한다.

## 새 DB 추가 절차

[PLUGIN_GUIDE.md](PLUGIN_GUIDE.md)를 참고한다. crate를 하나 만들고 `Driver`를 구현한 뒤 `src-tauri/src/drivers.rs`에 한 줄을 추가한다. **UI는 수정하지 않는다.**
