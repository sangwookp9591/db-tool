# P3b 드라이버 화면 수정

드라이버 탭은 `JellyRadio`의 선택적 segmented 변형으로 바꿨다. 하나의 34px 트랙 안에서 thumb가 이동하고 설치 개수는 작은 배지로 표시한다. 키보드 방향키와 기존 ARIA를 유지하며 reduced-motion에서는 이동 효과를 끈다. 기존 JellyRadio 기본 모드와 SlidingIndicator는 유지하므로 결과 뷰·온보딩·갤러리의 기존 사용처를 변경하지 않았다.

카탈로그 경고는 제목, 원인별 안내, 다시 시도, 파일 설치, 기본 접힘 기술 상세를 갖는 카드로 표시한다. 오프라인·404·시간 초과·서명 검증 실패를 구분하며 긴 상세는 줄바꿈하고 제한 높이 안에서 스크롤한다. 카탈로그 IPC 실패도 설치 실패 영역 대신 이 경고 카드로 전달한다. 빈 목록은 아이콘·제목·설명·파일 설치와 새로고침 CTA로 정렬했다.

제공된 이미지의 회색 하이라이트는 문구의 글자 경계를 따라가며 전역 `::selection { background: var(--surface-active) }`와 일치했다. 해당 문구에는 스켈레톤 클래스나 배경 스타일이 없었다. 일반 화면 문구의 드래그 선택을 막고 오류 및 기술 상세는 복사 가능하게 유지했다. 실제 사용자 앱을 조작하지 않았으므로 기존 WKWebView 선택 상태 자체를 읽은 것은 아니다.

`src-tauri/build.rs`는 `cargo xtask package-drivers`의 기본 출력인 `dist/drivers/index.json`을 `OUT_DIR`로 복사한다. 호스트는 `include_bytes!`로 포함한 바이트를 캐시가 없을 때 동기로 읽고, 기존 공식 키 목록으로 SignedCatalog 서명을 검증한다. 파일이 없으면 빈 목록이며 잘못된 JSON·서명·미등록 키는 거절한다. 2 MiB 제한을 빌드와 런타임에 적용한다. 설치 패키지는 기존 HTTPS 다운로드 및 무결성·서명 검증 경로를 사용한다. 기존 카탈로그 보안 테스트에 유효 스냅숏·빈 바이트·잘못된 JSON·미등록 키·변조 거절 검증을 추가했다.

현재 저장소에는 공식 서명된 `dist/drivers/index.json`이 없으므로 현재 개발 빌드는 빈 스냅숏이다. 공식 목록을 포함한 배포는 릴리스 담당자가 전체 드라이버를 package-drivers로 생성한 후 해당 서명 공개 키를 주입하여 앱을 빌드해야 한다. 테스트 키를 공식 키로 등록하거나 가짜 배포 목록을 추가하지 않았다.

검증 결과는 아래에 기록한다. UI 캡처는 Playwright Chromium의 로컬 mock이며 실제 macOS 앱·패키지 배포·DB 연결 증거가 아니다.

- [정상 midnight](design-shots/drivers-tabs-midnight.png), [정상 daylight](design-shots/drivers-tabs-daylight.png)
- [오프라인 midnight](design-shots/drivers-offline-midnight.png), [오프라인 daylight](design-shots/drivers-offline-daylight.png)

재현: `pnpm --dir ui dev --port 5195 --strictPort` 뒤 `cd ui && node src/drivers/verify-design.mjs`. 이 스크립트는 탭 키보드 이동, 트랙 높이, 이중 밑줄 제거, 접힌 상세와 줄바꿈, 재시도 종료, 화면 넘침, 오류 원인별 문구 및 reduced-motion을 확인한다.

공유 main의 시작 상태는 다수의 수정·미추적 P1/P2/P3/E1/O1 파일이 존재하는 상태였다. 이번 변경은 drivers UI/API/CSS, mock 카탈로그 시나리오 두 줄, JellyRadio와 관련 스타일, build.rs, driver_install.rs 및 기존 카탈로그 테스트, 캡처·검증 스크립트·이 보고서에 한정했다. dbcore/plugin_host/실행 커맨드, 사용자 앱 및 GUI, DB는 수정·접속하지 않았고 commit/push도 하지 않았다.

검증: `pnpm --dir ui exec tsc --noEmit`, scoped rustfmt, `git diff --check` 통과. `CARGO_TARGET_DIR=target-p2 cargo test -p axdb --lib driver_install::tests --no-default-features`는 6개 통과·1개 기존 ignored(실제 SDK 실행 파일을 요구하는 HTTP 통합 검사)였다. `CARGO_TARGET_DIR=target-p2 cargo clippy -p axdb --all-targets --no-default-features -- -D warnings`도 통과했다.

Playwright `verify-design.mjs`는 네 장의 요청 캡처와 키보드·상세·재시도·오류 원인·960px/reduced-motion 확인을 통과했다. 기존 `verify.mjs`는 설치→진행→연결 저장, 삭제/되돌리기, 로컬 파일 동의/취소/설치, 960×600 두 테마, 유휴/reduced-motion, pageerror 0을 통과했고 기존 available/installed/progress/unverified 캡처 여섯 장도 갱신했다. 추가 CTA로 버튼 이름이 중복되는 기존 스크립트 선택자는 툴바로 범위를 좁혔다.

최초 `pnpm --dir ui test --maxWorkers=1`은 22개 파일 중 21개, 109개 테스트 중 108개 통과했다(186.13초). 온보딩 설치 테스트가 5초, 종료 훅이 30초 제한에서 실패했다. 제한이나 테스트 코드를 바꾸지 않은 별도 온보딩 실행은 10/10 통과했다(47.55초). 병렬 Rust 컴파일이 진행 중이었으나 부하가 유일한 원인이라고 단정하지 않는다.

Rust 빌드 종료 후 전체 재검증에서도 108/109였고 이번에는 온보딩 설치 테스트 143행에서 설치 중인데 `다음` 버튼이 잠기지 않은 실제 중간 상태를 잡았다(45.61초). `DriverPicker`가 부모 잠금을 일반 `useEffect`로 전달하여 설치 상태의 첫 paint보다 늦어지는 것이 원인이었다. 호출처는 온보딩 장면 하나임을 확인하고 `useLayoutEffect`로 변경해 paint 전에 부모 잠금이 반영되도록 고쳤다. 테스트·타임아웃은 변경하지 않았다.

잠금 전달 수정 후 최종 전체 실행은 **22개 파일·109개 테스트 모두 통과**했다(51.61초). 최종 TypeScript와 diff 검사도 통과했다. Node의 localstorage-file 경고는 남았지만 테스트 실패는 없었다. 검증용 Vite 5195만 종료했으며 다른 작업자의 서버와 사용자 앱은 유지했다.
