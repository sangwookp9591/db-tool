# 리소스 측정과 최적화 (P1)

작업 명세: [P1-resource-opt](tasks/P1-resource-opt.md). 초기 참고치는 P1 전후 측정치로 취급하지 않는다.

최종 판단: **일관된 RAM/CPU 개선과 처리 시간 보존을 함께 확인한 후보가 없어 제품 설정은 유지했다.** 결과 참조, Tokio 2/4, secure mimalloc, release s/z를 실제 native A/B로 확인하고 후보 적용을 되돌렸다. 연결 추가의 비네트워크 단계 p95는 모두 100ms 미만이었다. 계측·원자료·재현 패치·검증 문서는 남긴다.

최종 소스는 `85231f2`와 기본 비활성 공통 계측이다. Rust 69 passed / 2 opt-in ignored, clippy와 vitest 45개가 통과했다. 쿼리 50회 후반 RSS +4.06MiB/회는 관측했으나 실제 누수/해결로 확정하지 못했다. 범위와 제외 표본은 아래에 명시했다.

## 측정 설계

`bench/resource_opt.py`는 macOS release `.app`의 Rust 호스트와 새로 생성된 WebContent / Networking / GPU 프로세스를 분리한다. RSS는 `ps`의 KiB, phys_footprint는 `/usr/bin/footprint`의 프로세스별 값을 수집한다. 두 값은 서로 다른 지표이므로 합치지 않는다. CPU는 누적 CPU 시간 증가 / 실제 경과 시간 × 100이며, 논리 코어 하나를 모두 쓰면 100%다. 유휴 CPU는 10초와 60초 표본 사이의 약 50초 구간으로 계산한다. 작업 후 CPU는 1초 구간이므로 0.01초 단위 `ps TIME`의 양자화 오차가 크다. 체크포인트 대기 중에는 호스트가 1초마다 승인 파일을 확인하는 측정 부하가 포함된다.

W8 종료에 대한 코디네이터 통보를 받은 다음에만 `--w8-complete`로 실행한다. 앱 시작과 각 작업군 앞에서 1분 load ≤ 8을 확인하고 최대 600초 대기한다. 실제 표본 직전에도 load를 다시 기록한다. 작업 직후 시점을 보존하기 위해 그때 부하가 높으면 기다려서 다른 시점으로 바꾸지 않고 실패 표본으로 남긴다. 게이트 실패, 같은 역할의 프로세스 중복, footprint 실패/누락은 승인된 표본에 포함하지 않는다. 프로세스 귀속은 시작 전후 PID 집합 차이이므로 다른 앱의 WebKit 시작이 겹치면 오염 가능성이 있다. 여러 앱을 동시에 실행하지 않고, 역할별 프로세스 개수를 검증한다. 아직 생성되지 않았거나 종료된 역할은 없는 것으로 기록하며 수치를 만들어 넣지 않는다. RSS 합계는 공유 페이지가 중복될 수 있어 Activity Monitor의 단일 숫자와 같다고 해석하지 않는다.

조건은 시작 직후, 10초, 60초, 10만 행 × 10열의 실제 PG 쿼리, 실제 Neo4j 300노드/300관계 그래프, 탭 10개, 전부 닫은 직후와 10초 후다. 탭 10개는 10만 행 표 탭, 그래프 탭, 연결 시 생긴 빈 쿼리 탭, 나머지 1천 행 표 탭으로 구성한다. 닫기 직전 10만 행 표를 다시 실행해 마지막 결과의 크기를 같게 맞춘다. 시점은 실제 경과 시간을 함께 기록하며 목표 시점을 넘으면 원자료의 실제 시각을 사용한다. 강제 GC는 하지 않는다.

각 구성은 새 앱 프로세스로 5회 실행한다. `--compare-app`으로 둘 이상의 고정 번들, `--workers default,2,4`로 같은 번들의 런타임 구성을 번갈아 실행할 수 있으며 짝수 회차는 순서를 뒤집는다. 공유 워크스페이스의 다른 빌드를 중지할 권한은 없으므로 시작 시 전체 `ps`와 각 표본의 컴파일러/Node 프로세스 목록, load를 보존한다. 첫 실행에서 같은 10만 행 쿼리를 50회 실행하고, 1천 행 쿼리 탭을 열고 실행하고 닫는 동작을 50회 반복한다. 1회 및 5회 간격으로 표본을 남기고 최소제곱 RSS 기울기(MiB/회)를 계산한다. 초기 워밍업 포함 추세와 15회 이후 구간을 구분한다. RSS 유지가 곧 도달 불가능한 객체의 누수를 의미하지는 않는다.

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

`568f43e` 소스의 두 번들은 빌드/계측 준비 확인용이며 최종 측정 대상에서 제외했다. [빌드 기록](../bench/results-resource-builds.json)의 `superseded-preparation-only` 상태와 [당시 패치](../bench/p1-candidates/00-summary-c1-preparation.patch)로 구분한다. 코디네이터의 후속 지시에 따라 **AI1 완료 후 최종 소스**를 고정해 기준/후보 `.app`을 만든다. 최종 소스와 lockfile 해시, 후보별 변경을 기록하고 다른 기능 변경이 A/B에 섞이지 않도록 한다.

[후보 패치와 측정 계획](../bench/p1-candidates/README.md)에 결과 소유권, Tokio 상한 2/4, mimalloc secure, release s/z를 준비했다. 미검증 후보를 공유 소스에 적용하지 않는다. 준비 중 직접 넣었던 요약 저장 변경은 P1 부분만 되돌렸다. AI1이 실제 표를 읽는 경로를 추가 중이므로 요약 전용 변경은 쓰지 않고, 최종 소비자를 보존하면서 닫힌 탭의 보조 참조를 해제하는 후보로 갱신한다.

## 전후 수치와 누수 점검

W8 종료 통보는 19:02에 받았다. 이후 준비 점검의 시작 게이트에서 load 39.68~66.81을 기록했으며 앱은 실행하지 않았다. 코디네이터가 V1/A1/M1 종료 후 별도로 조용한 구간을 통보하기로 해 게이트 대기를 중단했다. [대기 원자료](../bench/results-resource-calibration-blocked.json), [게이트 로그](../bench/results-resource-calibration-blocked.txt)에 보존했다. 당시 앱 수집은 없었다. 이후 기준선과 후보 비교는 아래 결과로 별도 기록한다.

준비용 C1 두 번들은 최종 측정에서 제외했다. 코디네이터가 20:10에 최종 `85231f2`와 독점 구간을 통보했다. 공통 계측을 반영한 현재 UI 빌드·vitest 45개와 기준 release 빌드는 통과했고 [최종 빌드 기록](../bench/results-resource-final-builds.json)에 해시를 남겼다. 전체 Rust 테스트·clippy, native timer 검증과 후보별 실측을 완료했다. 패치 적용 검사나 계산 self-check를 개선의 증거로 취급하지 않는다.

## 검증과 한계

빌드, Rust 테스트, clippy, vitest 최종 결과와 코디네이터 E2E 범위는 작업 종료 시 기록한다. 측정되지 않은 후보는 효과가 있다고 표시하지 않는다.

## 연결 추가 지연 조사 (추가 범위)

현재 AxDB는 App 초기 로딩에서 `drivers_list`를 받고, + 버튼은 `setEditing('new')`, 드라이버 선택은 로컬 상태/manifest 필드 구성만 수행한다. 이 두 경로에는 원격 플러그인 레지스트리 호출이 없다. `drivers_list` 호스트도 등록된 manifest를 복사해 반환한다. 이는 소스 분석 결과이며 지연 시간 실측은 아직 없다.

`profile_save`는 저장 체크가 꺼진 새 프로필도 동기 keychain 삭제를 호출한다. 병목인지는 단정하지 않으며 IPC/호스트/키체인 구간을 분리해 확인한다. +, 선택 변경, Test, Save, Connect의 실제 UI p50/p95를 조용한 구간에서 측정하고, 100ms 이상인 비네트워크 구간은 원인 확인 후 A/B한다. [공통 계측과 후보 계획](../bench/p1-candidates/README.md)에 초기/후속 시행 구분, 비밀 값 비노출, 실패/정리 기준을 적었다.

### 연결 흐름 기준선 결과

최종 `85231f2` release WKWebView에서 드라이버별 20회 측정했다. 모든 시행이 부하·프로세스·footprint 검사를 통과했다. 값은 UI 준비와 두 RAF를 거친 표시 대용 시간이며, 실제 픽셀 첫 페인트를 직접 잰 값은 아니다. 각 드라이버의 첫 시행 1개도 포함한다.

| 단계 | PG p50 / p95 (ms) | Neo4j p50 / p95 (ms) | 애플리케이션 처리 |
|---|---:|---:|---|
| + → 보이는 폼 | 47 / 74 | 75 / 77 | 로컬 상태/모달 |
| 드라이버 변경 → 필드 | 67 / 68 | 67 / 68 | 로컬 manifest/렌더 |
| Test → 서버 결과 | 66 / 68 | 68 / 100 | DB 연결·ping·서버 정보 |
| Save → 목록 | 66 / 68 | 67 / 67 | 키체인·파일·목록 갱신 |
| Connect → 탐색 첫 레벨 | 134 / 136 | 133 / 135 | 세션 비밀번호 자동 입력·DB·트리 |

+/드라이버 변경 단계의 IPC는 각각 0회였다. `drivers_list`는 시작 시 받은 캐시를 사용한다. 별도 warmed IPC 20회의 p50/p95는 1/1ms이고, 호스트 manifest 복사 시간은 22개 기록의 p50 0.004ms였다. 원격 레지스트리 요청으로 해석할 경로가 없다.

Save의 호스트 시간은 40회 p50/p95 6.34/9.30ms, keychain 삭제는 저장/정리 80회 기준 0.14/0.91ms였다. 이 조건에서 키체인을 병목이라고 할 근거가 없다. UI 기록에는 상태 갱신과 두 RAF 대기가 포함된다. 비네트워크 단계의 p95가 모두 100ms 미만이므로 modal 표시 방식을 바꾸거나 비밀 삭제를 생략하는 최적화는 적용하지 않는다.

기본값인 비밀번호 미저장 프로필을 사용했다. 저장된 실제 비밀번호의 조회/권한 프롬프트 비용을 측정한 결과는 아니다. Connect는 자격 증명 모달의 자동 입력을 포함하며 사람의 생각·입력 시간은 포함하지 않는다. W4의 결합된 addConnection 569ms 참고치와 직접 비교하지 않는다.

[원자료](../bench/results-resource-connections-baseline.json). 테스트 실패 시 원본 설정을 보존하는 별도 fault-injection 검사 1개도 통과했으나 이는 성능 증거가 아니다.

### 최종 소스 기준선 (5회)

최종 `85231f2`와 공통 계측으로 독립 앱 실행 5회를 마쳤다. 모두 부하·역할별 PID 개수·footprint 검사를 통과했다. 창 내부 크기/배율은 1280×768 / 2로 같았다. 아래 합계는 공유 RSS 중복 가능성을 포함하며 최적화 성과가 아니다.

| 조건 | RSS 중앙값 (최소–최대), MiB | footprint 중앙값, MiB | 유효 실행 |
|---|---:|---:|---:|
| 시작 직후 | 196.2 (196.0–197.6) | 151.3 | 5/5 |
| 10초 유휴 | 200.0 (199.6–200.0) | 87.2 | 5/5 |
| 60초 유휴 | 199.8 (199.5–199.8) | 82.3 | 5/5 |
| 10만 행 × 10열 | 613.1 (605.4–620.8) | 263.7 | 5/5 |
| 300노드 그래프 | 626.2 (616.8–636.7) | 257.4 | 5/5 |
| 탭 10개 | 732.6 (710.4–742.6) | 371.8 | 5/5 |
| 모두 닫은 직후 | 733.4 (712.6–743.5) | 355.8 | 5/5 |
| 닫은 뒤 10초 | 732.5 (704.1–739.8) | 326.7 | 5/5 |

첫 시행의 같은 쿼리 50회 RSS 기울기는 전체 +7.26MiB/회, 15회 이후 +4.06MiB/회였다. 탭 열기/실행/닫기 50회는 각각 -0.54 / -0.67MiB/회였다. 이는 1회 반복 추적의 관측값이며 할당자 캐시·회수 지연과 실제 누수를 구분하는 증거로 단정하지 않는다. 후보와 최종 구성의 같은 추적을 비교한다.

[기준선 원자료](../bench/results-resource-baseline-85231f2.json.gz) · [프로세스별 집계](../bench/results-resource-baseline-summary.json).

### 결과 보조 참조 해제 후보 — 채택하지 않음

기준/후보를 각 5회 교대 실행했고 10개 실행 모두 검사를 통과했다. 모든 탭을 닫은 직후 RSS 중앙값은 736.6→733.5MiB였지만, 10초 뒤에는 723.3→725.4MiB로 감소가 재현되지 않았다. 각 실행 범위도 겹쳤다. footprint 역시 직후 446.1→353.5MiB, 10초 뒤 352.3→427.6MiB로 방향이 일정하지 않았다. 반환·GC·할당자 회수 시점의 영향을 분리해 확정한 결과가 아니다.

후보 UI 빌드와 vitest 46개는 통과했고 데이터 원본/에이전트 요약 보존을 확인했으나, 그것이 RAM 개선 증거는 아니다. 명세에 따라 적용하지 않고 공유 소스의 후보 부분은 되돌렸다. [후보 패치](../bench/p1-candidates/01-result-summary.patch), [교대 원자료](../bench/results-resource-result-ownership-ab.json.gz), [프로세스별 집계](../bench/results-resource-ownership-summary.json)를 남긴다.

### Tokio 기본값 / worker 2 / worker 4 — 제한하지 않음

동일한 기준 실행 파일에서 `TOKIO_WORKER_THREADS`만 바꿔 각 5회, 총 15회를 정순/역순 교대했다. 한 번의 외부 부하 대기는 원자료에 남겼고 모든 실제 표본은 게이트를 통과했다. 아래 값은 중앙값이다.

| 항목 | 기본값 | 2 | 4 |
|---|---:|---:|---:|
| 60초 RSS (MiB) | 199.48 | 198.17 | 167.72 |
| 60초 footprint (MiB) | 82.33 | 81.63 | 86.74 |
| 50초 구간 유휴 CPU (%) | 0.0799 | 0.0800 | 0.0600 |
| 10만 행 후 RSS (MiB) | 601.73 | 599.33 | 597.72 |
| 10만 행 후 footprint (MiB) | 263.27 | 263.55 | 263.61 |
| 10만 행 UI 완료 대용 시간 (ms) | 735 | 710 | 714 |
| 탭 종료 10초 후 RSS (MiB) | 716.83 | 708.81 | 726.33 |
| 탭 종료 10초 후 footprint (MiB) | 452.11 | 590.14 | 482.50 |

worker 4의 유휴 RSS는 낮았지만 footprint가 낮아지지 않았고, 작업 후/종료 후 지표도 일관된 이득을 보이지 않았다. 유휴 CPU의 0.02%p 차이는 `ps TIME` 0.01초 단위와 약 50초 구간의 해상도 수준이다. 처리 시간의 작은 차이만으로 동시 처리 상한을 바꾸지 않는다. 제한 패치는 적용하지 않고 기본 런타임을 유지한다.

[교대 원자료](../bench/results-resource-runtime-workers-ab.json.gz) · [프로세스별 집계](../bench/results-resource-runtime-summary.json).

### secure mimalloc — 채택하지 않음

mimalloc 0.1.52 / libmimalloc-sys 0.1.49 secure 모드를 시스템 할당자와 각 5회 교대했다. 10개 실행은 모두 유효했다. 기존 lockfile 패키지 버전은 그대로였고 allocator 두 패키지만 추가한 비교였다. 값은 중앙값이다.

| 항목 | 시스템 | mimalloc secure |
|---|---:|---:|
| 60초 RSS / footprint (MiB) | 199.64 / 82.53 | 211.58 / 93.39 |
| 50초 구간 유휴 CPU (%) | 0.0600 | 0.0600 |
| 10만 행 후 RSS / footprint (MiB) | 620.75 / 256.82 | 604.70 / 497.53 |
| 10만 행 UI 완료 대용 시간 (ms) | 712 | 771 |
| 300노드 후 RSS / footprint (MiB) | 637.61 / 256.39 | 400.89 / 271.10 |
| 탭 종료 10초 후 RSS / footprint (MiB) | 730.88 / 324.39 | 539.17 / 325.39 |
| 닫기 전 재실행 UI 완료 대용 시간 (ms) | 636 | 750 |

RSS 감소는 관측됐지만 footprint가 줄지 않았고 유휴 비용과 대용 완료 시간이 증가했다. RSS와 footprint의 차이를 실제 메모리 절감으로 합쳐 주장하지 않는다. 이번 조건에서는 혼합된 결과이므로 전역 할당자 교체와 의존성/lockfile 변경을 되돌렸다. 보안 보호 모드를 끈 별도 할당자를 성능 개선으로 추정하지 않는다.

[교대 원자료](../bench/results-resource-mimalloc-secure-ab.json.gz) · [프로세스별 집계](../bench/results-resource-allocator-summary.json). 빌드 기록에는 native manifest 해시 누락을 발견해 기준/후보 목록을 보완한 경위를 남겼다. 실행 파일은 생성·서명된 고정 파일이며 이후 변경하지 않았다.

### release 3 / s / z — 기본 3 유지

같은 소스에서 opt-level만 바꿔 각 5회, 총 15개 앱 실행을 교대했다. `lto=true`, `codegen-units=1`, strip 및 panic 방식은 유지했다. z의 4번째 실행 10초 유휴 표본은 외부 load 13.95로 제외했고 그 지표는 4/5다. 나머지 지표는 모두 5/5이며, 무효 표본을 포함한 전체 성공으로 표시하지 않도록 runner가 종료 코드 1을 반환했다.

| 항목 | 3 | s | z |
|---|---:|---:|---:|
| 실행 파일 (bytes) | 20,258,704 | 15,005,872 | 11,609,488 |
| 60초 RSS / footprint (MiB) | 199.36 / 82.25 | 199.53 / 82.44 | 199.73 / 82.33 |
| 50초 유휴 CPU (%) | 0.0600 | 0.0800 | 0.0800 |
| 10만 행 후 RSS / footprint (MiB) | 620.12 / 259.21 | 621.25 / 259.74 | 626.45 / 236.66 |
| 10만 행 UI 완료 대용 시간 (ms) | 714 | 753 | 1,138 |
| 탭 종료 10초 후 RSS / footprint (MiB) | 746.30 / 314.41 | 733.73 / 321.47 | 738.69 / 330.38 |
| 닫기 전 재실행 UI 완료 대용 시간 (ms) | 632 | 686 | 1,055 |

파일은 작아졌지만 유휴/작업 후 메모리 이득이 일관되지 않고 처리 시간이 늘었다. RAM/CPU 최적화로 채택하지 않으며 기본 3을 유지한다. panic abort나 저장 정책 변경도 적용하지 않는다.

[교대 원자료](../bench/results-resource-release-profiles-ab.json.gz) · [프로세스별 집계와 제외 표본](../bench/results-resource-profiles-summary.json).

### 코디네이터 추가 조사: host 메모리 증가의 원인과 채택한 변경 (2026-10-15 23:00~23:55)

P1이 판단을 보류한 "쿼리 50회 후반 RSS +4.06MiB/회" 관측을 프로세스별 `phys_footprint`로 다시 나눴다. 그러자 기준 원자료에서 host(`axdb`)의 footprint가 같은 10만 행 쿼리를 반복하는 동안 58→687MiB로 직선 증가했고, 탭을 닫아도 줄지 않았다. RSS 합계만 보면 이 증가가 WebKit 쪽 변동에 묻혔다.

- **누수가 아님을 확인했다.** 같은 시나리오를 [bench/leakprobe.py](../bench/leakprobe.py)로 20회 재현했다(footprint 69→465MB). 이때 `heap -s` 기준 살아 있는 할당은 **11MB**였다. `vmmap`/`footprint`의 나머지는 `Malloc Small (empty)` 230MB와 `Malloc Large (empty)` 108MB의 dirty 페이지였다. 즉 macOS 시스템 malloc이 해제한 결과 버퍼 페이지를 OS에 돌려주지 않고 쥐고 있었다. [원자료](../bench/results-resource-host-heap/)
- **효과가 없던 시도:** 대용량 스트리밍 직후 `malloc_zone_pressure_relief(NULL, 0)`를 호출했다. 20회 후 457MB로 변화가 없어 되돌렸다.
- **mimalloc 기본 모드(비secure):** 반복해도 증가 추세는 없었지만(20회 후 150MB), 기본 purge 지연 때문에 작업 직후 265~276MB를 잡고 있었다.
- **채택: mimalloc 0.1.52 + `mi_option_purge_delay = 0`** (`src-tauri/src/main.rs`). 교대 A/B를 각 3회 실행했고 모든 표본이 부하 게이트를 통과했다. 반복 추적은 첫 시행 각 1회다. [원자료](../bench/results-resource-mimalloc-purge0-ab.json.gz)

| 항목 (중앙값) | 시스템 malloc | mimalloc purge 0 |
|---|---:|---:|
| host footprint 60초 유휴 (MiB) | 30.1 | 34.0 |
| 앱 전체 footprint 60초 유휴 (MiB) | 81.8 | 85.6 |
| 유휴 CPU (50초 구간, 전 프로세스 합) | 0.1% | 0.1% |
| 10만 행 UI 완료 대용 시간 (ms) | 746 | 702 |
| host footprint 탭 종료 10초 후 (MiB) | 62.6 | 53.2 |
| 앱 전체 footprint 탭 종료 10초 후 (MiB) | 322.1 | 309.6 |
| host footprint 같은 쿼리 20회 후 (MiB) | 466.0 | 52.1 |
| host footprint 같은 쿼리 50회 후 (MiB) | 696.1 | 62.8 |
| 앱 전체 footprint 같은 쿼리 50회 후 (MiB) | 1466.3 | 382.5 |
| 반복 쿼리 1회 시간 범위 (ms) | 623~707 | 623~719 |

대가는 유휴 host 메모리 약 +4MiB다. 처리 시간과 유휴 CPU는 해상도 안에서 같았다. P1의 secure 모드는 보호 기능 비용(10만 행 712→771ms)이 있었고, 반복 시나리오는 측정하지 않았다. 이번 변경은 secure를 끄는 대신 purge를 즉시 하는 것이다. 보안 측면에서는 시스템 malloc과 같은 수준(힙 하드닝 없음)으로 되돌아간 것이지, 이전보다 약해진 것이 아니다.

### 최종 적용 상태 (P1 시점, 위 변경 이전)

이번 후보에서 일관된 RAM/CPU 이득과 처리 시간 보존을 함께 확인한 변경이 없어, 제품의 할당자·Tokio 기본값·release 3·결과 보유 정책은 기준 상태를 유지한다. 각 후보는 측정용으로 적용/빌드한 뒤 해당 부분을 되돌렸다. [최종 실행 파일 기록](../bench/results-resource-final-builds.json)의 최종/기준 SHA-256이 같다. 따라서 이후의 전/후 유휴 재측정 차이를 최적화 성과로 해석하지 않는다.

측정 스크립트, opt-in 계측, 실제 데이터/프로세스 확인, 실패/원본 설정 보존 검증과 조사 문서는 남긴다. 100ms를 넘는 비네트워크 연결 추가 단계가 없었으므로 검증·키체인 절차를 줄이는 변경을 하지 않았다. 쿼리 50회 후반 RSS 증가 관측은 남아 있으며, 이를 실제 누수 또는 해결로 확정하지 않는다.

### 프로세스별 기준선과 후보 전후 분해

단위는 RSS/footprint MiB, CPU %다. 값은 중앙값이며 기준선은 각 조건 5회다. 60초 유휴 CPU는 약 50초 누적 구간이고 나머지는 작업 후 1초 구간이라 작은 CPU 차이를 개선으로 해석하지 않는다. RSS 공유 페이지 및 footprint 원장 특성은 합계 해석의 한계다.

| 조건 | 프로세스 | RSS | footprint | CPU |
|---|---|---:|---:|---:|
| 시작 | host | 105.41 | 31.55 | 2.77 |
| 시작 | WebContent | 49.42 | 53.97 | 0.92 |
| 시작 | Networking | 12.05 | 4.89 | 0.00 |
| 시작 | GPU | 29.56 | 60.81 | 0.92 |
| 10초 | host | 107.67 | 31.08 | 0.00 |
| 10초 | WebContent | 50.53 | 35.34 | 0.00 |
| 10초 | Networking | 12.03 | 4.73 | 0.00 |
| 10초 | GPU | 29.69 | 16.13 | 0.00 |
| 60초 | host | 107.61 | 31.00 | 0.04 |
| 60초 | WebContent | 50.45 | 34.61 | 0.02 |
| 60초 | Networking | 11.97 | 4.70 | 0.00 |
| 60초 | GPU | 29.69 | 12.13 | 0.00 |
| 10만 행 | host | 380.73 | 51.02 | 0.00 |
| 10만 행 | WebContent | 186.14 | 180.80 | 0.91 |
| 10만 행 | Networking | 10.73 | 5.20 | 0.00 |
| 10만 행 | GPU | 35.47 | 26.45 | 0.00 |
| 300노드 | host | 381.77 | 52.78 | 0.00 |
| 300노드 | WebContent | 198.38 | 169.91 | 0.92 |
| 300노드 | Networking | 10.19 | 5.25 | 0.00 |
| 300노드 | GPU | 35.95 | 30.25 | 0.00 |
| 탭 10개 | host | 389.59 | 63.24 | 0.00 |
| 탭 10개 | WebContent | 294.84 | 275.08 | 0.00 |
| 탭 10개 | Networking | 10.14 | 5.58 | 0.00 |
| 탭 10개 | GPU | 39.42 | 28.16 | 0.00 |
| 닫기 직후 | host | 386.84 | 65.69 | 0.00 |
| 닫기 직후 | WebContent | 296.06 | 258.03 | 0.00 |
| 닫기 직후 | Networking | 8.52 | 5.63 | 0.00 |
| 닫기 직후 | GPU | 39.55 | 25.99 | 0.00 |
| 닫기 10초 후 | host | 386.28 | 65.58 | 0.00 |
| 닫기 10초 후 | WebContent | 295.67 | 238.31 | 0.00 |
| 닫기 10초 후 | Networking | 8.52 | 5.63 | 0.00 |
| 닫기 10초 후 | GPU | 39.03 | 16.56 | 0.00 |

후보별 아래 표는 같은 교대 시험의 **탭 종료 10초 후** 값이다. 5회/조건이며 후보는 모두 미채택이다. 화살표는 개선을 뜻하지 않는다. 전체 조건별 범위와 표본 수는 각 JSON 집계에 있다.

| 후보 | 프로세스 | RSS 기준→후보 | footprint 기준→후보 | CPU 기준→후보 |
|---|---|---:|---:|---:|
| 보조 참조 | host | 388.48→389.17 | 103.08→175.13 | 0.00→0.00 |
| 보조 참조 | WebContent | 295.20→292.95 | 234.45→231.50 | 0.00→0.00 |
| 보조 참조 | Networking | 9.45→9.83 | 5.58→5.42 | 0.00→0.00 |
| 보조 참조 | GPU | 38.94→39.12 | 16.06→15.84 | 0.00→0.00 |
| worker 2 | host | 383.27→383.64 | 192.47→308.85 | 0.00→0.00 |
| worker 2 | WebContent | 293.69→277.16 | 238.42→257.47 | 0.00→0.00 |
| worker 2 | Networking | 9.02→8.91 | 5.72→5.73 | 0.00→0.00 |
| worker 2 | GPU | 38.28→37.80 | 16.75→17.22 | 0.00→0.00 |
| worker 4 | host | 383.27→384.58 | 192.47→220.58 | 0.00→0.00 |
| worker 4 | WebContent | 293.69→295.91 | 238.42→239.25 | 0.00→0.00 |
| worker 4 | Networking | 9.02→8.53 | 5.72→5.75 | 0.00→0.00 |
| worker 4 | GPU | 38.28→37.94 | 16.75→16.20 | 0.00→0.00 |
| mimalloc secure | host | 386.50→194.47 | 64.64→68.02 | 0.00→0.00 |
| mimalloc secure | WebContent | 297.98→295.55 | 237.95→237.78 | 0.00→0.00 |
| mimalloc secure | Networking | 8.28→9.83 | 5.77→5.55 | 0.00→0.00 |
| mimalloc secure | GPU | 38.94→39.02 | 16.66→16.17 | 0.00→0.00 |
| release s | host | 395.89→389.83 | 62.17→64.22 | 0.00→0.00 |
| release s | WebContent | 296.31→296.09 | 233.00→235.22 | 0.00→0.00 |
| release s | Networking | 11.92→10.44 | 4.78→5.22 | 0.00→0.00 |
| release s | GPU | 41.98→40.05 | 14.75→15.72 | 0.00→0.00 |
| release z | host | 395.89→388.98 | 62.17→65.85 | 0.00→0.00 |
| release z | WebContent | 296.31→300.62 | 233.00→238.31 | 0.00→0.00 |
| release z | Networking | 11.92→9.77 | 4.78→5.45 | 0.00→0.00 |
| release z | GPU | 41.98→39.12 | 14.75→16.14 | 0.00→0.00 |

최종 제품 설정은 기준과 같아 최종 전/후 바이너리가 동일하다. 위 후보 시험을 채택된 최적화의 전후 표로 바꿔 표시하지 않는다.

## 최종 검증과 남은 한계

- `CARGO_TARGET_DIR=target-p1`, jobs 3: 현재 최종 소스 release 빌드 통과. 고정 `.app` 서명 검증 통과.
- 실제 테스트 DB를 지정한 `cargo test --locked --workspace --release`: **69 passed, 0 failed, 2 ignored**. PG/Neo4j/Mongo 실연결 계약·맥락·읽기 전용·취소·미리보기와 SQLite를 포함한다.
- ignored는 실제 Anthropic 유료 요청과 별도 opt-in OS keychain CRUD 왕복이다. 해당 공급자/실제 자격 증명 검증을 완료한 것으로 표시하지 않는다.
- `cargo clippy --locked --workspace --release -- -D warnings` 통과.
- 최종 `pnpm --dir ui test`: **9 files / 45 tests passed**. 후보 시점의 46개 결과는 미채택 후보 테스트를 포함한 별도 기록이다.
- Python 수집/집계 self-check, 설정 원본 보존 fault-injection 1개 통과. 이는 native 성능이나 전체 E2E 증거가 아니다.
- 유휴 재측정 **18/18 유효**, 앱 종료와 원본 설정 복원 확인.

[검증 요약](../bench/results-resource-validation.json), [Rust 출력](../bench/results-resource-cargo_test.txt), [clippy 출력](../bench/results-resource-clippy.txt), [vitest 출력](../bench/results-resource-vitest.txt)을 남긴다. 소스 해시는 실제 측정한 기준 빌드와 같음을 다시 확인했다. 후보 변경이나 Cargo.lock/할당자/프로필 변경은 남지 않았다. 제품 기능을 바꾸는 최적화가 없다는 결론과 계측 기반 추가를 구분한다.

실제 최신 기능의 통합 E2E 최종 회귀 확인은 코디네이터 소관이다. 50회 쿼리 후반 RSS의 +4.06MiB/회 관측은 해결됐다고 표시하지 않는다. 할당자/원장/회수 지연과 실제 누수를 가르는 추가 allocation 진단은 이번 결과로 확정하지 못했다. macOS/WKWebView 결과이며 다른 OS·디바이스나 production 성능으로 확대하지 않는다.

테스트 DB의 `ResourceP1_` 라벨 노드를 다시 조회해 잔여 0개를 확인했다. 마지막 기본 프로필 release 재빌드도 통과했으며 commit/push는 수행하지 않았다.

## 0.2.0 설치형 드라이버 기준선

최종 상태: **유효 기준선 5회/40개 표본과 A/B·native 스크롤 비교를 완료했다. `will-change` 제거는 RAM 개선이 재현되지 않아 미채택·원복했으며 제품 최적화 변경은 남기지 않았다.** 아래 준비 검증을 성능 개선으로 표시하지 않는다.

측정 소스는 코디네이터가 릴리스 원본으로 확인한 `d07c12f`다. 제공된 0.2.0 universal 앱은 드라이버 설치형 UI와 번들 내부 `resource.ts`의 자동화가 맞지 않아 연결 단계에서 중단됐다. 별도 detached 작업 트리에서 **계측 스크립트만 보정한 d07c12f 빌드**를 만들며, 제품 코드는 그대로 둔다. 이 앱은 arm64 release / Developer ID 서명이고 공증·업데이터 아카이브는 만들지 않는다. 원래 universal 복사본, 사용자 앱, 메인 `target/`·`dist/`는 수정하지 않는다.

실제 호스트 `Installer.install`을 호출하는 opt-in 테스트로 서명된 PostgreSQL·Neo4j 0.2.0 패키지를 먼저 격리 경로에 설치한다. 각 실행은 새 `AXDB_CONFIG_DIR`를 만들고 그 안의 `drivers`를 이 실제 설치 경로에 연결한다. 미검증 패키지 허용은 끈다. 패키지 서명·해시·압축 해제·실행 파일 handshake·등록을 통과한 실제 설치이며, 드라이버 디렉터리를 복사해 설치를 흉내 내지 않는다. [실제 설치 검증](../bench/results-resource-r2-install-smoke.json)은 1 passed였다. 설치 영수증의 성공 결과·도우미 SHA와 원 패키지/설치 manifest 일치를 검사하며, 앱의 서명·실행 파일 해시·handshake 검증도 유지한다. 반복 재설치 중 한 번 관측한 3초 handshake 초과는 별도 실패 자료로 남기고 제한을 늘리거나 재시도로 통과 처리하지 않았다. 테스트 DB는 PG `127.0.0.1:55432`, Neo4j `127.0.0.1:57687`만 사용한다.

수집기는 호스트와 WebKit, 격리 설치 경로 아래 직접 자식 PG·Neo4j 프로세스를 분리한다. 앱/드라이버/전체 합계를 실행별로 구한 뒤 중앙값을 낸다. 드라이버가 필요한 단계에서 해당 PID를 관측하지 못하면 표본을 거절한다. CPU는 기존과 같이 10초~60초 사이의 누적 CPU 시간 차이다. RSS와 footprint를 합치지 않으며, 프로세스 합계가 공유 페이지 중복을 포함할 수 있다는 기존 한계도 유지한다.

저전력 모드는 `lowpowermode=1`이다. 절대 처리 시간의 성능 판정은 하지 않는다. launch/작업 전 load ≤8 게이트를 유지하며 오염 표본을 정상 표본으로 바꾸지 않는다. 창은 측정 PID만 활성화하고 Retina 화면으로 옮긴다. `open`의 비동기 활성화 뒤 배율이 바뀌는 문제가 있어 실제 창 위치와 WebView `1280×768 / scale=2`를 [표시 진단](../bench/results-resource-r2-display-calibration.json)에서 확인했다. 그러나 반복 실행에서는 이동 명령 성공 뒤에도 WebView가 2→1로 돌아가는 사례가 있어 해당 표본을 거절했다. 코디네이터가 허용한 **1280×768 / scale=1 폴백**으로 일관된 5회 수집을 진행한다. 기존 기준선은 scale=2이므로 WebView 배킹 스토어 조건이 다르며 합계 차이를 그대로 최적화 성과로 해석할 수 없다. 창 준비 체크포인트 뒤 실제 viewport를 다시 기록하며 다른 앱 창·디스플레이 설정은 변경하지 않는다.

계측 호환성 보정은 커스텀 GlideSelect 선택, aria-label/필드 래퍼 탐색, 외부 제출 버튼에 대응하는 `requestSubmit`, 연결 카드에서 분리된 탐색기 대기, 접근성 행 수 표시 확인으로 한정한다. 정상 입력·선택 값, 실제 10만 행/정확한 첫 값, 그래프 노드 수, 탭 닫기 뒤 worker 종료 검증을 유지한다. 기존 UI 테스트 113개와 두 Python self-check가 통과했으며 native 5회 완료 증거와 구분한다.

준비 중 비활성 실행(RAF 정지), 원본 폼 선택자, 탐색기 위치, 행 수 표시 선택자에서 실패한 실행은 별도 원자료로 남기고 완료 기준선에서 제외한다. 이들 실행의 앱/드라이버 종료를 확인했다. 표시 진단도 성능 표본으로 집계하지 않는다.

### 유효 5회 결과

배율 1 폴백 기준으로 6회 실행 중 5회의 전체 매트릭스를 선택했다. 한 실행의 부하 초과/귀속 중복은 전체 실행 단위로 제외했고, 최초 4회와 독립 보충 1회를 원자료 참조와 함께 결합했다. **40/40 유효 표본, 실제 viewport 1280×768/1, 앱·드라이버 종료 5/5**를 확인했다. 첫 5회 원자료의 검증 실패를 지우거나 정상으로 바꾸지 않았다.

각 셀은 RSS / footprint 중앙값(MiB)이다. 합계 RSS만 최소–최대도 표시한다. 합계는 실행별 합산 뒤 중앙값이므로 앱/드라이버 중앙값끼리 더한 값과 다를 수 있다. 기존 값은 배율 2여서 직접 개선율로 해석하지 않는다.

| 조건 | 0.2.0 앱 | 드라이버 | 0.2.0 합계 RSS(범위) / footprint | 기존 합계(배율 2) |
|---|---:|---:|---:|---:|
| 시작 직후 | 266.6 / 142.1 | 0.0 / 0.0 | 266.6 (237.2–270.3) / 142.1 | 196.2 / 151.3 |
| 10초 유휴 | 265.4 / 122.7 | 0.0 / 0.0 | 265.4 (213.3–269.0) / 122.7 | 200.0 / 87.2 |
| 60초 유휴 | 259.8 / 115.1 | 0.0 / 0.0 | 259.8 (192.8–268.7) / 115.1 | 199.8 / 82.3 |
| 10만 행 × 10열 | 389.9 / 316.4 | 231.9 / 20.0 | 621.8 (609.9–638.7) / 339.3 | 613.1 / 263.7 |
| 300노드 그래프 | 442.2 / 352.1 | 243.5 / 25.8 | 684.9 (677.6–690.4) / 379.5 | 626.2 / 257.4 |
| 탭 10개 | 580.2 / 489.3 | 269.0 / 21.2 | 853.3 (802.7–903.3) / 508.6 | 732.6 / 371.8 |
| 모두 닫은 직후 | 583.6 / 472.7 | 267.3 / 21.2 | 856.7 (747.7–927.0) / 498.4 | 733.4 / 355.8 |
| 닫은 뒤 10초 | 572.3 / 393.4 | 234.8 / 21.1 | 767.7 (631.8–932.0) / 428.2 | 732.5 / 326.7 |

10초~60초의 유휴 CPU 합계는 중앙값 **0.180%** (0.040–0.760%, 5회)였다. 논리 코어 하나를 모두 쓰면 100%인 기준이며 하네스 폴링 부하를 포함한다. 이 유휴 구간은 아직 연결 전이므로 드라이버 프로세스가 0개다. 작업 후 1초 CPU 표본은 약 1% 단위 양자화 영향이 있어 처리 CPU나 작은 개선의 증거로 쓰지 않는다.

[유효 원자료 결합](../bench/results-resource-020-baseline-valid.json.gz) · [프로세스별 집계](../bench/results-resource-020-baseline-summary.json) · [첫 5회와 오염 실행](../bench/results-resource-020-installed-scale1.json.gz) · [보충 1회](../bench/results-resource-020-installed-scale1-supplement.json) · [계측 전용 빌드와 소스 해시](../bench/results-resource-r2-baseline-build.json).

### 개선 후보와 측정 근거

확인한 큰 증가분은 WebContent/GPU 쪽이며, 드라이버 두 프로세스의 사용 후 footprint 합계는 약 21MiB였다. 탭 닫기 때 결과 참조·탭별 결과 맵을 지우는 코드와 graph worker 종료 검증이 이미 있다. 반복 쿼리의 RSS 고수위를 곧바로 도달 불가능 객체의 누수라고 단정하지 않는다. 연결은 탭을 닫아도 열린 상태이므로 그 시점에 드라이버를 강제로 종료하는 변경은 적용하지 않는다.

가설은 가상 표 행마다 고정한 `will-change: transform` 힌트의 자원 비용이다. [W3C 명세](https://www.w3.org/TR/css-will-change-1/#dont-spam-will-change-across-too-many-properties-or-elements)는 힌트 남용의 자원 비용과 변경이 끝난 요소에서 최적화 자원을 회수할 필요를 설명하며, 실제 최적화 방식은 브라우저마다 다를 수 있다고 명시한다. 이 힌트는 이전 기준선 소스에도 있었으므로 설치형 구조 회귀의 원인으로 확정한 것이 아니다.

후보는 `VirtualTable.tsx`의 행 힌트 한 속성만 제거하고 실제 transform·위치·크기·데이터·검증은 유지했다. 별도 서명 앱으로 비교했고, 아래 결과에 따라 **후보를 미채택하고 원래 소스로 되돌렸다.**

### A/B 결과와 채택 판정

독립 기준선 5회의 load ≤8 조건은 바꾸지 않았다. 사용자 Chrome/WindowServer 부하로 A/B 시작이 지연돼 코디네이터가 **A/B에만 load ≤14, 같은 지점의 짝 간 load 차이 ≤3**을 허용했다. 지점별 통과 짝만 사용하고 핵심 지점에 3쌍 이상이면 판정한다는 지시를 적용했다. 5회 교대/10앱의 개별 표본 80개와 종료 10/10은 통과했고, 부하 조건을 적용한 지점별 수는 아래와 같다.

초기 수집기는 한 지점 탈락 때 회차 전체를 거절하는 최종 assert 때문에 비정상 종료를 기록했다. 그 원자료와 부정 플래그는 그대로 보존하며, 지점 단위라는 코디네이터의 명확화 뒤 `resource_report.py --paired-points`로 이미 기록된 개별 체크를 사용했다. 거절된 지점의 값을 정상으로 바꾸지 않았다.

| 지점 | 계획 짝 | 유효 짝 |
|---|---:|---:|
| 시작 직후 | 5 | 5 |
| 10초 유휴 | 5 | 4 |
| 60초 유휴 | 5 | 4 |
| 10만 행 | 5 | 4 |
| 300노드 | 5 | 4 |
| 탭 10개 | 5 | 4 |
| 모두 닫은 직후 | 5 | 4 |
| 닫은 뒤 10초 | 5 | 4 |

핵심 지점의 합계 footprint(MiB)다. Δ는 같은 짝의 후보−기준 차이를 먼저 구한 뒤 중앙값을 낸 값이며, 음수면 후보가 작다.

| 지점 | 유효 짝 | 기준 중앙값 | 후보 중앙값 | 짝 Δ 중앙값 (범위) | 개선/악화 짝 |
|---|---:|---:|---:|---:|---:|
| table-100000 | 4 | 344.2 | 351.3 | +1.6 (-39.6–+19.6) | 2/2 |
| tabs-10 | 4 | 534.6 | 529.5 | -0.6 (-15.6–+6.4) | 2/2 |
| tabs-closed-10 | 4 | 441.8 | 444.3 | -3.3 (-52.8–+28.3) | 2/2 |

세 지점 모두 개선 2쌍/악화 2쌍으로 방향이 섞였다. 닫은 뒤 10초의 RSS 중앙값은 594.4→536.6MiB였지만 footprint는 441.8→444.3MiB여서 이를 RAM 절감의 확정 증거로 채택하지 않는다. 유휴 CPU 역시 작은 차이를 개선으로 판정하지 않았다.

기존 native matrix의 표 스크롤 8개 시나리오×3회를 두 앱에서 비교했다. 24쌍 모두 부하 조건을 통과했고 각 앱의 전체 48개 native 시나리오·정리도 완료됐다. 아래 값은 각 시행의 프레임 p95를 모은 중앙값의 후보/기준 비율이며, 낮을수록 빠르다. 저전력 모드이므로 절대 프레임 시간 예산의 통과 판정은 하지 않는다.

| 표 / 모드 | 유효 짝 | 후보/기준 프레임 p95 비율 |
|---|---:|---:|
| table-1000-10-legacy | 3 | 1.000 |
| table-1000-10-stream | 3 | 1.000 |
| table-10000-10-legacy | 3 | 1.000 |
| table-10000-10-stream | 3 | 1.000 |
| table-10000-200-legacy | 3 | 0.708 |
| table-10000-200-stream | 3 | 0.778 |
| table-100000-10-legacy | 3 | 1.000 |
| table-100000-10-stream | 3 | 1.000 |

넓은 1만 행×200열에서는 상대 스크롤 지표가 좋아졌지만, CPU 사용률 절감이나 요구된 핵심 구간의 RAM 절감이 입증된 것은 아니다. 이번 RAM 최적화 후보의 채택 조건을 충족하지 못해 원복했고, 이 부가 관측은 별도 UI 성능 연구 근거로만 남긴다.

후보 UI 빌드는 통과했다. 전체 병렬 UI 검증은 **112 passed / 1 onboarding 5초 timeout**이었으며, 분리한 온보딩 파일은 시간 제한 변경 없이 **10 passed**였다. 실패한 전체 결과를 지우거나 전체 통과라고 표시하지 않는다. 기준 앱의 이전 UI 검증 113 passed, Python self-check, native 성능/기능 증거와 각각 구분한다.

[A/B 원자료](../bench/results-resource-020-will-change-ab14.json.gz) · [지점 단위 집계](../bench/results-resource-020-will-change-ab14-summary.json) · [스크롤 원자료](../bench/results-resource-020-will-change-scroll.json) · [판정·짝별 차이](../bench/results-resource-r2-decision.json) · [폐기한 후보 패치](../bench/r2-candidates/discarded-will-change.patch).

테스트 Neo4j의 측정 라벨 잔여 노드 0은 실제 조회로 확인했다. 앱/드라이버 측정 합계에는 DB 서버 프로세스는 포함하지 않는다. macOS의 고정 0.2.0 계측 빌드, 배율 1, 저전력 모드라는 범위를 넘어 다른 OS·배율·production 또는 현재 0.3.0 전체 E2E 성능으로 일반화하지 않는다. 콜드 재설치의 한 차례 handshake 초과는 이번 변경으로 해결했다고 표시하지 않는다.

### 전달과 최종 확인

[최종 데이터 감사](../bench/results-resource-r2-validation.json), [마지막 실제 Neo4j 정리 조회](../bench/results-resource-r2-final-graph-cleanup.json), [빌드 캐시 정리](../bench/results-resource-r2-target-cleanup.json)를 남겼다. 별도 작업 트리의 `target/`과 R2가 만든 메인 `target-r2/`만 삭제했고, 원본 `target/`·`dist/`는 수정하지 않았다. 측정 앱·설치 도우미 복사본은 `r2/`에 보존했다. 제품 후보는 원복됐고 commit/push는 하지 않았다.

[후보 전체 UI 결과](../bench/results-resource-r2-candidate-ui-test.txt), [온보딩 분리 진단](../bench/results-resource-r2-candidate-onboarding-focused.txt), [기준 UI 결과](../bench/results-resource-r2-ui-v3-test.txt), [기준 release 빌드](../bench/results-resource-r2-app-v3-build.txt), [후보 release 빌드](../bench/results-resource-r2-will-change-build.txt)를 각각 보존한다. 측정 앱이 release임은 빌드·서명 기록으로 확인한다. 기존 native matrix의 `Tauri debug` 환경 라벨은 하드코딩된 문자열이므로 실제 빌드 모드의 증거로 사용하지 않는다.

메인용 문서·수집기·집계기 패치는 `bench/r2-delivery/`에 남겼고 `git apply --check`로 적용 가능성을 확인했다. 메인의 `ui/src/perf/resource.ts`는 이미 측정한 계측 보정과 같아 덮어쓰지 않는다. 미채택 제품 패치는 재현용 `bench/r2-candidates/`에만 보존한다.

## 0.3.0 경량화

판정: 부분 개선, 최우선 `닫은 뒤 10초 ≤ 시작 + 50MiB` 목표 미달. 아래 native 비교는 1280×768/DPR2·load≤8의 한 쌍이며 위 R2 DPR1 기준선 중앙값과 직접 비교하지 않는다. 변경·실패·미수행 범위와 원자료는 [L1_REPORT](L1_REPORT.md)에 있다.

| 지표 | 기준/같은 조건 0.2.0 | L1 |
|---|---:|---:|
| 진입 JS / gzip | 제공 179 / 59KB | 76,405 / 26,381 bytes |
| 초기 정적 JS 전체 / gzip | 동등 측정 없음 | 186,715 / 85,519 bytes |
| 초기 CSS / gzip | 제공 126 / 30KB | 132,195 / 34,786 bytes |
| 결과 CSS 초기 로드 A/B / gzip | 현재 UI eager 149,593 / 37,743 bytes | 분리 132,195 / 34,786 bytes |
| 바이너리 | 실제 R2 arm64 17,022,976 bytes | arm64 26,898,864 bytes; 크기 목표 미달 |
| 시작→활성화 포함 2rAF | 4,094.6ms | 3,356.3ms; 정확한 first paint 아님 |
| 시작 앱 footprint | 132.4MiB | 121.2MiB |
| 10만 행 앱 footprint | 373.0MiB | 335.6MiB |
| 탭 10개 앱 footprint | 636.6MiB | 496.3MiB |
| 모두 닫고 10초 앱 footprint | 449.1MiB | 324.5MiB |
| 닫고 10초 − 시작 | +316.8MiB | +203.3MiB; +50 미달 |
| idle-60 앱 RSS / CPU | 197.8MiB / 0% | 251.4MiB / 0% |
| 닫고 10초 앱 RSS / CPU | 647.3MiB / 0% | 507.4MiB / 0% |
| 닫고 10초 드라이버 RSS / CPU | 218.2MiB / 0% | 170.3MiB / 0% |

앱 footprint는 host+WebContent+GPU+Networking 합이며 드라이버는 별도다. RSS와 footprint를 혼동하지 않는다. CPU는 해당 구간 측정이며 타이머 0의 증거가 아니다. [native 원자료](../bench/results-resource-030-l1-lazy-column-retina2.json.gz), [계산 요약](../bench/results-l1-native-summary.json), [bundle](../bench/results-l1-bundle.json), [CSS A/B](../bench/results-l1-css-split.json).

기능별 lazy import와 CSS 분리, raw MessagePack Channel, 인코딩 열 직접 조회와 지연 셀 디코드, 완료 콜백 해제, graph Worker/canvas 수명 버그 수정, Registry 지연 초기화, 드라이버 5분 단발 종료와 wait, restore 만료 단발 타이머, AI 텍스트 50ms 묶음을 적용했다. V8 추가 보유 힙 53,606,992→787,560 bytes는 native 메모리가 아닌 행 객체 비용 비교다. 실제 SQLite 강제 idle 경계의 RSS 8,432KiB→0은 실제 5분 대기 증거가 아니다.

vmmap 진단에서 결과 DOM/canvas는 제거됐고 WebContent의 WebKit Malloc이 가장 큰 잔여 영역이었다. 메모리 압력 모의 요청은 권한 오류로 거부되어 회수 가능한 캐시인지 확정하지 못했다. 마지막 idle 빈 렌더 실험은 수집 준비 단계에서 실패해 효과를 입증하지 못했고 되돌렸다. 해당 진단들은 성능 accepted에서 제외했다.

최신 집중 19검사·tsc, workspace 269검사/6 ignored, clippy -D warnings 및 Python self-check는 통과했다. 최신 전체 UI는 sidebar paging timeout 1건이 남아 코디네이터가 통합을 담당한다. PG 진정한 스트리밍/MCP byte cap은 L2 인계, release 프로필·전체 자식 수명·숨김 CPU·반복 native와 +50MiB 달성은 미완료다. 최종 아티팩트는 [영수증](../bench/results-l1-final-artifact.json)의 보관 앱이며 canonical bundle은 폐기 실험 빌드다. 릴리스 통과를 주장하지 않는다.


## L2: PG true streaming와 MCP 결과 상한 (2026-10-19)

PG의 protocol 1 선택 `execute_stream`을 capability로 협상한다. 같은 보호 실행 경로에서 200/2,000행 FETCH와 `simple_query_raw`, async sink와 기존 SDK 8프레임 채널을 연결했다. 시작의 미정 행 수와 종료의 실제 행 수·잘림·elapsed를 별도 이벤트로 전달한다. 기존 execute와 0.2.0 조합은 유지한다. MCP structured 결과는 기본 200 items/256KiB이며 초과분은 클라이언트·토큰·연결·세션에 묶인 bounded LRU 핸들로 페이지 조회한다.

아래는 동일 L2 debug 드라이버의 legacy/선택 streaming 경로를 교차 3쌍 측정한 중앙값이다. 드라이버 stdio 소비 경계이며 host/UI/WebKit·DB 서버는 포함하지 않는다. macOS `proc_pid_rusage(RUSAGE_INFO_V2)`를 약 2ms마다 샘플링한 최댓값으로 정확한 OS high-water mark는 아니다. 빌드가 병행될 수 있는 환경이며 load gate와 릴리스/native 앱 회귀 측정은 수행하지 않았다.

| 결과 총행 | 경로 | 피크 RSS MiB | 피크 physical footprint MiB | 첫 행 청크 ms | 전체 ms |
|---|---|---:|---:|---:|---:|
| 100,000 | legacy | 234.70 | 222.38 | 865.75 | 2,052.29 |
| 100,000 | streaming | 44.61 | 18.98 | 16.74 | 1,203.79 |
| 1,000,000 | legacy | 2,196.06 | 2,184.71 | 8,632.78 | 20,459.96 |
| 1,000,000 | streaming | 46.62 | 26.83 | 18.85 | 12,886.29 |

기존 결과 집합당 100,000행 상한을 바꾸지 않았다. 따라서 총 1,000,000행은 같은 요청 안에서 100,000행 결과 집합 10개를 사용했다. 단일 결과 1,000,000행 또는 기존 호스트의 누적 256MiB 응답 한도를 넘는 UI 전송 근거가 아니다. 느린 소비자(각 rows 청크에 2ms 대기) 단일 1,000,000행 실험은 RSS 50.97MiB, footprint 30.34MiB, 전체 19,920.52ms였다. 행 수 10배에 따라 드라이버 보유 메모리가 비례 증가하는 현상을 제거했지만, L1의 앱 종료 뒤 +50MiB 목표를 달성했다는 뜻은 아니다.

[원자료](../bench/results-l2-streaming.json), [계산 요약](../bench/results-l2-streaming-summary.json), [L2 보고서](L2_REPORT.md). MySQL true streaming은 보류했으며 기존 쿼리·취소·타임존 경로를 변경하지 않았다. 전체 workspace 검증은 Mongo 병렬 fixture 충돌로 실패했으며 집중 검사의 통과와 구분한다.

## L3: MySQL·MariaDB true streaming (2026-10-19)

L2에서 보류한 MySQL 확장을 완료했다. `capabilities.execute_stream=true`를 선언하고 protocol 1 선택 메서드를 기존 SDK의 8프레임 async 채널에 연결했다. `mysql_async::QueryResult::stream<Row>`로 text·prepared 결과를 읽으며 첫 200행·이후 최대 2,000행, 보수적 크기 추정 약 256KiB마다 sink를 await한다. legacy 수집 루프·guarded 취소·연결 보안 옵션은 그대로 유지한다. 시작은 미정 행 수, 종료는 실제 행 수·잘림·elapsed metadata이며 스트리밍 결과에는 행을 보관하지 않는다.

변경 전 main `d93ebd5`에서 보관한 debug 바이너리의 legacy 경로와 최종 debug 바이너리의 streaming을 DB·행 수별 교차 3쌍 비교했다. 아래는 각 지표의 중앙값이며 **전 → 후** 순서다. MySQL 8.4.11, MariaDB 11.8.9의 로컬 테스트 DB만 사용했다. 드라이버 stdio 경계로 host·UI·WebView·DB 서버 메모리와 화면 표시 시간은 제외한다. `proc_pid_rusage(RUSAGE_INFO_V2)`를 약 2ms마다 샘플링한 피크이며 정확한 OS high-water mark가 아니다. 공유 작업 트리의 빌드·다른 작업이 병행됐고 load gate는 적용하지 않았다.

| DB | 결과 총행 | 피크 RSS MiB | 피크 physical footprint MiB | 첫 rows 청크 ms | 전체 ms |
|---|---:|---:|---:|---:|---:|
| MySQL | 100,000 | 73.95 → 26.25 | 61.39 → 14.75 | 606.74 → 97.42 | 974.36 → 698.15 |
| MySQL | 1,000,000 | 581.36 → 43.98 | 568.25 → 24.13 | 5,506.80 → 85.31 | 8,540.53 → 5,973.91 |
| MariaDB | 100,000 | 73.61 → 26.39 | 61.30 → 15.02 | 536.68 → 98.74 | 898.44 → 801.08 |
| MariaDB | 1,000,000 | 584.59 → 44.30 | 572.14 → 24.84 | 4,311.82 → 82.60 | 6,434.44 → 4,760.39 |

기존 결과당 100,000행 상한을 유지했다. 총 1,000,000행은 한 요청 안의 100,000행 결과 집합 10개이며, 단일 결과 1,000,000행이나 host 누적 응답 256MiB 초과 전송을 검증한 것이 아니다. 행 한도 뒤에도 결과를 drain해 다음 결과 집합·세션 상태를 보존하므로 서버 작업량 상한은 보장하지 않는다. 단일 큰 셀은 분할하지 않으며 기존 16MiB 프로토콜 프레임 상한을 따른다.

각 rows 청크에 2ms 대기한 1,000,000 총행의 별도 실험에서 MySQL RSS/footprint는 45.14/24.02MiB, 전체 4,766.57ms였고 MariaDB는 43.58/25.17MiB, 전체 4,892.43ms였다. 총행 수에 비례해 메모리가 증가하던 수집 경로를 제거한 근거이며, release 성능·native 앱 footprint·L1 종료 뒤 +50MiB 목표 달성으로 일반화하지 않는다.

[측정 하네스](../bench/mysql_stream_bench.py), [26회 원자료·중앙값·바이너리 SHA-256](../bench/results-l3-streaming.json), [검증 명세](../bench/results-l3-verification.json), [검증 로그](../bench/results-l3-validation.txt), [L3 보고서](L3_REPORT.md). 재현은 보관한 변경 전 바이너리를 `uv run --script bench/mysql_stream_bench.py --baseline <변경-전-바이너리>`에 전달한다. baseline 생략 시 같은 바이너리의 legacy/streaming 비교이며 이번 표의 별도 바이너리 비교와 구분한다.

## M3: 닫힌 탭 retainer 제거 (2026-10-20)

SwipeRow의 최초 effect cleanup이 actions/icon VNode를 통해 과거 App과 닫힌 탭을 보유한 경로를 CDP에서 확인했다. width는 배열 대신 개수만 캡처하고 spring의 종료·취소 callback 참조도 해제했다. 탭 맵과 graph probe는 원래부터 비워졌으며, 임의 GC·압박·캐시 삭제를 제품에 넣지 않았다.

| 지표 | 기준 → 수정 |
|---|---|
| 일반 모션 Chromium, 닫고 GC 뒤 JS 힙 | 80,896,208 → 19,132,696 bytes |
| 같은 스냅숏의 detached DOM | 6,088 → 0 |
| Native 닫고 10초 앱 footprint 중앙값, 유효 5쌍 | 374.7 → 353.6MiB |
| Native 짝 차이 중앙값 | −23.8MiB; 3쌍 감소·2쌍 증가 |
| Native 시작 +50MiB 목표 | 수정 5회 모두 미달(+122.5~+233.8MiB) |

Native는 격리된 dev/debug=0 앱, 1280×768/DPR2, load≤14와 짝 차이≤3의 **지점별** 판정이다. raw 전체 쌍의 제외 플래그를 수정하지 않았으며 startup 등 실패 지점은 따로 제외했다. 두 앱의 모든 프로세스 종료를 확인했다. JS 참조 제거는 재현됐지만 일관된 native 감소·엔진 내부 원인·release 결과는 확정하지 않는다. host 194 passed/7 ignored, clippy -D warnings, 최신 UI 146 passed/1 skipped, tsc를 통과했다.

[M3 보고서](M3_REPORT.md) · [소유 경로·크기·검증](M3_RETAINERS.md) · [Chromium 영수증](../bench/results-m3-artifacts.json) · [Native 5쌍 집계](../bench/results-m3-native-summary.json).

## M4: 안전 시점 WebView 재활용 평가 (2026-10-20)

**제품 미채택.** 같은 native 창의 WebView 교체는 WebContent PID를 바꾸고 앱 footprint를 시작 +30.8MiB / +22.0MiB로 낮춰 두 준비 완료 지점에서 +50MiB 목표에 들어왔다. 호스트 세션과 저장한 UI 스냅숏도 유지했다. 그러나 조용한 숨김 상태에서 후속 비동기 대기가 103.8초 / 177.3초로 지연돼, 비활성 상태에서 안정적으로 끝난다는 조건은 통과하지 못했다. reload는 같은 PID를 유지했고 추가 10초 뒤 시작 +136.0MiB로 목표에 미달했다.

MCP 읽기 부하 진단은 별도 비점수 회차다. reload/교체 각각 840건, 실패 0건이었지만 이 부하 자체가 호출 기록 UI와 footprint에 영향을 주므로 무부하 R2/L1 점수에 합치지 않는다. cold GPU가 높은 시작값, 교체 뒤 실제 103.8초가 걸린 '10초' 표본과 진단 활성화가 겹친 대조군의 창 배치 실패도 분리했다. 첫 페인트 무지연이나 깜빡임 부재를 검증한 결과가 아니다.

제품 소스와 설정은 변경하지 않았다. 전용 dev/debug=0 앱과 target-m4만 사용했다. 호스트 195 passed / 7 ignored, clippy -D warnings, 제품 UI 146 passed / 1 skipped, 제품/실험 tsc는 통과했으나 조용한 교체 native E2E는 실패했다. [M4 보고서](M4_REPORT.md) · [상세 판정·재현](M4_WEBVIEW_RECYCLE.md) · [v3 짝 집계](../bench/results-m4-footprint-v3-summary.json) · [v4 진행 실패](../bench/results-m4-footprint-v4.json) · [MCP 진단](../bench/results-m4-bridge-v4.json) · [검증 명세](../bench/results-m4-validation.json).


## M5: 유휴 WebView 재활용 정책과 활성화 기준 (2026-10-20)

정책·설정·RAM 상태 복원과 MCP quiesce는 구현했지만 **기본 꺼짐을 유지**한다. 총 6쌍 중 startup 부하 차이 5.44인 한 쌍을 제외한 유효 5쌍에서 활성화 직후 footprint는 시작 +72.0~+87.0MiB(중앙값 +79.9MiB)로 +50MiB 목표에 모두 미달했다. 대조군보다 짝 차이 중앙값 191.8MiB를 반환했으나 목표 달성으로 보고하지 않는다. 초안·세션·사이드바는 모두 보존됐고 native 포커스→2rAF와 시각 조회 IPC의 상한은 92~245ms였다.

Rust의 10초 sleep은 약 10,003ms에 끝났고, 짧은 NSProcessInfo activity도 숨김 WebView의 IPC 관측 지연을 없애지 못했다. 숨김 지연은 별도 기록하고 점수에서 제외했다. 실제 앱 E2E 21/21·5/5·13/13과 최종 호스트 단위 131 passed/4 ignored, 후속 정책 4검사, clippy·tsc·UI 147 passed/1 skipped는 통과했다. 첫 Rust 전체 305 passed/7 ignored 뒤 후속 전체는 기존 Mongo view 스냅숏 오류 1건으로 중단되어 최종 workspace green을 주장하지 않는다. 측정 바이너리와 이후 안전 보강 앱을 구분하며, 실제 타이밍/설치 경합·지속 MCP 부하·키체인·영상 깜빡임·release는 미검증이다.

[M5 보고](M5_REPORT.md) · [상세 판정·한계](M5_WEBVIEW_RECYCLE.md) · [유효 5쌍 집계](../bench/results-m5-paired-summary.json) · [전체 실패 보존](../bench/results-m5-validation.json) · [최종 집중 검증](../bench/results-m5-focused-validation.json) · [실제 앱 E2E](../bench/results-m5-e2e.json).
