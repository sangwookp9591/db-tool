# W2 Neo4j driver

## 구현 범위

- `crates/driver-neo4j`의 `Neo4jDriver`/`Session` 구현 및 workspace 편입.
- 호스트 의존성 추가 및 `src-tauri/src/drivers.rs` 등록 한 줄. 등록으로 목록 순서가 달라져 `src-tauri/src/lib.rs`의 기존 PostgreSQL 테스트 assertion 하나를 순서 무관한 ID 검사로 수정했다. W2에서 UI 파일은 수정하지 않았다.
- Cypher 파라미터(JSON, 문자열을 담은 `{t:"Int",v:"9007199254740993"}` 포함), 표·그래프 동시 결과, List/Map/Path 재귀 수집, ID 중복 제거, 행·그래프 한도와 초과 표시.
- 정수 문자열, 비유한 실수, bytes(앞 64바이트 hex), 날짜/시각/시간대/기간/좌표/노드/관계/경로 변환.
- labels/relationship types/property keys/indexes/constraints의 지연 탐색, 라벨 속성 메타데이터 및 샘플 100개 fallback, 안전한 백틱 식별자, browse와 OpenQuery, limit+1 페이지 및 expand.
- 서버 오류 코드 보존, 인증/연결/쿼리/타임아웃/취소 분류, 비밀번호 오류 redaction.
- 실행마다 전용 연결 풀을 만들어 취소/타임아웃으로 future가 버려지면 소켓도 폐기한다. 정상 한도 도달은 `finish()`의 DISCARD로 정리한다. 다음 요청은 새 연결로 실행한다.
- 쓰기 summary의 0이 아닌 counters를 message로, counters의 합을 affected로 반환한다. affected는 고유 엔티티 수가 아니라 노드/관계/속성/라벨/스키마 변경 counters의 합이다.

## neo4rs 소스 확인 및 선택

- 설치 소스와 [0.8 공식 문서](https://docs.rs/neo4rs/0.8.0/neo4rs/)를 확인했다. 0.8.0에는 RUN READ 모드 및 summary 공개 API가 부족하므로 `=0.9.0-rc.10`과 `unstable-v1`을 고정했다.
- 0.9.0-rc.10 `row.rs`: `Row::keys()`와 `Row::get::<BoltType>()`로 임의 컬럼을 조회한다. `types/serde/typ.rs`: 공개 `IntoDeserializer` 구현을 통해 시간/기간을 읽으며, 기간은 4원소 tuple로 월/일/초/나노초를 보존한다.
- `types.rs`와 `bolt/structs/mod.rs`: Node/Relationship/Path, temporal 및 point를 지원한다. 직접 BoltType 매칭으로 변환하고 Path의 signed relationship index로 방향과 양 끝 노드를 복원한다.
- `version.rs`: 공개 Graph 실행 경로는 Bolt 4.4/4.3/4.1/4.0을 협상한다. `bolt/structs/mod.rs`의 Row용 BoltType 변환에서도 element ID가 보존되지 않는다. 허용된 legacy 정수 ID 문자열 fallback을 사용하며 expand는 `id(n)`을 사용한다. 외부 element ID가 주어지면 `elementId(n)`을 사용한다. 사용자 Cypher 재작성은 하지 않는다.
- `stream.rs`: v2 기능 경로의 `finish()`/`ResultSummary`가 DISCARD 및 counters를 제공한다.
- `graph.rs`/`query.rs`: `execute_read`가 서버에 `mode=r`을 전달할 수 있다.
- `pool.rs`: upstream `log::info!`에 URI가 포함된다. URI 내 credentials는 드라이버에서 거부하고, 호스트 담당자에게 log target 차단을 요청했고 현재 `file_logger`가 `db_tool` target만 허용하도록 반영된 코드를 확인했다. W2는 호스트 로깅을 변경하지 않았다.

## 검증

- `cargo test -p driver-neo4j --lib`: 최종 3개 단위 테스트 통과(bytes/특수 실수/재귀 그래프 한도/정수/기간/식별자).
- `DBTOOL_TEST_NEO4J='bolt://127.0.0.1:57687|neo4j|testpassword' cargo test -p driver-neo4j --test neo4j -- --nocapture`: 실제 컨테이너 통합 테스트 통과(최종 workspace 실행 2.19초). 파라미터·큰 정수·temporal 전종·2D/3D point·그래프/표·counters·한도·오류·500ms timeout·cancel·이후 동일 세션 재실행·expand·explorer·browse·역방향 Path·첫 non-null 컬럼 타입·변경 0건 counters·READ 모드 CREATE 거부와 SELECT 성공을 assert로 검증했다.
- `cargo test -p dbcore -p driver-postgres -p driver-neo4j` (PG/Neo4j 두 환경 변수 설정): 11개 테스트 통과(계약 2, PG 단위 3+실제 통합 2, Neo4j 단위 3+실제 통합 1). PG READ ONLY 검증도 포함한다.
- `cargo test --workspace` (PG/Neo4j 두 환경 변수 설정): 최종 **18개 테스트 통과**(호스트 7, dbcore 2, Neo4j 3+1, PG 3+2; doc tests도 통과). 앞선 실행은 공유 파일 편집 중 문법 오류, 기존 호스트 테스트의 첫 드라이버 순서 가정, 생성 중인 W3 agent_bridge 모듈 누락을 발견했다. 순서 가정 assertion만 W2에서 수정했으며 다른 워커 코드는 수정하지 않았다.
- `cargo clippy --workspace -- -D warnings`: 최종 전체 workspace **통과**. `cargo clippy -p driver-neo4j --all-targets -- -D warnings`도 통과.
- fixture는 실행별 `DbtT_W2_<pid>` 라벨이며 정리 쿼리는 해당 라벨에만 DETACH DELETE 한다. 실제 서버 이미지는 `neo4j:5-community`다. 테스트 연결은 127.0.0.1:57687만 허용하며, 닫힌 포트 오류 검증만 임시 loopback 포트를 사용한다.
- `git diff --stat -- ui/`에는 다른 워커의 변경이 존재한다. W2는 이를 수정하거나 되돌리지 않았다.

## 계약 및 남은 한계

- W2에서 dbcore 계약은 변경하지 않았다. W3가 추가한 read_only 필드를 사용한다. `read_only_exec=true`, 요청의 `read_only=true`이면 `execute_read`로 서버 RUN `mode=r`을 적용한다. 내부 탐색/browse/expand도 READ 모드이다. 실제 CREATE 거부와 SELECT 성공을 확인했다.
- `transactions=false`: 실행별 연결이므로 요청 사이 명시적 트랜잭션을 유지하지 않는다.
- `multi_statement=false`: 여러 문장 분리기는 추가하지 않았으며 Neo4j 한 문장 실행을 그대로 사용한다.
- `cancel=Abort`: 서버 취소 보장이 아닌 클라이언트 스트림/소켓 폐기이다. 완료 전 쓰기 요청이 서버에서 commit됐는지는 취소 오류만으로 알 수 없다.
- 행 한도에 정확히 도달하면 일반 execute는 보수적으로 truncated=true이며 추가 행 확인을 위해 전체 결과를 읽지 않는다. browse는 limit+1로 정확한 has_more를 판정한다.
- 빈 결과는 neo4rs가 RUN 컬럼 메타데이터를 공개하지 않아 빈 columns를 반환하며, 컬럼 순서는 이름순이다. 각 컬럼 타입은 첫 non-null 셀에서 결정한다.
- 비용이 큰 라벨별 전체 count는 생략했다. 메타데이터 fallback은 샘플 100개에 존재하는 속성만 보여 준다.
- 실제 TLS 인증서 연결/클러스터 routing/GUI 표시는 검증하지 않았다. URI scheme 허용과 TLS/routing은 neo4rs에 위임한다.
- `cargo fmt -p driver-neo4j --check` 통과. placeholder/TODO/stub로 기능을 대체하지 않았으며 단위·실제 컨테이너 검증으로 확인했다.
- git commit은 하지 않았다.
