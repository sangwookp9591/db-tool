# W7 — 연결 해제, GitHub Star, AI 모델·로그인 UX

ADR-0004 전체, ADR-0003, ARCHITECTURE와 README를 읽고 기존 dirty main에서 W7 소유 변경만 추가했다. commit/push, 사용자 DB·SSH, /Applications/AxDB.app·GUI, dist/, target/, target-rel/은 건드리지 않았다. Rust 산출물은 target-w7에만 생성했다. W1~W6의 기존 변경은 보존했다.

DB 연결 카드 메뉴·상태바에 연결 해제를 추가하고, 실행 중이면 확인 후 취소한다. 편집한 쿼리와 탭 ID를 보존하고 재연결 화면을 표시한다. 호스트 session_close는 참조가 남아 있어도 터널 cancellation을 즉시 전달하고 disconnected 이벤트를 보낸다. AI 공급자 카드에 대칭 로그인/해제와 키체인 API 키 삭제, ChatGPT 계정별 토큰 폐기를 연결했다. 원격 폐기가 확인되지 않으면 로컬 폐기와 구분해 표시한다. MCP는 브리지 전체 끄기와 별도인 토큰 폐기 확인을 둔다. 드라이버 삭제는 기존 FuseButton·열린 세션 거부 경로가 이미 있어 재사용했다.

Star 조건은 settings.json의 star에 first_seen, successful_queries, shown_at, dismissed, snooze_until, shown_count, starred_clicked를 저장한다. 3일·성공 20회, 최대 2회, 14일 스누즈를 호스트에서 판단한다. UI는 온보딩·오류·실행 중을 피하고 60초 유휴 뒤 평가한다. debug/자동화와 mock에서는 자동 표시하지 않는다. 상시 설정 메뉴와 카드의 PulseHeart는 GitHub를 외부 브라우저로 열며 GitHub API로 star하지 않는다. 최신 코디네이터 추가 지시에 따라 카드 표시 때만 공개 stargazers_count를 호스트가 조회한다(3초 timeout, 24시간 settings.json 캐시, 무인증, 실패 시 숫자 숨김). 클릭으로 공개 숫자를 가짜 +1 하지 않으며 감사 상태만 로컬에 저장한다.

AI 입력 아래 모델·Effort 칩, 검색과 공급자별 BranchedMenu, 로그인/Expired/미연결 상태, Settings 이동, Shift+Cmd+.를 추가했다. ai_models({provider?})는 기존 배열 계약을 유지하며 efforts/status/cached/auth를 추가한다. 공급자별 모델 API 응답을 1시간 캐시하고 실패 시 마지막 목록을 쓴다. 최초 실패는 문서화된 기본 모델만 표시한다. 알려진 모델의 effort만 허용하며 미지원 모델은 칩을 비활성화하고 요청 파라미터도 생략한다. 요청은 Anthropic output_config.effort, OpenAI reasoning.effort에 반영한다. AI 출력은 계속 제안/Operation이며 자동 실행 경로를 추가하지 않았다.

Claude/ChatGPT 연결 모달은 공식 ant CLI 또는 기존 OpenAI 루프백 경로와 기본 브라우저를 쓴다. 인증 도메인 allowlist, 진행·완료·실패·취소, 코드 붙여넣기 자동 제출과 명시적 클립보드 버튼을 구현했다. 별도 팝업을 만들지 않아 외부 브라우저를 앱에서 닫지는 않는다. 기존 공급자 클라이언트 ID를 복사하거나 우회하지 않았다. 사용자 제공 React Bits PulseHeart·StatusMark JS/CSS를 Preact로 포팅하고 NOTICE에 추가했다. motion/Hugeicons 등 새 의존성은 없다. StatusMark는 로그인·연결 테스트·드라이버 설치에 적용했으며 running만 회전하고 완료/닫힘 후 멈춘다. reduced-motion을 따른다.

검증: target-w7에서 cargo build --workspace --bins 통과. AXDB_DRIVER_DIR=target-w7/debug와 허용된 테스트 PG(127.0.0.1:55432, axdb_test)를 설정한 cargo test -p axdb --lib는 **117 passed / 4 ignored**였다. cargo clippy -p axdb --all-targets -- -D warnings, tsc --noEmit, git diff --check 통과. **pnpm --dir ui test는 33파일·132테스트 통과**(29.62초)했다. 해제 상태·모델/effort 매핑·로그인 도메인/코드·Star 조건/스누즈/클릭/캐시와 애니메이션 기하를 단위 검증했다.

Headless Playwright mock E2E는 해제/재연결 쿼리 보존, Star 수동 미리보기·해제, 모델/effort 저장·단축키, Claude 코드 paste 자동 제출, ChatGPT poll 완료를 확인했다. docs/design-shots/w7-{disconnect,star,model-picker}-{midnight,daylight}.png와 w7-connect-{claude,chatgpt}-{midnight,daylight}.png 총 10장을 생성하고 Star·로그인·모델 화면을 직접 확인했다. mock 캡처는 실제 로그인·DB 네이티브 UI의 증거가 아니다.

중간 실패도 보존한다: 처음 모델 테스트는 브라우저 전역 import 때문에 실패해 순수 함수로 분리했다. 첫 Rust 실행은 플러그인 경로 미설정, 다음 실행은 테스트 환경값 형식과 새 cancellation 테스트 인자 순서가 잘못돼 수정했다. UI 전체 중간 실행은 onboarding 이미지 내보내기 5초 timeout 및 afterAll 30초 timeout이 있었고, 관련 코드를 바꾸거나 timeout을 늘리지 않았다. 이후 전체 UI 실행들이 통과했다. 로그인 캡처 하네스는 잘못된 text locator와 ClipboardEvent 구성 때문에 실패해 실제 ClipboardEvent로 수정했다.

추가 전체 cargo test -p axdb 실행에서는 W4 administration SQLite 플러그인 복원 경로의 연결 단절과 PG DSN 환경 구성 오류를 확인했다. 초기 DSN에는 admin 통합 테스트용 비밀번호가 없었고, fixture DSN 재구성 때는 quoted public 필드를 context 테스트의 문자열 검사가 거부했다. 비밀은 출력하지 않았으며 이 실패들은 앞선 W7 관련 lib·UI 통과와 구분한다.

실제 OAuth/API 키 로그인·공급자 모델 API/추론 응답·원격 토큰 폐기·OS 키체인 삭제·공개 GitHub count 네트워크·SSH/프록시 실연결·배포 앱 검증은 하지 않았다. 이들은 mock/단위/테스트 PG 결과로 대체 주장하지 않는다. 알 수 없는 신규 모델은 effort 미지원으로 보수적으로 처리하며 공식 문서와 매핑을 갱신해야 한다.

공식 근거: [Anthropic effort](https://platform.claude.com/docs/en/build-with-claude/effort), [OpenAI reasoning](https://developers.openai.com/api/docs/guides/reasoning), [OpenAI 모델 안내](https://developers.openai.com/api/docs/guides/deployment-checklist), [React Bits 목록](https://reactbits.dev/llms.txt). React Bits 소스는 코디네이터가 전달한 scratchpad/reactbits의 사용자 제공 원본을 사용했다.

W7 소유 경로(공유 파일은 기존 변경에 추가한 hunk만): src-tauri/src/{star.rs,lib.rs,assistant.rs,assistant/provider.rs,assistant/oauth.rs,net/mod.rs}; ui/src/{App.tsx,Connections.tsx,api.ts,StarPrompt.tsx,disconnect.ts,disconnect.test.ts,w7.css}; ui/src/ai/{AssistantPanel.tsx,ModelPicker.tsx,modelSelection.ts,modelPicker.test.ts,AiConnectModal.tsx,loginFlow.ts,loginFlow.test.ts}; ui/src/ax/{AiAccounts.tsx,aiContract.ts,ConnectionStatus.tsx,mock.ts,accountsMock.test.ts}; ui/src/agent/ClientsPanel.tsx; ui/src/drivers/DriverCard.tsx; ui/src/ui/{PulseHeart.tsx,PulseHeart.css,pulseMath.ts,pulseMath.test.ts,StatusMark.tsx,StatusMark.css,statusMath.ts,statusMath.test.ts}; ui/{w7-e2e.mjs,w7-connect-e2e.mjs}; THIRD_PARTY_NOTICES.md와 본 보고서·10장 캡처. 시작부터 다른 작업의 tracked 수정과 untracked 파일이 있었고 종료에도 유지했다.

최종 확장 검사 결과: 최신 target-w7 workspace binaries를 재빌드하고 bench/fixtures.json의 허용된 테스트 PG 접속값을 메모리에서만 조합해 cargo test -p axdb를 실행했다. lib는 다시 **117 passed / 4 ignored**였지만 tests/administration.rs에서 **admin_sqlite_backup_restore_and_cancel:68**(backup 호출)과 **admin_pg_roundtrip_roles_copy_progress:142**가 모두 Connection("드라이버 연결이 끊겼습니다. 다시 연결하세요")로 실패해 전체 명령은 exit 101이다. 이 실패를 stale 바이너리만의 문제로 단정하지 않는다. administration의 나머지 3개 ok는 해당 DB 환경값을 켜지 않아 실DB 증거가 아니며, 뒤의 plugin_contracts와 doc test는 이 전체 실행에서 도달하지 못했다. W4/코디네이터에게 파일·행·오류를 전달했으며, 해당 소유 파일이나 실패를 감추는 테스트 변경은 하지 않았다. W7이 띄운 mock Vite 서버(51977)는 종료했고 기존 서버는 유지했다.
