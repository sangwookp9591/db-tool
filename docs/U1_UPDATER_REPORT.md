# U1 — AxDB 앱 자동 업데이트

0.2.0에 Tauri v2 updater/process/opener를 연결했다. 상태바 배지, 현재 버전 → 새 버전, 릴리스 날짜·노트, 다운로드 진행률·설치·다시 시작, 나중에·버전 건너뛰기, 설정의 수동 확인과 자동 확인 토글을 구현했다. 기존 0.1.x 배포본에는 updater가 없으므로 0.2.0은 설치 파일로 한 번 설치해야 한다.

## 구현과 경계

`ui/src/update/`는 기존 Dialog와 Button 및 테마 토큰을 사용한다. 시작 10초 뒤 requestIdleCallback에서 확인한 뒤 24시간마다 확인하며, 콜백이 없는 WebView에서는 10초 지연 후 확인한다. 자동 네트워크 실패는 토스트나 대화상자를 띄우지 않는다. `settings.json`의 `updater.auto_check`는 기본 true이고 `skipped_version`은 자동 알림에만 적용되어 수동 확인으로 다시 접근할 수 있다. 설정은 기존 저장 락·private atomic writer를 사용해 다른 항목과 손상된 파일을 보존한다.

Rust에서 debug 빌드, AXDB_BENCH가 설정된 실행, AXDB_AGENT_BRIDGE=1을 판별해 자동 확인을 차단한다. 수동 확인은 허용한다. capability는 기존 core 권한에 check/download/install, restart, HTTP(S) 외부 링크 열기만 추가했다. updater 공개 키와 HTTPS GitHub 피드, createUpdaterArtifacts, Windows passive 설치 모드를 설정했다. 설치는 명시적인 버튼으로만 시작하며 진행 중 중복 작업과 닫기를 막는다. Windows NSIS는 설치 시 프로세스를 종료하고 다시 실행하므로 macOS의 설치 완료 → 다시 시작 버튼과 OS 동작에 차이가 있다.

릴리스 노트는 Preact 텍스트 노드로 HTML을 escape하고 제목·순서/비순서 목록·링크만 렌더링한다. HTTP(S) 절대 URL만 허용하고 사용자명·암호가 붙은 URL과 javascript/data/file/상대 링크를 차단한다. 네이티브 링크는 tauri opener로 연다. 별도 유휴 애니메이션은 없고 기존 디자인 시스템의 reduced-motion 처리를 따른다. 전체 크기를 모르는 다운로드는 바이트 수를 표시하며 진행 막대 자체를 애니메이션하지 않는다.

`mock.ts`의 `?update=available|none|error|download-error|install-error`는 브라우저 전용이다. mock의 다시 시작은 검증 가능한 이벤트만 발생시킨다. 실제 updater 다운로드·설치·재시작과 혼동하지 않는다.

## 릴리스

`scripts/release.sh <버전> [--publish]`는 세 버전 일치와 릴리스 노트를 먼저 검사하고 키체인 암호를 환경 변수로만 읽는다. 기본 타깃 디렉터리는 target-u1이다. universal macOS 앱·DMG 서명 → 공증 → staple/validate → 최종 앱 재압축·updater 서명 → Windows NSIS와 서명 → 세 플랫폼 latest.json 순서이며 기본 출력은 `dist/release/<버전>/`뿐이다. `--publish`에서만 gh release create --latest를 호출한다. 기존 출력 디렉터리와 기존 릴리스 태그는 덮어쓰지 않는다. 개인 키·암호를 인자나 로그에 넣지 않고 GitHub 호출 전에 환경 변수에서 제거한다. 상세 준비·키 분실 위험은 [RELEASE_SIGNING.md](RELEASE_SIGNING.md#8-앱-자동-업데이트-서명-키와-통합-릴리스)에 있다.

## 검증

- `pnpm --dir ui exec tsc --noEmit`: 통과.
- `pnpm --dir ui build`: 통과.
- `pnpm --dir ui test`: 최종 워크스페이스 실행에서 23 files / 113 tests 통과(24.79초). 앞선 병렬 빌드·검사 중에는 기존 onboarding 키보드 테스트가 5초 제한을 넘어 112 passed / 1 failed인 실행도 있었다. 별도 임시 복사본에서 변경 전 App과 변경 후 App 모두 113개 통과를 확인했고 작업용 Vite 서버를 종료한 뒤 최종 워크스페이스 검사도 통과했다. 타임아웃·재시도 설정과 기존 테스트 코드는 바꾸지 않았다. 자원/개발 서버 경쟁에 민감한 시간 제한은 남아 있다.
- `CARGO_TARGET_DIR=target-u1 cargo check -p axdb`: 통과.
- `CARGO_TARGET_DIR=target-u1 cargo clippy -p axdb --all-targets -- -D warnings`와 최종 `CARGO_TARGET_DIR=target-u1 cargo clippy --workspace --all-targets -- -D warnings`: 모두 통과.
- `cargo test -p axdb`: 새 target-u1에 드라이버 실행 파일이 없던 첫 실행은 56 passed / 4 failed / 3 ignored였다. Unknown driver를 확인한 뒤 workspace 실행 파일을 빌드하고 AXDB_DRIVER_DIR을 명시하는 환경으로 다시 검증하며 기존 테스트 코드는 수정하지 않았다. 드라이버를 준비한 최종 실행은 `AXDB_DRIVER_DIR="$PWD/target-u1/debug" CARGO_TARGET_DIR=target-u1 cargo test -p axdb`로 90 passed / 4 ignored / 0 failed였다.
- `cargo test -p axdb --test updater_feed`: 통과. 네이티브 updater 플러그인과 실제 loopback HTTP 서버에서 0.1.9 package metadata의 MockRuntime이 0.2.0-test를 감지했다. GUI·다운로드·설치는 실행하지 않았다. 이 검사는 서명 검증 증거가 아니다.
- `python3 -B scripts/test-release-manifest.py`, `python3 -B scripts/test-release.py`, `bash -n scripts/release.sh`: 통과. 세 플랫폼 URL·서명·노트, 누락 서명 거부, 기본 업로드 차단, 명시적 publish 순서, bash -x에서도 가짜 비밀 미노출, GitHub 호출 전 비밀 환경 변수 제거를 검증했다. 실제 Apple/GitHub 빌드는 가짜 실행 도구로 대체한 검사이다.
- `cd ui && node update-e2e.mjs`: 통과. 두 테마에서 자동 배지 → 안전한 노트 → 진행 → 설치 완료 → 모의 재시작, 최신 메시지, 자동 오류 무알림·수동 오류, 건너뛰기와 수동 재확인, 자동 확인 토글을 검증했다. HTML script 노드·javascript 링크 0개, 제목·목록·링크 DOM과 페이지 오류 0개를 확인했다.

1280×800 캡처는 `docs/design-shots/update-{badge,notes,progress,restart,latest}-{midnight,daylight}.png` 10개다. daylight 릴리스 노트 캡처를 직접 열어 본문과 하단 버튼이 표시되는 것을 확인했다. 별도 960×600 검사에서도 두 테마의 설정·대화상자·설치 버튼이 화면 안에 들어왔고 normal-motion 배지의 실행 중 애니메이션은 0개였다.

## 남은 실제 배포 검증

실제 Apple 서명·공증·Windows 설치 파일 빌드와 GitHub 공개는 실행하지 않았다. 실제 서명된 업데이트를 0.1.x로 빌드한 debug GUI 앱이 감지하는 검증도 실행하지 않았다. 사용자 앱·GUI를 건드리지 않는 제약을 지켰으며, 대신 GUI 없는 네이티브 플러그인 HTTP 감지 검사까지만 수행했다. 실제 아카이브의 updater 서명 검증·설치·재시작 및 각 OS 권한 처리는 별도 배포 환경 검증이 필요하다.

시작 시 기존 수정은 `.gitignore`, Cargo.lock, Cargo.toml, crates/driver-package/src/lib.rs, src-tauri/tauri.conf.json, ui/package.json이었다. 버전 0.2.0 등 선행 수정을 유지했으며 U1은 필요한 설정·의존성 줄만 추가했다. `/Applications/AxDB.app`, 사용자 GUI·DB, dist/drivers, target-rel을 변경하지 않았고 commit/push/publish를 하지 않았다.
