# D5 드라이버 타일 · 2026-10-19

드라이버 화면을 78px 굴절 유리와 설명 카드에서 **52px 원본 로고 타일 + 이름 한 줄**로 바꿨다. 960×600에서 SQLite, Redis, MySQL, PostgreSQL, Neo4j, MongoDB 6개가 한 줄에 보인다. 설명·버전·크기·패러다임은 300ms 호버/키보드 포커스 툴팁으로 옮겼고 터치 500ms 길게 누르기도 설치 없이 툴팁만 연다. 드라이버 화면은 GlassIcons/WebGL을 로드하지 않는다.

미설치 타일의 + 배지는 바로 설치를 시작한다. 설치 중에는 테두리 진행 링과 퍼센트, 완료 시 체크, 업데이트 시 점, 실패 시 빨간 느낌표를 표시하며 StatusMark를 재사용한다. 실행 중 타일은 취소 팝오버, 설치된 타일은 연결 추가/업데이트/삭제 팝오버를 연다. 삭제의 5초 되돌리기와 팝오버를 닫으면 삭제 예약 취소되는 동작을 유지했다. 100MiB 이상 패키지는 다운로드 크기를 확인하고 설치하며, 파일 설치의 미검증 서명 동의 절차는 기존 호스트 흐름을 그대로 사용한다. 대기열은 하단의 작은 진행 영역과 완료 개수로 표시한다.

온보딩 DriverPicker도 같은 타일을 쓴다. 방향키/Home/End 탐색, 다중 행 위·아래 이동, Enter 설치/관리, 상태가 포함된 접근성 이름을 지원한다. 온보딩 체크박스는 기존 이름을 유지하고 체크 상태와 aria-description으로 상태를 전달한다. 스켈레톤도 6개 타일의 위치·크기와 동일하다.

원본은 코디네이터가 제공한 devicon v2.16.0 컬러 SVG의 112×112 WebP 변환본이며, 로고 도형을 다시 그리지 않았다. 7종 합계 14,476 bytes, 파일별 1,132~3,680 bytes다. 밝은 타일 위에 원래 색으로 표시하며 외부 네트워크 이미지 요청은 없다. SVG 로고 사본은 제거했고 THIRD_PARTY_NOTICES.md에 devicon MIT 전문 및 지원 DB 식별용 상표 고지를 추가했다. MariaDB 자산은 준비했지만 카탈로그 항목은 추가하지 않았다.

공용 DriverLogo는 manifest/catalog의 optional `icon_key`로만 번들 로고를 찾고, 키가 없거나 알 수 없으면 패러다임 아이콘을 쓴다. 연결 카드와 쿼리 탭에 적용했고 D6 연결 타임라인도 같은 컴포넌트를 재사용한다. MySQL은 mock 카탈로그와 mock manifest에만 추가했다.

## 검증

- `pnpm --dir ui exec tsc --noEmit`: 통과.
- `pnpm --dir ui test`: 최종 36 suites / 141 tests 통과, 기존 1 suite / 1 test skipped, 2026-10-19 18:32:36 KST 시작, 24.12초.
- `AXDB_UI_URL=http://127.0.0.1:5199 node src/drivers/verify-glass.mjs`: 양 테마 960×600, 6개 한 줄, 실제 WebP naturalWidth 112, 로딩 전후 geometry 동일, **초기 카탈로그 로딩 CLS 0**, 툴팁/좌우·다중 행 방향키, 취소, 오류/재시도, 삭제 되돌리기, 업데이트, 터치 길게 누르기 통과. 초기 로딩 CLS를 전체 설치 흐름의 CLS로 일반화하지 않는다.
- 기존 `verify.mjs`: 설치 후 연결 저장, 삭제/되돌리기, 파일 동의/취소/설치, reduced motion/idle, overflow 및 페이지 오류 검사 통과.
- 기존 `verify-queue.mjs`: 온보딩 2개 선택, 순차 설치, 탐색 잠금/해제 및 설치 목록 검증 통과.
- 기존 `verify-design.mjs`: segmented 키보드 탐색, 오프라인/서명 오류 상세와 재시도, 양 테마 overflow 검사 통과.
- `git diff --check`: 통과. 새 unit-test 파일은 추가하지 않고 기존 검증 스크립트/선택자를 변경했다.

처음 전체 테스트에서는 이전 설치/삭제 버튼 선택자와 Vite 런타임 오류가 실패했다. 선택자를 타일 상태 이름으로 맞췄고, 세 브라우저 테스트 서버에 독립 cacheDir과 `optimizeDeps.rolldownOptions.transform.jsx.importSource = 'preact'`를 설정했다. 설치된 Vite 8 소스는 import.meta.glob TSX를 스캔할 때 tsconfig:false로 변환하므로 scanner JSX 설정이 없으면 react/jsx-dev-runtime을 찾고 초기 dependency scan이 실패했다. 캐시 분리만으로는 이 실패가 해결되지 않았으며 JSX 설정을 추가한 뒤 전체 검사가 통과했다. [Vite dependency optimization 설정](https://vite.dev/config/dep-optimization-options)을 함께 확인했다. 앱의 shared vite.config.mjs는 수정하지 않았다.

## 캡처와 범위

필수 캡처는 `docs/design-shots/drivers-v2-{available,installed,progress}-{midnight,daylight}.png`다. 같은 접두사로 skeleton, tooltip, failed, update, menu, onboarding 캡처와 `drivers-v2-check.json`을 남겼다. 설치됨/업데이트 화면은 브라우저 route에서만 설치 버전을 주입한 fixture이며 실제 패키지 설치 증거가 아니다.

시작부터 App.tsx, Connections.tsx, mock.ts, THIRD_PARTY_NOTICES.md, branched-menu.test.ts 등이 다른 작업으로 수정돼 있었다. 해당 파일은 로고 import/렌더, icon_key/mock 항목, 고지, 테스트 선택자와 서버 설정 hunk만 추가했고 기존 변경을 보존했다. 추가로 UI contract, 드라이버 전용 파일/자산/검증 스크립트, 기존 onboarding/extra 브라우저 테스트의 서버 설정, ax-e2e/w7-e2e의 설치 선택자를 변경했다.

**통합 대기:** Rust manifest와 native catalog의 `icon_key` 생산자는 이 작업에서 변경하지 않았다. 해당 키가 없는 실제 앱은 패러다임 아이콘으로 돌아가므로, 공식 드라이버의 브랜드 로고를 네이티브에 표시하려면 M1/코디네이터가 manifest와 catalog에 키를 전달해야 한다. 이 사항은 코디네이터에 escalation으로 전달했다. 실제 MySQL 드라이버 구현은 M1 범위다.

사용자 앱·GUI·DB, cargo/tauri 빌드, native WebView 검증, commit/push는 실행하지 않았다. 검증은 격리된 Vite mock 서버와 headless Chromium에 한정된다.

검증에 사용한 D5 전용 Vite 프로세스(5199)는 완료 후 종료했다. 다른 작업의 서버와 캐시는 보존했다.
