# AxDB 메모리·CPU 부하: 외부 사례 조사

조사일 2026-10-19. 표기: **[사실]** = 링크를 직접 열어 확인한 내용, **[추정]** = 사례를 AxDB에 대입한 내 판단(검증 필요).
링크는 모두 실제로 열어 본 것만 넣었다. 코드 저장소는 읽지 않았고 AxDB 내부 구조는 의뢰서 설명만 근거로 했다.
난이도와 예상 효과는 전부 [추정]이다.

## 확인 못 한 것 (먼저 밝힘)
- DB GUI가 Arrow IPC로 결과를 보내 메모리를 줄였다는 정량 사례: 못 찾음. DuckDB UI는 "DuckDB 메모리 표현에 가까운 바이너리"라고만 밝히고 수치가 없다.
- Tauri 앱 cold start 최적화 글(수치 포함): 못 찾음. 5번 주제는 공식 문서 권고 수준이다.
- VS Code·Zed가 유휴 언어 서버를 자동 종료하는 정책의 공식 문서·이슈: 못 찾음. 6번은 프로세스 생명주기 결함 사례만 확인했다.
- Deno, Vector, Materialize, Firezone, Lapce의 RSS 감소 사례: 검색 결과에 안 나와 쓰지 않았다.
- tokio 유휴 CPU 관련 이슈: 직접 해당하는 것 못 찾음(tokio #4569는 부하 중 park 비용이라 유휴 문제가 아님).

---

## AxDB에 적용할 우선순위 Top 10 (DB 경로 + AI·MCP 경로)

| # | 항목 | 예상 효과 | 난이도 | 핵심 근거 |
|---|---|---|---|---|
| 1 | 큰 결과를 JSON 대신 바이너리 열 청크로 전송 (`tauri::ipc::Response` 또는 Channel에 raw bytes), 이벤트 `emit`/`eval`로 대량 데이터 보내지 않기 | 큼. 웹뷰 쪽 733MB 유지의 가장 유력한 원인 후보를 제거 | 중 | [Tauri 문서](https://v2.tauri.app/develop/calling-rust/), [#4026](https://github.com/tauri-apps/tauri/issues/4026), [#12724](https://github.com/tauri-apps/tauri/issues/12724), [wry #1489](https://github.com/tauri-apps/wry/issues/1489) |
| 2 | 웹뷰 보유 메모리 위생: Channel `onmessage` 해제, 탭 닫을 때 참조 끊기, 창 재생성 옵션 | 큼(WebContent 쪽 하락). 측정 필요 | 하~중 | [#13133](https://github.com/tauri-apps/tauri/issues/13133), [#4026 코멘트](https://github.com/tauri-apps/tauri/issues/4026) |
| 3 | 드라이버·MCP 자식 프로세스 생명주기: stdin EOF 시 종료, 부모 PID 감시, 프로세스 그룹 종료, 좀비 회수, 유휴 종료 | 큼(프로세스 누적 방지). 세션이 많은 AI·MCP 경로에서 특히 | 하~중 | [TS SDK #208](https://github.com/modelcontextprotocol/typescript-sdk/issues/208), [#2002](https://github.com/modelcontextprotocol/typescript-sdk/issues/2002), [#2023](https://github.com/modelcontextprotocol/typescript-sdk/issues/2023), [Zed #55950](https://github.com/zed-industries/zed/issues/55950), [claude-code #58915](https://github.com/anthropics/claude-code/issues/58915) |
| 4 | AI 스트리밍 렌더 스로틀(약 50ms)과 토큰마다 Markdown 전체 재파싱 금지, 메시지·도구 결과 메모리 상한 | 중~큼(AI 패널 CPU) | 하 | [AI SDK 문서](https://ai-sdk.dev/docs/troubleshooting/react-maximum-update-depth-exceeded), [vercel/ai #18547](https://github.com/vercel/ai/issues/18547), [Zed #57425](https://github.com/zed-industries/zed/issues/57425) |
| 5 | 도구 호출 결과(큰 쿼리 결과)를 모델 컨텍스트로 보낼 때 토큰·바이트 상한, 초과분은 파일/핸들로 대체 | 중~큼(메모리·토큰·지연) | 하 | [Claude Code MCP 문서](https://code.claude.com/docs/en/mcp), [TS SDK #771](https://github.com/modelcontextprotocol/typescript-sdk/issues/771), [#2961](https://github.com/modelcontextprotocol/typescript-sdk/issues/2961) |
| 6 | 드라이버에서 행 전체를 `Vec`로 모으지 않는 스트리밍 쿼리(`query_raw`/커서·포털) + 청크 크기 상한 | 중(드라이버 피크 RSS) | 중 | [tokio-postgres Client 문서](https://docs.rs/tokio-postgres/latest/tokio_postgres/struct.Client.html) |
| 7 | 할당자: mimalloc `purge_delay=0`을 호스트·모든 드라이버에 일관 적용하고, 릴리스 빌드에서 footprint로 재검증. `malloc_zone_pressure_relief`에 기대지 않기 | 이미 적용 중. 누락 프로세스 점검과 회귀 방지 | 하 | [mimalloc 문서](https://microsoft.github.io/mimalloc/environment.html), [tako #2019](https://github.com/takushio2525/tako/issues/2019), [cpython #158972](https://github.com/python/cpython/issues/158972) |
| 8 | 유휴 CPU 0: 무한 CSS 애니메이션 제거(합성 가능한 opacity·transform만), 주기적 emit·폴링 제거, 숨김 시 타이머 정지, OAuth 갱신은 만료 시각 기준 단발 타이머 | 중(유휴 CPU) | 하 | [adom-bridge #52](https://wiki.adom.inc/adom/adom-bridge/issues/52), [claude-code #90228](https://github.com/anthropics/claude-code/issues/90228), [Chrome 타이머 스로틀](https://developer.chrome.com/blog/timer-throttling-in-chrome-88) |
| 9 | 가상화 표가 행 객체 없이 열 배열에서 셀을 직접 읽도록(`getCellContent` 방식, 캔버스 표 검토) | 중(UI 쪽 JS 힙 감소) | 중 | [glide-data-grid](https://github.com/glideapps/glide-data-grid), [Perspective](https://github.com/finos/perspective) |
| 10 | MCP 세션 관리: 유휴 세션 타임아웃과 맵 정리, 동시 세션 수 제한, 요청 타임아웃, 재연결 백오프 상한 | 중(MCP 서버 기능을 낼 때) | 하~중 | [rmcp #1244](https://github.com/modelcontextprotocol/rust-sdk/issues/1244), [MCP 수명주기 명세](https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle) |

순위 근거(요약): 1·2는 AxDB 실측에서 가장 큰 수치(웹뷰 포함 733MB, 닫아도 유지)와 맞닿아 있다. 3·5는 사용자가 AI·MCP를 쓴다는 점에서 세션이 누적될수록 커지는 문제이고, 구현이 작다. 7은 이미 효과를 봤으므로 "유지·검증"이 목적이라 우선순위가 낮다.

---

## 1. Tauri/WKWebView/WebView2 메모리 증가와 IPC

| 문제 | 링크 | 해결/결과 | AxDB 적용 가능성·예상 효과 |
|---|---|---|---|
| 큰 반환값은 JSON 직렬화로 느리고 메모리를 쓴다 | [Tauri 문서: Calling Rust](https://v2.tauri.app/develop/calling-rust/) | **[사실]** 문서가 `serde::Serialize` 반환은 JSON으로 직렬화되어 큰 데이터에서 느려질 수 있다고 경고하고, 최적 방법으로 `tauri::ipc::Response`를, 스트리밍은 Channel을 권장. 이벤트보다 Channel이 순서·처리량에 적합하다고 명시 | **[추정]** AxDB가 이미 청크 스트리밍이라면 청크 본문을 JSON이 아니라 `Response`/Channel의 raw bytes로 보내고 UI에서 `ArrayBuffer`로 받는 것이 변경점. 효과: 직렬화 CPU와 임시 문자열 감소 |
| 명령 반복 호출로 메모리가 계속 증가, 웹뷰 프로세스가 큼 | [tauri #4026](https://github.com/tauri-apps/tauri/issues/4026) (macOS 재현) | **[사실]** 원인 미확정, 이슈는 열려 있음. 메인테이너가 "모든 플랫폼 웹뷰에 어떤 형태든 누수 이슈가 있다"고 언급. `eval`만으로도 재현됨. Linux는 WebKit 프로세스 내부 누수. 사용자 우회책: 로컬 HTTP 서버 경유, Rust→JS 전송 최소화, 창을 숨기지 말고 닫고 재생성(WebView2 이슈 기반 제안, Tauri에서 검증 안 됨) | **[추정]** AxDB의 "탭 모두 닫아도 733MB 유지"와 증상이 일치. 큰 데이터를 웹뷰로 덜 보내고(서버 측 페이지네이션·가시 영역만), 최후 수단으로 결과 전용 창/웹뷰 재생성을 실험할 가치가 있음. 효과는 측정 전 불명 |
| 이벤트 `emit` 반복 시 프론트 메모리 급증 | [tauri #12724](https://github.com/tauri-apps/tauri/issues/12724) | **[사실]** 200만 이벤트 후 프론트 약 1.1GB, 백엔드 약 120MB. 열려 있음, 해결책 없음 | **[추정]** 결과 청크를 `emit`으로 보내고 있다면 Channel/`Response`로 교체 |
| `webview.evaluate_script()` 반복 시 웹뷰 프로세스 약 870MB | [wry #1489](https://github.com/tauri-apps/wry/issues/1489) | **[사실]** 열려 있음, 재현 코드 있음 | **[추정]** 큰 데이터를 `eval`로 주입하지 않기 |
| Channel `onmessage`가 `window`에 영구 보관되어 컴포넌트가 GC 안 됨 | [tauri #13133](https://github.com/tauri-apps/tauri/issues/13133) | **[사실]** 원인은 `transformCallback`이 콜백을 `window`에 UUID로 저장하고 삭제하지 않는 것. PR #13136으로 종료(2025-04-12). 수정 버전은 페이지에 없음. 우회: `delete channel.onmessage`를 finally에서 호출 | **[추정]** AxDB가 쿼리마다 Channel을 만든다면 직접 해당. Tauri 버전을 확인하고 우회책 적용. 클로저가 결과 배열을 잡고 있으면 탭을 닫아도 힙이 남음 |
| 큰 바이너리 IPC 속도가 플랫폼별로 크게 다름 | [discussion #11915](https://github.com/orgs/tauri-apps/discussions/11915) | **[사실]** 비공식 벤치마크: 10MB가 macOS 약 5ms, Windows 약 200ms. 메인테이너는 WebView2만 공유 메모리를 지원하지만 "이상하게 느리다"고 말함. 시스템 웹뷰로는 더 개선하기 어렵다는 의견 | **[추정]** Windows에서는 큰 청크를 줄이고 크기를 작게 나눠 반복 측정할 것. macOS는 raw bytes 경로가 충분히 빠름 |
| 이벤트로 ArrayBuffer를 못 보내 직렬화 비용 발생 | [tauri #13405](https://github.com/tauri-apps/tauri/issues/13405) | **[사실]** 열려 있음. 3MB 전송에 200ms라는 보고. 현 워크어라운드는 "Rust가 알림, FE가 명령으로 요청해 array buffer 수령" 또는 롱폴링 | **[추정]** "알림 + 풀(pull)" 패턴은 AxDB의 청크 스트리밍에 그대로 맞음(UI가 소비 속도를 정함) |

## 2. 결과 전송 포맷과 열 기반 렌더링

| 문제 | 링크 | 해결/결과 | AxDB 적용 가능성·예상 효과 |
|---|---|---|---|
| 서버→브라우저 결과 전송 비용 | [DuckDB UI 발표](https://duckdb.org/2025/03/12/duckdb-ui.html) | **[사실]** 결과를 "DuckDB 메모리 표현에 가까운 효율적 바이너리"로 전송, 업데이트 통지에는 SSE. 수치·가상화 설명은 글에 없음 | **[추정]** JSON이 아닌 열 단위 바이너리가 업계 방향이라는 정도의 근거. 직접 포맷을 베끼기보다 단순한 자체 열 청크(타입별 TypedArray + null 비트맵 + 문자열 오프셋)를 권장 |
| 대용량 데이터 표 | [Perspective](https://github.com/finos/perspective) | **[사실]** 열 기반 엔진, Arrow 스트리밍 입출력 지원, 가상 스크롤. 데이터를 브라우저에 두거나 "가시 부분만 서버에서 스트리밍"하는 가상 서버 모드 제공. 디스크 페이징(OPFS) 지원 | **[추정]** 핵심 아이디어는 "보이는 구간만 요청". AxDB는 표가 가상화되어 있으니 서버(드라이버) 쪽 결과 보관 + 구간 요청 구조가 JS 힙을 줄임 |
| 행 객체 대량 생성 | [glide-data-grid](https://github.com/glideapps/glide-data-grid) | **[사실]** 행 수·열 정의·`getCellContent` 콜백만 요구하고 셀은 지연 생성, 캔버스 렌더링. "수백 DOM 요소를 프레임마다 로드/언로드하면 답이 없다"는 FAQ | **[추정]** UI에서 `rows: object[]`를 만들지 말고 `columns: TypedArray[]`를 보관, 렌더 시 `(row, col)`으로 직접 읽기. 10만×10열이면 객체 100만 개 대신 배열 10개 |
| Arrow IPC를 DB GUI에서 쓴 사례 | 못 찾음 | **[사실]** 검색 결과에서 Tauri+Arrow 사례를 확인하지 못함 | **[추정]** Arrow JS 의존성(번들 증가)보다 자체 단순 포맷이 번들·시작 속도에 유리할 가능성. 비교 실험 필요 |

## 3. Rust 장기 실행 프로세스의 메모리 반환과 단편화

| 문제 | 링크 | 해결/결과 | AxDB 적용 가능성·예상 효과 |
|---|---|---|---|
| mimalloc 반환 지연 | [mimalloc 환경 변수 문서](https://microsoft.github.io/mimalloc/environment.html) | **[사실]** `MIMALLOC_PURGE_DELAY` v3 기본 1000ms, 0이면 즉시 반환(메모리↓, 성능↓), -1이면 반환 안 함 | AxDB 실측(52~68MB, 17.6MB)과 일치. 유지 |
| `purge_delay=0`이 필요한 경우 | [mimalloc_rust #141](https://github.com/purpleprotocol/mimalloc_rust/issues/141) | **[사실]** Linux에서 0이 아니면 `RES`가 안 떨어진다는 재현 보고(0.1.46, 열려 있음). 업스트림 수정 여부 미확인 | **[추정]** Windows·Linux 빌드 대상이라면 동일하게 0 설정 확인 |
| macOS에서 mimalloc도 안 돌려주는 사례 | [cpython #158972](https://github.com/python/cpython/issues/158972) | **[사실]** CPython free-threaded(내장 mimalloc)에서 macOS는 `MIMALLOC_PURGE_DELAY=0`을 줘도 RSS가 안 떨어짐. GIL 빌드는 거의 반환 | **[추정]** AxDB의 Rust mimalloc은 반환이 확인됐으나, 빌드 구성·mimalloc 버전이 바뀌면 깨질 수 있으니 릴리스 빌드에서 footprint 회귀 테스트를 둘 것 |
| `malloc_zone_pressure_relief`는 macOS에서 효과가 없을 수 있음 | [tako #2019](https://github.com/takushio2525/tako/issues/2019) | **[사실]** 이 앱 실측: `malloc_zone_pressure_relief(NULL,0)`은 0을 반환하며 반환 효과 없음. 대신 시작 시 환경 변수 `MallocLargeCache=0`이면 해제 즉시 반환(325→65MB), CPU는 동일~+3할. 환경 변수는 시작 시에만 유효 | **[추정]** 시스템 malloc으로 돌아갈 일이 있으면 대안. mimalloc을 쓰므로 현재는 불필요 |
| 대용량 임시 문서 후 디바운스된 relief 호출 | [openpencil heap_pressure.rs](https://github.com/ZSeven-W/openpencil/blob/d155cd08c09cdb79f4c0a2c733dc5f9376765abd/crates/op-host-desktop/src/heap_pressure.rs) | **[사실]** 750ms 트레일링 디바운스로 별도 스레드에서 `malloc_zone_pressure_relief` 호출하는 코드. 효과 수치는 없음(위 tako 결과와 상충) | **[추정]** 단독으로 믿지 말고 측정 후 채택 |
| macOS 할당자가 최고점을 유지 | [companion-emergence #443](https://github.com/hanamorix/companion-emergence/issues/443) | **[사실]** Python 앱 사례지만 macOS에서 해제 후 RSS 약 23.6% 유지, 약 850MB에서 정체. 질문으로 "단명 서브프로세스에서 실행"을 제시 | **[추정]** 이 문제가 AxDB가 드라이버를 자식 프로세스로 둔 이유와 같은 계열. 무거운 쿼리를 쓰는 드라이버를 쿼리 후 유휴 종료(6번)하면 할당자 종류와 무관하게 회수됨 |
| jemalloc 설정 | [jemalloc TUNING.md](https://raw.githubusercontent.com/jemalloc/jemalloc/dev/TUNING.md) | **[사실]** `dirty_decay_ms`/`muzzy_decay_ms`를 줄이면 메모리↓ CPU↑, `background_thread:true`는 꼬리 지연 개선. 최저 메모리 예: decay 0, `narenas:1`(할당이 드문 경우에만) | mimalloc이 이미 목표를 달성했으므로 jemalloc 전환은 불필요 |
| Zed의 할당자 | [Zed main.rs](https://github.com/zed-industries/zed/blob/56934ec0560bce8929025497411de7f5322fb6ca/crates/zed/src/main.rs) | **[사실]** `mimalloc` feature가 켜지면 `#[global_allocator]`로 사용. 릴리스 프로필은 `lto = "thin"`, `codegen-units = 1` | 참고용 |
| "RSS가 정상인데 footprint는 크다" 측정 함정 | [Zed #18673 코멘트](https://github.com/zed-industries/zed/issues/18673) | **[사실]** 한 사용자가 macOS 30시간 세션에서 physical footprint 22GB, RSS 약 600MB, 약 21.5GB가 스왑아웃이라고 보고(사용자 보고, 원인 미확정) | **[추정]** AxDB가 이미 footprint로 측정하는 것은 올바름. 유지 |
| 행 전체를 `Vec`에 모으는 쿼리 API | [tokio-postgres Client 문서](https://docs.rs/tokio-postgres/latest/tokio_postgres/struct.Client.html) | **[사실]** `query`는 `Vec<Row>`를 반환(전부 메모리). `query_raw`는 `RowStream` 반환. 문서는 내부 버퍼링 여부를 명시하지 않음 | **[추정]** 드라이버의 피크 RSS(113MB)는 일부 이 버퍼링 때문일 수 있음. 코드 확인 필요 |

## 4. 유휴 CPU 0

| 문제 | 링크 | 해결/결과 | AxDB 적용 가능성·예상 효과 |
|---|---|---|---|
| 무한 CSS 애니메이션이 유휴 렌더러 CPU를 먹음 | [adom-bridge #52](https://wiki.adom.inc/adom/adom-bridge/issues/52) (Tauri 앱) | **[사실]** 무한 `box-shadow` 애니메이션은 합성 불가라 매 프레임 스타일 재계산+페인트. Tauri `emit`이 숨겨진 웹뷰를 잠깐 깨워 다시 그림. LED를 정적으로 바꾼 뒤 창 열림 약 0.65→0.066코어, 숨김 약 1.0→0.00코어 | **[추정]** 로딩 스피너·상태 점·AI 입력 중 표시 등에서 box-shadow·width·top 애니메이션을 점검하고, 필요한 것만 opacity/transform으로. 숨김 상태에서 주기 `emit`을 보내지 않기 |
| 애니메이션이 CPU를 쓰는 Tauri 사례 | [tauri #7183](https://github.com/tauri-apps/tauri/issues/7183) | **[사실]** Linux WebKitWebProcess 고CPU. 한 사용자 보고로는 MUI 애니메이션 비활성화 후 크게 감소(해결 보고, 종료됨) | 같은 결론: 애니메이션 점검 |
| Windows `run-iteration` 고CPU | [tauri #10373](https://github.com/tauri-apps/tauri/issues/10373) | **[사실]** 원인 미확정, "not planned"로 종료 | 참고만 |
| 백그라운드 탭 타이머 스로틀 | [Chrome 타이머 스로틀](https://developer.chrome.com/blog/timer-throttling-in-chrome-88) | **[사실]** 숨김 5분 이후, 체인 5 이상, 30초 무음 등 조건에서 분당 1회로 제한. Chromium 기준이라 WebView2에는 근거가 되지만 WKWebView 동작은 이 글로 확인되지 않음 | **[추정]** 타이머에 의존하지 말고, 숨김 시 `visibilitychange`로 직접 정지 |
| tokio 워커 수·유휴 | [tokio Builder 문서](https://docs.rs/tokio/latest/tokio/runtime/struct.Builder.html) | **[사실]** `worker_threads` 기본은 코어 수, blocking 풀 최대 512, 유휴 blocking 스레드 10초 후 종료 | **[추정]** 드라이버 자식 프로세스는 `current_thread` 또는 `worker_threads(1~2)`로 스레드·스택 메모리 절감. 유휴 CPU는 park 때문에 0에 가까우므로 주기 타이머(하트비트·폴링)만 점검 |
| 유휴 세션이 CPU를 계속 씀 | [claude-code #90228](https://github.com/anthropics/claude-code/issues/90228) | **[사실]** 몇 주 유휴인 세션의 원격 제어 브리지가 재연결 루프로 약 7% CPU 지속(전사 기록 124회 재연결 근거). 미해결 | **[추정]** AI 로그인 토큰 갱신·MCP 재연결은 지수 백오프 상한과 "사용 중일 때만" 조건 필요 |

## 5. 시작 속도와 번들 크기

| 문제 | 링크 | 해결/결과 | AxDB 적용 가능성·예상 효과 |
|---|---|---|---|
| Rust 릴리스 프로필 | [Tauri 공식: App Size](https://v2.tauri.app/concept/size/) | **[사실]** `codegen-units = 1`, `lto = true`, `opt-level = "s"`, `panic = "abort"`, `strip = true` 권장. 수치와 시작 시간 설명은 없음 | **[추정]** 이미 적용했다면 해당 없음. `panic=abort`는 드라이버·호스트에서 패닉 시 프로세스 종료 동작이 바뀌므로 복구 로직과 함께 검토 |
| 프로필별 효과 | [min-sized-rust](https://github.com/johnthagen/min-sized-rust) | **[사실]** 수치 없이 정성 설명만. `opt-level "s"`가 `"z"`보다 작을 수도 있음, `codegen-units`는 기본 16 | 크기보다 시작·메모리가 목적이므로 이득은 작을 것 |
| Zed 릴리스 프로필 | [Zed Cargo.toml](https://github.com/zed-industries/zed/blob/56934ec0560bce8929025497411de7f5322fb6ca/Cargo.toml) | **[사실]** `lto="thin"`, `codegen-units=1` | thin LTO는 빌드 시간과 효과의 절충안 |
| CodeMirror 언어 지연 로드 | [@codemirror/language-data](https://github.com/codemirror/language-data) | **[사실]** README는 "언어 메타데이터와 동적 로딩"을 구현한다고만 설명(세부 API는 확인 못 함) | **[추정]** SQL 방언·Cypher·JSON 등을 편집기 최초 렌더에서 빼고 연결 종류에 맞춰 `import()`. 84KB 중 일부를 첫 화면에서 제거 가능 |
| Tauri cold start 사례 | 못 찾음 | — | **[추정]** 첫 화면 gzip 153KB는 이미 작은 편. 시작 시간은 드라이버 프로세스를 연결 시점에 띄우는 지연 기동이 더 큰 요소일 가능성 |

## 6. 자식 프로세스 모델의 유휴 정리

| 문제 | 링크 | 해결/결과 | AxDB 적용 가능성·예상 효과 |
|---|---|---|---|
| 호스트가 사라져도 서버 프로세스가 남음 | [MCP TS SDK #208](https://github.com/modelcontextprotocol/typescript-sdk/issues/208), [vscode-languageserver-node main.ts](https://github.com/microsoft/vscode-languageserver-node/blob/main/server/src/node/main.ts) | **[사실]** VS Code·LSP 경험: 서버를 호스트 PID와 함께 띄우고 3초마다 `process.kill(pid, 0)`으로 확인해 사라졌으면 종료. SDK 반영은 PR 3건이 시도됐으나 미결 | **[추정]** 드라이버 자식 프로세스가 부모 PID 감시 + stdin EOF 종료를 갖게 하면 호스트 크래시 시 고아 방지. 구현 몇십 줄 |
| stdin이 닫혀도 서버가 안 끝남 | [TS SDK #2002](https://github.com/modelcontextprotocol/typescript-sdk/issues/2002) | **[사실]** `StdioServerTransport`가 stdin `close`/`end`를 듣지 않아, 하루 사용에 고아 37개 | **[추정]** 드라이버 메인 루프에서 stdio 읽기 EOF를 정상 종료 신호로 처리 |
| `close()`가 직계 자식만 죽임 | [TS SDK #2023](https://github.com/modelcontextprotocol/typescript-sdk/issues/2023) | **[사실]** `npx`·`uvx` 같은 래퍼 뒤의 손자 프로세스가 고아로 남음. 프로세스 그룹 종료 필요 | **[추정]** 드라이버를 래퍼 없이 직접 실행하고 별도 프로세스 그룹으로 띄워 그룹 단위 종료 |
| 종료 시 SIGKILL만 사용 | [Zed #59323](https://github.com/zed-industries/zed/issues/59323), [rmcp #347](https://github.com/modelcontextprotocol/rust-sdk/issues/347) | **[사실]** MCP 명세는 stdin 닫기 → SIGTERM → SIGKILL 순서를 권장([수명주기](https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle)). Zed·rmcp는 바로 SIGKILL하던 사례 | **[추정]** 드라이버에 "닫기 → 짧은 유예 → 강제 종료" 순서. 백업·복원 중 드라이버가 종료되지 않도록 중요 |
| 죽인 자식을 회수하지 않아 좀비 누적 | [Zed #55950](https://github.com/zed-industries/zed/issues/55950) | **[사실]** macOS에서 `SIGKILL` 후 `waitpid` 없이 좀비 100개 이상(RSS 0이지만 프로세스 테이블 점유, 사용자 한도 도달 보고) | **[추정]** Rust `std::process::Child`/tokio `Child`는 `wait()`를 호출해야 함. kill 후 반드시 wait |
| 세션 창을 닫아도 MCP 자식이 남음 | [claude-code #58915](https://github.com/anthropics/claude-code/issues/58915), [#1935](https://github.com/anthropics/claude-code/issues/1935) | **[사실]** 하루에 MCP 프로세스 60개, RSS 약 3.1GB(#58915). #1935는 1.0.21에서 수정이라 답변됐으나 이후 재발 보고 있음 | **[추정]** AI·MCP를 쓰는 AxDB는 세션 ↔ 자식 프로세스 매핑을 하나의 레지스트리로 관리하고, 세션 종료 시 확실히 정리 |
| 유휴 서버 자동 종료/재시작 정책 | 못 찾음 | — | **[추정]** 정책 제안: 드라이버 N분(예: 5~10분) 유휴 → 정상 종료 → 다음 요청 시 재기동. 재시작 비용(수십~수백 ms)과 연결 풀 재수립을 측정해 N 결정. 근거는 위 3번 항목(할당자가 유휴 후에도 메모리를 보유) |

## 7. AI·MCP 세션 리소스

| 문제 | 링크 | 해결/결과 | AxDB 적용 가능성·예상 효과 |
|---|---|---|---|
| 토큰마다 UI 재렌더 | [AI SDK 문서](https://ai-sdk.dev/docs/troubleshooting/react-maximum-update-depth-exceeded), [vercel/ai #18547](https://github.com/vercel/ai/issues/18547) | **[사실]** 기본은 청크마다 재렌더, 특히 Markdown에서 부하. `throttle: 50` 권장. #18547은 v8에서 기본 50ms 제안(미결). 문서의 옵션명은 현재 `throttle`(예전 `experimental_throttle`) | **[추정]** Preact UI에서도 토큰 버퍼에 쌓고 50ms(또는 rAF)마다 한 번만 상태 갱신. 이미 확정된 앞부분 블록은 다시 파싱하지 않기 |
| 스트리밍 업데이트가 내비게이션을 굶김 | [vercel/ai #21849](https://github.com/vercel/ai/issues/21849) | **[사실]** 스트림 청크마다 urgent 렌더가 전환을 계속 중단해 최대 약 5초 지연(React `useSyncExternalStore` 특성, 종료됨) | Preact에는 직접 해당하지 않을 수 있음. "스트리밍 중 다른 화면 전환이 막히는지" 확인 항목으로 |
| Open WebUI의 청크 저장 | [open-webui #8087](https://github.com/open-webui/open-webui/issues/8087) | **[사실]** 메인테이너 추정: 청크마다 응답을 저장. "최적화 PR은 환영하나 방법이 안 보인다"며 토론으로 전환. 해결 없음 | **[추정]** AxDB는 대화 저장을 토큰마다 쓰지 않고 완료 시점·디바운스로 |
| AI 패널 메모리가 대화 턴에 따라 폭증 | [Zed #57425](https://github.com/zed-industries/zed/issues/57425), [#57478](https://github.com/zed-industries/zed/issues/57478) | **[사실]** 약 10턴 뒤 스트리밍 편집 중 RSS가 수 GB로 급증(종료됨). 이슈에 붙은 원인 분석은 작성자가 AI로 생성했고 검증되지 않았다고 스스로 밝힘. **원인은 확정 사실이 아님** | **[추정]** 일반 교훈: 스트리밍 청크마다 전체 문서 스냅샷을 만들지 말기, 대화 기록과 도구 결과에 상한 |
| 큰 MCP 도구 결과 | [Claude Code MCP 문서](https://code.claude.com/docs/en/mcp) | **[사실]** 도구 출력이 10,000토큰 초과 시 경고, 기본 25,000토큰 상한. 한도 초과 텍스트는 파일로 저장하고 대화에는 경로만 남김. HTTP/SSE 응답 16MiB 초과 시 실패 | **[추정]** AxDB의 AI가 쿼리 결과를 모델에 줄 때: 행 수·바이트 상한, 스키마·행 수·상위 N행·집계만 요약해 전달하고 전체는 결과 핸들로 보관(UI에서 보기). 10만 행을 문맥에 넣지 않음 |
| 큰 stdio 메시지의 이차 시간 | [TS SDK #2961](https://github.com/modelcontextprotocol/typescript-sdk/issues/2961), [#771](https://github.com/modelcontextprotocol/typescript-sdk/issues/771) | **[사실]** #2961: 100MB 규모 쿼리 결과 같은 큰 메시지에서 ReadBuffer가 청크마다 전체 복사·재스캔(열려 있음). #771: 50MB SSE 응답이 힙 7~14배로 증폭(원인은 `eventsource-parser`라는 댓글) | **[추정]** 자체 stdio MessagePack 프레임은 길이 접두 프레임으로 한 번에 할당해 읽기(재스캔 없음). 프레임 최대 크기 상한 |
| 무제한 수신 버퍼 | [rmcp #1030](https://github.com/modelcontextprotocol/rust-sdk/issues/1030) | **[사실]** 이슈 제목만 확인(stdio 수신 줄 버퍼가 무제한, 종료됨). 본문은 열지 않음 | **[추정]** 프레임 길이 상한 적용 |
| 세션 맵이 정리되지 않음 | [rmcp #1244](https://github.com/modelcontextprotocol/rust-sdk/issues/1244) | **[사실]** 유휴 타임아웃으로 워커가 끝나도 `LocalSessionManager`의 세션 항목이 영구 잔류(HTTP `DELETE`일 때만 제거). 종료됨. 댓글에서는 `main` 기준 일부 경로는 정리된다는 상반된 분석도 있음 | **[추정]** MCP 서버를 낸다면 세션 수 상한, 유휴 타임아웃, 맵 정리(스위퍼) 필요 |
| 세션 요청 정리 누락 | [python-sdk #1385](https://github.com/modelcontextprotocol/python-sdk/issues/1385) | **[사실]** 요청 응답기가 `_in_flight`에서 제거되지 않아 세션 메모리 증가(보안 보고 형태, 종료됨) | 같은 교훈: 요청 단위 상태는 완료·취소·타임아웃에서 반드시 제거 |
| 요청 타임아웃과 취소 | [MCP 수명주기 명세](https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle) | **[사실]** 모든 요청에 타임아웃을 두고, 초과 시 취소 알림을 보내며, 진행 알림으로 갱신해도 최대 타임아웃은 강제 | **[추정]** 도구 호출·쿼리에 타임아웃과 취소 연결(쿼리도 취소) |
| 재연결 정책 | [Claude Code MCP 문서](https://code.claude.com/docs/en/mcp) | **[사실]** 원격 서버 끊김 시 최대 5회 지수 백오프(1초 시작, 2배), stdio 서버는 자동 재연결 안 함 | **[추정]** 무한 재연결·고정 주기 폴링 금지(유휴 CPU 이슈 #90228 참조) |
| OAuth 토큰 갱신 타이머·폴링 | 못 찾음(직접 사례 없음) | — | **[추정]** 만료 시각 기준 단발 `setTimeout`/tokio 타이머, 사용 시점 지연 갱신(lazy refresh), 숨김·절전 시 폴링 없음 |

---

## 부록: 측정 제안 [추정]
1. 항목 1·2 전후: 10만×10열 10회 반복 후 `footprint`(호스트·WebContent·드라이버 별도)로 비교. 기준선 733MB 대비.
2. 탭 닫기 후 유지 메모리: Channel `onmessage` 해제 유무 A/B, 결과 전용 웹뷰 재생성 A/B.
3. 유휴 CPU: 30분 유휴에서 프로세스별 CPU 시간(`ps -o time`)이 늘지 않는지. 창 숨김·최소화 상태 포함.
4. 릴리스 빌드에서 mimalloc purge 회귀 테스트(위 cpython 사례처럼 구성이 바뀌면 깨질 수 있음).
