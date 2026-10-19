# 아키텍처와 드라이버 계약

## 레이아웃

```
Cargo workspace
  crates/dbcore            계약: Driver/Session trait, Manifest, Value, ResultSet, Explorer, Error, Registry
  crates/driver-protocol   드라이버 프로세스 프로토콜 (길이 접두사 + MessagePack 프레임)
  crates/driver-sdk        axdb_driver_sdk::serve(driver): Driver를 플러그인 실행 파일로 만든다
  crates/driver-package    .axdriver 패키지, 서명된 목록(index.json): 생성·검증·안전한 압축 해제
  crates/driver-postgres   tokio-postgres          ┐
  crates/driver-neo4j      neo4rs (Bolt)           │ 각각 src/bin/axdb-driver-<id>.rs 진입점으로
  crates/driver-sqlite     rusqlite                │ axdb-driver-<id> 실행 파일을 만든다
  crates/driver-mongodb    mongodb                 │
  crates/driver-redis      redis                   ┘
  crates/driver-example-kv 메모리 전용 최소 예제(계약 테스트용)
  xtask                    cargo xtask package-drivers / gen-signing-key
  src-tauri                호스트: 프로필/키체인, plugin_host, driver_install, 세션/실행/이력/로그, 미리보기, 권한, MCP 브리지, AI, 내보내기, 업데이터
ui/                        Vite + Preact + TypeScript, CodeMirror 6, 가상 테이블, 캔버스 그래프, 온보딩, 드라이버 화면
scripts/                   release.sh(앱 릴리스), publish-drivers.sh(드라이버 업로드), release-manifest.py(latest.json)
bench/                     측정 스크립트와 결과
```

의존 방향은 `driver-* → dbcore`, `driver-* → driver-sdk → driver-protocol → dbcore`, `src-tauri → driver-protocol, driver-package, dbcore`이다. **호스트는 드라이버 crate에 의존하지 않는다.** 실제 DB 드라이버 crate는 호스트의 dev-dependency로만 있고 계약 테스트에서만 쓴다. 앱은 드라이버 없이 배포되며, 드라이버는 설치된 실행 파일을 자식 프로세스로 띄워 쓴다([ADR-0003](adr/0003-installable-drivers.md)).

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
pub enum PreviewSupport { None, Transactional, DryRun } // serde default None
pub struct Capabilities {
    pub execute_stream: bool,      // serde default false; protocol-1 optional incremental execution
    pub schema_graph: bool,        // serde default false; 엔티티·필드·관계 다이어그램
    pub filtered_browse: bool,     // serde default false; typed Value의 AND 동등 필터
    pub preview_changes: PreviewSupport, // serde(default); capability-driven UI/tools
    pub read_only_exec: bool,      // serde(default)=false; DB-enforced read-only execution
    pub cancel: CancelSupport,     // None | Abort(클라이언트만 중단) | Server(서버에 취소 요청)
    pub timeout: bool,
    pub params: bool,              // 쿼리 파라미터 (JSON 객체)
    pub transactions: bool,        // 명시적 BEGIN/COMMIT 세션 유지 가능 여부
    pub paged_browse: bool,        // 탐색 노드에서 페이지 조회
    pub graph_expand: bool,        // 그래프 노드 이웃 확장
    pub multi_statement: bool,     // 한 번에 여러 문장 → 여러 ResultSet
    pub edit_rows: bool,           // primary-key row edits; PostgreSQL/SQLite true
}

#[async_trait] pub trait Driver: Send + Sync {
    fn manifest(&self) -> &DriverManifest;
    async fn connect(&self, cfg: &ConnectConfig) -> Result<Box<dyn Session>, DbError>;
}
pub struct ConnectConfig { pub fields: BTreeMap<String,String>, pub secret: Option<String>, pub connect_timeout_ms: u64 }
// Debug 구현에서 secret은 "***"로 출력한다.

#[async_trait] pub trait Session: Send + Sync {
    async fn preview_edits(&self, req: &EditRequest, cancel: CancelToken) -> Result<ChangePreview, DbError>; // default Unsupported
    async fn apply_edits(&self, req: &EditRequest, cancel: CancelToken) -> Result<EditResult, DbError>; // default Unsupported
    async fn ping(&self) -> Result<(), DbError>;
    async fn server_info(&self) -> Result<String, DbError>;
    async fn list_children(&self, parent: Option<&str>) -> Result<Vec<ExplorerNode>, DbError>;
    async fn schema_graph(&self, scope: Option<&str>) -> Result<SchemaGraph, DbError>;         // default Unsupported
    async fn browse(&self, node_id: &str, page: PageRequest) -> Result<ResultSet, DbError>;      // capabilities.paged_browse
    async fn preview(&self, req: &ExecRequest, cancel: CancelToken) -> Result<ChangePreview, DbError>; // default Unsupported
    async fn execute_stream_raw(&self, req: &ExecRequest, cancel: CancelToken, sink: &dyn AsyncResultSink) -> Result<Vec<ResultSet>, DbError>; // default Unsupported, capability negotiation required
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
    pub actions: Vec<NodeAction>, // { id, label, kind: Browse | OpenQuery{text} | Insert{text} | OpenSchemaGraph{scope} }
}
pub struct PageRequest { pub offset: u64, pub limit: u32, pub filter: Vec<(String, Value)> /* serde default [] */ }
pub struct ExecRequest { pub read_only: bool /*serde(default)=false*/, pub text: String, pub params: serde_json::Map<String,Value>, pub row_limit: u32, pub graph_node_limit: u32, pub timeout_ms: u64, pub preview_sample_limit: u32 /*serde(default)=50*/ }

pub struct ChangePreview { pub statements: Vec<StatementPreview>, pub committed: bool /*always false*/, pub notes: Vec<String> }
pub enum StatementKind { Insert, Update, Delete, Ddl, GraphWrite, Other }
pub struct StatementPreview {
    pub text: String, pub kind: StatementKind, pub affected: Option<u64>,
    pub before: Option<Table>, pub after: Option<Table>, // each <= sample limit (host/driver cap 1000)
    pub schema_diff: Option<SchemaDiff>, pub counters: Vec<(String,i64)>, pub warnings: Vec<String>,
}
pub struct SchemaDiff { pub added: Vec<String>, pub removed: Vec<String>, pub changed: Vec<String> }

pub struct ResultSet {
    #[serde(default)] pub edit_target: Option<EditTarget>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditTarget { pub node_id: String, pub key_columns: Vec<String> }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op")]
pub enum RowEdit {
    Update { key: Vec<(String, Value)>, set: Vec<(String, Value)> },
    Insert { values: Vec<(String, Value)> },
    Delete { key: Vec<(String, Value)> },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditRequest {
    pub node_id: String,
    pub edits: Vec<RowEdit>,
    #[serde(default = "default_timeout_ms")] pub timeout_ms: u64,
    #[serde(default = "default_preview_sample_limit")] pub preview_sample_limit: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditResult { pub affected: Vec<u64>, pub statements: Vec<String> }

pub struct DbError { pub kind: ErrorKind, pub message: String, pub detail: Option<String>, pub position: Option<u32> }
pub enum ErrorKind { Connection, Auth, Query, Timeout, Cancelled, Unsupported, Internal, Conflict }
```

`CancelToken`은 `tokio_util::sync::CancellationToken`이다. 드라이버는 `select!`로 토큰을 지켜본다. `CancelSupport::Server` 드라이버(PostgreSQL)는 토큰이 발동하면 서버에 취소 요청을 보낸다.

## 호스트 (src-tauri) 명령

| command | 역할 |
|---|---|
| `profiles_list / profile_save / profile_delete` | `profiles.json`에는 secret 필드를 **제외**하고 저장한다. secret은 OS 키체인(`keyring`, service=`axdb`, account=`profile:{id}`)에 둔다 |
| `connection_test(profile, secret?)` | 연결 → `ping` → `server_info` → 닫기 |
| `session_open(profile_id) → session_id` / `session_close` | 여러 연결을 동시에 연다 |
| `explorer_children(session_id, parent?)` | |
| `browse(session_id, node_id, offset, limit, run_id, filter?)` | typed Value AND 필터, 미지원 capability는 거부 |
| `schema_graph(session_id, scope?)` | 공통 SchemaGraph, 기본 Session 구현은 Unsupported |
| `execute(session_id, run_id, req)` | 타임아웃(`tokio::time::timeout`)과 취소를 모두 적용한다. 결과를 이력에 기록한다 |
| `preview(session_id, run_id, req, source?)` | 드라이버 preview 실행, 취소/타임아웃 후 정리 대기, 이력 kind=preview. source 기본 ui, 에이전트는 agent. 거부 배치는 notes만 반환하고 실행하지 않는다 |
| `edit_preview(session_id, run_id, req)` | 기본 키 기반 편집을 bound parameters로 실행하고 전부 롤백. read-only는 Auth, capability 미지원은 Unsupported |
| `edit_apply(session_id, run_id, req)` | 같은 세션·요청의 60초 이내 성공 미리보기 필요(일회용). 한 트랜잭션; Update/Delete 영향 수가 1이 아니면 Conflict와 전체 롤백 |
| `execute_stream(session_id, run_id, req, on_event)` | 같은 실행·취소·이력 경로를 쓴다. 그래프와 legacy 경로의 작은 표는 기존 ResultSet 배열을 반환한다. 협상된 PG의 row_limit>2,000 요청은 최종 행 수가 작더라도 즉시 Channel로 보내고 빈 배열을 반환한다 |
| `cancel(run_id)` | 탭을 닫거나 사용자가 취소할 때 호출 |
| `expand(session_id, element_id, limit)` | |
| `history_list(limit)` | `history.jsonl`: 시각, 프로필명, 쿼리 텍스트, 소요 시간, 행 수, 오류 요약. 파라미터 값과 secret은 기록하지 않는다 |

### 드라이버·업데이트·내보내기 명령

| command | 역할 |
|---|---|
| `drivers_list` | **설치된** 드라이버의 manifest 목록(설치 폴더와 debug 전용 `AXDB_DRIVER_DIR`) |
| `drivers_catalog` | 캐시된 서명 목록과 설치 상태를 즉시 반환하고, 백그라운드에서 `index-v2.json`을 ETag로 갱신하고, 404/410일 때만 `index.json`으로 폴백한 뒤 `drivers-catalog-updated` 이벤트를 보낸다 |
| `driver_install({source, allowUnverified?, onProgress})` | 카탈로그 id 또는 로컬 `.axdriver` 경로로 설치. `onProgress` 채널에 `download → verify → signature → extract → handshake → register → complete` 단계와 bytes, 전송 속도를 보낸다 |
| `driver_update / driver_remove / driver_pick_file / driver_cancel` | 업데이트, 삭제(열린 세션이 있으면 거부), 파일 선택, 진행 중 설치 취소(커밋을 시작한 뒤에는 거부) |
| `updater_settings / updater_configure` | 업데이트 자동 확인 토글과 건너뛴 버전(`settings.json`의 `updater`) |
| `onboarding_status / onboarding_complete / onboarding_mcp_config` | 첫 실행 안내 상태와 MCP 클라이언트 설정 JSON |
| `export_save_dialog / export_write / export_reveal` | 저장 대화상자에서 고른 경로에 묶인 일회용 토큰으로만 파일을 쓴다. 대상 symlink는 거부하고 임시 파일 → rename으로 저장한다 |

세션 오류 중 `ErrorKind::Connection`(네트워크 끊김 등)이 나면 호스트는 세션을 dead로 표시한다. UI는 재연결 버튼을 보여 준다.

로그는 `tracing`을 쓴다. 연결 필드 중 secret인 것, 연결 URI, 파라미터 값은 로그에 남기지 않는다. 검증 스크립트가 로그와 설정 파일을 grep해 이를 확인한다.

## 결과 뷰어와 계측

큰 표는 `Channel<Response>`의 raw MessagePack 바이트로 `start → rows* → end`를 결과 집합별로 보내고 마지막에 `done`으로 끝난다. row_limit≤2,000 요청·legacy 작은 결과와 그래프의 직접 invoke 반환은 유지한다. 협상된 PG의 큰 행 예산 요청은 실제 결과가 작더라도 즉시 스트리밍한다. 첫 청크는 200행, 이후 2,000행이며 컬럼별 공통 태그와 값 배열을 사용한다. 혼합 타입은 셀 태그를 유지하고 정수·decimal·JSON 원문은 문자열로 보존한다. UI는 결과 인덱스·행 offset·폭을 검증하고 rAF 단위로 반영한다. 동일 태그의 큰 열은 검증된 인코딩 버퍼와 scalar offset으로 보관하고 보이는 셀만 디코드한다. 수신한 열 청크는 그대로 보관하고, 기존 행 배열 인터페이스는 조회한 행만 셀 객체로 만드는 읽기 전용 Proxy로 제공한다. 정렬은 열 값을 직접 읽으며 완료·실패 시 Channel onmessage의 결과 참조를 끊는다. PG는 handshake의 `capabilities.execute_stream`을 협상한 경우 선택 `execute_stream` 메서드를 사용한다. `stream_start`는 최종 행 수가 미정인 컬럼 메타데이터이고, `rows*` 뒤 `stream_end`가 실제 행 수·elapsed·truncated를 확정한다. 구 드라이버 및 작은 요청(row_limit≤2,000)은 기존 `execute`와 start/rows 재조립을 유지한다. 실행이 시작된 선택 메서드를 legacy 실행으로 재시도하지 않는다.

표는 행·열을 가상화하고 보이는 셀만 포맷해 캐시한다. 정렬·필터는 로드된 원본 행을 복사하지 않고 인덱스 배열을 만든다. 그래프 레이아웃은 Worker에서 수렴시킨 뒤 Float32Array로 전달하고, Canvas는 입력·크기 변경·데이터 변경 때만 그린다. 기존 노드는 확장 시 고정하고 새 노드만 부모 근처에서 200ms 동안 이동한다. 숨긴 뷰와 비활성 탭은 유지하되 Worker와 렌더를 멈춘다. 첫 표시와 Reset은 fit-to-view이며, 다중·역방향 관계는 곡선으로 분리하고 작은 줌에서는 캡션·화살표를 생략한다.

`window.axdb.perf`는 최대 2,000개 측정값과 `marks/clear/run`을 제공하며 `get_perf_metrics` 에이전트 도구는 읽기만 한다. 실제 DB 벤치는 `AXDB_BENCH=1`일 때만 동적으로 로드하고 테스트 접속값은 `bench/fixtures.json`에서 호스트가 검증해 제공한다. 일반 앱에는 벤치가 자동 실행되지 않는다. 원자료와 측정의 한계는 [VIEWER_PERF.md](../bench/VIEWER_PERF.md)에 있다.

## 드라이버 프로세스 (`plugin_host`, `driver-protocol`, `driver-sdk`)

드라이버는 독립 실행 파일이다. 호스트의 `PluginDriver`/`PluginSession`이 `dbcore::Driver`/`Session`을 구현해 RPC로 대리 호출하므로 UI·권한·미리보기·MCP·AI 계층은 드라이버가 프로세스 안인지 밖인지 모른다.

- **실행 모델:** 로컬 드라이버 검색과 프로브는 Host 생성에서 제외하고 Registry를 처음 읽을 때 수행한다. 드라이버당 프로세스 하나를 처음 연결할 때 지연 실행하고, 그 안에서 세션 ID로 여러 연결을 다중화한다. 마지막 세션 종료 후 5분 단발 타이머로 프로세스를 종료하고 wait로 회수하며, 그 전에 연결하면 타이머를 취소한다. 다음 연결은 새 프로세스로 시작한다. 설치만 한 드라이버는 프로세스를 띄우지 않는다.
- **프로토콜(protocol 1):** big-endian u32 길이 + MessagePack 본문. 요청 `{id, method, params}`, 응답 `{id, result|error}`, 실행 청크 `{id, chunk}`, 취소 `{cancel: id}`. 프레임 하나는 최대 16 MiB, 일반 요청 하나의 전체 응답은 최대 256 MiB이며 초과분을 조용히 잘라내지 않고 연결을 끊는다. `backup_stream`의 검증된 64 KiB 이하 byte 청크는 누적 응답 한도에서 제외하고, 8칸 채널과 파일 sink로 backpressure를 유지한다. 핸드셰이크의 `protocol`이 다르면 "드라이버 업데이트 필요"로 거부한다.
- **스트리밍:** `Session::execute_stream`을 `PluginSession`이 재정의한다. PG의 선택 `execute_stream`은 같은 세션 mutex·읽기 전용·취소·타임아웃·정리 경로에서 `simple_query_raw`와 200/2,000행 FETCH 커서를 사용하고, async sink가 SDK의 기존 8프레임 채널을 await한다. 텍스트 타입 변환을 재사용한다. 청크는 약 256KiB를 기준으로 나누며 단일 큰 셀에는 기존 16MiB 프레임 상한이 적용된다. DML RETURNING/SHOW/EXPLAIN은 raw stream을 끝까지 drain한다. 결과 인덱스·행 폭·최종 행 수를 호스트가 검증하고 LiveResults가 최종 metadata를 UI end에 반영한다. 다른 드라이버와 구 host의 `execute`는 기존 수집 후 청크 경로다.
- **취소와 크래시:** 취소는 해당 요청의 `CancelToken`으로 전달한다. DB 롤백 정리 3초와 전송 유예 3초 안에 응답이 없으면 프로세스를 종료하고 그 드라이버의 세션을 dead로 표시한다. 다음 연결 때 프로세스를 다시 띄우며, 기존 세션이나 쿼리를 재실행하지 않는다. 다른 드라이버는 영향을 받지 않는다.
- **비밀:** 비밀번호는 `ConnectConfig.secret`으로만 전달한다(argv·환경 변수·프로필에 넣지 않음). 드라이버 stderr는 호스트가 로그로 받으며 기존 redaction을 적용하고 8 KiB를 넘는 줄은 버린다. 자식 프로세스 환경에서 서명·AI·테스트 비밀은 제외한다.
- **백업 파일 스트리밍:** `backup_create_file`은 저장 대화상자의 일회용 path/token으로만 쓴다. 드라이버 `backup_stream` → 기존 `{id,chunk}` envelope → 호스트 zstd writer가 전체 payload를 수집하지 않고 SHA-256을 계산하며, manifest는 `.axbackup` v2 trailer에 둔다. 취소·실패 시 옆의 임시 파일을 삭제하고 기존 파일을 보존한다. `restore_pick_file`은 선택한 일반 파일을 스트리밍 검증하여 비공개 익명 임시 파일에 고정하고 세션에 묶인 토큰만 UI에 반환한다. `restore_file_preview/apply`는 이 토큰·payload digest·overwrite·새 DB 이름에 묶인 일회용 승인 영수증을 쓰며, `restore_start/chunk/finish/abort` 선택 메서드로 64 KiB씩 전송한다. SDK도 익명 파일에 스테이징하고, 실패·취소·세션 종료·업로드 유휴 만료 시 정리한다. 기존 v1 파일과 bounded 단일-frame 메서드는 호환용으로 남긴다. `restore_options`가 덮어쓰기·새 DB 생성 가능 여부와 서버 제약을 제공하므로 UI는 driver-id로 분기하지 않는다.
- **SDK:** `axdb_driver_sdk::serve(driver)` 한 줄이 프레임 읽기·쓰기, 세션 다중화, 취소 토큰 연결을 맡는다. 드라이버 프로세스도 앱과 같은 mimalloc 설정으로 해제한 메모리를 OS에 돌려준다.

## 드라이버 패키지와 설치 (`driver-package`, `driver_install`)

- **`.axdriver`:** zip. `manifest.json`(id, name, version, protocol, description, paradigm, homepage, license, 플랫폼별 경로와 sha256), `bin/<target-triple>/<exe>`, `manifest.sig`(manifest의 Ed25519 서명). 대상 플랫폼은 `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`다.
- **신뢰:** 앱에 공식 공개 키를 내장한다(테스트와 키 교체용으로 `AXDB_OFFICIAL_DRIVER_PUBLIC_KEY`, `AXDB_PREVIOUS_DRIVER_PUBLIC_KEY`로 덮어쓸 수 있다). 서명이 없거나 알 수 없는 키인 로컬 패키지는 사용자가 명시적으로 동의해야 설치된다.
- **레지스트리:** GitHub Releases의 `drivers-index` 릴리스에 구버전 스키마의 `index.json`(v1)과 항목별 `icon_key`를 포함한 `index-v2.json`을 각각 같은 키로 서명해 둔다. 패키지 manifest는 v0.2.0 호환을 위해 `icon_key`를 직렬화하지 않으며, 설치 후 아이콘은 handshake의 `DriverManifest`로 전달한다. 패키지는 `drivers-<id>-v<버전>` 릴리스의 `<id>-<버전>.axdriver`다. 드라이버 릴리스는 `--latest=false`로 만들어 `releases/latest`가 앱 릴리스를 가리키게 유지한다.
- **앱 시작 시 네트워크 호출은 없다.** 설치된 드라이버는 앱 데이터의 `drivers/<id>/<version>/`에서만 읽고, `drivers/<id>/current.json`이 활성 버전을 가리킨다. 드라이버 화면을 열면 캐시한 목록을 바로 보여 주고 `index-v2.json`을 먼저 조회하며 404/410일 때만 v1으로 폴백한다. ETag는 캐시의 원본 URL에만 보내고, 빌드 시 오프라인 스냅숏은 v2를 사용한다.
- **설치 단계:** 다운로드 → 패키지 해시 → manifest 서명과 실행 파일 해시 → 임시 폴더에 압축 해제 → 폴더 rename(원자적) → `PluginDriver::load_cancelable` 핸드셰이크(manifest의 id·이름·paradigm 일치 확인) → 활성 표시 → Registry 등록. 활성화 전에는 기존 버전이 그대로 동작한다. 이전 버전은 롤백용으로 1개만 보관한다.
- **한도:** 운영 환경에서는 HTTPS만 허용하고 리다이렉트 3회, 요청 90초, 패키지 256 MiB, 압축 해제 후 512 MiB, 목록 2 MiB로 제한한다. 경로 탈출, 중복·예상 밖 항목, symlink, 특수 파일은 거부한다.
- **빌드와 서명:** `cargo xtask package-drivers`가 대상별 크로스 빌드, (macOS) Developer ID 서명·공증, 패키지, Ed25519 서명, `index.json`·`index-v2.json` 생성을 맡는다. 서명 개인 키는 키체인(`axdb-driver-signing`)이나 `AXDB_DRIVER_SIGNING_KEY`로만 받는다. `scripts/publish-drivers.sh`가 검증한 산출물을 올린다.

## 앱 업데이트 (`updater`)

Tauri updater로 서명된 업데이트를 받는다. 공개 키는 `src-tauri/tauri.conf.json`에 있고 피드는 `https://github.com/sangwookp9591/db-tool/releases/latest/download/latest.json`이다. 시작 10초 뒤 유휴 시점과 이후 24시간마다 자동 확인하며(`settings.json`의 `updater.auto_check`, 기본 true), debug 빌드·`AXDB_BENCH`·`AXDB_AGENT_BRIDGE=1` 실행에서는 자동 확인을 하지 않는다. 설치와 다시 시작은 사용자가 버튼을 눌렀을 때만 진행한다. 릴리스 산출물과 `latest.json`은 `scripts/release.sh`가 만든다([RELEASE_SIGNING.md](RELEASE_SIGNING.md)).

## 새 DB 추가 절차

[PLUGIN_GUIDE.md](PLUGIN_GUIDE.md)를 참고한다. crate를 하나 만들고 `Driver`를 구현한 뒤 `src/bin/axdb-driver-<id>.rs`에서 `axdb_driver_sdk::serve`를 호출한다. 호스트와 **UI는 수정하지 않는다.** 공식 패키지로 배포하려면 `xtask`의 드라이버 목록에도 항목을 추가한다.

### 변경 미리보기 경계

PG·SQLite·Neo4j는 `Transactional`을 선언한다. PG는 BEGIN/ROLLBACK(기존 사용자 트랜잭션은 SAVEPOINT/ROLLBACK TO/RELEASE), SQLite는 SAVEPOINT/ROLLBACK TO/RELEASE(열린 사용자 트랜잭션에서는 interrupt가 외부 트랜잭션까지 되돌릴 수 있으므로 실행 전 거부), Neo4j는 명시적 Bolt 트랜잭션과 rollback을 쓴다. 샘플은 기본 50행, 드라이버/호스트 최대 1,000행, 에이전트 요청 최대 200행이다. SQL 트랜잭션/세션 제어문과 비트랜잭션 작업은 실행 전에 거부하고 이유를 notes에 담는다. preview는 쓰기 승인과 별개이며 적용은 기존 execute 경로를 사용한다. 시퀀스·외부 함수·트리거 부작용, 샘플 제한과 잠금 시간은 [C1 보고서](C1_PREVIEW_REPORT.md)를 따른다.
