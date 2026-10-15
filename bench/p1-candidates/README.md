# P1 최적화 후보

최적화 후보는 모두 측정 후 **미채택**이다. `05-dialog-layout.patch`는 비네트워크 단계가 100ms 이내여서 실험/적용하지 않았다. 공유 소스의 P1 요약 저장 변경은 되돌렸고, 기존 V1/A1 기능 변경은 보존했다. `00-summary-c1-preparation.patch`는 이전 C1 준비 빌드의 재현 자료이며 최종 소스에 적용하지 않는다. 최종 측정은 AI1 완료 커밋 `85231f2`와 동일한 공통 계측을 기준으로 했다. 준비 검사 이력에는 AI1 작성 중 적용 실패와 갱신 기록이 포함되며 최종 성능 결과와 구분한다.

## 공통 측정 순서

1. 코디네이터의 **조용한 구간 시작** 통보를 기다린다. 최종 AI1 소스와 lockfile, UI 의존성을 고정해 기준 `.app`을 만든다. `CARGO_TARGET_DIR=target-p1`, `CARGO_BUILD_JOBS=3`을 사용한다.
2. 후보는 고정 소스 사본에 하나씩 적용한다. 패치 적용 검사, 빌드, 소스/실행 파일 해시를 기록한다. mimalloc 후보는 사본의 lockfile만 갱신하고 버전을 기록한다. 공유 lockfile은 바꾸지 않는다.
3. 앱 시작과 작업군 전 1분 load ≤ 8을 최대 600초 기다린다. 작업 직후 다시 부하를 확인하며 초과 표본은 실패로 남긴다. load, ps, swap 상태를 함께 기록한다.
4. 기준/후보를 교대하고 짝수 회차에는 순서를 뒤집는다. 각 조건 5회: 시작 직후, 10초, 60초, 실제 PG 10만 행, Neo4j 300노드, 탭 10개, 모두 닫은 직후와 10초 후. 프로세스별 RSS·phys_footprint·CPU를 기록한다. 동일한 창 크기·배율인지 함께 확인한다.
5. 각 구성 첫 시행에서 같은 쿼리 50회, 쿼리 탭 열기/실행/닫기 50회를 수행한다. 1회 및 5회 간격 표본으로 전체/15회 이후 RSS 기울기를 MiB/회로 계산한다. 강제 GC는 하지 않는다.
6. 유효 표본 수와 범위, 실패를 모두 보고한다. 메모리 차이가 일관되지 않거나 처리 시간/정확성/보안이 나빠지면 채택하지 않는다. 측정하지 않은 후보도 채택하지 않는다. 채택 시 현재 공유 소스의 해당 부분만 반영하고 Rust 테스트·clippy·UI 빌드·vitest를 확인한다. 통합 E2E는 코디네이터가 확인한다.

## 후보별 계획

| 패치 | 변경 | 비교할 값 / 채택 전 확인 |
|---|---|---|
| `01-result-summary.patch` | 에이전트 요약을 별도로 보관하고, AI1이 읽는 raw 결과의 소유 탭이 닫히거나 연결 해제되면 보조 raw 참조를 해제한다. 실제 Query/Browse 결과는 변형하지 않는다 | WebContent의 탭 종료 직후/10초 후 RSS·footprint와 50회 반복 추세. 에이전트 요약 유지, 활성 탭의 AI1 표 접근, 취소/닫기 뒤 늦게 도착하는 콜백의 소유권을 확인한다. **최종 AI1 소비자에 맞춰 다시 갱신해야 한다.** |
| `02-workers-2.patch` | Tokio 기본 worker 수를 가용 CPU와 2 중 작은 값으로 제한한다 | Rust 호스트와 앱 전체 RSS·footprint, 50초 유휴 CPU, 10만 행 처리 시간. 명시된 `TOKIO_WORKER_THREADS`는 유지한다. 연결·취소·타임아웃과 동시 쿼리의 진행 여부도 확인한다. |
| `02-workers-4.patch` | 위와 같되 상한 4 | 상한 2와 별도로 비교한다. 두 패치는 함께 적용하지 않는다. blocking pool 상한은 바꾸지 않는다. 런타임을 앱 종료까지 유지하고 I/O·타이머를 활성화한다. |
| `03-mimalloc-secure.patch` | Rust 전역 할당자를 mimalloc 0.1.52 secure 모드로 교체한다 | 시스템 할당자 대비 호스트 RSS·footprint, 쿼리 CPU/시간, 50회 반복 추세, 바이너리 크기. WebKit 힙 개선으로 해석하지 않는다. 보호 기능을 끄지 않으며 네이티브 DB 드라이버와 취소·종료 경로를 확인한다. 의존성은 이득 확인 전 공유 소스에 넣지 않는다. |
| `04-release-s.patch` | release `opt-level = "s"` | 기본 3 대비 바이너리 크기·RSS·footprint와 10만 행 처리 시간. LTO/cgu/strip 및 panic 방식은 그대로 둔다. |
| `04-release-z.patch` | release `opt-level = "z"` | 위와 별도로 비교한다. 파일 크기 감소만으로 RAM/CPU 개선을 주장하지 않는다. s와 z는 함께 적용하지 않는다. |
| `05-dialog-layout.patch` | modal 표시를 DOM 삽입 후 layout effect에서 수행한다 | 연결 폼 열기의 p50/p95가 100ms를 넘고 IPC가 없는 경우에만 A/B한다. 최초 표시, autofocus, Escape/취소, 비밀번호 지우기와 다른 Dialog를 검증한다. |

실행 우선순위는 최종 기준선의 프로세스 분해 결과로 정한다. 작은 Rust 호스트보다 WebContent가 대부분이면 결과 소유권 후보를 먼저 확인한다. 후순위 후보를 실행하지 않았다면 이유와 함께 미측정으로 남긴다.

## 연결 추가 흐름

`90-connection-instrumentation.patch`는 공통 계측이며 최적화 후보가 아니다. 최종 `85231f2` 소스에 반영해 기준/후보 양쪽에 동일하게 사용한다. UI 빌드와 vitest 45개, 기준 release 빌드가 통과했다. native timer 검증과 실제 측정은 별도로 진행한다.

- native `.app`에서 PG와 Neo4j 각각 20회 실제 UI 흐름을 실행한다. 단계는 +→보이는 폼, 다른 드라이버→해당 필드 재구성, Test→서버 결과, Save→목록/모달 닫힘, Connect→탐색 첫 레벨이다. 연결 단계는 세션용 비밀번호 자동 입력을 포함하며 사람의 입력 대기 시간은 제외한다.
- 각 단계는 DOM 준비와 두 번의 RAF를 거친 표시 대용 시간, IPC 명령명/시간을 기록한다. 첫 시행과 후속 시행을 구분하며 p50/p95, 범위와 표본 수를 보고한다. 병렬 IPC의 시간을 단순 합산하지 않는다.
- 현재 +/선택 변경은 App에 이미 있는 manifest를 사용한다. 추가 버튼 안에 `drivers_list`를 강제로 넣지 않는다. 실제 단계의 호출 수와 별도로 warmed `drivers_list` IPC 20회, 호스트 부트스트랩 명령 시간을 기록한다.
- 호스트는 `drivers_list`, 저장/삭제, 연결 테스트/열기와 keychain get/save/delete 구간을 opt-in 로그에 남긴다. API 인자, 비밀번호, 행 데이터는 기록하지 않는다. IPC 외곽과 호스트 구간의 차이는 큐/직렬화/계측 비용을 포함한다.
- 동기 keychain이나 파일 쓰기가 느린지는 측정 후 판단한다. 비밀 삭제·저장 완료를 생략하거나 성공으로 먼저 표시하지 않는다. 100ms를 넘는 비네트워크 구간만 원인을 확인해 별도 후보로 A/B한다.
- 단계마다 부하/프로세스 기록과 창 크기·배율을 함께 확인한다. 실패 표본과 타임아웃을 보존하고 개선으로 집계하지 않는다. 게이트 실패/중단 시 UI가 정리할 수 있도록 abort 체크포인트를 전달한다.

계측 패치의 오류 전달/인자 비노출 테스트와 native timer 오류 종료 기록 테스트는 실제 빌드 뒤 실행한다. 적용 검사와 Python 파서 fixture 검사는 native 성능 검증이 아니다.

## 추가하지 않은 변경

- Neo4j pool은 이미 1, PG는 세션별 단일 연결이며 네트워크 세션도 연결 요청 때 생성된다.
- blocking pool의 기본 상한 512는 유휴 스레드 512개를 미리 생성한다는 뜻이 아니다. 필요할 때 생성하므로 상한만 낮추는 변경은 유휴 메모리 절감 근거가 없다.
- 그래프 Worker의 pause/terminate, CodeMirror destroy, 유휴 polling 제거와 동적 import는 이미 구현되어 있다. 탭의 결과를 자동 LRU 삭제하면 동작이 바뀌므로 추가하지 않는다.
- `incognito`/비영속 data store는 저장 정책을 바꾸며, `panic="abort"`는 실패 격리와 미저장 작업에 영향을 준다. 기존 동작 보존 조건상 적용 후보에서 제외한다.
- `lto=true`(fat), `codegen-units=1`, `strip=true`는 이미 설정되어 있다.

## 근거

- [Tokio Builder](https://docs.rs/tokio/latest/tokio/runtime/struct.Builder.html), [Tauri 런타임 설정](https://docs.rs/tauri/latest/tauri/async_runtime/fn.set.html)
- [mimalloc Rust 0.1.52 / secure](https://docs.rs/mimalloc/0.1.52/mimalloc/), [mimalloc 원문](https://github.com/microsoft/mimalloc)
- [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html), [Rust Performance Book](https://nnethercote.github.io/perf-book/build-configuration.html)
- 조사·적용 상태의 기준 문서: [RESOURCE_OPT.md](../../docs/RESOURCE_OPT.md)

## 최종 결과

- 결과 보조 참조: 각 5회 전후 방향이 일정하지 않아 되돌렸다.
- worker 2/4: 각 5회, footprint/작업 후 지표의 일관된 이득이 없어 기본값을 유지했다.
- secure mimalloc: 각 5회, 일부 RSS 감소와 달리 footprint/처리 시간의 이득이 없어 의존성을 되돌렸다.
- release s/z: 각 5회, 파일은 줄었으나 처리 시간이 늘고 RAM 이득이 일관되지 않아 기본 3을 유지했다. z 10초 유휴 1개는 외부 부하로 제외했다.
- Dialog: 실제 비네트워크 p95가 모두 100ms 이내여서 변경하지 않았다.
- 공통 계측은 기본 비활성 경로, 값 비노출 및 오류 전달을 검증해 남긴다.

전체 결과·범위·미확인 항목은 [RESOURCE_OPT.md](../../docs/RESOURCE_OPT.md), [검증 기록](../results-resource-validation.json)에 있다. 후속 적용 시 과거 패치를 강제로 적용하지 않고 현재 소스 소비자와 파일 해시를 다시 확인한다.
