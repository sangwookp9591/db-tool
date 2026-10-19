# M4: 안전 시점 WebView 재활용

2026-10-20. **평가 완료·제품 미채택.** ADR-0004, ADR-0003, ARCHITECTURE, README와 M2/M3의 결과를 기준으로 한다. 제품 dist/, target/, target-rel/, 사용자 앱과 사용자 DB/SSH는 변경하지 않았다. native 창 조작은 전용 측정 앱으로 제한했다.

`target-m4/source`의 독립 소스 사본과 `target-m4/M4Probe.app`만 사용한다. 전용 identifier와 WKWebsiteDataStore UUID, AXDB_CONFIG_DIR를 사용하며 CARGO_TARGET_DIR=target-m4, CARGO_BUILD_JOBS=6, dev/debug=0/incremental=0으로 빌드한다. 실험 코드는 [미적용 패치](../bench/m4-prototype.patch)에 보관한다. 제품에서 AI 출력은 계속 계획이며, 승인/실행 경로를 바꾸지 않는다.

먼저 같은 바이너리의 대조군, `WebviewWindow.reload()`, 같은 native 창에서 기존 WebView를 닫고 새 WebView를 만드는 방식을 비교한다. Tauri 2.12.4의 [reload](https://docs.rs/tauri/2.12.4/tauri/webview/struct.WebviewWindow.html#method.reload)는 페이지 재로딩이며 콘텐츠 프로세스 종료를 보장하지 않는다. [Window.add_child](https://docs.rs/tauri/2.12.4/tauri/window/struct.Window.html#method.add_child)는 `unstable` 기능이 필요하다. 이 기능은 실험 사본에서만 켠다. 사설 WebKit selector, 프로세스 kill, browsing-data 삭제, 임의 메모리 압박은 사용하지 않는다.

실험은 R2/L1의 시작 → 유휴 10/60초 → 실제 PG 10만 행×10열 → Neo4j 300노드 → 탭 10개 → 모두 닫기 → 10초 흐름을 유지한다. 그 다음 native 창을 숨기고 실제 120초 뒤 재활용한다. 대조군은 같은 숨김 대기만 수행한다. 열린 탭과 host 실행이 남아 있거나 대기 중 창이 다시 보이거나 활성화되면 중단한다. 실험에는 설치/백업/가져오기를 동시에 실행하지 않는다. 이 제한된 실험 게이트를 제품의 전체 안전 정책 구현으로 취급하지 않는다.

결과 행은 저장하지 않고 호스트 RAM에 UI 연결 맵, 서버 정보, 사이드바 상태와 실험 체크포인트만 보관한다. 재로딩 후 같은 세션 ID로 UI를 복원하고 server_info를 실제 조회한다. 프로필과 온보딩은 기존 host 저장소, 테마는 기존 localStorage에서 복구된다. 실험은 모든 탭이 닫힌 상태이므로 편집기 초안 보존과 전체 작업/설치/승인 경합 검증을 대체하지 않는다.

[측정기](../bench/m4_memory.py)는 R2의 host+WebContent+GPU+Networking physical footprint, 1280×768/DPR2, load≤14, 역할별 PID 개수 검증을 재사용한다. 드라이버 메모리는 앱 합계에서 제외한다. 시작뿐 아니라 cold GPU 영향을 줄인 유휴 10/60초 기준도 함께 비교하고, 짝 지점 load 차이는 ≤3이어야 한다. MCP 연속 요청은 `--bridge-probe`의 별도 비점수 회차에서 검사하며 응답 본문과 토큰은 기록하지 않는다. 숨김 복원이 끝난 뒤 측정 앱 창만 표시·활성화하고 invoke 시작부터 2rAF까지 시간을 잰다. 이 값에는 native show/focus IPC 시간도 포함된다.

첫 [파일럿](../bench/results-m4-pilot.json)은 복원 대기에서 `.connection[data-active]`를 연결된 항목 수로 오인해 실패했다. 이 속성은 선택한 연결 하나만 나타낸다. `.status-dot.connected`로 실제 연결 수를 확인하고 UI 스냅숏 전체 일치 검사도 추가했다. 실패 앱·드라이버 종료를 확인하고 생성한 테스트 그래프 라벨만 삭제했다.

두 번째 [진단 회차](../bench/results-m4-paired-v2.json)에서는 연속 MCP 요청 자체가 호출 기록 UI를 갱신하며 메모리를 늘렸다. 재활용하지 않은 대조군도 닫고 10초 377.3MiB에서 숨김 대기 뒤 566.6MiB로 증가했고 846개 요청을 처리했다. 이 값은 무부하 R2/L1 목표의 A/B 점수로 사용하지 않는다. 대조군 완료 후 다음 reload의 유휴 구간에서 수집기를 직접 중단하고 앱·드라이버 종료를 확인했다. 오류 조건이나 timeout을 완화하지 않았으며, 최종 footprint 회차와 MCP 부하 진단을 분리했다.

## 채택 판단

**제품에는 채택하지 않는다.** reload는 목표에 미달했고, 같은 창의 WebView 교체는 메모리를 반환했지만 조용한 숨김 상태에서 비동기 진행이 장시간 정체됐다. 복원 완료라는 사실만으로 사용자에게 보이지 않는 재활용이 완성됐다고 판정하지 않는다. 제품의 자동 재활용, 기본 켬 설정, 유휴 정책과 편집기 초안 복구 기능을 추가하지 않았다. 미적용 실험 패치를 제품 기능으로 취급하면 안 된다.

| 회차 / 방식 | 시작 MiB | 유휴 10초 MiB | 닫고 10초 MiB | 숨김 120초 후 준비 완료 MiB | 준비 완료 − 시작 MiB |
|---|---:|---:|---:|---:|---:|
| v3 대조군 | 199.2 | 126.3 | 366.1 | 339.0 | +139.8 |
| v3 reload | 130.1 | 130.4 | 414.6 | 291.6 | +161.5 |
| v3 같은 창 교체 | 128.9 | 133.2 | 343.4 | 159.7 | +30.8 |
| v4 같은 창 교체 | 139.4 | 136.0 | 338.5 | 161.4 | +22.0 |

첫 세 행은 같은 바이너리의 [v3 원자료](../bench/results-m4-footprint-v3.json)다. 준비 완료 지점의 짝 load 차이는 reload 1.31, 교체 2.47로 모두 ≤3이었다. 교체는 대조군보다 179.3MiB 작았고 WebContent PID도 바뀌었다. reload는 같은 PID를 유지했다. 각 회차의 같은 세션 ID와 server_info 조회, 저장한 UI 스냅숏 일치 검사를 통과했다. 교체의 이 두 준비 완료 지점은 시작과 유휴 10/60초 어느 기준으로도 +50MiB 안이다. 단, v4는 이후 진행 실패로 전체 시나리오가 완료되지 않았다.

v3 대조군은 추가 10초 후 227.8MiB, 활성화 후 284.1MiB였다. 높은 cold 시작값 199.2MiB만 기준으로 하면 10초 후는 +28.6MiB지만 유휴 10초 기준으로는 +101.5MiB다. 따라서 cold GPU spike를 목표 통과 근거로 사용하지 않는다. reload는 추가 10초 후 266.1MiB로 시작 +136.0MiB였고, 같은 대조군보다 오히려 38.2MiB 높았다. 활성화→2rAF 근사값은 대조군 101ms, reload 90ms였다.

v3 교체의 추가 10초 지점은 실제 준비 완료 표본에서 **103.8초** 뒤 수집됐다. 이때 134.8MiB, 활성화 후 220.0MiB, 활성화 요청→2rAF 242ms가 관측됐지만, 동일 시점 A/B 점수에서는 제외한다. [집계](../bench/results-m4-footprint-v3-summary.json)는 원래 R2의 accepted 플래그를 수정하지 않고 별도의 시간 게이트로 이를 제외한다. 교체의 첫 페인트가 지연되지 않는다는 증거로도 사용하지 않는다. 두 번째 대조군은 창 배치가 실패했다. 진단 활성화 명령이 이미 이 다음 대조군으로 넘어간 뒤 실행됐으므로 이 실패를 제품 결함으로 귀속하지 않는다. 모든 해당 host/driver 종료는 확인했다.

v4에서는 JavaScript setTimeout 대신 native tokio 타이머로 10초를 기다리게 바꾸고 15초 상한 검사를 추가했다. [v4 원자료](../bench/results-m4-footprint-v4.json)에서 준비 완료까지는 368ms였고 같은 UI/세션이 복원됐지만, 그 뒤 10초 대기는 **177,267ms**로 기록돼 `Hidden native wait exceeded 15 seconds`로 실패했다. 정체 중 별도의 MCP 읽기 요청은 49ms에 응답했고 창을 활성화하지 않았다([관측](../bench/results-m4-hidden-progress-diagnostic.json)). 이 요청 뒤 대기 실패가 보고됐다. 실제 대기와 응답 전달 중 어느 구간이 지연됐는지, WebKit·Tauri/Wry·OS 전원 정책 중 어느 계층이 원인인지는 이번 자료로 확정하지 않는다. 단순히 JS 타이머만 교체해 해결됐다고 주장하지 않는다. 실패 앱 종료와 해당 테스트 그래프 라벨 삭제를 [확인](../bench/results-m4-v4-cleanup.json)했다.

이 결과는 프로세스를 바꾸면 메모리를 반환할 수 있음을 보인다. 그러나 비활성 상태에서 안정적으로 끝나고 다시 활성화할 때 첫 페인트가 지연되지 않는다는 요구를 충족한 구현은 아니다. 후속 권장 범위는 새 WebView의 숨김 상태 IPC/타이머 진행을 최소 재현으로 분리하고, 공개 API만으로 첫 페인트를 준비할 수 있는지 확인하는 것이다. 그 다음에야 초안·설정·설치/백업/가져오기·편집/승인 경합을 포함한 제품 안전 정책을 구현하고 다시 짝 비교할 수 있다.

## MCP 연속성

[별도 부하 진단](../bench/results-m4-bridge-v4.json)은 같은 v4 앱에서 실제 localhost MCP `get_ui_state`를 약 150ms 간격으로 순차 요청했다. raw 결과에 `bridge_stress=true`, `score_eligible=false`를 명시했다. URL과 토큰을 메모리에 보관해 재활용 후에도 같은 서버로 요청했으며 본문·토큰을 증거 파일에 기록하지 않았다.

| 방식 | 요청 | 실패 | 최대 응답 | native 10초 대기의 실제 UI 경과 | 활성화 요청→2rAF |
|---|---:|---:|---:|---:|---:|
| reload | 840 | 0 | 193.5ms | 10,179ms | 106ms |
| 같은 창 교체 | 840 | 0 | 285.4ms | 10,005ms | 93ms |

두 회차 모두 상태/세션 복원과 전체 UI 흐름을 완료하고 host/driver 종료를 확인했다. 요청을 재시도해 통과시킨 결과가 아니다. **요청이 지속되는 조건**에서 MCP 읽기 연속성을 확인한 것이며, 조용한 유휴 상태의 177초 진행 실패를 대체하거나 지우지 않는다. 쓰기·파괴적 작업·승인 대기 중 재활용을 허용했다는 증거도 아니다. 이 경합에 대한 제품 정책은 구현하지 않았다.

## 검증과 재현

전용 소스 사본에서 `CARGO_TARGET_DIR=target-m4 CARGO_BUILD_JOBS=6 cargo build --workspace --bins`를 먼저 완료했다([최종 build](../bench/results-m4-build.txt)). 이어 `AXDB_DRIVER_DIR=target-m4/debug cargo test -p axdb`는 **195 passed / 7 ignored / 0 failed**, `cargo clippy -p axdb --all-targets -- -D warnings`와 실험 UI tsc는 통과했다([검증 명세](../bench/results-m4-validation.json), [host](../bench/results-m4-host.txt), [clippy](../bench/results-m4-clippy.txt), [실험 tsc](../bench/results-m4-tsc-probe.txt)). 지정된 PG axdb_test, Neo4j, Mongo, Redis 테스트 서버와 임시 SQLite를 제공했다. MySQL·TLS·별도 Neo4j 복원 대상·실제 IAM/SSH·OS 키체인 실연결·유료 AI처럼 제공하지 않거나 opt-in인 경로의 early-return/ignored는 실연결 증거로 세지 않는다. backup_streaming 검사가 갱신한 W4 증거는 소스 사본 안에만 생겨 기존 W4 파일을 변경하지 않았다.

제품 UI는 소스 변경 없이 전체 검사 **146 passed / 1 skipped**와 tsc를 통과했다([UI](../bench/results-m4-ui.txt), [tsc](../bench/results-m4-tsc.txt)). 실제 앱의 R2/L1 UI 흐름, 같은 세션 조회, MCP 왕복을 실행했지만 조용한 교체 회차의 native E2E는 실패했다. 전체 native E2E 통과로 보고하지 않는다. 기존 `bench/e2e-bridge.mjs` 전체 시나리오, 편집기 초안·진행 중 설치/백업/가져오기·승인 경합, 실제 사용자 온보딩/설정 상태, 영상 기반 첫 페인트·깜빡임, release·공증·배포는 검증하지 않았다.

두 Python 수집/집계 self-check, 구문 검사, 두 패치의 apply-check, 문서 링크·공백·비밀값 포함 검사도 수행했다. [산출물 영수증](../bench/results-m4-artifacts.json)에 기준 commit, 두 앱/소스/패치 SHA-256, 설치형 테스트 드라이버 SHA-256과 검증 결과를 남긴다. 시작은 clean main이었으며 제품 Rust/TypeScript/Cargo 설정 diff는 종료 시에도 0이다. 남긴 파일은 bench 실험/원자료/미적용 패치와 이 보고서, M4_REPORT 및 RESOURCE_OPT의 결과 요약이다. commit/push는 하지 않았다.

최종 정리 검사에서 진단 LaunchServices 활성화 뒤 별도의 `target-m4/M4Probe.app` 인스턴스가 남은 것을 발견했다. 기록된 수집 대상 PID의 종료 확인만으로 이 별도 인스턴스까지 종료됐다고 볼 수 없었다. 해당 경로와 PID를 확인해 종료했고, 수집기는 이제 프로세스 경로를 검증한 **기존 PID만** 활성화한다. 이 수정은 측정 후 적용했으며, [별도 native 창 검증](../bench/results-m4-pid-activation.json)에서 DB 작업 없이 배치 성공·추가 인스턴스 없음·종료를 확인했다. 무관한 PID 거부 self-check도 추가했다. 영수증에는 실제 측정 당시 수집기 SHA와 [이후 수정 차이](../bench/m4-collector-pid.patch)를 구분했다. 과거 원자료의 점수나 해시를 새 코드의 실행 결과로 바꾸지 않았다. 모든 소유 테스트 그래프 라벨의 잔여 노드 0, 측정 앱/드라이버 잔여 프로세스 0도 최종 확인했다([정리/검증](../bench/results-m4-final-checks.json)).

재현은 영수증의 기준 commit을 **비어 있는 별도 소스 디렉터리**로 복사한 뒤 [v3 패치](../bench/m4-prototype-v3.patch) 또는 [v4 패치](../bench/m4-prototype.patch)를 그 사본에만 적용한다. UI 출력도 target-m4 아래 별도 경로에 둔다. v4의 TAURI_CONFIG는 identifier=`dev.axdb.m4probe.v4`, devUrl=null, frontendDist=전용 UI 절대 경로로 지정한다. `.app`의 CFBundleExecutable=axdb, CFBundleIdentifier=같은 probe identifier, NSHighResolutionCapable=true를 지정하고 ad-hoc 서명한다. 제품/사용자 앱을 대체하지 않는다. 테스트 드라이버는 영수증이 가리키는 실제 서명 설치 경로를 사용한다.

```sh
python3 bench/m4_memory.py --self-check
python3 bench/m4_summary.py --self-check
M4_DRIVERS="$(python3 -c 'import json; from pathlib import Path; print(Path(json.loads(Path("bench/results-m3-install.json").read_text())["config"]) / "drivers")')"
# 새 --name은 기존 원자료를 덮어쓰지 않아야 한다.
python3 bench/m4_memory.py target-m4/M4ProbeV4.app \
  --drivers "$M4_DRIVERS" \
  --name fresh-footprint --modes control,replace
python3 bench/m4_memory.py target-m4/M4ProbeV4.app \
  --drivers "$M4_DRIVERS" \
  --name fresh-bridge --modes reload,replace --bridge-probe
```
