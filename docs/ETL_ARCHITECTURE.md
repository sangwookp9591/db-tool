# ETL 아키텍처

결정 배경과 언어 비교는 [ADR-0005](adr/0005-etl-engine.md)에 있다. 이 문서는 구성 요소, 데이터 흐름, 계약, 작업 순서를 정한다.

## 구성

```
ui/src/ax/EtlPipelines.tsx        워크벤치 "데이터 파이프라인" 페이지: 목록 · 편집 · 미리보기 · 실행
        │ api('etl_*')
src-tauri/src/etl.rs              Tauri 명령, 세션 어댑터(SessionExtractor / SessionLoader),
        │                         미리보기 영수증, 진행 이벤트, 저장(etl/pipelines.json, etl/runs.jsonl), 이력
        │ axdb_etl::engine::{preview, run}
crates/etl (axdb-etl)             spec(명세·검증) · transform(컴파일된 행 변환) · engine(배치 실행기)
        │ dbcore::Session (browse / execute / import_rows)
plugin_host → axdb-driver-<id>    기존 드라이버 프로세스. 드라이버 변경 없음
```

의존 방향: `src-tauri → axdb-etl → dbcore`. 엔진은 Tauri·tokio 런타임·파일 시스템을 모른다. 타임아웃과 세션 접근은 호스트 어댑터가 맡는다.

## 데이터 흐름

1. **추출(Extract)**
   - `Source::Table`: `browse(node_id, offset, limit=batch_size)`를 `has_more`가 false가 될 때까지 반복한다(`paged_browse` 필수). 메모리는 배치 하나 분량이다.
   - `Source::Query`: `execute(read_only=true, row_limit=min(max_rows, 100_000))`를 한 번 실행하고 메모리에서 배치로 나눈다(`read_only_exec` 필수). 첫 표 결과만 쓰며, 잘렸으면 `source_truncated=true`.
2. **변환(Transform)**: 첫 배치의 컬럼으로 단계를 한 번 컴파일해 컬럼 이름을 인덱스로 바꾼다. 이후 배치의 컬럼 구성이 다르면 실패한다. 행 오류는 `on_error`에 따라 중단(`abort`, 기본) 또는 건너뛰기(`skip_row`, 표본 20개 보관).
3. **적재(Load)**: 변환된 배치를 `import_rows`(배치당 1..10,000행, 배치당 한 트랜잭션)로 쓴다. `create_table`이면 첫 배치에 출력 컬럼으로 테이블을 만든다(타입: 명시적 cast 우선, 없으면 표본 값으로 integer/float/boolean/json/text 추론).
4. **승인**: `etl_preview`가 표본(최대 200행)을 변환하고 대상에서 rollback 미리보기를 실행한 뒤, `fingerprint("etl:{source_session}:{target_session}", pipeline)`로 60초 일회용 영수증을 남긴다. `etl_run`은 같은 지문의 영수증을 소비해야 시작한다. 명세를 바꾸면 다시 미리보기해야 한다.
5. **취소**: `run_id`의 CancelToken을 배치 사이와 `import_rows` 호출에 전달한다. `browse`에는 취소 인자가 없으므로 진행 중인 페이지 하나는 끝까지 기다린다(세션을 끊지 않는다).

## 변환 단계(`Step`, `op` 태그)

| op | 동작 |
|---|---|
| `select` | 컬럼 선택·순서·이름 변경(같은 컬럼을 두 번 선택 가능) |
| `rename` / `drop` | 이름 변경 / 제거(전부 제거는 거부) |
| `cast` | text·int·float·decimal·bool·date·date_time·json. NULL 유지, 실패는 행 오류 |
| `filter` | eq·ne·lt·le·gt·ge·is_null·not_null·contains·starts_with. NULL 비교는 unknown(제외). 여러 개는 AND |
| `constant` / `fill_null` | 상수 컬럼 추가·덮어쓰기 / NULL 대체 |
| `text` | trim·upper·lower |
| `concat` | 입력 컬럼의 텍스트를 구분자로 합침(NULL 생략) |
| `dedupe` | 키 컬럼이 이미 나온 행 제외(실행당 키 1,000,000개 상한) |

## 호스트 명령

| command | 역할 |
|---|---|
| `etl_list / etl_save / etl_delete` | 명세 저장. 저장 시 `validate()`, id가 없으면 uuid 부여, 최대 200개 |
| `etl_preview(pipeline, sourceSessionId, targetSessionId, runId, sampleRows?)` | `PreviewReport` 반환, 영수증 기록, 이력 kind `etl_preview` |
| `etl_run(pipeline, sourceSessionId, targetSessionId, runId)` | 영수증 소비 후 실행, `etl-progress` 이벤트, `RunReport` 반환, `etl/runs.jsonl`에 기록(최근 500개), 이력 kind `etl_run` |
| `etl_runs(limit?)` | 최근 실행 기록 |

공통 검사: 세션의 프로필 id가 명세의 `profile_id`와 일치해야 한다. 대상 접속이 `ReadOnly`면 Auth 오류, 대상 드라이버에 `import_rows`가 없으면 Unsupported. 값과 쿼리 파라미터는 이력·로그·실행 기록에 남기지 않는다.

## 실패 의미

배치마다 커밋하므로 중간 실패 시 앞선 배치는 반영된 상태다. `RunReport`는 `batches_committed`, `rows_written`, `error`를 남기고 UI는 "N개 배치가 이미 반영됨"을 표시한다. 연결 오류로 끝나면 마지막 배치의 반영 여부를 확인하라고 안내한다(기존 import와 같은 문구).

## 작업 순서(작은 커밋 단위)

1. 설계 문서(ADR-0005, 이 문서)
2. `axdb-etl` spec: 명세 직렬화·기본값·검증 테스트
3. `axdb-etl` transform: 단계별 동작·컴파일 오류·cast·비교 테스트
4. `axdb-etl` engine: 배치·건너뛰기/중단·적재 실패·취소·max_rows·미리보기 테스트(가짜 Extractor/Loader)
5. 호스트 `etl.rs`: 어댑터와 명령. SQLite→SQLite 실제 드라이버 테스트(영수증 필수, 읽기 전용 대상 거부, 프로필 불일치 거부, create_table, 테이블·쿼리 소스)
6. UI 계약·mock·워크벤치 페이지와 단위 테스트

## 알려진 제약 (테스트·측정으로 확인)

- **64바이트를 넘는 바이너리 값은 복사할 수 없다.** `browse`의 `Value::Bytes`는 앞 64바이트의 hex만 담는다. 잘린 값은 적재 단계에서 거부되어 실행이 실패하며, 조용히 잘린 채 쓰이지 않는다(`etl_migration::long_binary_values_fail_instead_of_being_truncated`).
- **offset 페이지 조회는 행 수의 제곱에 비례해 느려진다.** 100만 행 SQLite 배치 1,000에서 10만 행 대비 처리량이 절반 이하로 떨어졌다. 큰 테이블은 배치 5,000 이상을 쓴다. 근본 해결은 드라이버 keyset 커서다([측정](research/etl-benchmark.md)).
- **쓰기 모드는 append뿐이다.** 채워진 테이블에 다시 실행하면 PK 중복으로 첫 배치에서 실패하고 부분 행은 남지 않는다. upsert·truncate·워터마크 증분은 아직 없다.
- **조인·집계·정렬 단계가 없다.** 소스 쿼리의 SQL로 대신한다(쿼리 소스는 100,000행 상한).

## 경쟁 도구 대비 다음 우선순위

[경쟁 조사](research/etl-competitors.md)의 격차 상위 항목을 데스크톱 사용 기준으로 정렬했다.

1. 쓰기 모드(upsert/truncate)와 워터마크 증분 추출 — 반복 실행이 일상인 DB 도구에서 가장 자주 부딪힌다.
2. 거부 행 보관(파일·테이블)과 재처리 — 지금은 표본 20개만 남는다.
3. 단계별 표본 미리보기 — 변환 설계 중 바로 확인.
4. keyset 커서 조회와 적재 요청의 열 단위 인코딩 — 측정된 두 병목.
5. 명세 파일 내보내기(Git 친화)와 앱 실행 중 스케줄.

AxDB만의 차별점은 실행 전 대상 롤백 미리보기와, 명세·세션 쌍에 묶인 일회용 승인이다. 조사한 6개 도구에는 같은 기능이 없다.

## 이후 단계

- 스케줄(앱 실행 중에만)과 파이프라인 의존성(DAG)
- 파일 소스·대상(CSV/JSONL 스트리밍), 워터마크 기반 증분 추출
- DataFusion 기반 SQL 변환 단계(의존성 크기 측정 후)
- MCP 에이전트 도구(`plan_etl`: 미리보기만, 실행은 사람 승인)
