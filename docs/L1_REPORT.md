# L1 경량화 검증 보고서

판정: 부분 개선, 최우선 목표 미달. 같은 Retina 2배율·load≤8의 0.2.0/L1 한 쌍에서 모두 닫고 10초 뒤 앱 footprint는 449.1→324.5MiB로 감소했지만 L1 시작값 121.2MiB보다 203.3MiB 높아 `startup + 50MiB`를 충족하지 못했다. 반복 검증이나 릴리스 통과를 주장하지 않는다.

ADR-0004·0003, README와 ARCHITECTURE를 읽고 기능·커맨드·승인 경로를 보존했다. AI 출력은 실행 계획이며 자동 실행하지 않는다. 사용자 앱·DB·SSH에는 접근하지 않았고 commit/push도 하지 않았다. 기존 dirty 공유 파일의 다른 작업자 변경을 보존했으며 L1 빌드와 분석 산출물은 target-l1만 사용했다. R2 측정 보정과 기준선 문서를 반영했고 기존 R2 절은 보존했다.

## 남긴 변경과 측정 근거

NavigationTab/CodeMirror, AI 패널·모델 선택기, 연결 폼·네트워크·IAM 단계, 온보딩, 업데이트 대화상자·릴리스 노트, 명령 팔레트, 클라이언트 화면, 실행 계획, 내보내기, Star 카드와 장식 컴포넌트를 필요할 때 가져온다. 유휴 prefetch는 없다. 셸의 온보딩 상태와 전역 ⌘K는 즉시 유지한다. lazy 전환 뒤 실제 ⌘K 회귀를 발견해 공통 훅으로 수정했다.

진입 JS는 76,405 bytes / gzip 26,381 bytes로 제공된 0.2.0 진입 목표 59KB 이하다. 정적 공유 청크까지 합친 JS는 186,715 / gzip 85,519 bytes이므로 전체 초기 JS가 59KB 이하라는 뜻은 아니다. 약 270KB dist 청크는 CodeMirror state/view/language/streamparser와 Lezer, 별도 약 41KB는 SQL/autocomplete/LR였고 둘 다 초기 정적 import 경로에서 빠졌다. [bundle](../bench/results-l1-bundle.json), [청크 출처](../bench/results-l1-chunk-sources.json).

결과·편집기 CSS 181개 규칙을 results.css로 이동했다. 동일 현재 UI에서 이 CSS를 초기 로드한 경우 149,593 / gzip 37,743 bytes, 분리 후 초기 CSS는 132,195 / gzip 34,786 bytes였다. 총 CSS가 제공된 0.2.0보다 작아졌다는 주장은 하지 않는다. 근거 없는 미사용 선택자 삭제는 하지 않았다. [CSS 비교](../bench/results-l1-css-split.json).

큰 결과는 기존 execute_stream의 Channel<Response> raw MessagePack을 사용한다. 프레임 16MiB, 구조·UTF-8·중복 키·정밀도·중첩을 검증한다. 열 값은 인코딩 버퍼와 offset으로 보관해 보이는 셀만 디코드하고 가상 표와 정렬은 열을 직접 읽는다. 기존 행 소비자는 읽기 전용 배열 Proxy로 요청한 행만 구성한다. 작은 결과/그래프의 invoke 반환과 int8/decimal/JSON 문자열 보존은 유지했다. 새 JS 의존성은 없으며 Rust는 기존 rmp-serde를 재사용했다. 완료·실패·unmount 시 Channel 콜백 참조를 끊는다.

동일 10만 행×10열의 V8 추가 보유 힙은 행 객체 53,606,992→열 보존 787,560 bytes, 행 구성 23.18→0.45ms였다. 이는 이미 존재하는 packed 문자열 backing을 제외한 비교이며 WKWebView IPC 파싱 시간이나 native footprint가 아니다. 네이티브 진단에서도 100만 scalar 중 실제 decodedStrings=4,805, 화면에 16행만 렌더됨을 확인했다. [V8 실험](../bench/results-l1-columnar-v8.json).

GraphView는 layout 단계에서 만든 Worker를 passive effect에서 회수해 빠른 unmount 때 cleanup이 등록되지 않는 결함이 있었다. 소유권과 해제를 layout effect로 맞춰 handler=null, terminate, rAF 취소를 보장하고 unmount 때 canvas backing을 0으로 만든다. 숨긴 탭의 그래프는 보존한다. 실제 Chromium RED→GREEN 검사가 이 수명주기를 확인했다. 별도 성능 A/B로 이 수정만의 MiB 효과를 분리하지는 못했다.

Host의 동기 검색·프로브 초기화는 LazyLock의 첫 Registry 조회로 옮겼다. 드라이버 마지막 세션 종료 뒤 기본 5분 단발 종료, 재사용 시 취소, 오래된 deadline 무효화, kill 뒤 wait 회수를 넣었다. 실제 SQLite 플러그인의 강제 만료 경계에서 RSS 8,432KiB→0, 재시작·회수를 검증했다. 실제 5분 대기 측정은 아니다. SDK의 상시 30초 restore 만료 폴링은 업로드가 있을 때만 가장 가까운 만료를 기다린다. 기존 stdin EOF 종료를 유지했다. [수명주기 증거](../bench/results-l1-driver-idle.json).

AI 응답은 약 50ms씩 묶어 렌더하고 종료·실패·unmount 때 타이머와 수신 참조를 해제한다. 현재 한 응답만 표시하고 Markdown 파서를 쓰지 않아 긴 대화 가상화·블록 재파싱은 대상이 없다. 이 변경만의 native CPU A/B는 분리하지 못했다.

## 네이티브 결과와 한계

같은 설치형 서명 PG/Neo4j 0.2.0 패키지를 새 비공개 테스트 경로에 실제 설치한 뒤 두 앱을 순서대로 측정했다. 모든 지점 load≤8, 1280×768/DPR2, 프로세스 소유권 검사·종료 확인을 통과한 한 쌍이다. 원래 R2 DPR1 중앙값과 직접 혼합하지 않는다. [원자료](../bench/results-resource-030-l1-lazy-column-retina2.json.gz), [계산 요약](../bench/results-l1-native-summary.json), [설치 영수증](../bench/results-resource-l1-install.json).

| 지표 | 같은 조건 0.2.0 | L1 lazy-column |
|---|---:|---:|
| 시작 앱 footprint | 132.4MiB | 121.2MiB |
| 10만 행 앱 footprint | 373.0MiB | 335.6MiB |
| 탭 10개 앱 footprint | 636.6MiB | 496.3MiB |
| 모두 닫고 10초 앱 footprint | 449.1MiB | 324.5MiB |
| 닫고 10초 − 시작 | +316.8MiB | +203.3MiB; 목표 미달 |
| 시작→활성화 포함 2rAF 체크포인트 | 4,094.6ms | 3,356.3ms |
| idle-60 앱 RSS / 구간 CPU | 197.8MiB / 0% | 251.4MiB / 0% |
| 닫고 10초 앱 RSS / 구간 CPU | 647.3MiB / 0% | 507.4MiB / 0% |
| 닫고 10초 드라이버 RSS / 구간 CPU | 218.2MiB / 0% | 170.3MiB / 0% |
| 10만 행 전체 쿼리 wall | 469ms | 471ms |

앱은 host+WebContent+GPU+Networking 합이고 드라이버는 별도다. RSS와 physical footprint는 다른 지표이며 시작 RSS는 오히려 증가했다. 체크포인트는 launch부터 창 활성화와 기존 2rAF까지로 정확한 GPU first paint가 아니다. CPU 0%는 해당 구간 측정이며 타이머가 전혀 없다는 증명이 아니다. 쿼리 wall은 파싱 단독 시간이 아니다. 한 쌍으로 p50/p95·재현성을 주장하지 않는다. 병렬 작업이 합쳐진 현재 0.3.0 앱과 0.2.0 비교이며 모든 차이를 L1 한 변경에 귀속할 수 없다.

첫 eager-column 후보는 닫은 뒤 메모리가 더 나빠져 열 scalar의 지연 디코드로 대체했다. 최초 pilot은 요청 DPR1과 실제 DPR2 불일치를 발견했고 하네스 viewport gate를 강화했다. 원자료 accepted를 고치지 않고 별도 geometry audit로 제외 이유를 남겼다. [pilot audit](../bench/results-l1-pilot-geometry-audit.json).

후속 vmmap 진단은 성능 점수에서 모든 accepted를 의도적으로 제외했다. 닫은 뒤 결과 DOM과 canvas가 없고, WebContent physical footprint 281.3MiB 중 가장 큰 영역은 WebKit Malloc이었다. 해당 영역 resident 181.6MiB/dirty 176.6MiB, JS VM resident/dirty 1,184KiB와 swapped 33.7MiB였다. 이것만으로 캐시 또는 누수라고 단정할 수 없다. `memory_pressure -S -l warn -s 1` 모의 요청은 exit255/Operation not permitted여서 압력 후 30초 회수 측정을 하지 못했다. 관리자 권한이나 시스템 전체 메모리 할당은 하지 않았다. [진단 원자료](../bench/results-resource-030-l1-vm-before-empty.json), [vmmap](../bench/results-l1-before-empty-diag-vmmap-tabs-closed-10.txt).

마지막 탭 종료 때 참조 정리와 다음 idle 빈 렌더를 추가한 마지막 실험은 prepare-window에서 Benchmark collector stopped로 종료했다. 효과를 입증하지 못해 훅과 그 전용 검사를 제거하고 App을 실험 전 해시로 정확히 복원했다. [실패 원자료](../bench/results-resource-030-l1-vm-after-empty.json). 재시도로 통과 수치를 만들지 않았다.

최종 유지 소스와 일치하는 로컬 서명 arm64 앱은 target-l1/bench-apps/AxDB-observed-before-empty.app이며 [최종 아티팩트 영수증](../bench/results-l1-final-artifact.json)에 SHA와 핵심 소스 해시 대조를 남겼다. canonical release/bundle 앱은 폐기한 empty-render 실험 빌드이므로 릴리스에 사용하지 않는다. 바이너리는 26,898,864 bytes로 실제 R2 arm64 앱의 17,022,976 bytes보다 크다. 기존 opt-level=3/lto=fat/codegen-units=1/strip은 유지했고 프로필/allocator/의존성 제거 실험은 미수행이다. 서명 검증은 통과했지만 공증·배포 검증은 아니다.

## 검사와 남은 범위

최종 rollback 뒤 git diff --check 및 L1 상대 링크 검사, tsc 및 graphLifecycle/msgpack/stream/viewer 4파일 19검사 통과, Python 하네스·report self-check 통과다. 이전 전체 UI는 141 passed/1 GC skipped였으나 최신 전체는 143 passed/1 sidebar paging timeout/1 GC skipped였다. 코디네이터가 해당 병렬 UI 통합 실패를 맡았으며 최신 전체 green으로 보고하지 않는다. GC 전용 측정은 expose-gc로 별도 통과했다.

cargo build --workspace --bins 뒤 AXDB_DRIVER_DIR=target-l1/debug로 workspace 269 passed/6 ignored, workspace clippy --all-targets --locked -D warnings 통과했다. 허용된 네 테스트 서버와 SQLite를 사용했지만 MySQL·TLS·추가 복원·유료 공급자처럼 환경이 없는 테스트의 early-return은 실DB 검증으로 세지 않는다. 초기 Mongo/Redis fixture URI 계약 오류 두 번과 병렬 rustls manifest 반영 중 clippy E0433은 별도로 기록했으며 이후 통과로 없었던 실패로 만들지 않는다. 원 로그 비밀값은 가렸다.

PG true streaming과 MCP 결과 byte cap/핸들은 코디네이터가 L2로 분리했다. 부모 PID 감시·프로세스 그룹 종료, 앱 강제 종료 후 자식 생존, 실제 5분 idle 종료, 숨김 창 CPU, 모든 OAuth 갱신·애니메이션 전수 검사, 전체 E2E, 반복 native 성능·순수 파싱·정확한 first paint 및 목표 +50MiB 달성은 미완료다. 시작 RSS·CSS·바이너리도 0.2.0 이하를 입증하지 못했다. 따라서 원래 요청 전체의 성공이나 릴리스 준비 완료로 보고하지 않는다.

## L2 인계: PG true streaming / MCP 결과 상한과 핸들

코디네이터 지시(2026-10-19 18:17 KST): 핵심 footprint A/B를 먼저 끝내고 추가 11·12는 L1 범위에서 제외한다. 두 항목은 미완료이며 별도 L2가 맡는다. 아래는 조사로 확인한 현행 경계와 권장 계약으로, 새 타입·필드·메서드 이름을 채택한 결정은 아니다.

PG 현행 경로는 driver-postgres::PgSession::run → cursor_rows/stream_rows의 Vec<SimpleQueryRow> → Vec<Value> 표 → driver-sdk::dispatch(Request::Execute)의 execute → 기존 ExecuteChunk → PluginSession::execute_stream → ResultSink → LiveResults다. cursor_rows의 FETCH도 simple_query로 전체 응답을 모으므로 현재 P1은 DB에서 첫 행을 받는 시점의 스트리밍이 아니다. LiveResults::start는 result.row_count를 최종 기대 수로 저장하고 다음 start/finish에서 검증한다. ResultSink는 동기 start/rows만 제공한다.

프로토콜 1 안에서 선택 메서드와 명시적인 지원 협상이 필요하다. 구버전은 기존 execute로 동작해야 하며, Unsupported를 받더라도 실행이 시작되기 전이라는 보장이 있을 때만 fallback해야 한다. 일부 행이나 쓰기가 이미 실행된 뒤 기존 execute를 재실행하면 안 된다. 처음에는 행 수·elapsed·truncated가 미정임을 명시하고 최종 metadata를 결과 index에 묶어 전달한다. row_count=0은 실제 빈 표와 구별되지 않으므로 미정 sentinel로 재사용하지 않는 안을 권장한다. 새 스트림 이벤트는 그 선택 메서드를 협상한 상대에게만 보내고, 기존 start/rows/frame 및 legacy reassembly 계약을 유지한다.

SDK의 기존 8프레임 writer queue와 16MiB frame/256MiB 일반 응답 상한을 보존한다. 비동기 PG row stream은 소비자가 느리면 다음 청크를 await하며 읽기를 늦춰야 한다. 무제한 채널이나 전체 결과의 중간 Vec로 backpressure를 우회하지 않는다. 현재 동기 ResultSink에서 blocking_send를 Tokio async worker에 직접 호출하지 않는다. async sink 계약이나 별도의 명확한 async 경계를 정하고, multi-result의 start→rows→final 순서·offset·폭·최종 행 수 검증을 host와 UI 양쪽에 남긴다. UI는 현재 L1의 PackedTable/appendPacked/tableCell을 그대로 소비할 수 있으며 final metadata만 정확히 반영해야 한다.

PG는 기존 execute_cancelled/run_guarded의 세션 mutex·타임아웃·취소·read_only·롤백 정리를 공유해야 한다. 이 경로를 우회한 별도 실행기는 금지한다. query_raw 또는 text raw stream을 써도 기존 타입 변환을 재사용해 int8/decimal/JSON 원문 문자열을 보존한다. SELECT cursor의 limit+1, 사용자 명시 트랜잭션, DML RETURNING/SHOW/EXPLAIN과 multi-statement의 다른 실행 의미도 유지해야 한다. 새 세션에서 임의로 쿼리를 재시작하지 않는다. host 이력과 context::remember에는 완성된 최종 metadata의 실제 행 수를 전달한다.

MCP 현행 경계는 agent_dispatch::execute의 실제 결과 → UI forward(results) → JSON response와 cap_rows, agent registry::compact의 총 200행/노드/문서다. 큰 문자열 한 셀만으로도 바이트 예산을 넘을 수 있고, bridge rpc는 value.to_string()과 structuredContent를 함께 만들어 응답을 복제한다. 큰 payload를 UI emit으로 넘기기 전에 별도 바이트 상한을 적용하고, 전체 값을 stringify한 뒤 길이만 재는 방식의 추가 대형 버퍼도 피하는 안을 권장한다. AI context 자체에는 20행/100열, 전체 128KB 상한과 명시적 행 전송 토글이 이미 있으므로 이 보안 경계를 느슨하게 만들지 않는다.

실제 조회 가능한 결과 핸들은 raw 결과를 제한된 비공개 저장소에 남기고, 모델에는 metadata·잘림 표시·핸들만 전달해야 한다. 핸들을 client/connection/run에 묶고 조회 때 현재 권한과 폐기 상태를 다시 검사한다. TTL, 동시 핸들 수, 전체 저장 바이트 상한, 요청 타임아웃, 만료·연결 종료·토큰 폐기·앱 종료 및 실패 시 정리를 정의해야 한다. 파일이면 0600과 경로 토큰 검증을 재사용하고 행 데이터·비밀을 이력/감사 로그에 넣지 않는다. 페이지 조회도 행 수와 UTF-8 바이트 예산을 함께 지키며, 핸들 조회 때문에 DB 쿼리를 다시 실행하지 않는다. 다른 클라이언트에게 핸들이 전달돼도 결과를 읽을 수 없어야 한다.

권장 검증은 old/new 플러그인 호환·부분 실행 뒤 fallback 금지·취소/타임아웃/rollback·multi-result 순서·정밀도·행 한도와 느린 소비자의 bounded queue다. PG 10만 행의 driver/host/WebContent 피크 RSS와 첫 행/전체 완료를 같은 조건으로 비교한다. MCP는 단일 거대 UTF-8 셀, 중첩 값, 200행을 넘는 결과, client 간 핸들 접근 거부, 권한 회수, TTL/상한/오류 파일 정리와 응답 바이트 예산을 검사한다. 로컬/mock 증거를 외부 모델 제공자나 native footprint 검증으로 확대하지 않는다.


