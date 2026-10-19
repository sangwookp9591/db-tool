# D1 · AxDB 디자인 통합 결과

2026-10-16. 디자인·공식 로고·19종 컴포넌트 통합과 지정 검증을 마쳤다. **수용 판단이 필요한 항목은 cold 첫 표시 50ms와 전체 .app 증가량**이다. 최초 표시는 같은 조건의 기준선보다 빨라졌지만 50ms에는 도달하지 못했다. 코디네이터가 지정한 20분 진단 상한 안에서 수치와 미달 항목을 남겼으며, 성능 예산 전체 통과를 주장하지 않는다.

## 변경

이전 워커의 테마와 레이아웃을 이어받아 Midnight·Daylight·System, 첫 페인트 전 테마, 로컬 Pretendard Variable dynamic subset·JetBrains Mono Variable, lucide-preact를 통합했다. Rail, 연결 카드와 메뉴, 사이드바 리사이즈, 탭, 실행 옵션, 타입 배지·NULL pill·행 선택, 그래프·JSON·ERD 도구, 상태바, 이력, 오류·토스트와 모든 대화상자를 정리했다. 상태바 서버 정보는 기존 `Session.server_info()`를 전달하는 작은 명령으로 가져온다. 실패하면 드라이버 이름을 표시하고 정보를 얻지 못했음을 툴팁으로 알린다.

공통 Button은 모든 버튼에 적용했다. 그리드의 bare 경로는 내부 span·observer·모션 없이 native button 하나만 만들며 가시 셀을 재사용한다. 종료한 탭의 결과 캐시를 해제하고, 연결이 특정되지 않은 에이전트 결과를 활성 쿼리의 결과로 오인 표시하지 않는다. 기존 aria-label·role·단축키와 manifest 중심 분기를 유지했다. UI의 DB driver ID 리터럴 분기는 mock/tests 제외 0이다.

공식 마크는 Rail, 빈 상태, Beam 허브, 공통 Dialog 헤더와 favicon에 적용했다. 마크·아이콘 자산 및 Tauri 아이콘 설정은 코디네이터의 작업을 사용했다. 초기의 문자 A 타일은 제거했다. 네이티브 캡처에 실제 로고가 보이는 것을 직접 확인했다.

## 인터랙션과 실제 동작

| 추가 인터랙션 | 배치와 이유 | 출처 |
|---|---|---|
| CommandPalette | 연결·로드된 탐색 항목·최근 쿼리를 ⌘K/Ctrl+K로 검색 | [21st.dev Command](https://21st.dev/@shadcn/components/command) |
| SlidingIndicator | 탭·복수 결과 뷰의 위치 표시, 단일 뷰 측정 생략 | [affectUI Segmented Control](https://www.affectui.com/components/segmented-control) |
| SpotlightCard | 포인터 이동 때만 연결 카드 강조 | [React Bits](https://reactbits.dev/components/spotlight-card) |
| ToastStack | 연결·실행·복사·오류 알림, 최대 3개를 사이드바 하단에 배치하여 결과를 가리지 않음 | [affectUI Toaster](https://www.affectui.com/components/toaster) |
| Tooltip | 기존 접근성 이름을 유지하는 아이콘 설명 | [21st.dev Tooltip](https://21st.dev/@shadcn/components/tooltip) |
| Skeleton | 결과 대기 중 정적인 자리 표시, 완료 시 제거 | [affectUI Shimmer Loader](https://www.affectui.com/motion/shimmer-loader) |
| IntegrationBeam | 빈 상태 진입 1회, 이후 실제 쿼리·MCP 이벤트에만 반응 | [사용자 레퍼런스](design-shots/ref-integration-beam.md) |

D2의 기본 9종과 위 7종, D3의 Button·SuggestionCard·ToolCall/Group을 실제 호출부에 연결했다. 출처와 라이선스는 [고지](../THIRD_PARTY_NOTICES.md), API는 [D2](D2_MICRO_REPORT.md)·[D3](D3_COMPONENTS_REPORT.md), 추가 계약은 [DESIGN 5절](DESIGN.md#5-마이크로-인터랙션-매핑)에 있다. 작은 Button width의 480ms 단발 morph는 사용자가 요청한 예외이며 그리드에는 적용하지 않는다.

삭제·이력 삭제·토큰 폐기는 5초 뒤 실제 API를 호출한다. 취소하거나 화면을 닫으면 예약을 취소한다. AI 제안 수락은 새 탭 삽입이며 실행하지 않는다. AI 창을 닫고 쿼리를 수정한 뒤 다시 열어 Undo할 수 있고, 수정·실행 이력이 있으면 확인을 받는다. 취소는 탭만 닫으며 DB 변경을 되돌린다고 표시하지 않는다. 닫을 때 API 키·로그인 코드 초안은 비운다.

검사 신호는 로컬 파서, 지원하는 단순 SELECT의 조회 스키마 참조, 사용자가 요청한 READ ONLY EXPLAIN으로만 올린다. 복잡한 참조를 추측해 통과시키지 않는다. EXPLAIN은 단일 쿼리만 허용하고 ANALYZE를 붙이지 않는다. mock은 EXPLAIN 성공으로 승급하지 않는다. 실제 PG의 READ ONLY EXPLAIN도 별도로 확인했다.

MCP 카드는 실제 시작·종료, client/connection ID, 결정, 소요 시간과 행 수 요약을 표시한다. SQL·인자·행 원문·오류 본문은 이벤트에 실어 보존하지 않는다. 재생할 안전한 인자가 없는 호출에는 Retry를 제공하지 않으며 쓰기·preview·권한 거부에는 Retry를 연결하지 않는다. AI 맥락 카드는 별도 재조회를 사용한 것처럼 표시하지 않고, 서버가 공급자 요청에 실제 사용한 redacted context와 수집 시간을 반환한다. 이 변화는 선택적 응답 필드로 기존 호출과 호환된다. 실제 Anthropic 요청은 수행하지 않았다.

## 사용자 피드백 처리

[모달 원본 피드백](design-shots/user-feedback-modal-scroll.png)에 따라 공통 Dialog를 고정 헤더·본문 스크롤·고정 액션으로 나눴다. 높이는 min(720px,85vh), 내부 스크롤바는 6px이며 위·아래 fade를 사용한다. BorderGlow는 바깥에 두고 입력 focus의 이중 경계를 없앴다. input/select/number는 32px이며 필수 점과 chevron을 통일했다. 연결·인증·고급 그룹은 field key/kind/secret으로 나누고 host/port는 두 열로 배치한다.

두 테마와 1440×1000·1024×700, PG·Mongo 8조합에서 footer 위치·입력 높이와 Chromium CDP IME `ㄱ → 가 → 가나다` 보존을 확인했다. OS 입력기/WKWebView 조합 입력의 별도 실측은 아니다. [원자료](design-shots/modal-checks.json).

추가 리뷰의 토스트 겹침, 넓은 숫자 간격, 첫 숫자 열 폭·헤더 정렬, 실행 문구 혼용, 비좁은 kbd를 수정했다. 토스트는 결과 밖에 놓고 메트릭은 작은 칩으로 만들었다. 첫 숫자 열은 타입의 자리수와 표본을 고려하며 오른쪽 정렬을 유지한다. 실행·취소 등 공통 문구는 한국어, 단축키는 운영체제에 맞춰 표시한다.

## 성능: 같은 조건의 전후 비교

최종 기준 UI는 **20a1608**이다. [기준선 확인](design-shots/baseline-source-check.json). 두 Vite 사본, 같은 fixture·1440×1000·Chromium 156·교대 3회·기존 double-rAF 방법을 사용했다. 최종 Worker에는 pacing 실험을 남기지 않았다. [before](design-shots/results-paired-before-20a1608-final.json) / [after](design-shots/results-paired-after-20a1608-final.json).

| 지표 | before → after | 판정 |
|---|---|---|
| 10만 행 cold 첫 요청 | 133.7 → **83.0ms** | 개선됐으나 ≤50ms 미달 |
| 같은 페이지 후속 2회 | 25.9 / 32.5 → 32.5 / 39.8ms | 두 표본은 ≤50ms |
| 첫 표시 p50 / p95(3표본 최대) | 32.5 / 133.7 → 39.8 / 83.0ms | 전 지표 무회귀라고 주장하지 않음 |
| 전체 로드 p50 / p95 | 202.2 / 272.6 → 203.7 / 215.0ms | 중앙값 +1.5ms, 최댓값 개선 |
| 300노드 팬·줌 p95 | 최대 16.8 → 최대 **16.7ms** | ≤17ms 충족 |
| 흔들림·CLS | 평균 이동 0px, CLS 0 → 0 | 충족 |
| 가시 셀 | 288 → 306 | 28px에서 계약의 26px 행으로 변경한 결과 |

Trace에서 첫 동기 표시줄 측정과 반복 셀 구성, 폰트의 첫 layout 비용을 확인했다. 초기 geometry 측정은 ResizeObserver 뒤로 옮기고 가시 행을 재사용하며 불필요한 초기 reset을 제거했다. 공통 bare Button의 VNode key를 명시하여 가상화 중 잘못된 노드 재사용도 막았다. contain:strict와 가로·세로 가상화는 유지한다.

폰트 bytes와 공통 glyph metrics를 실제 main과 독립 viewer가 같은 준비 절차로 처리한다. 이 비용은 시작 FCP에 포함한다. 실제 executeQuery와 fixtureStream은 첫 행을 우선 paint한 뒤 후속 청크를 4ms 단위로 처리하며, occlusion으로 rAF가 멈춰도 완료가 정지하지 않는다. FIFO·취소·오류·occlusion을 검사했다.

폰트 kerning/ligature 변경은 일관된 이득이 없어 채택하지 않았다. Worker 1ms pacing 실험은 양쪽 임시 사본에만 적용했으며 cold before 99.7 / after 75.4ms로 50ms를 해결하지 못했고 전체 로드를 늘려 원복했다. 따라서 fixture만의 문제로 단정하지 않는다. 중간·실패·진단 표본은 `results-viewer-d1-*`, `results-paired-*-diagnostic`, `font-*-experiment.json`에 보존했다. 단일 trace의 46.5ms를 최종 통과 수치로 골라 쓰지 않았다.

## 네이티브·시작·크기

최신 앱 E2E는 **21/21, 5/5, 13/13**이다. [실행파일 해시와 결과](design-shots/native-e2e.json), [추가 확인](design-shots/native-final-extra.json). System 테마 복원, 서버 버전 표시, 실제 READ ONLY EXPLAIN을 확인했다. 기존 이력 삭제, AI context redaction, 활동 메타데이터에서 민감값을 제외하는 Rust 검사도 통과했다. UI **64/64**, tsc, `cargo clippy --workspace --all-targets -- -D warnings` 통과. 전체 Rust/실제 AI 공급자 테스트는 실행하지 않았다.

유효한 CPU 구간은 **60.794초**, app+새 WebKit 프로세스 1개의 누적 CPU 0.06초, **0.098694%**다. ps TIME은 0.01초 해상도여서 경계 가까운 단일 표본임을 명시한다. 90초 동안 이동 0px·render 0·`document.getAnimations()=0`. [원자료](design-shots/native-idle-performance.json). 반복 횟수 기반 최초 CPU 실행은 측정 창을 넘겨 무효 처리했고 제외했다.

전원은 AC·`lowpowermode=1`을 유지했다. WKWebView의 300노드 p95 34~35ms는 관측된 30Hz 환경으로 별도 기록하며, 코디네이터 지시에 따라 Chromium 예산 판정에 섞지 않는다. 저전력 제한과 일치하지만 설정을 바꾸는 대조실험은 하지 않았다. [WebKit 프레임 제한 이슈](https://bugs.webkit.org/show_bug.cgi?id=281429).

프로덕션 preview의 새 컨텍스트 교대 3회 FCP는 before **104/56/60ms**, after **132/108/108ms**다. 로컬 폰트와 metrics 준비를 포함해 시작 중앙값은 60→108ms로 증가했다. [FCP](design-shots/first-paint.json).

JS gzip은 **171,534→199,196 bytes**, 증가 **27,662 bytes(27.0KiB)**로 40KB 안이다. 로컬 woff2는 **2,998,128 bytes(2.86MiB)**로 폰트 3MiB 안이다. 동일한 개발용 .app wrapper는 **90,925,121→94,271,260 bytes**, 총 **3.19MiB 증가**이며 새 공식 icns 174,193 bytes도 포함한다. 전체 .app 증가를 3MiB로 엄격히 해석하면 초과이므로 숨기지 않는다. 서명된 release 번들 측정은 아니다. [크기·방법·해시](design-shots/size-comparison.json). 검증한 실행파일과 아이콘을 담은 로컬 결과물은 `target/design-debug/AxDB.app`에 보존했다.

## 화면과 정리

로고가 포함된 네이티브 14장은 `native-{empty,pg-table,neo4j-graph,mongo-documents,erd,connection,ai}-{midnight,daylight}.png`다. 실제 테스트 DB 화면이며 Mongo는 m1 fixture를 사용했다. 브라우저 대응 화면, 명령 팔레트·이력·Beam·AI 제안 카드와 1024px 모달 위/아래 캡처도 [design-shots](design-shots/)에 있다. [브라우저](design-shots/browser-checks.json), [표](design-shots/table-check.json), [AI 삽입·수정 후 Undo·footer 저장](design-shots/ai-integration-checks.json), [모션](design-shots/integration-checks.json) 증거를 분리했다.

이전의 흰 참고 캡처는 유효한 빈 화면으로 취급하지 않고 교체했다. 참고 앱의 원래 설정 파일 해시가 그대로임을 확인했다.

시작 시 이미 24개 추적 파일이 수정되어 있었고 이를 보존해 이어갔다. D2/D3 컴포넌트와 코디네이터 로고 작업을 함께 사용했으며 그 작업을 D1 단독 성과로 계산하지 않는다. 사용자 DB 접속, commit/push는 없었다. 테스트 앱·소유 dev/preview 서버·임시 사본은 정리하고, 검증 로그·스크린샷·결과 .app만 보존한다. **남은 수용 판단은 cold 50ms, 시작 지연 증가와 전체 .app 3MiB 해석**이다.
