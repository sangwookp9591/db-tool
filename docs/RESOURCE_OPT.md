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
