# ADR-0001: Tabularis 포크 대신 독립 구현 (동일 스택, 다형 결과 계약)

- 상태: 채택 (2026-10-15)
- 결정자: 코디네이터 (Opus 5.5)

## 조사한 사실 (소스 직접 확인)

Tabularis `ba22659` (2026-10-14, v0.28.0), `scratchpad/ref/tabularis`:

| 항목 | 확인 내용 | 근거 |
|---|---|---|
| 라이선스 | Apache-2.0 | `LICENSE` |
| 스택 | Tauri 2 + Rust(sqlx, tokio-postgres) + React 19 + Monaco | `package.json`, `src-tauri/Cargo.toml` |
| 규모 | Rust 84,954줄(270파일), TS/TSX 108,284줄(588파일) | `wc -l` |
| AI 기능 | AI Rust 4,720줄, MCP 2,386줄, AI 관련 TS 약 5,200줄 | `ai*.rs`, `mcp/` |
| 드라이버 계약 | `DatabaseDriver` trait, 메서드 약 70개(대부분 테이블/뷰/루틴/트리거/DDL) | `src-tauri/src/drivers/driver_trait.rs:343-1115` |
| 결과 모델 | `QueryResult { columns: Vec<String>, rows: Vec<Vec<Value>>, … additional_results }` — **표 하나뿐**, 컬럼 타입 정보 없음 | `src-tauri/src/models.rs:507-524` |
| 기능 선언 | `DriverCapabilities` bool 플래그 + `SqlDialect`(SQL 변형만) | `driver_trait.rs:33-181` |
| 외부 플러그인 | 줄 단위 JSON-RPC over stdio, `.tabularium` manifest. Neo4j/그래프/MongoDB 플러그인 없음 | `plugins/PLUGIN_GUIDE.md`, `plugins/registry.json` |
| UI 하드코딩 | 드라이버 문자열 직접 비교 42곳(`NewConnectionModal` 9, `TriggerEditorModal` 8 …) | grep |
| 취소 | tokio task abort. DB 서버 쪽 취소 없음 | `commands.rs:81-119` |
| 문서/코드 불일치 | 가이드 예제의 `total_count` 필드를 호스트가 역직렬화하지 못함 (추정: 예제를 따르면 실패) | `PLUGIN_GUIDE.md:1118` vs `plugins/driver.rs:1199` |

Neo4j Browser (`neo4j/neo4j-browser`): **GPL-3.0**. 코드를 재사용하면 앱 전체가 GPL이 되므로 코드는 가져오지 않고 동작(그래프/표 전환, 이웃 확장, 결과 한도)만 참고한다.

## 판단

**포크했을 때 얻는 것:** 완성된 SQL 클라이언트 UI, keyring 연동, JSON-RPC 플러그인 호스트.

**포크했을 때 치르는 비용:**
1. 결과 모델이 표 하나라서, 그래프/문서/키-값 결과를 넣으려면 `QueryResult`, TS 타입, 결과 뷰, 내보내기, 노트북, AI 컨텍스트 등 표를 가정하는 코어 전반을 고쳐야 한다. 플러그인 추가만으로 끝나지 않는다.
2. 탐색 트리가 database → schema → table/view/routine 계층에 묶여 있다. 라벨·관계 타입·키 공간은 "가짜 테이블"로 위장해야 한다. 요구사항의 "모든 DB를 SQL·테이블 모델에 억지로 맞추지 않는다"에 정면으로 어긋난다.
3. 경량·빠른 실행이 목표인데 약 19만 줄과 AI/MCP/노트북/k8s 터널 등을 함께 유지해야 한다. 빼내려면 그만큼 수정량이 커진다.
4. 업스트림은 하루 단위로 활발히 바뀐다. 코어 결과 모델을 바꾼 포크는 사실상 병합할 수 없게 된다.

**결정:** Tabularis와 같은 스택(Tauri 2 + Rust)으로 **독립 구현**한다. Tabularis의 설계 중 검증된 아이디어(기능 플래그로 UI 제어, keyring 저장, 연결 단위 작업 abort)는 **동작 참고**로만 쓰고 코드는 복사하지 않는다. 새로 정의할 핵심은 둘이다.
- DB 유형별로 다른 결과를 담는 **다형 결과 계약**: 표 / 그래프 / 문서 / 메시지를 동시에 담을 수 있다.
- **드라이버가 정의하는 범용 탐색 트리**: 노드 종류와 동작을 드라이버가 선언한다.

## 결과

- 프런트엔드는 Monaco 대신 CodeMirror 6을, React 대신 Preact를 쓴다. 가벼운 실행을 위한 선택이다.
- 표 가상화와 그래프 캔버스 렌더러는 직접 작성한다. 그래프 배치 계산만 d3-force(ISC)를 쓴다.
- 외부 프로세스 플러그인(JSON-RPC)은 1차 범위에서 제외한다. 드라이버는 같은 워크스페이스의 Rust crate로 만들고 등록 한 줄로 추가한다. 계약 타입이 serde 직렬화 가능하므로 나중에 프로세스 경계로 옮길 수 있다.

## 재사용 / 참고 기록

| 출처 | 라이선스 | 구분 | 내용 |
|---|---|---|---|
| Tabularis | Apache-2.0 | 동작 참고 | 기능 플래그로 UI 분기, keyring crate로 자격 증명 보관, 실행 중 작업 abort |
| Neo4j Browser | GPL-3.0 | 동작 참고만 (코드 미사용) | 그래프/표 결과 전환, 노드 더블클릭 시 이웃 확장, 초기 표시 노드 한도 |
| Beekeeper Studio / DbGate | GPL-3.0 / GPL-3.0 | 미사용 | — |
