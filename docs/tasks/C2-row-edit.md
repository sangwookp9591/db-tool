# C2-row-edit: 표 편집 → 미리보기 → 적용 (계약 + 드라이버 + 호스트 + UI)

이 작업은 반드시 끝까지 완료한다. 합리적인 가정으로 계속 진행하고, 결정한 내용은 마지막 보고에 적는다.

## 목적
STATUS(v2 계획) Wave 3의 마지막 항목이다. 지금은 모든 드라이버가 `edit_rows: false`다. 탐색(browse)으로 연 표에서 셀 수정·행 추가·행 삭제를 모아 두고, C1의 미리보기로 영향을 확인한 뒤, 하나의 트랜잭션으로 적용한다. "편집 → 미리보기 → 확인 → 적용"을 벗어난 쓰기 경로는 만들지 않는다.

## 고정 계약 (두 워커가 같이 쓴다. 이름·직렬화 형태를 바꾸지 않는다)

### Rust (`crates/dbcore`, 가산적 변경, 기존 JSON 호환)
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditTarget { pub node_id: String, pub key_columns: Vec<String> }

// ResultSet에 추가. #[serde(default)]. browse 결과가 기본 키로 행을 식별할 수 있을 때만 Some.
pub edit_target: Option<EditTarget>,

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

// Session 트레이트, 기본 구현은 Unsupported
async fn preview_edits(&self, req: &EditRequest, cancel: CancelToken) -> Result<ChangePreview, DbError>;
async fn apply_edits(&self, req: &EditRequest, cancel: CancelToken) -> Result<EditResult, DbError>;
```
(`CancelToken`은 기존 `execute`/`preview`와 같은 타입. `ErrorKind::Conflict`는 가산적으로 추가하고 TS `ErrorKind`에도 반영한다.)

### TypeScript (`ui/src/contract.ts`, `ui/src/api.ts`)
- `ResultSet.edit_target?: { node_id: string; key_columns: string[] } | null`
- `RowEdit = { op: 'Update'; key: [string, Value][]; set: [string, Value][] } | { op: 'Insert'; values: [string, Value][] } | { op: 'Delete'; key: [string, Value][] }`
- `EditRequest { node_id; edits; timeout_ms?; preview_sample_limit? }`, `EditResult { affected: number[]; statements: string[] }`
- Tauri commands: `edit_preview(sessionId, runId, req) -> ChangePreview`, `edit_apply(sessionId, runId, req) -> EditResult` (인자 이름은 기존 `preview` command와 같은 camelCase 규칙)

## 의미 규칙
- 행 식별은 기본 키만 쓴다. 기본 키가 없는 테이블·뷰·조인 결과는 `edit_target: None`이고 UI는 편집을 숨긴다. PG `ctid`, SQLite `rowid` 같은 물리 식별자는 쓰지 않는다(보고서에 근거를 적는다).
- 값은 **반드시 바인딩 파라미터**로 보낸다. 식별자(스키마·테이블·컬럼)는 드라이버 규칙으로 인용하고, `node_id`와 컬럼 이름은 실제 카탈로그에 있는 것만 허용한다(없으면 오류). 문자열 이어붙이기로 값을 SQL에 넣지 않는다.
- `Value` → 컬럼 타입 변환은 드라이버가 맡는다(예: PG는 텍스트 바인딩 + `::type` 캐스트 등). 변환할 수 없으면 해당 편집의 위치를 담은 Query 오류.
- `apply_edits`: 한 트랜잭션. Update/Delete는 각각 정확히 1행이어야 한다. 0행이나 2행 이상이면 전체 롤백하고 `ErrorKind::Conflict`(없으면 가산적으로 추가)를 반환하며 메시지에 편집 순번을 넣는다. 취소·타임아웃·오류는 전부 롤백. 정리 실패 시 연결 폐기(C1과 같은 방식).
- `preview_edits`: 같은 문장을 C1 미리보기 경로(롤백 보장)로 실행해 `ChangePreview`를 돌려준다. 문장 텍스트는 파라미터 자리표시자가 보이는 형태로 둔다.
- 사용자가 연 트랜잭션 안에서는(PG) SAVEPOINT로 적용하고, SQLite는 C1처럼 열린 사용자 트랜잭션에서 거부한다.
- 편집 수 상한 1,000개. 초과하면 Unsupported.

## 호스트 (`src-tauri`)
- `edit_preview`, `edit_apply` command 등록. 기존 run/취소/타임아웃/observe 패턴을 재사용한다.
- 읽기 전용 연결(`access_mode == ReadOnly`)이면 둘 다 Auth 오류. capability `edit_rows == false`면 Unsupported.
- `edit_apply`는 **60초 안에 같은 요청 지문으로 성공한 `edit_preview`가 있어야** 실행한다(기존 `host.previews` 지문 저장 방식 재사용, 한 번 쓰면 소거). 없으면 Auth 오류 "Preview required".
- 이력: `kind: "edit_preview"` / `"edit_apply"`, text는 생성된 문장(자격 증명 redaction 유지).
- 에이전트 도구는 이번 범위가 아니다(추가하지 않는다).

## 드라이버
- PostgreSQL, SQLite: `edit_rows: true`, browse가 테이블에 대해 `edit_target`을 채운다.
- Neo4j, MongoDB, Redis, KV 예제: `edit_rows: false` 유지, 변경 없음.

## UI (`ui/`)
- 조건: `capabilities.edit_rows && result.edit_target` 이고 연결이 읽기 전용이 아닐 때만 결과 표 툴바에 "편집" 토글. 드라이버 id로 분기하지 않는다(grep 0 유지).
- 편집 모드: 셀 더블클릭/Enter로 입력, Esc 취소, NULL 지정, 행 추가(빈 행), 행 삭제 표시. 수정된 셀·추가 행·삭제 행을 시각적으로 구분하고(디자인 v3 토큰 사용, `docs/DESIGN.md`), 대기 중 변경 수를 표시. 키 컬럼은 기존 행에서 편집 불가.
- 입력 문자열 → `Value`는 컬럼 `type_name`과 원래 값의 태그(`t`)를 기준으로 변환한다. 변환 규칙은 `ui/src/values.ts` 근처의 순수 함수로 두고 vitest로 검증한다.
- "변경 미리보기" → `edit_preview` → 기존 C1 미리보기 표시 컴포넌트를 재사용(필요하면 공용 컴포넌트로 추출). "적용"은 미리보기 성공 후에만 활성, 확인 대화상자(기존 Dialog) 뒤 `edit_apply`, 성공 시 대기 변경 비우고 현재 페이지 다시 browse. 실패(Conflict 포함)는 변경을 보존한 채 오류 표시. 미리보기 후 편집을 더 바꾸면 적용 버튼은 다시 비활성.
- `ui/src/mock.ts`: 브라우저 mock에서도 흐름이 보이도록 한다(mock임을 명시).
- 가상화 표 성능(W4: 10만 행 첫 표시, 스크롤)에 회귀를 만들지 않는다. 편집 모드가 꺼져 있을 때 VirtualTable 렌더 경로는 기존과 같아야 한다.

## Constraints
- 사용자 DB 접속 금지. 테스트 DB만: PG `host=127.0.0.1 port=55432 user=postgres password=testpw dbname=axdb_test`, SQLite 임시 파일. (Neo4j 57687은 회귀용)
- 기존 테스트·E2E 회귀를 깨지 않는다. 공유 파일은 수정 직전에 다시 읽는다.
- git commit 하지 않는다(코디네이터가 단계별로 커밋). 완료 시 `--files-modified` 정확히.

## Observable acceptance
- 백엔드: 실제 PG·SQLite 통합 테스트 — Update/Insert/Delete 적용 후 재조회로 반영 확인, 미리보기 후 미반영 확인, 0행/다중행 Conflict 시 전체 롤백 확인, 잘못된 컬럼·테이블 거부, 주입 시도 문자열(`'; DROP TABLE x; --`, 따옴표·백슬래시·유니코드)이 값으로 그대로 저장됨, 취소/타임아웃 뒤 세션 재사용, 읽기 전용 연결 거부, 미리보기 없는 apply 거부(호스트 테스트). `cargo test --workspace`(AXDB_TEST_PG, AXDB_TEST_NEO4J), `cargo clippy --workspace --all-targets -- -D warnings` 통과.
- UI: `pnpm --dir ui exec tsc --noEmit`, `pnpm --dir ui test`, `pnpm --dir ui build` 통과, 값 변환·대기 변경 상태 vitest, mock 브라우저 화면 확인(스크린샷 경로 보고).
- 보고서 `docs/C2_EDIT_REPORT.md`(백엔드 절/UI 절 각자 작성).
