# 검증 결과, 성능 측정, 남은 한계

이 문서는 코디네이터(Opus 5.5)가 독립적으로 확인한 결과다. 워커의 자체 보고는 근거로 쓰지 않았고, 사실과 추정은 구분해 적는다.

- 환경: Apple M5, 24GB, macOS 27.0.1. 테스트 DB는 사용자 DB와 분리한 Docker 컨테이너를 썼다: `postgres:16-alpine`(55432), `neo4j:5-community`(57687), SQLite 임시 파일
- 쓰기 작업은 이 테스트 DB에서만 했다(fixture: PG 스키마 `e2e`, Neo4j 라벨 `DbtE_*`). 사용자 DB(5432/5439/32802 등)에는 접속하지 않았다.

## 1. 요구 조건별 결과

| # | 조건 | 결과 | 근거 |
|---|---|---|---|
| 1 | PG·Neo4j 실제 연결로 주요 기능 검증 | 충족 | 실제 앱 + MCP 브리지 E2E `bench/e2e-bridge.mjs` 21/21(빌드 85dbcbb, 3e09c82, 8c0bb73 각각 재실행). 같은 앱 화면에서 표·그래프 렌더링을 스크린샷으로 확인 |
| 2 | 표·그래프 결과와 DB 고유 타입의 정확한 표시 | 충족 | 아래 2절 |
| 3 | 연결 실패, 쿼리 오류, 타임아웃, 큰 결과 처리 | 충족 | 아래 3절 |
| 4 | 자격 증명이 평문 설정과 로그에 없음 | 충족(코드 경로) | 아래 4절 |
| 5 | 세 번째 DB를 실제 연결 가능한 플러그인으로 추가 | 충족 | SQLite, 커밋 `3e09c82` |
| 6 | 공통 UI의 DB별 조건문을 늘리지 않음 | 충족 | `3e09c82`의 diff에서 `ui/`, `dbcore`, 기존 드라이버 변경 0. UI 소스의 DB id 문자열 비교 grep 0건 |
| 7 | 새 DB 플러그인 개발 방법과 최소 예제 | 충족 | [PLUGIN_GUIDE.md](PLUGIN_GUIDE.md), `crates/driver-example-kv`(컴파일·테스트됨, 앱에는 미등록) |
| 8 | 실행·빌드 방법, 검증 결과, 성능, 한계 | 이 문서와 README | 성능 비교는 5절 |

## 2. 타입과 결과 표시 (실제 앱)

**PostgreSQL.** 다음 값을 직접 확인했다.

| 타입 | 확인한 값 |
|---|---|
| int8 최댓값/최솟값 | `9223372036854775807` / `-9223372036854775808`. 문자열로 전송해 정밀도 손실 없음 |
| numeric(30,9) | `12345678901234567890.123456789`, `-0.000000001` |
| timestamptz | 서버가 렌더한 원문(세션 TZ 기준) |
| jsonb | 내부의 큰 수까지 원문 그대로 |
| bytea | `<4 bytes> 0x00ffabcd` |
| 그 밖 | uuid, `text[]`, 한글 텍스트 |

- jsonb의 `null` 값과 SQL `NULL`이 화면에서 구분된다.
- 컬럼은 요청한 순서대로 나오고, 헤더 아래에 타입명이 표시된다.

**Neo4j.**

- 파라미터 쿼리 하나로 Node, Relationship, Path, List, Map이 섞인 결과를 받으면 graph 뷰(중복 제거된 노드 2개, 관계 1개)와 table 뷰가 함께 생긴다. 두 뷰는 전환할 수 있다.
- 다음 값을 직접 확인했다.

| 타입 | 확인한 값 |
|---|---|
| 큰 정수 | `9007199254740993` |
| DateTime | 원문, 나노초 포함 |
| Point | 2D·3D |
| Path | 노드 캡션으로 요약 표시 |

- 컬럼 순서는 RETURN 순서를 따른다. 실제 앱에서 결함을 발견해 수정했다. 수정 후 `z,a,m`이 그대로 나오고, 0행 결과에도 헤더 `x,y`가 나온다.
- 이웃 확장: Alice → 노드 3개, 관계 2개
- 그래프 한도: 1,200개 이상의 노드를 요청하면 잘렸다는 표시(truncated)가 붙는다. 에이전트 응답은 노드 200개로 제한된다.

**SQLite.** INTEGER 최댓값, REAL, JSON 원문, DATETIME, BLOB, NULL을 같은 공통 UI에서 렌더링했다. UI는 수정하지 않았다.

## 3. 오류와 한도 (실제 앱)

| 상황 | PG | Neo4j | SQLite |
|---|---|---|---|
| 쿼리 오류 | Query + 메시지/위치 | Query + Neo4j 코드/위치 | Query + offset |
| 타임아웃(700ms) | Timeout 772~1341ms, 서버 측 취소, 이후 같은 세션 정상 | Timeout 721~751ms, 이후 같은 세션 정상 | Timeout 692ms |
| 연결 실패 | 닫힌 포트 → Connection, 잘못된 비밀번호 → Auth 배지 | 테스트(Auth/Connection) | 없는 경로 → Connection(테스트) |
| 큰 결과 | 250,000행 → row_limit 1,000에서 잘림, 5~62ms. 서버 측 커서 FETCH로 전체 행을 메모리에 올리지 않음 | 행·그래프 노드 한도 | step 단위 한도 |
| 여러 결과 집합 | `SELECT 1; SELECT 'x', 2` → 결과 2개 | — | multi_statement |
| 에이전트 쓰기 | DB 수준 거부(`read-only transaction`) | 거부(READ 모드) | 거부(`readonly database`) |

## 4. 자격 증명

- 실제 앱을 E2E로 돌린 뒤 설정 디렉터리 전체(profiles.json, history.jsonl, 로그)와 앱 stdout에서 테스트 비밀번호를 grep했다. 결과는 0건이다. 브리지 토큰도 로그에 없다.
- 로그에는 연결 필드의 이름만 남고 값은 남지 않는다. profiles.json의 권한은 0600이고 secret 필드가 없다.
- 키체인: 이번 E2E에서 앱이 OS 키체인 항목(이름 변경 전 서비스, 이름 변경 후 `axdb`)을 읽어 연결했다. 키체인 항목 생성은 `security` CLI로 했다. 앱이 직접 쓰는 경로는 5절의 최종 검증에서 확인한다.
- 제약: 서명하지 않은 debug 바이너리는 빌드마다 코드 식별자가 바뀐다. 그래서 다른 바이너리가 만든 키체인 항목에 접근할 때 macOS가 로그인 암호를 묻는다(W7b에서 관찰). 배포 시 코드 서명이 필요하다.

## 5. 성능

측정한 값만 적는다. 환경은 1절과 같다. 원자료와 방법은 링크한 문서에 있다.

**뷰어(W4/W4b, Chromium 측정, [VIEWER_PERF.md](../bench/VIEWER_PERF.md))**
- 10만 행 첫 표시: 396 → 41ms
- 그래프 마운트 후 5초간 흔들림: 6.85px → 0, CLS 0
- 300노드 팬·줌 p95: 16.8ms
- 문서 5천 개 JSON 트리: DOM 37행

**연결 추가 흐름(P1, native release, PG·Neo4j 각 20회)**
- 네트워크를 거치지 않는 단계(폼 열기, 드라이버 변경, 저장)는 모두 p95 77ms 이하였다.
- Connect(연결과 첫 트리 표시 포함)는 p50 134ms다.
- +/선택 단계의 IPC는 0회다.

**Tabularis 0.28.0과의 비교([COMPARE.md](../bench/COMPARE.md), 부하 게이트 통과 표본만)**
- big 쿼리 첫 행: 221ms 대 163ms. 기본 행 수가 500 대 1,000으로 달라 처리량 비교로 일반화하지 않는다.
- 결과 표시 후 footprint: 428 대 187MB
- 60초 유휴 RSS: 278 대 198MiB
- 60초 유휴 footprint: 140 대 81MiB
- 유휴 CPU: 둘 다 0.1% 미만
- `.app` 크기: 32.1 대 11.9MiB

**메모리 최적화([RESOURCE_OPT.md](RESOURCE_OPT.md))**
- 같은 10만 행 쿼리를 50회 반복할 때 host footprint가 계속 늘었다. 원인을 macOS 시스템 malloc의 빈 페이지 보존으로 확인했다(살아 있는 힙은 11MB).
- mimalloc을 purge 즉시 모드로 적용했다. 50회 후 host footprint는 696 → 63MiB, 앱 전체 footprint는 1,466 → 383MiB가 됐다.
- 대가는 유휴 host +4MiB다. 처리 시간과 유휴 CPU는 같았다.
- 측정했지만 이득이 일관되지 않아 적용하지 않은 것: Tokio worker 2·4개, secure mimalloc, opt-level s/z, 결과 보조 참조 해제

**"Choose a database"가 느린 이유(Tabularis)**
- 이 문구는 Tabularis `NewConnectionModal.tsx`에 있다. 모달은 `fetch_plugin_registry`를 거친다.
  - 원격 목록 API(실측 1.8~1.9초, 68KB)를 받는다.
  - 플러그인 20개의 상세를 8개씩 병렬로 요청한다(각 0.4~1.7초).
  - 레거시 `registry.json`을 병합한다.
  - 이 과정 때문에 표시까지 약 3~5초가 걸린다. 내장 PG 드라이버가 deprecated라 `postgresql` 플러그인 다운로드 경로도 탄다.
- AxDB는 로컬 레지스트리를 쓰고 원격 호출이 없다. `drivers_list`는 warm 상태에서 1ms다.

## 6. 남은 한계

- **실제 Anthropic 호출은 검증하지 않았다.** API 키가 없고 호출은 과금된다. 요청 조립, SSE, refusal, `cache_control` 위치는 mock HTTP로 검증했다. 키를 주면 바로 검증할 수 있다.
- MongoDB는 스키마 그래프(ERD)를 지원하지 않는다. capability로 선언하지 않았으므로 UI에 나타나지 않는다.
- 외부 프로세스(JSON-RPC) 플러그인은 구현하지 않았다(ADR-0001). 드라이버는 컴파일 시 등록한다.
- 키체인: 앱의 GUI 쓰기 경로는 실제 OS 키체인 왕복 테스트(`real_os_keychain_roundtrip`, 명시 실행)로 대신 검증했다. E2E는 debug 전용 `AXDB_DEV_SECRET_STORE`를 썼고, 이 경로는 release에서 컴파일되지 않는다. 배포하려면 코드 서명이 필요하다.
- timestamptz는 서버 세션 TZ 기준 원문으로 표시한다.
- TLS 연결, Windows/Linux, Redis는 검증하지 않았다.
- Tabularis 생성 쿼리 15쌍은 비교 측정하지 않았다.
- WebContent(UI)의 footprint는 탭을 모두 닫은 뒤에도 GC 시점에 따라 230~320MiB에 머문다. 결과 보조 참조 해제 후보로는 개선되지 않았다.
- `cargo test` 전체 4회 중 첫 회에 PG 미리보기 테스트(`real_host_commands`)가 1건 일시적으로 실패했다. 이후 3회 연속 69/69로 통과했다.
- 측정 중 OrbStack이 재시작됐다(약 19:51, 원인 외부). 테스트 DB 3개만 다시 시작했다. 사용자 컨테이너(zivo-*, office-meal 등)는 **정지 상태 그대로**다.

## 7. v2 고도화 보고

| 항목 | 이전 | 이번 결과 | 검증 |
|---|---|---|---|
| 1 플러그인 구조 | 관계형 2개 + SQLite | **MongoDB 추가**(M1). 문서 패러다임, 비SQL 명령, ObjectId·Decimal128 등. 커밋 diff는 crate와 등록 3줄뿐(ui/dbcore 0) | 실제 Mongo 계약 테스트, UI driver-id grep 0 |
| 2 뷰어·연결 탐색 | 표, 그래프, 결과 내 전환 (재사용) | **ERD/스키마 그래프**(PG FK, SQLite, Neo4j), **가상화 JSON 트리**, **FK 셀 → 참조 행**, **그래프↔표 선택 동기화**, **탐색 이력**(V1) | 실제 DB, 팬·줌 p95 16.8ms |
| 3 변경 미리보기 | 없음 | **실행 후 반드시 롤백**하는 데이터·스키마 diff. 롤백할 수 없는 문장은 거부(C1, PG·Neo4j·SQLite) | preview-e2e 5/5 |
| 4 권한·읽기 전용 | 에이전트 읽기 전용 + 승인 (재사용) | **연결별 read_only**(host 강제), **클라이언트별 권한 프로필**(A1) | permissions-e2e 13/13 |
| 5 MCP 권한 | 단일 토큰 브리지 (재사용) | **범위 토큰**(해시 저장), 연결별 none/read/preview/write_with_approval, 허용 도구, 감사 로그, 회전·폐기 | 13/13 + 외부 에이전트 시나리오 |
| 6 AI 지원 | 없음 | **맥락 MCP 도구 3종**, 선택 기능 Anthropic 도우미(기본 꺼짐, 생성 쿼리는 실행하지 않음)(AI1/AI1b) | [시나리오](../bench/results-ai-agent-scenario.md). 실제 API 호출은 미검증 |
| 7 성능·보안·리소스 | W4·W8 (재사용) | **host 메모리 보존 문제 해결(mimalloc purge 0)**, 연결 추가 흐름 분해, Tabularis 유휴 재비교 | 5절 |

**회귀(최종 트리)**
- `cargo test --workspace`: 69 통과, 2 ignored(유료 API, 실제 키체인). 실제 PG·Neo4j·Mongo·SQLite에 연결했다.
- `clippy --all-targets -D warnings`, vitest 45/45, UI driver-id 비교 grep 0
- 실제 앱 E2E: e2e-bridge 21/21(PG·Neo4j·SQLite), preview 5/5, permissions 13/13
