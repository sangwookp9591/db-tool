# M5: 유휴 WebView 재활용 제품 정책과 활성화 검증

2026-10-20. ADR-0004·ADR-0003·ARCHITECTURE·README와 [M4 기록](M4_WEBVIEW_RECYCLE.md)을 기준으로 한다. **정책과 설정은 구현했으나 자동 활성화는 채택하지 않는다(기본 꺼짐).** 유효 5쌍 모두 활성화 직후 +50MiB 목표에 미달했다. 실제 앱 E2E 3종은 통과했지만, 후속 Rust 전체 실행에는 기존 Mongo view 스냅숏 오류 1건이 남았다.

## 원인 분리

[첫 진단](../bench/results-m5-first-diagnostic.json)에서 Rust의 10초 sleep은 10,003ms에 끝났고 UI가 응답을 관측하기까지 추가 2,043ms가 걸렸다. 창 표시와 native 포커스만으로 새 child WebView의 `document.hasFocus()`가 참이 되지 않아 이 회차는 실패했고 점수에서 제외했다. 명시적으로 새 child에 포커스를 주는 경로를 추가한 뒤 최소 재현을 분리했다.

[activity 없음](../bench/results-m5-quick-noactivity-v3.json) / [짧은 activity](../bench/results-m5-quick-activity-v3.json)의 Rust sleep은 각각 10,002.7 / 10,002.5ms였다. UI 관측은 12,031 / 12,042ms, Rust 완료 뒤 IPC 전달은 2,027 / 2,038ms였다. 두 회차 모두 호스트가 2초 뒤 창을 표시하고 새 child를 포커스한 시점에 UI 진행이 재개됐다. 빠른 v2의 2,044ms 값에는 활성화 후 footprint checkpoint 대기가 섞였으므로 제외했고, v3부터 이 표본을 활성화 측정 앞에 두지 않았다. native 포커스 → 2rAF는 81 / 238ms였다. 이 짧은 무부하 재현에는 DB 부하와 실제 2분 유휴가 없으므로 메모리 채택 점수로 쓰지 않는다.

따라서 M4의 177초 지연을 Rust 타이머 지연으로 확정할 근거는 없다. 이번 재현에서는 타이머가 제때 끝났고, 호스트에 건 짧은 activity도 숨김 WebView의 IPC 관측 지연을 제거하지 못했다. WebKit의 숨김 페이지 진행 제한에 가까운 패턴이지만 App Nap·Tauri/Wry·WebKit 중 특정 내부 원인으로 단정하지 않는다. 177초 자체를 이번 실행에서 재현한 것도 아니다.

공개 API인 [NSProcessInfo activity](https://developer.apple.com/documentation/foundation/processinfo?language=objc)와 [objc2-foundation](https://docs.rs/objc2-foundation/0.3.2/objc2_foundation/struct.NSProcessInfo.html)의 `UserInitiatedAllowingIdleSystemSleep`을 썼다. 제품에서는 교체 준비 직전에 시작해 복원 준비 완료 또는 최대 12초에 종료한다. 동일 스레드에서 생성한 opaque token을 정확히 한 번 끝내며, 2분 유휴 구간 전체에 걸거나 상시 App Nap을 끄지 않는다. 별도 진단의 15초 activity와 제품의 12초 상한을 구분한다.

## 구현 범위

호스트의 native 포커스 이벤트가 비활성을 기록하고 tokio 타이머로 120초를 기다린다. 1만 행 이상을 UI에 연 이력이 있고 설정이 켜져 있을 때만 저장 요청을 보낸다. UI는 결과가 남은 탭·실행·수정 중 쿼리·대화상자·작업 화면·설치 화면·AI 패널 상태·진행 중 API가 있으면 거부한다. 호스트는 교체 직전에 포커스, 실행 run, 승인/미리보기 영수증, 설치 mutex를 다시 확인한다. 준비 요청 번호는 한 번만 예약할 수 있어 중복 commit이 먼저 진행 중인 복원을 정리하지 못한다. 결과 행은 스냅숏에 넣지 않고 최대 2MiB의 상태만 호스트 RAM에 보관한다.

교체 전 활성화는 generation을 무효화해 취소한다. 교체가 시작된 후에는 같은 native 창의 child를 완성하고 새 child에 포커스를 준다. child 생성이 실패했을 때 호스트 스냅숏을 보존하고 다음 활성화에서 생성을 다시 시도한다. 연결/드라이버 프로세스와 세션은 교체하지 않는다. 초안 텍스트, 파라미터, 행 한도, timeout, 편집기 분할 비율, 탭과 활성 탭, 사이드바, 이력 패널, 테마, 연결 세션 맵을 복원한다. 수정 중인 쿼리와 복원 범위 밖의 임시 폼/AI 상태가 있으면 보수적으로 교체를 막는다. AI 패널은 닫아도 대화 상태를 유지하므로, 한 번 마운트한 실행에서는 재활용을 보수적으로 제외한다. 게이트를 통과하지 못한 경우 다음 비활성 주기에서 다시 평가한다.

MCP는 서버를 재시작하거나 실행 중 요청을 재전송하지 않는다. quiesce 플래그를 세운 뒤 permit/forward 대기 요청이 없는지 확인하며, 기존 요청이 있으면 교체를 포기한다. 교체 구간의 새 요청에는 실행 전에 `Busy`와 `retryable=true`를 반환한다. 새 UI의 연결 등록이 끝나면 같은 브리지를 다시 연다. 쓰기 재시도를 자동으로 수행하지 않는다. AI 출력은 계속 Operation 계획이며 실행/승인 경로를 변경하지 않았다.

## 측정과 판정

[수집기](../bench/m5_memory.py)는 R2의 host+WebContent+GPU+Networking physical footprint와 PID 소유 검증, 1280×768/DPR2, load≤14를 유지한다. 드라이버 메모리는 앱 합계에서 제외한다. 메모리 회차는 M4와 같은 M3 서명 설치 PG/Neo4j 드라이버 **0.2.0**을 썼고, Rust/E2E는 새로 빌드한 target-m5/debug **0.3.0** 플러그인을 썼다. 최신 드라이버/릴리스 조합의 footprint로 일반화하지 않는다. 시작 → 10만 행×10열 → 300노드 → 탭 10개 → 모두 닫기 → Neo4j 초안 1개 → 숨김 2분 → 교체/대조 → 활성화 흐름이다. 초안은 실행하지 않고 사용자 파라미터와 행 한도를 바꿔 둔다. 열린 결과 탭은 0개다. 대조/교체 순서를 교차해 5쌍을 수집한다.

[집계기](../bench/m5_summary.py)는 활성화 직후 footprint가 시작 +50MiB 이하인지, 실제 native 포커스 이벤트 → 2rAF가 300ms 이하인지, 세션과 전체 스냅숏이 같은지 판정한다. 시작/닫고 10초/활성화 지점의 짝 load 차이는 ≤3이어야 한다. 숨김 진행 지연은 별도 열에 기록하며 점수 게이트에 넣지 않는다. 첫 파일럿은 params=false인 PG에서 측정기가 파라미터 입력창을 가정해 실패했다([원자료](../bench/results-m5-product-pilot.json)); 파라미터를 지원하는 Neo4j 초안으로 고쳤으며 실패 자료를 삭제하거나 성공으로 바꾸지 않았다.

기존 [5쌍 원자료](../bench/results-m5-paired-five-v2.json) 중 3번 쌍은 startup load 차이 5.44로 ≤3 기준을 넘었다. 원래 point accepted 플래그는 바꾸지 않고 짝 집계에서만 제외했다. 동일 바이너리로 [추가 한 쌍](../bench/results-m5-paired-extra.json)을 수집하여 **총 6쌍 중 유효 5쌍**을 확보했다. 메모리 또는 페인트 실패를 재측정해 지우지 않았다. [합친 원자료](../bench/results-m5-paired-combined.json)와 [집계](../bench/results-m5-paired-summary.json)에 제외 쌍도 남아 있다.

| 시도 | 짝 판정 | 대조 활성 MiB | 교체 시작 MiB | 교체 활성 MiB | 교체 − 시작 MiB | 활성화 상한 ms | 세션·초안·사이드바 |
|---|---|---:|---:|---:|---:|---:|---|
| 1 | 유효 | 403.2 | 130.7 | 211.4 | +80.7 | 245 | 일치 |
| 2 | 유효 | 483.7 | 132.5 | 204.5 | +72.0 | 166 | 일치 |
| 3 | 제외: 시작 부하 차이 5.44 | 367.2 | 178.0 | 208.2 | +30.2 | 117 | 일치 |
| 4 | 유효 | 382.3 | 125.5 | 212.5 | +87.0 | 92 | 일치 |
| 5 | 유효 | 378.7 | 125.3 | 205.2 | +79.9 | 145 | 일치 |
| 6 | 유효 | 398.8 | 128.2 | 201.2 | +73.0 | 243 | 일치 |

유효 교체군의 활성화 직후 footprint는 201.2~212.5MiB, 시작 대비 **+72.0~+87.0MiB(중앙값 +79.9MiB)**였다. 대조군보다 짝 차이 중앙값 191.8MiB를 반환했지만, +50MiB 목표는 **5회 모두 미달**했다. 3번의 시작 +30.2MiB는 시작값 178.0MiB의 일시적 상승 영향을 받았고, 유휴 10초 121.2MiB 기준으로는 +87.0MiB다. 부하 불일치로 제외한 이 쌍을 성공 근거로 쓰지 않는다. 유효 교체군의 유휴 10초 대비 증가는 +75.5~+89.9MiB였다.

활성화 시간은 native `Focused(true)` callback 시각부터 2rAF 뒤 시각 조회 IPC까지의 보수적인 상한으로 **92~245ms**, 5회 모두 ≤300ms였다. 결과 탭은 0개, 실행하지 않은 Neo4j 초안은 1개였으며 세션 ID·실제 server_info 조회와 전체 UI 스냅숏(파라미터·행 한도·사이드바 트리 포함)이 모두 일치했다. 화면 영상에 의한 실제 첫 페인트·깜빡임 판정으로 일반화하지 않는다.

숨김 지연은 점수에서 제외했다. 닫고 10초 표본→복원 ready는 122.657~122.695초이며 이 값에는 초안 준비와 숨김 전 전환도 포함된다. 복원 후 Rust 10초 sleep은 10,002.2~10,003.2ms, UI 관측은 12,027~12,042ms였다. UI 전달은 호스트의 창 표시까지 약 2초 더 걸렸다. 숨김 진행 시간을 줄여 보이게 하거나 15초 timeout으로 성공 여부를 정하지 않았다.

## 검증과 제한

첫 격리 소스의 Rust workspace 전체는 **305 passed / 7 ignored / 0 failed**였고 [이전 검증](../bench/results-m5-before-lock-validation.json)에 보존했다. 후속 [전체 실행](../bench/results-m5-validation.json)은 **188 passed / 1 failed / 5 ignored까지 진행한 뒤 중단**됐다. `plugin_contracts::mongo_management::snapshot_indexes_and_standalone_import_refusal`이 `schema_snapshot(None)`에서 병행 schema fixture의 view를 collection처럼 인덱스 조회하여 Mongo code 166 `CommandNotSupportedOnView`를 받았다. 해당 Mongo 코드는 변경하지 않았고, 실패 로그를 삭제하거나 재시도로 통과시키지 않았다. 전체 workspace를 최종 green으로 보고하지 않는다.

최종 안전 보강의 [호스트 단위 전체](../bench/results-m5-focused-validation.json)는 **131 passed / 4 ignored**, 제품 UI는 **147 passed / 1 skipped**, tsc와 clippy도 통과했다. 이후 추가한 중복 commit 예약까지 포함한 [정책 집중 검사](../bench/results-m5-policy-final-validation.json)는 **4 passed**, clippy도 통과했다. 전체 단위 검사 뒤의 집중 검사이므로 최종 workspace 전체 통과와 구분한다. 설치형 경로는 먼저 `cargo build --workspace --bins`를 완료하고 `AXDB_DRIVER_DIR=target-m5/debug`로 검사했다. 지정된 PG·Neo4j·Mongo·Redis만 실연결했고 MySQL·TLS·별도 복원 대상·IAM·실제 키체인처럼 opt-in이 없는 early-return/ignored 경로는 실연결 증거로 세지 않는다.

메모리 측정 바이너리는 수집 중 변경하지 않았다. [측정 영수증](../bench/results-m5-measured-artifacts.json), [정확한 소스 재구성](../bench/results-m5-repro-check.json), [최종 앱 영수증](../bench/results-m5-final-artifacts.json)을 분리했다. 측정 뒤에는 상태 mutex를 잡고 Wry를 호출하지 않도록 하고, 실패한 child 생성만 복구하며, 준비 요청의 중복 commit을 막고, update/plan 화면을 추가 차단하며, 원래 inert 상태와 MCP tools/list 오류 형식을 보존했다. [후속 차이](../bench/m5-after-measurement.patch)를 측정 시점 코드의 실행 결과로 바꾸지 않는다. 실제 앱 E2E는 이 안전 보강이 포함된 별도 FinalProbe 앱으로 수행한다.

실제 앱 E2E 3종은 별도 FinalProbe 앱/프로필에서 **e2e-bridge 21/21, preview-e2e 5/5, permissions-e2e 13/13**으로 모두 통과했다([실행 명세](../bench/results-m5-e2e.json)). 실제 localhost MCP·PG·Neo4j·임시 SQLite와 native 승인/거부 버튼을 사용했다. 자격 증명은 격리 debug 앱에서 기존 테스트 fixture를 RAM으로 읽는 seam을 사용한다. 기존 점검명에 keychain이 있어도 OS 키체인 저장/조회 증거로 보고하지 않는다.

활성화 전/중 취소와 중복 commit은 상태 전이 단위 검사로 확인했으며, 실제 native 타이밍 경합·설치/백업/가져오기와 동시 재활용·재활용 중 지속 MCP 부하·영상 기반 깜빡임·다른 OS·release/공증/배포는 검증하지 않았다. 이 한계와 메모리 기준 미달 때문에 기본 꺼짐을 유지한다. 사용자 앱·DB·SSH, dist/·target/·target-rel/은 변경하지 않았으며 commit/push도 하지 않았다.

[정리 영수증](../bench/results-m5-final-cleanup.json)에서 소유한 측정 라벨 20개의 잔여 노드 0, 측정 앱/target-m5 드라이버 프로세스 0, 기록된 종료 검사 일치, 비활성 discovery 파일 제거와 추가 내용/산출물의 비밀값 포함 검사를 확인했다. 기존 소스의 변경하지 않은 테스트 접속 상수는 보존했다. 시작은 clean main이었고 종료 변경은 M5 소스·측정/검증 산출물·이 문서와 RESOURCE_OPT다. 기존 RESOURCE_OPT의 M4 `.json`/`.json.gz` 링크 불일치 1건은 [문서 점검 영수증](../bench/results-m5-document-checks.json)에 남기고 보존했다.

재현은 기준 commit의 빈 소스 사본에 [측정 제품 패치](../bench/m5-product-measured.patch)와 [측정 probe 패치](../bench/m5-probe-measured.patch)를 순서대로 적용한다. 최종 안전 보강은 [제품 패치](../bench/m5-product.patch)와 [probe 패치](../bench/m5-probe.patch)를 사용한다. CARGO_TARGET_DIR=target-m5, CARGO_BUILD_JOBS=6, dev/debug=0/incremental=false이며, TAURI_CONFIG는 identifier=dev.axdb.m5probe, devUrl=null, frontendDist=전용 UI 출력으로 둔다. `.app` identifier도 같게 하고 ad-hoc 서명한다. 원자료 이름은 새 이름을 사용해 기존 파일을 덮어쓰지 않는다.

```sh
python3 bench/m5_memory.py --self-check
python3 bench/m5_summary.py --self-check
python3 bench/m5_memory.py target-m5/M5Probe.app --drivers <격리된-서명-드라이버-경로> \
  --name fresh-pairs --modes control,replace,replace,control,control,replace,replace,control,control,replace --product
python3 bench/m5_validate.py --host-only
python3 bench/m5_e2e.py target-m5/FinalProbe.app
```
