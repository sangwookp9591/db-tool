# M2 WebKit 메모리 보유 조사

판정: **목표 미달, 제품 정리 코드는 남기지 않는다.** 2026-10-19~20, ADR-0004, ADR-0003, ARCHITECTURE, README, L1_REPORT를 기준으로 조사했다. 공개 캐시 삭제, 제한된 압박, JS 할당 진단, 결과 페이지 보관 실험 중 전체 워크벤치의 `시작 +50MiB`를 달성한 방법은 없었다. 아래 프로토타입과 설계는 제품 구현 완료가 아니다.

## 판정 기준과 격리

원래 통과 기준은 R2/L1 전체 시나리오에서 host + WebContent + GPU + Networking의 physical footprint가 모든 탭을 닫고 10초 후 시작 +50MiB 이내인 것이다. 드라이버 메모리와 RSS는 별도로 본다. 축소 시나리오의 통과를 전체 목표 통과로 확장하지 않는다. 회수 가능성이 입증돼도 기존 기준을 자동으로 변경하지 않으며, 압력 전/후 footprint, 실제 pressure notification, 반복 plateau를 별도 후보 지표로 제안한다.

전용 `target-m2`, `CARGO_BUILD_JOBS=4`, UI 출력 `target-m2/ui-probe`를 사용한다. 제품 dist/target/target-rel, 사용자 앱과 사용자 DB/SSH는 변경하거나 조작하지 않는다. 측정 앱의 identifier와 WKWebsiteDataStore UUID를 제품과 다르게 만들어 공개 browsing-data 삭제 실험도 테스트 저장소로 한정한다. commit/push와 AI 자동 실행은 하지 않는다.

`bench/m2_memory.py`는 R2의 PID 소유권, footprint 표본, Retina 창 배치와 체크포인트 ACK를 재사용한다. 1280×768/DPR2, 지점 load≤14와 비교 쌍 load 차이≤3을 확인한다. heap/vmmap 진단이 들어간 회차는 별도 진단 근거로만 쓰고, 진단 도구가 붙지 않은 비교를 구분한다.

## 결과량 축소 프로토타입

모든 모드에서 같은 PG 테스트 서버의 같은 read-only `generate_series(1,100000)` 쿼리로 10개 열을 만든다. int8 5개 열과 문자열 5개 열이며, 첫/마지막 페이지의 2^53 초과 정수를 문자열로 확인한다. raw 모드는 기존 MessagePack Channel과 lazy columnar 표를, window 모드는 호스트에 한 결과를 보관하고 처음 200행과 마지막 200행만 요청하는 페이지를, headless 모드는 같은 raw 수신 뒤 표 렌더를 생략한다. 쿼리를 페이지마다 재실행하지 않는다.

실험 전용 저장소는 고정된 10만 행×10열 쿼리 하나만 보관하고 재실행/해제/앱 종료 시 회수한다. 제품용 핸들 계약이 아니다. window 표는 현재 페이지의 실제 200행을 표시하며, 아직 받지 않은 99,800행을 가짜 셀로 채우지 않는다. 전체 가상 스크롤, host 정렬/필터, 내보내기, 여러 탭, 취소/폐기 경합을 완성했다는 주장은 하지 않는다.

제품에 적용한다면 host 결과 핸들을 세션·탭·실행에 묶고, 전체 byte/동시 핸들/페이지 byte 한도를 둔다. VirtualTable은 보이는 범위와 overscan만 요청하고 오래된 응답을 무시한다. 정렬·검색도 보관된 결과에 적용해야 원래 '불러온 전체 행' 의미를 보존한다. 내보내기와 명시적 AI 샘플은 host에서 제한된 범위만 읽으며, 세션 종료/탭 닫기/재실행/취소/실패 때 해제한다. 승인과 AI의 계획 전용 불변식은 바뀌지 않는다.

## 공개 API와 진단 한계

Apple은 [WKWebsiteDataTypeMemoryCache](https://developer.apple.com/documentation/webkit/wkwebsitedatatypememorycache)와 [WKWebsiteDataStore.removeData](https://developer.apple.com/documentation/webkit/wkwebsitedatastore/removedata%28oftypes%3Amodifiedsince%3Acompletionhandler%3A%29)를 공개한다. 이는 웹사이트 캐시 데이터 제거이며 WebContent의 모든 allocator 보유 페이지나 JavaScript GC를 회수한다는 보장은 아니다. [WKWebsiteDataStore의 별도 identifier](https://developer.apple.com/documentation/webkit/wkwebsitedatastore/init%28foridentifier%3A%29)는 테스트 저장소 격리에 사용한다.

설치된 Tauri 2.12.4 / wry 0.57.0의 `clear_all_browsing_data`는 공개 API를 제공한다. [Tauri 문서](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindow.html#method.clear_all_browsing_data), [wry 구현](https://github.com/tauri-apps/wry/blob/wry-v0.57.0/src/wkwebview/mod.rs)을 확인했다. macOS 구현은 allWebsiteDataTypes와 epoch 시각으로 removeData를 요청하고 completion을 기다리지 않는다. 따라서 실험의 `cache-requested-3`은 요청 3초 후 표본이지 완료 콜백의 증거가 아니다. 모든 데이터 제거는 cookies/localStorage/IndexedDB도 포함할 수 있어 제품의 탭 종료 훅에 그대로 넣지 않는다. NSURLCache 정책만으로 WKWebView 자체 allocator를 회수할 수 있다고 가정하지 않는다. 사설 selector와 강제 JSC GC는 사용하지 않는다.

Web Inspector 대신 `heap -s -z --noContent`와 `vmmap -summary`를 WebContent PID에 적용한다. heap 검사가 성공해도 이는 native malloc census이며 JSC의 객체 참조 스냅샷과 같지 않다. [WebKit Memory Inspection](https://docs.webkit.org/Infrastructure/MemoryInspection.html)은 C++ class별 heap 분해에 별도 WebKit 빌드 플래그가 필요함을 설명한다. OS WebKit의 집계 영역만 보고 특정 JS 문자열이나 렌더링 클래스를 확정하지 않는다.

압박 보조 프로세스는 무작위 페이지 최대 2GiB를 실제로 쓰고 10초 유지한 뒤 해제한다. supervisor는 25초 상한과 가용 메모리 15% 중단 조건을 적용하며, 공개 dispatch memory-pressure source로 warn/critical 수신 여부를 기록한다. 단순 할당만 성공하고 pressure notification이 없다면 '캐시는 회수 가능하다'는 결론을 내리지 않는다.

## 측정 및 최종 상태

첫 축소 파일럿은 창 활성화 전에 2rAF를 기다려 첫 체크포인트를 기록하지 못했다. 이 회차는 직접 중단했고 앱 종료를 확인했다. R2처럼 prepare-window에서는 rAF를 기다리지 않게 바꾼 뒤 정상 진입했다. [실패 기록](../bench/results-m2-pilot.json)은 유효 표본이 아니다.

[축소 파일럿](../bench/results-m2-pilot-v2.json)은 세 모드 모두 완료·프로세스 종료를 확인했다. 같은 dev/debug=0 바이너리이며 제품 release의 효과로 일반화하지 않는다. raw/window의 활성 결과 지점은 load 차이 2.27로 허용 범위였다. 시작 지점 차이는 3.17로 짝 비교에서 제외하고, 30초 지점 차이도 3을 넘어 제외했다.

| 축소 시나리오, 앱 footprint MiB | raw + 가상 표 | host Vec 보관 + 200행 페이지 | raw + 렌더 생략 |
|---|---:|---:|---:|
| 시작 | 129.8 | 124.9 | 195.7 |
| 유휴 10초 | 120.1 | 112.9 | 116.1 |
| 10만 행 쿼리 후 | 214.9 | 319.7 | 160.6 |
| 닫고 10초 | 131.7 | 129.8 | 159.3 |
| 닫고 30초 | 131.0 | 126.0 | 158.8 |
| browsing-data 삭제 요청 3초 후 | 131.4 | 126.5 | 159.3 |

한 번의 결과에서 전체 raw 전송만으로도 닫고 10초 뒤 시작 수준으로 돌아왔다. 따라서 L1 전체 +203MiB를 '큰 결과의 문자열이 반드시 남는다'로 설명할 수 없다. 렌더 생략 모드에서도 약 43MiB(유휴 10초 기준)가 남아, 렌더링만으로 모든 보유를 설명할 수도 없다. native heap 진단과 전체 워크벤치 재현이 필요하다.

200행 페이지는 WebContent를 132.9→103.8MiB로 줄였으나, typed Vec를 보관한 host는 45.4→181.2MiB로 증가했다. 앱 전체 활성 footprint는 오히려 +104.8MiB였다. **이 Vec 보관 구현은 기각한다.** 향후 페이지 설계에서는 인코딩된 bounded 청크나 비공개 파일을 보관해야 하며, WebContent만 줄인 수치를 앱 전체 개선으로 보고하지 않는다. 마지막 페이지 200행과 최종 int8 값은 실제 요청으로 검증했다.

초기 GPU가 76MiB였다가 10초 후 16~21MiB로 내려간 회차가 있다. 따라서 높은 cold 시작값에 기대어 목표를 통과했다고 하지 않으며 유휴 10초 기준도 병기한다. 이 파일럿에는 pressure helper를 실행하지 않았다.

### 전체 R2/L1 흐름 재현과 압박

[전체 진단 회차](../bench/results-m2-full-diagnostic.json)는 같은 설치형 PG/Neo4j 테스트 드라이버로 10만 행, 300노드 그래프, 탭 10개를 실제 UI에서 실행했다. 시작 197.5MiB(초기 GPU spike 포함), 유휴 10초 122.6MiB, 유휴 60초 115.9MiB, 10만 행 330.6MiB, 탭 10개 482.0MiB, 닫고 10초 375.6MiB였다. 시작 대비 +178.1MiB, 유휴 10초 대비 +253.0MiB로 어느 기준에서도 +50MiB를 넘었다. heap/vmmap/leaks를 함께 실행한 진단 회차여서 성능 점수나 개선 A/B로 사용하지 않는다.

`heap -s -z --noContent`, `vmmap -summary`와 `leaks --noContent --outputGraph`가 성공했다. 원래 닫힌 뒤 결과 DOM/canvas가 없다는 관찰과 별개로, native census에는 HTMLButtonElement 1,212개와 SVGSVGElement 638개가 남았다. 이는 현재 문서에 연결된 DOM이라는 뜻이 아니며, 강한 JS 참조·GC 지연·WebKit 내부 보유 중 무엇인지는 native 스냅샷만으로 확정하지 않는다. native 참조 트리는 해당 버튼들을 `WebKit Owned`로 묶어 JS 함수까지의 소유 경로를 보여 주지 못했다. 스냅샷은 `target-m2/full-diagnostic-raw-1.memgraph`(34.90MB, 로컬 파일)에 있고 텍스트 census를 bench에 남긴다.

닫고 10초의 WebKit Malloc은 453,539개 할당, 265,626,796 bytes를 보유했다. 그중 비분류(non-object)가 약 241.8MiB이며, 크기별로는 16KiB 7,373개가 115.2MiB를 차지했다. [JSC MarkedBlock](https://github.com/WebKit/WebKit/blob/main/Source/JavaScriptCore/heap/MarkedBlock.h)은 최소 16KiB 블록을 사용하므로 JSC 객체 힙 블록도 후보이나, **크기만으로 해당 할당을 MarkedBlock이라고 확정하지 않는다.** `JS VM Reservations/Gigacage` 한 행이 작다는 이유로 전체 JS 객체 힙도 작다고 해석하지 않는다. 세부 범주를 가리킬 수 없는 OS WebKit에서는 C++/JS 코드별 누수를 확정할 자료가 부족하다.

공개 browsing-data 삭제 요청 3초 후 앱 footprint는 413.1MiB였다. 이어 보조 프로세스가 무작위 데이터 2GiB를 실제로 할당하고 10초 유지한 뒤 정상 종료했다. 가용 비율은 41%에서 일시적으로 30%까지 내려갔고 종료 후 40%였다. warn/critical dispatch 알림은 없었다. 압박 뒤 앱 footprint 413.9MiB, WebContent 329.8MiB로 회수되지 않았다. 전체 시스템 swap used는 약 403.6MiB 증가했으므로 더 큰 압박으로 확대하지 않았다.

| WebKit Malloc, MiB | 닫고 10초 | cache 요청 후 | 2GiB 압박 후 |
|---|---:|---:|---:|
| resident | 247.5 | 158.3 | 86.0 |
| dirty | 227.1 | 158.1 | 85.7 |
| swapped | 약 7.3 | 75.8 | 148.0 |
| heap census 할당량 | 약 253.3 | 약 253.3 | 약 253.3 |

resident 감소가 해제라는 뜻은 아니다. dirty+swapped와 할당량은 거의 유지돼, 이번 압박은 주로 압축·스왑을 유도했다. 회수 가능한 캐시라는 판정이나 footprint 대신 RSS를 쓰자는 기준 변경은 근거가 없다. 실제 warn/critical 하의 회수 가능성은 여전히 미검증이다.

### 도구 없는 대조와 JS 수거 진단

[도구 없는 전체 회차](../bench/results-m2-full-churn.json)는 시작 127.5→닫고 10초 354.5MiB(+227.0), 닫고 30초 347.4MiB였다. 캐시 요청 후 348.0MiB였고, 100,000개 임시 JS 객체를 8배치 생성한 뒤에는 오히려 430.5MiB였다. artificial allocation을 제품 정리 수단으로 남기지 않는다.

[참조 진단 회차](../bench/results-m2-full-references.json)는 닫고 10초 374.2MiB였다. 닫힌 탭 wrapper 10개는 10/30초와 임시 JS 할당 후에도 WeakRef에서 관찰됐다. 임시 배열 8개 자체는 모두 수거됐으나, 이것만으로 모든 세대/DOM wrapper에 대한 full GC가 실행됐다고 단정할 수 없다. 현재 앱 루트와 window.axdb의 데이터 속성·Map/Set을 getter 없이 탐색한 2,347개 객체에서는 해당 wrapper로 이어지는 경로를 찾지 못했다. 함수 클로저, native/opaque roots, 거대 결과 배열 내부는 이 탐색으로 보지 못한다.

[하네스 DOM 보관 대조](../bench/results-m2-full-dom-release.json)는 close 버튼 NodeList와 활성 탭 DOM을 await 사이에 보관하지 않고 다시 조회했다. 모든 원래 행 수·정밀도·그래프 검사를 유지했으며, 시작 125.6→닫고 10초 363.3MiB(+237.7), 닫고 30초 359.4MiB였다. wrapper 10개는 여전히 관찰됐고 직접 속성 경로는 없었다. 임시 JS 객체 뒤 475.0MiB, 64MiB Uint8Array를 네 배치 할당·쓰기한 뒤 362.3MiB였다. 네 버퍼의 WeakRef는 모두 비워졌지만 탭 wrapper는 남았다. 이 또한 어떤 full GC를 강제했다는 증거가 아니며 정리 기법으로 채택하지 않는다. 서로 다른 실험 바이너리의 회차 간 MiB 차이를 단일 수정의 효과로 귀속하지 않는다.

### 남긴 것과 다음 단계

제품 소스의 실험 커맨드·UUID 설정·벤치 훅과 전용 모듈은 원복한다. [프로토타입 패치](../bench/m2-prototype.patch)는 재현과 검토용이며 적용되지 않은 상태로 보관한다. [측정 하네스](../bench/m2_memory.py), [압박 보조 소스](../bench/m2_pressure.c), 원자료와 이 보고서만 남긴다. [패치/최종 probe 영수증](../bench/results-m2-prototype-receipt.json), [테스트 드라이버 provenance](../bench/results-m2-driver-provenance.json)는 로컬 실험 증거다. `target-m2/M2Probe.app`는 실험 코드를 포함한 dev/debug=0·ad-hoc 서명 앱으로, 제품 릴리스에 사용하지 않는다. 기존 `docs/M2_REPORT.md`는 다른 MongoDB 작업 보고서이므로 덮어쓰지 않는다.

현재 확정할 수 있는 것은 큰 raw 결과만의 축소 재현은 회수됐고, 전체 UI를 거친 뒤의 WebKit 할당·닫힌 wrapper 보유는 회수되지 않았다는 점이다. 앱의 강한 클로저 참조와 WebKit의 GC/opaque-root 보유를 더 구분해야 한다. 다음 작업은 별도 계측 WebKit 또는 JS 객체 소유 경로를 볼 수 있는 허용 도구로 닫힌 탭 wrapper의 root를 찾는 것이다. 데이터 양 축소를 진행한다면 typed Vec 대신 byte-bounded 인코딩 청크/파일 보관을 사용하고, 먼저 전체 앱의 활성/종료 footprint와 peak를 함께 비교해야 한다. 그 다음 후보는 결과 realm/WebView 수명 격리이나, 연결·승인·편집 상태 보존과 재생성 비용을 포함한 설계·측정이 먼저 필요하다. 사설 강제 GC나 사용자 저장소 전체 삭제는 대안으로 채택하지 않는다.

기준 변경을 권장할 수 있는 자료는 없다. 기존 `시작 +50MiB`를 유지하고, 보조 지표로 유휴 10초 기준 delta, 실제 pressure event 유무, resident와 swapped, 반복 plateau를 병기하는 안만 제안한다. 특히 RSS 하락이나 synthetic allocation 성공만으로 회수 가능성을 선언하지 않는다.

### 검증과 재현

원복 후 `cargo build --workspace --bins` → `AXDB_DRIVER_DIR=target-m2/debug cargo test -p axdb`는 194 passed / 7 ignored였다. PG·Neo4j만 지정된 테스트 서버 환경 변수를 제공했으며 다른 DB·TLS·실제 IAM/SSH·공급자처럼 환경이 없어 early-return한 검사는 실연결 증거로 세지 않는다. [build](../bench/results-m2-build.txt), [host test](../bench/results-m2-host-test.txt), [clippy](../bench/results-m2-host-clippy.txt)의 `-p axdb --all-targets -D warnings`는 통과했다. 비밀값은 결과 로그에서 제거했고 전체 M2 산출물도 별도로 검사했다.

프로토타입을 제거하기 전 페이지 경계 검사는 [1 passed](../bench/results-m2-prototype-test.txt), getter를 호출하지 않는 참조 탐색 검사는 1 passed였다. 원복 후 [tsc](../bench/results-m2-tsc.txt)는 통과했지만 [최신 전체 UI 검사](../bench/results-m2-ui-test.txt)는 143 passed / 1 failed / 1 skipped였다. 실패는 `src/ui/branched-menu.test.ts:170` Archive paging의 5초 timeout으로, [L1](L1_REPORT.md)에도 같은 실패가 기록돼 있다. 초기에 실행한 전체 UI는 144 passed / 1 skipped였으나 이것으로 최신 실패를 대체하지 않는다. 재시도·timeout 확대·별도 UI 수정을 하지 않았다. Python R2/M2 self-check, Python 구문 검사, C 보조 프로그램 `-Wall -Wextra -Werror`와 2GiB 초과 인자 거부 검사, 로컬 링크·공백·patch apply-check 및 최종 probe 해시 대조도 통과했다.

재현은 격리한 소스 사본에만 `bench/m2-prototype.patch`를 적용하고 UI를 `target-m2/ui-probe`에 빌드한다. `CARGO_TARGET_DIR=target-m2 CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`과 `TAURI_CONFIG`의 identifier=`dev.axdb.m2probe`, devUrl=null, frontendDist=`../target-m2/ui-probe`를 사용해 빌드한다. JSON의 dataStoreIdentifier는 현재 Tauri codegen에서 `[u8;16]`/`Vec<u8>` E0308을 일으켜 최초 빌드가 실패했다. 패치는 probe identifier일 때만 공개 런타임 config의 UUID를 설정해 우회하며, SDK나 dependency는 변경하지 않는다.

macOS `.app`의 CFBundleExecutable=axdb, CFBundleIdentifier=dev.axdb.m2probe, NSHighResolutionCapable=true를 지정하고 전용 바이너리를 넣어 ad-hoc 서명한다. `bench/m2_memory.py`에 이 앱과 [L1 실제 테스트 설치 영수증](../bench/results-resource-l1-install.json)의 drivers 경로, 중복되지 않는 name을 넘긴다. 축소 실험은 기본 m2 시나리오와 raw/window/headless 모드를, 전체 흐름은 resource 시나리오를 사용한다. diagnostics/pressure는 별도 회차로만 사용한다. 원자료의 accepted 플래그는 수집 당시대로 보존했고, [요약](../bench/results-m2-summary.json)에 진단 제외와 실패한 짝 load 지점을 별도로 표시했다. 새 하네스는 score_accepted도 분리한다. native lock과 PID/viewport/load gate를 우회하지 않는다.

작업 시작은 clean `d93ebd5`였고, 병렬 MySQL 작업이 `77564eb`로 반영된 동안 내 실험 hunk만 되돌렸다. 종료 시 제품 4개 파일의 내 diff는 0이며, 다른 작업자의 `docs/RESOURCE_OPT.md`와 MySQL 통합 테스트 파일은 수정·제거하지 않았다. host의 backup_streaming 통합 검사가 기존 W4 증거 파일을 갱신하는 부작용은 작성 코드와 시작 상태를 확인해 [M2 사본](../bench/results-m2-backup-streaming.json)으로 보관한 뒤 원래 W4 파일을 복원했다. 내가 남긴 것은 이 문서·요약 보고서·bench 하네스/압박 소스·미적용 패치·M2 원자료/검사 로그다. 원복된 제품으로 release를 다시 빌드하거나 설치·공증하지 않았으므로 릴리스 통과를 주장하지 않는다.
