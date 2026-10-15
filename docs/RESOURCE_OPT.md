# 리소스 측정과 최적화 (P1)

작업 명세: [P1-resource-opt](tasks/P1-resource-opt.md). 초기 참고치는 [W8 비교](../bench/COMPARE.md)에 있으며 P1 전후 측정치로 취급하지 않는다.

## 측정 설계

`bench/resource_opt.py`는 macOS release `.app`의 Rust 호스트와 새로 생성된 WebContent / Networking / GPU 프로세스를 분리한다. RSS는 `ps`의 KiB, phys_footprint는 `/usr/bin/footprint`의 프로세스별 값을 수집한다. 두 값은 서로 다른 지표이므로 합치지 않는다. CPU는 누적 CPU 시간 증가 / 실제 경과 시간 × 100이며, 논리 코어 하나를 모두 쓰면 100%다. 유휴 CPU는 10초와 60초 표본 사이의 약 50초 구간으로 계산한다. 작업 후 CPU는 1초 구간이므로 0.01초 단위 `ps TIME`의 양자화 오차가 크다. 체크포인트 대기 중에는 호스트가 1초마다 승인 파일을 확인하는 측정 부하가 포함된다.

W8 종료에 대한 코디네이터 통보를 받은 다음에만 `--w8-complete`로 실행한다. 각 실행과 각 체크포인트 앞에서 1분 load ≤ 8을 확인하고, 최대 600초 대기 후 통과/실패를 원자료에 남긴다. 게이트 실패, 같은 역할의 프로세스 중복, footprint 실패/누락은 승인된 표본에 포함하지 않는다. 프로세스 귀속은 시작 전후 PID 집합 차이이므로 다른 앱의 WebKit 시작이 겹치면 오염 가능성이 있다. 여러 앱을 동시에 실행하지 않고, 역할별 프로세스 개수를 검증한다. 아직 생성되지 않았거나 종료된 역할은 없는 것으로 기록하며 수치를 만들어 넣지 않는다. RSS 합계는 공유 페이지가 중복될 수 있어 Activity Monitor의 단일 숫자와 같다고 해석하지 않는다.

조건은 시작 직후, 10초, 60초, 10만 행 × 10열의 실제 PG 쿼리, 실제 Neo4j 300노드/300관계 그래프, 결과가 있는 탭 10개, 전부 닫은 직후와 10초 후다. 탭 10개는 10만 행 표 탭, 그래프 탭, 연결 시 생긴 빈 쿼리 탭, 나머지 1천 행 표 탭으로 구성한다. 시점은 실제 경과 시간을 함께 기록하며, 수집/게이트 대기로 목표 시점을 넘으면 원자료의 실제 시각을 사용한다. 강제 GC는 하지 않는다.

각 구성은 새 앱 프로세스로 5회 실행한다. 첫 실행에서 같은 10만 행 쿼리를 50회 실행하고, 1천 행 쿼리 탭을 열고 실행하고 닫는 동작을 50회 반복한다. 1회 및 5회 간격으로 표본을 남기고 최소제곱 RSS 기울기(MiB/회)를 계산한다. 초기 워밍업 포함 추세와 안정 구간을 구분한다. RSS 유지가 곧 도달 불가능한 객체의 누수를 의미하지는 않는다.

기존 `AXDB_BENCH`의 고정 loopback 테스트 DB 설정만 쓴다. 별도의 임시 설정 경로와 native viewer lock을 사용하고, 생성한 프로필/세션/그래프 라벨을 정리한다. 체크포인트 IPC는 `AXDB_BENCH=1`에서만 활성화하며 파일 경로를 입력받지 않는다. 일반 앱은 측정용 UI 모듈을 로드하지 않는다.

## 조사와 적용 판단

2026-10-15에 아래 원문을 확인했다. 외부 수치를 이 앱의 개선율로 사용하지 않는다.

| 기법 / 출처 | 현재 코드 / 적용 판단 |
|---|---|
| [Tokio worker_threads / blocking pool](https://docs.rs/tokio/latest/tokio/runtime/struct.Builder.html), [Tauri runtime 주입](https://docs.rs/tauri/latest/tauri/async_runtime/fn.set.html) | 기본 worker 수는 가용 코어 수다. `TOKIO_WORKER_THREADS=2/4`로 동일 바이너리를 먼저 비교한다. blocking pool은 필요할 때 생성되므로 상한을 낮춘다고 유휴 스레드가 그만큼 사라지는 것은 아니다. 너무 낮추면 파일/DNS/SQLite 작업이 대기하므로 측정 없이 낮추지 않는다. |
| [mimalloc 원문](https://github.com/microsoft/mimalloc), [Rust Performance Book: 할당자와 빌드](https://nnethercote.github.io/perf-book/build-configuration.html) | 호스트 프로세스만 바뀌며 WebKit의 JS 힙을 고치지 않는다. 시스템 할당자 대비 이득의 실측이 없으면 의존성을 도입하지 않는다. |
| [neo4rs ConfigBuilder](https://docs.rs/neo4rs/latest/neo4rs/struct.ConfigBuilder.html), [tokio-postgres connect](https://docs.rs/tokio-postgres/latest/tokio_postgres/fn.connect.html) | 이미 Neo4j `max_connections(1)`, PG 세션별 단일 연결이다. 드라이버 등록은 manifest 생성이며 네트워크 세션은 connect 요청 때 생성한다. 중복 적용하지 않는다. |
| [Tauri devtools / incognito / backgroundThrottling](https://v2.tauri.app/reference/config/), [Apple WKWebsiteDataStore](https://developer.apple.com/documentation/webkit/wkwebsitedatastore) | release 의존성에 devtools feature가 없다. incognito와 비영속 data store는 저장/캐시 정책을 바꾸므로 메모리 절감 목적으로 무조건 켜지 않는다. 백그라운드 throttle은 이미 GraphView의 active/visibilitychange와 Worker pause/resume으로 처리한다. |
| [Tauri 메모리 측정 범위 논의 #5889](https://github.com/tauri-apps/tauri/issues/5889) | 호스트만 재면 WebKit 비용을 놓친다. 네 프로세스의 RSS와 footprint를 각각 남기는 측정 설계에 적용했다. |
| [Wry evaluate_script 누수 이슈 #1489](https://github.com/tauri-apps/wry/issues/1489), [Tauri WithWebview 누수 이슈 #15210](https://github.com/tauri-apps/tauri/issues/15210) | 이슈는 문제 가능성의 근거이며 현재 앱의 누수 증거가 아니다. 이 앱은 해당 API를 직접 호출하지 않는다. 50회 반복 추세로 별도 확인한다. |
| UI 번들/결과 소유권 | GraphView, SQL/Cypher 언어, 벤치 모듈은 이미 동적 import다. 탭 닫기는 컴포넌트 unmount, CodeMirror destroy, Worker terminate를 수행한다. App의 마지막 결과 요약이 전체 결과를 보유하는 경로는 비교 후보다. 자동 LRU 퇴출은 열어 둔 결과를 잃게 하므로 기존 동작 보존 조건상 채택하지 않는다. |
| [Rust heap allocations](https://nnethercote.github.io/perf-book/heap-allocations.html) | Rust 결과 전송은 소유권 이동과 컬럼형 배치 consume으로 처리하고 반환 이후 drop한다. 실측 없이 버퍼 풀이나 전역 캐시를 추가하지 않는다. |
| [Cargo release profiles](https://doc.rust-lang.org/cargo/reference/profiles.html), [Rust Performance Book 빌드 설정](https://nnethercote.github.io/perf-book/build-configuration.html) | 현재 `opt-level=3`, `codegen-units=1`, `strip=true`, `lto=true`(fat)다. 크기 최적화 s/z와 panic abort는 별도 실험으로 판단한다. abort는 panic 격리 의미를 바꾸므로 크기만 보고 적용하지 않는다. |
| 로그/이력, 유휴 타이머 | 이력은 실행 완료 시 기록하고 유휴 polling 타이머가 없다. ResultStatus 타이머는 값 변경에만 작동하고 unmount 때 해제된다. 버퍼링으로 종료 시 이력 손실 위험을 늘리는 변경은 실측 근거 없이 하지 않는다. |

## 실행과 원자료

```sh
python3 bench/resource_opt.py --self-check
pnpm --dir ui build
CARGO_TARGET_DIR=target-p1 cargo build -p axdb --release
# .app 준비/서명 뒤, W8 종료 통보를 확인하고 실행한다.
python3 bench/resource_opt.py target-p1/release/bundle/macos/AxDB.app --w8-complete --variant baseline
TOKIO_WORKER_THREADS=2 python3 bench/resource_opt.py target-p1/release/bundle/macos/AxDB.app --w8-complete --variant workers2
```

원자료에는 앱 실행 파일 SHA-256, 크기, 플랫폼, worker 환경 변수, 실제 시점, load 게이트, 각 PID의 RSS/footprint/CPU와 footprint 원문을 남긴다. 기존 결과 파일 덮어쓰기는 거부한다.

## 전후 수치와 누수 점검

아직 W8 측정 독점 구간이 해제되지 않아 P1 수치를 기록하지 않았다. 준비용 Python 계산 self-check와 UI 타입 검사/번들 빌드가 통과했으나 이는 리소스 개선의 증거가 아니다. 해제 통보 전 실제 release 컴파일을 잠시 시작한 잘못은 즉시 중단하고 코디네이터에게 W8 중첩 구간 제외를 요청했다.

## 검증과 한계

빌드, Rust 테스트, clippy, vitest 최종 결과와 코디네이터 E2E 범위는 작업 종료 시 기록한다. 측정되지 않은 후보는 효과가 있다고 표시하지 않는다.
