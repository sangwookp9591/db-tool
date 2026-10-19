# P3 드라이버 설치 UX

ADR-0003에 따라 드라이버 레일, 설치됨/사용 가능 목록, 버전·크기·설명, 설치·업데이트·삭제, 파일 설치와 명시적 신뢰 동의를 구현했다. 삭제는 5초 유예·되돌리기를 제공하고 화면을 떠나면 예약을 취소한다. 설치되지 않은 기존 프로필에는 “드라이버 필요 · 설치”를 표시하며 연결 추가·편집·연결 실행도 설치 화면으로 안내한다. 기존 aria/testid와 S1 사이드바 구조는 유지했다. O1의 DB 드라이버 장면에는 공통 `DriverPicker`를 연결했다.

추가 사용자 요구의 [React Bits GlassIcons 원본](https://reactbits.dev/r/GlassIcons-TS-CSS)을 Preact로 포팅했다. 원본 굴절·조명 셰이더와 스프링 동작을 유지하면서 로고 atlas를 plate 샘플에 합성해 이미지 자체가 유리 아래에서 굴절된다. PostgreSQL·Neo4j·MongoDB·Redis·SQLite의 단순화 SVG는 저장소에 포함하고 data URL로 번들링한다. 요청한 size 78, gap 66, 최대 3열, roundness .62, refraction 1.35, bevel 1, frost .2, shine 1, offset/lift/hover-label, tint #dfdfdf/.2, spread 0, parallax 1 설정을 적용했다. 좁은 온보딩 영역은 2열로 접고 목록만 스크롤한다. 설치/선택 배지는 유리 밖에 표시하며 아이콘 클릭으로 상세 또는 선택을 연다.

그리드당 WebGL2 컨텍스트는 하나다. RAF는 정착하면 멈추며 reduced-motion 변경도 반영한다. 저프레임 환경에서도 스프링 시간이 불필요하게 늘어나지 않도록 실제 경과 시간을 안정적인 작은 적분 단계로 처리한다. 최초 진입은 화면별 한 번만 재생한다. WebGL이 없거나 컨텍스트를 잃으면 캔버스를 숨기고 이미지·유리·빛의 DOM 폴백을 사용한다. `THIRD_PARTY_NOTICES.md`에 원본과 동일한 MIT + Commons Clause 항목을 추가했다.

설치 작업은 다운로드→검증→서명 확인→설치→연결 확인 단계를 표시하고 바이트·전송 속도·남은 시간을 제공한다. P2의 `source/allowUnverified/onProgress`, `driver_cancel()->bool`, bytes/bytesPerSecond 계약을 사용하며 이벤트를 RAF 한 번으로 합친다. 완료 뒤 늦게 도착한 이벤트는 무시하고 검증 단계에서도 완료된 전송량을 유지한다. 여러 선택은 순차 큐로 표시하고 취소하면 남은 대기 작업도 중단한다. 커밋 이후 취소 거절은 마무리 중으로 안내한다. 실패는 원인과 재시도를 제공하며 모든 큐 항목이 실제 완료된 경우에만 완료 문구를 표시한다.

목록은 검증된 형태의 localStorage 메타데이터를 동기로 읽으며 모의/네이티브 캐시 키를 분리한다. 설치 권한과 무결성의 판단은 계속 호스트가 담당한다. 캐시가 없을 때만 같은 크기의 1회 sweep 스켈레톤을 사용하고, 갱신 중에는 작은 상태 문구를 표시한다. `DriversPanel`과 `GlassIcons`는 lazy chunk이며 빌드 기준 gzip은 각각 약 2.42 KB와 8.85 KB다. 로고 네트워크 요청은 없다. 시작 시 카탈로그 요청은 없고 화면 또는 온보딩 드라이버 장면에서만 호스트 로컬 캐시를 읽어 목록을 갱신한다. 카탈로그 밖 파일 설치도 `installed`와 `drivers_list`를 합쳐 표시·삭제한다.

검증 중 확인한 회귀는 수정했다. mock에서 네이티브 Channel을 생성하던 오류는 모의 수신 객체로 분리했고, 초기화 실패 때도 작업 잠금이 풀린다. 레일 버튼이 고정 에이전트 버튼 아래로 검색 버튼을 밀던 문제는 드라이버 항목을 기존 에이전트 슬롯 뒤로 이동해 해결했다. 갤러리 테스트의 최초 페이지 준비는 기존 30초 준비 단계로 옮겼으며 동작 테스트의 5초 제한은 유지했다. 초기 전체 실행의 92/104, 101/105, 이후 108/109 결과 및 시간 초과/누락 완료 문구 실패를 전체 통과로 숨기지 않았고 수정 후 최종 `pnpm --dir ui test --maxWorkers=1`은 22개 파일·109개 테스트 모두 통과했다(98.10초). `pnpm --dir ui build`의 TypeScript 검사와 production build, `git diff --check`도 통과했다.

재현: `pnpm --dir ui build`, `pnpm --dir ui preview --port 5194 --strictPort` 뒤 ui 폴더에서 `AXDB_UI_URL=http://127.0.0.1:5194 node src/drivers/verify.mjs`와 `AXDB_UI_URL=http://127.0.0.1:5194 node src/drivers/verify-glass.mjs`를 실행한다. 앞 스크립트는 설치→진행→연결 저장, 미설치 프로필 전환, 삭제·되돌리기, 파일 경고 취소·동의, 다크·라이트, 960×600, 유휴·reduced-motion, pageerror 0을 검증했다. 뒤 스크립트는 lazy import·캐시 첫 표시·스켈레톤·실패/재시도·취소·WebGL 1개·정착 후 프레임 0·컨텍스트 손실 폴백을 검증한다. 최신 측정은 캐시 없음 스켈레톤 5.8ms, 메모리 캐시 재열기 콘텐츠 2.7ms, 새로고침 후 저장 캐시 콘텐츠 18.7ms로 캐시 목표 50ms 미만을 충족했다. 드라이버 영역의 스켈레톤→목록 CLS는 0이며 전체 앱 초기 사이드바 배치의 별도 CLS 0.00403은 분리 기록했다. `verify-queue.mjs`로 두 드라이버 선택→순차 큐→완료→실제 설치 목록 반영과 탐색 잠금도 확인했다. 수치는 [측정 JSON](design-shots/drivers-performance.json)에 기록한다.

실제 macOS WKWebView는 `swiftc -parse-as-library ui/src/drivers/verify-wk.swift -o /tmp/axdb-p3-wk-smoke && /tmp/axdb-p3-wk-smoke`로 별도 오프스크린 창에서 확인한다. 이 창은 로컬 mock preview만 읽고 끝나면 닫힌다. WebGL GPU=true, 다크·라이트와 컨텍스트 손실 DOM 폴백을 확인했으며 P1 앱이나 `/Applications/AxDB.app`은 조작하지 않았다. [다크](design-shots/drivers-glass-midnight.png), [라이트](design-shots/drivers-glass-daylight.png), [스켈레톤](design-shots/drivers-skeleton-midnight.png), [진행](design-shots/drivers-progress-daylight.png), [실패](design-shots/drivers-error-midnight.png), [WKWebView](design-shots/drivers-glass-wk-midnight.png), [파일 경고](design-shots/drivers-unverified-midnight.png)에 화면 증거를 남겼다.

공유 main의 기존 병렬 변경을 보존했고 P3는 drivers UI·새 GlassIcons/로고·관련 App/Connections/mock 연결부·기존 탐색기 fixture·출처·보고/캡처만 변경했다. commit/push하지 않았고 Rust 빌드나 실제 DB 접속도 하지 않았다. 브라우저 및 WKWebView의 모의 성공은 실제 패키지 배포·네이티브 설치/취소·서명 검증 성공을 의미하지 않으며 그 증거는 P2 범위다.

P3 전용 Vite dev(5193)와 preview(5194), 격리 WKWebView 창은 검증 후 종료했다. 다른 실행 서버·사용자 앱은 유지했다. 최종 WKWebView 폴백 캡처에서는 숨겨진 WebGL 캔버스의 잔상이 아니라 DOM 이미지·유리 레이어가 표시되는 것까지 시각 확인했다.
