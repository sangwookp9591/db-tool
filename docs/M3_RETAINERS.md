# M3 닫힌 탭 retainer 조사

2026-10-20. ADR-0004·ADR-0003·ARCHITECTURE·README와 M2_WEBKIT_MEMORY·L1_REPORT를 읽고 수행했다. **우리 코드의 닫힌 탭 보유 경로를 확인하고 수정했다.** Chromium의 detached DOM은 강제 GC 뒤 0개가 된다. 네이티브 5쌍의 닫힘 footprint 중앙값은 374.7→353.6MiB였으나 2쌍은 증가했고, **수정본 5회 모두 시작 +50MiB 목표는 미달**했다. 이 작업은 릴리스·공증·설치 검증이 아니다.

## 소유 경로와 최소 수정

닫힌 결과의 상위 경로는 현재 문서의 History 행 → SwipeRow의 초기 effect cleanup/stop 핸들 → 공유 함수 Context → `actions` → `icon` VNode → Preact의 부모 `__` 링크 → 과거 App VNode → 닫힌 `.tab-body` → 결과·편집기·DOM이다. native SVG가 root 쪽에 보인다는 이유로 브라우저 내부 캐시라고 판단하면 안 된다. 실제로 그 아래에 앱의 함수·VNode 참조가 있었다. `handles`, `lastRuns`, `lastResults`, `lastResultsTab`, `graphProbes`, 실행 `runs`는 원본에서도 이미 비워져 있었다.

[SwipeRow](../ui/src/ui/SwipeRow.tsx)의 `width` 함수는 fallback 폭을 위해 `actions.length`를 읽었다. 이 한 참조 때문에 같은 렌더 scope를 공유하는 최초 cleanup이 아이콘을 포함한 배열 전체를 보유했다. `actionCount = actions.length` 숫자만 캡처하도록 바꿨다. 폭·키보드·포인터·wheel·reduced-motion 동작과 action callbacks는 유지한다. 배열을 복사하거나 탭 결과를 강제로 퇴출하지 않는다.

[spring](../ui/src/ui/spring.ts)도 stop 핸들이 끝난 paint 클로저를 보유했다. 즉시 완료/reduced-motion에서는 모듈 수준 no-op을 반환하고, 정상 완료·취소에서는 paint를 같은 no-op으로 교체한다. 애니메이션 중의 callback과 유한 종료 시간은 유지한다. spring만 바꾼 중간 후보는 detached DOM 5,428→5,310으로 대부분 남았고, SwipeRow의 배열 캡처를 함께 제거한 뒤 0개가 됐다. 임의 GC·할당 압박·브라우저 저장소 삭제·라이브러리 변경은 제품에 넣지 않았다.

[Chrome 공식 문서](https://developer.chrome.com/docs/devtools/memory-problems/heap-snapshots)는 Context가 클로저 변수를 담으며, DOM과 JS의 양방향 참조를 스냅숏에서 추적할 수 있음을 설명한다. 스냅숏 생성 자체도 GC를 수행하므로 아래 `closed-10`을 GC 전 힙으로 해석하지 않는다. `closed-gc`에서는 별도로 CDP `HeapProfiler.collectGarbage`도 호출했다.

## 측정 방법과 크기 해석

[Playwright 하네스](../bench/m3_retainers.mjs)는 앱의 실제 Preact UI와 mock 백엔드를 격리 Chromium에서 실행한다. 1280×768/DPR2, 시작 → 10만 행 표 → 300노드 그래프 → 탭 10개 → 전부 닫고 10초 → 명시적 full GC → 스냅숏이다. 기본 fixture의 정확한 int8 첫 값과 그래프 positions 600개, 실제 행 수를 검증한다. mock 표는 기존 6열, 그래프는 299관계이며, native R2는 실제 PG의 10열·Neo4j 300관계다. 엔진 간 절대값을 직접 비교하지 않고 각 환경 안의 전후만 비교한다.

[분석기](../bench/m3_heap.py)는 snapshot meta에서 필드 위치를 읽고 weak edge를 제외해 root 경로를 찾는다. `detachedness=2`인 native DOM wrapper를 집계하며, `Memory.getDOMCounters.nodes` 전체 수와 구별한다. `--cuts`의 크기는 지정한 Context edge **집합**을 제거했을 때 strong-root에서 도달할 수 없게 되는 shallow bytes 합이다. 개별 객체 dominator 크기나 서로 더할 수 있는 독립 항목이 아니다. 스냅숏 파일 bytes, Runtime.getHeapUsage usedSize, native footprint도 서로 다른 값이다.

| 일반 모션 cold-cache 기준의 retainer 집합 | 해당 edge 집합에만 의존하는 bytes | 객체 수 | 그중 detached DOM |
|---|---:|---:|---:|
| Context의 `actions` edge들 | 15,738,306 | 581,978 | 6,088 |
| Context의 `paint` edge들 | 5,016,108 | 210,460 | 1,237 |
| 두 집합 합집합 | 15,738,894 | 581,999 | 6,088 |
| handles / lastRuns / graphProbes / runs | 0 entries | — | 보유 원인 아님 |

서로 다른 root가 같은 결과를 가리킬 수 있어 위 cut 크기만으로 전체 개선량을 설명하지 않는다. 실제 코드 변경 후 수거 결과를 함께 판정한다. 닫은 뒤에도 남는 module source·CodeMirror/언어 모듈·스타일·스키마 탐색 캐시는 열린 앱이 사용하는 자원이다. 닫힌 탭 참조가 없는 항목은 삭제하지 않았다. 이 증거는 M2의 모든 WebKit Malloc 할당을 식별한 결과가 아니며, 남은 native allocator 보유를 엔진 누수라고 확정하지 않는다.

## Chromium 전후 결과

주요 판정은 새 dependency cache와 일반 모션(`no-preference`)으로 각각 완주한 [기준·후보 영수증](../bench/results-m3-artifacts.json), [retainer 분석](../bench/results-m3-chromium-heaps.json)에 있다. Chromium 버전은 영수증에 기록했다. 첫 reduced-motion 완주에서도 JS 힙 78,913,476→19,054,924 bytes와 detached DOM 5,428→0을 관찰했으며 일반 모션에서도 재현됐다.

| 지점, JS used heap MiB | 원본 | 수정본 |
|---|---:|---:|
| startup | 9.90 | 9.89 |
| table-100000 | 39.84 | 39.84 |
| graph-300 | 43.11 | 42.32 |
| tabs-10 | 82.02 | 76.12 |
| closed-gc | 77.15 | 18.25 |

닫힌 뒤 일반 모션의 JS used heap은 **80,896,208→19,132,696 bytes(−76.3%)**, snapshot 파일은 **185,352,653→37,914,833 bytes**, detached DOM은 **6,088→0**이다. 문서에 남은 전체 DOM 노드는 7,956→1,141, JS listener는 1,959→297이다. 전체 DOM 수가 0이라는 주장은 아니다. 원본·후보 모두 마지막 결과가 비고 연결은 유지되며, 후보의 Map backing에도 entries가 없다. 시작보다 남는 약 8.35MiB의 JS 증가는 이 시나리오에서 처음 불러온 편집기·SQL·그래프 모듈과 열린 워크벤치의 자원도 포함한다.

## Native footprint: 5쌍, 전체 목표 미달

같은 고정 baseline/candidate 앱으로 순서를 교차한 총 10회 실행을 완료했고 앱과 자식 드라이버의 종료를 모두 확인했다. [첫 2쌍](../bench/results-resource-m3-native-pair.json.gz), [추가 3쌍](../bench/results-resource-m3-native-extra.json.gz), [집계·지점별 판정](../bench/results-m3-native-summary.json)에 원자료·SHA·제외 지점이 있다. 앱 footprint는 host + WebContent + GPU + Networking이며 드라이버는 제외한다. RSS를 footprint 대신 쓰지 않는다. 아래는 닫고 10초 체크포인트이며, 실제 경과 시간과 수집 비용은 원자료에 보존한다.

| 쌍 | 기준 MiB | 수정 MiB | 수정 − 기준 MiB | 해당 지점 load 차이 | 수정의 닫힘 − 시작 MiB |
|---|---:|---:|---:|---:|---:|
| 1 | 431.1 | 251.8 | -179.3 | 2.25 | +122.5 |
| 2 | 374.7 | 380.4 | +5.7 | 2.24 | +233.3 |
| 3 | 377.4 | 353.6 | -23.8 | 0.93 | +233.8 |
| 4 | 360.2 | 378.0 | +17.8 | 0.51 | +181.0 |
| 5 | 374.7 | 307.2 | -67.5 | 1.15 | +178.4 |

닫힘 지점은 **5/5쌍 모두** 개별 표본의 load≤14·역할별 PID·footprint·1280×768/DPR2 게이트와 짝 load 차이≤3을 통과했다. 각 구성의 중앙값은 374.7→353.6MiB(약 −5.6%), 짝 차이의 중앙값은 −23.8MiB다. 세 쌍 감소·두 쌍 증가이며 −179.3MiB 한 쌍을 일반적인 개선 폭으로 보고하지 않는다. 일관된 native RAM 감소나 통계적 유의성을 입증한 결과가 아니다.

전체 지점을 한꺼번에 보는 raw `paired_load_quality.accepted`는 1·2·5쌍에서 false다. 1쌍의 startup/idle-10, 2쌍의 닫은 직후, 5쌍의 startup이 짝 load 차이 3을 넘어 해당 지점만 제외했다. accepted를 고쳐 쓰지 않고 기존 `resource_report.py --paired-points` 방식으로 지점마다 판정했다. startup 중앙값의 유효 쌍은 3개이고 닫힘 중앙값의 유효 쌍은 5개이므로 서로 빼서 종료 delta를 만들지 않는다. 표의 마지막 열은 각 수정본 실행 자체의 시작값과 비교한 값이다.

수정본의 닫힘−시작은 +122.5~+233.8MiB, 닫힘−idle-10은 +126.8~+255.3MiB로 **5회 모두 +50MiB 미달**이다. 활성 10만 행·그래프 지점 중앙값도 각각 326.4→328.4MiB, 433.5→434.7MiB로 줄지 않았다. JS의 강한 참조 제거와 native OS footprint의 즉시 반환은 동일한 판정이 아니다. native 잔여 할당의 정확한 소유 경로나 엔진 내부 원인, release 앱의 결과는 이번 증거만으로 확정하지 않는다. 실제 JS 해제 효과가 반복 측정된 두 제품 수정만 유지한다.

## 검증과 격리

[회귀 검사](../ui/src/perf/tabRetainers.test.ts)는 별도 headless Chromium에서 10탭의 open/run/close 뒤 detached DOM 0, handles/lastRuns/graphProbes/runs의 실제 Map backing에 entries 없음, 마지막 결과·활성 탭 없음, 연결 2개 유지까지 검사한다. 또한 caller가 stop 핸들을 계속 보관한 상태에서 즉시 완료·정상 완료·취소의 paint owner가 WeakRef에서 수거되는지 확인한다. 원본 두 파일만 복원한 격리 소스 사본에서 두 검사가 실제 실패했고(닫힌 DOM 4,965개), 수정본에서 둘 다 통과했다. 공유 트리의 제품 코드를 RED 검사를 위해 되돌리지 않았다.

초기 수집 준비 중 미설치 mock PG, 닫힌 실행 옵션, 연결 완료를 기다리지 않은 조회를 발견해 하네스를 수정했다. 별도 cold-cache 검증에서는 Vite 8의 dependency scanner가 기본 React JSX로 스캔해 Preact가 이중 로드되는 오류를 확인했다. 수집기만 별도 cacheDir와 `optimizeDeps.rolldownOptions.transform.jsx.importSource=preact`를 사용하도록 고쳤다. 이 실패들은 완주 자료와 분리해 target-m3에 보존하며 실패 회차를 성과로 세지 않는다.

사용자 앱·사용자 DB·SSH·다른 앱 창에는 접근하지 않았다. 코디네이터가 명시적으로 허용한 target-m3 전용 측정 앱의 창만 기존 R2 하네스로 배치·활성화한다. 테스트 앱은 전용 CFBundleIdentifier와 AXDB_CONFIG_DIR를 쓰며, 설치형 드라이버도 비공개 테스트 경로에 실제 서명 패키지를 설치해 사용한다. commit/push는 하지 않았다. Operation·Capabilities·프로토콜·커맨드·AI 승인/실행 경로는 변경하지 않았다.

공통 검증은 `CARGO_TARGET_DIR=target-m3 CARGO_BUILD_JOBS=6`으로 `cargo build --workspace --bins` 뒤 `AXDB_DRIVER_DIR=target-m3/debug cargo test -p axdb`를 실행해 **194 passed / 7 ignored**였다([로그](../bench/results-m3-host-test.txt)). 허용된 PG·Neo4j·Mongo·Redis 테스트 서버와 SQLite를 사용했고, 별도 MySQL·TLS·Neo4j 복원 대상·실제 IAM/SSH·유료 AI 공급자처럼 제공하지 않은 환경의 early-return/ignored는 실연결 검증으로 세지 않는다. backup_streaming 테스트가 갱신하는 기존 W4 원자료는 원래 바이트로 복원하고 [M3 실행 사본](../bench/results-m3-backup-streaming.json)만 따로 남겼다.

최신 [전체 UI](../bench/results-m3-ui-verified.txt)는 **146 passed / 1 skipped**, [tsc](../bench/results-m3-tsc-verified.txt)도 통과했다. [집중 browser 검사](../bench/results-m3-browser-final.txt)는 두 파일·3검사 통과다. 최초 전체 UI 146/1에서 이후 cold-cache graphLifecycle 준비 실패, 새 retainer 준비 실패가 각각 발생했다([첫 실패](../bench/results-m3-ui-test.txt), [두 번째 실패](../bench/results-m3-ui-final.txt)). 기존 onboarding/branched-menu/extra 검사의 패턴대로 그래프·retainer 검사에 Preact scanner 설정과 독립 cacheDir를 적용한 뒤 최종 전체를 검증했다. timeout 확대·실패 숨김·통과할 때까지 재시도는 사용하지 않았다.

[heap parser self-check](../bench/results-m3-heap-self-check.txt)는 weak root 지름길을 제외한 실제 strong Context 경로와 빈 Map 판정을 검사한다. [R2 self-check](../bench/results-m3-r2-self-check.txt), Python 구문 검사, 수집기 JS 구문 검사, diff 공백·로컬 문서 링크 검사도 실행했다. 제품 UI 최종 빌드 파일들은 native 후보 앱에 들어간 UI 빌드와 byte-for-byte 일치한다. 소스·앱·스냅숏 SHA는 [영수증](../bench/results-m3-artifacts.json)에 있다.

`cargo clippy -p axdb --all-targets -- -D warnings`도 [통과](../bench/results-m3-clippy.txt)했다. 측정용 native 앱은 `dev, debug=0, incremental=0`이며 release 앱의 footprint·공증·배포·사용자 설치 상태를 검증한 결과가 아니다. CARGO_BUILD_JOBS=6과 target-m3만 사용했다. 시작 시 유일한 기존 untracked `src-tauri/tests/mysql_streaming_compat.rs`는 수정하지 않았다. 이번 제품 변경은 SwipeRow와 spring 두 파일이며, 나머지는 수집기·회귀 검사·검증 자료·문서다.

## 재현

```sh
node bench/m3_retainers.mjs candidate no-preference
python3 bench/m3_heap.py target-m3/candidate/closed-gc.heapsnapshot \
  --cuts --output target-m3/candidate-analysis.json
pnpm --dir ui exec vitest run src/perf/tabRetainers.test.ts
python3 bench/m3_heap.py --self-check
```

기준 비교는 별도 UI 소스 사본에서 SwipeRow.tsx와 spring.ts만 원본으로 돌리고 `AXDB_M3_UI_ROOT`로 그 사본을 지정한다. 공유 작업 트리를 원복하지 않는다. 수집기 출력 디렉터리가 이미 있으면 덮어쓰지 않고 실패한다. native는 [R2 수집기](../bench/resource_opt.py)에 target-m3의 baseline.app/candidate.app, [실제 서명 설치 영수증](../bench/results-m3-install.json), 두 패키지와 영수증의 helper를 넘겨 `--retina --paired-load-14 --no-leak`으로 교차 실행했다. 이 앱·대상 경로·서명·프로필은 사용자 앱과 분리돼 있다. 새 실행 때는 새 variant 이름을 사용해 원자료를 보존한다.
