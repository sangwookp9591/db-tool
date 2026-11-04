# ADR-0005: ETL 엔진 언어와 아키텍처

- 상태: 채택 (2026-11-04)
- 결정자: 사용자(요구), 코디네이터(설계)
- 관련: [ADR-0003](0003-installable-drivers.md), [ETL_ARCHITECTURE.md](../ETL_ARCHITECTURE.md)

## 배경

사용자 요구: "해당 시스템에 ETL 기능 추가. ETL 엔진은 Rust, Go, Zig 중 가장 적절한 언어를 채택하고 아키텍처를 설계한 뒤 작업 진행."

참고로 제시된 도구는 Airflow(코드 정의 DAG + 스케줄), NiFi(실시간 흐름·라우팅), Apache Hop / PDI(Kettle) / Talend(시각적 변환 단계), Kestra(YAML 선언 + 캔버스, GitOps)다. AxDB는 서버가 아니라 **단일 사용자 데스크톱 앱**이고, 이미 6종 드라이버(PG, MySQL, SQLite, MongoDB, Neo4j, Redis)를 별도 프로세스로 띄워 `dbcore::Session`으로 추상화한다.

## 언어 비교 (Context7 문서 조회 근거, 2026-11-04)

| 기준 | Rust | Go | Zig |
|---|---|---|---|
| 기존 코드와 통합 | workspace 전체가 Rust 2024 + tokio. `dbcore::Session`, `plugin_host`, 권한·미리보기 영수증·이력을 **같은 프로세스에서 직접 호출** | 별도 sidecar 프로세스 + 새 IPC 계약이 필요. 드라이버 접근을 위해 host RPC를 다시 노출하거나 Go 드라이버를 새로 작성 | sidecar + C ABI. DB 드라이버 생태계가 사실상 없음 |
| 데이터 처리 생태계 | Apache Arrow(arrow-rs), DataFusion: `SessionContext::register_batch`, `DataFrame::execute_stream`으로 `SendableRecordBatchStream` 점진 실행(`/apache/datafusion`) — 향후 SQL 변환 단계로 확장 가능 | Redpanda Connect(Benthos): input → processors → output YAML, `max_in_flight`·batching이 성숙(`/redpanda-data/connect`). 단, 서버형 스트림 처리기 | 0.16 문서 기준 async 함수는 0.11에서 퇴행 후 "저수준 primitive로 재도입 예정", inline asm 제약 등 불안정 표기(`/websites/ziglang/0.16.0`) |
| 안정성·메모리 | 소유권으로 큐/버퍼 상한과 취소를 타입으로 강제, `await_holding_lock` deny 정책 이미 적용 | GC. 대용량 배치에서 메모리 상한을 명시적으로 걸기 어렵다 | 수동 메모리 관리, 1.0 이전 언어 변경 |
| 배포 | 앱 바이너리에 포함(추가 실행 파일·서명 없음) | 플랫폼 5종 sidecar 빌드·서명·공증 추가 | 동일하게 sidecar 부담 + 툴체인 추가 |

**결정: Rust.** Go의 Benthos식 파이프라인 모델(입력 → 처리기 → 출력, 배치·in-flight 상한)은 설계 참고로만 차용한다. Zig는 언어 안정성과 드라이버 생태계 부재로 제외한다.

## 결정

1. **새 crate `crates/etl`(`axdb-etl`)**: `dbcore`에만 의존하는 순수 엔진. 파이프라인 명세(serde, JSON/YAML 호환 구조), 검증, 변환 연산자, 배치 실행기, `Extractor`/`Loader` trait를 둔다. Tauri·파일 시스템에 의존하지 않으므로 단위 테스트가 빠르다.
2. **호스트 어댑터 `src-tauri/src/etl.rs`**: 열린 세션을 `Extractor`/`Loader`로 감싸고, 기존 경계를 그대로 재사용한다.
   - 소스: 쿼리는 `read_only: true` 실행(`read_only_exec` 필수), 테이블은 `browse` 페이지 추출(`paged_browse` 필수).
   - 대상: 드라이버 `import_rows`(배치당 1..10,000행, 배치당 한 트랜잭션), 접속 `ReadOnly`면 거부.
   - 승인: `etl_preview`가 대상에 rollback 미리보기를 실행하고 명세 지문(fingerprint)에 묶인 **60초 일회용 영수증**을 남긴다. `etl_run`은 영수증 없이는 거부한다(기존 import/edit와 같은 규칙).
   - 취소·이력: `host.run(run_id)` 토큰, `cancel(run_id)`, `history.jsonl`(kind `etl_preview`/`etl_run`, 값 미기록).
3. **저장**: 파이프라인 명세는 앱 데이터 `etl/pipelines.json`(secret 없음, 프로필 id만 참조), 실행 기록은 `etl/runs.jsonl`(상한 500줄).
4. **UI**: "데이터 파이프라인" 화면. Hop/PDI식 단계 목록 편집(소스 → 변환 단계들 → 대상), 미리보기(변환 샘플 + 대상 rollback 미리보기), 실행 진행률·취소. UI는 driver id로 분기하지 않고 capability만 본다.

## 범위와 단계

- **1단계(이번 작업)**: 단일 소스 → 선형 변환 → 단일 대상, 수동 실행, 미리보기 영수증, 진행률·취소, 실행 기록.
- **2단계**: 파이프라인 간 의존성(DAG)과 앱 실행 중 스케줄(Airflow/Kestra의 스케줄 개념). 앱이 꺼져 있으면 실행하지 않는다는 점을 명시.
- **3단계**: 파일 소스/대상(CSV·JSONL 스트리밍), 증분 추출(워터마크 컬럼), DataFusion 기반 SQL 변환 단계(의존성 크기 측정 후 결정).
- **범위 밖**: 다중 사용자 서버, 분산 실행, NiFi식 상시 실시간 흐름. 데스크톱 앱의 메모리 상한 원칙과 맞지 않는다.

## 결과

- 장점: 새 프로세스·IPC·서명 대상이 없다. 권한·미리보기·취소·이력 경계를 한 곳에서 재사용한다. 엔진 crate는 Tauri 없이 테스트된다.
- 비용: 배치 단위 커밋이므로 실행 도중 실패하면 앞선 배치는 이미 반영된다. 실행 기록에 커밋된 배치 수와 마지막 오프셋을 남기고, UI에서 이를 명시한다.
- 제약: 쿼리 소스는 `ResultSink`가 동기라 backpressure를 걸 수 없으므로 `row_limit`(최대 100,000) 안에서 한 번에 추출한다. 큰 데이터는 테이블 소스(페이지 추출)를 쓴다.
