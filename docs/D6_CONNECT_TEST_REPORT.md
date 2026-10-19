# D6 연결 테스트 연출

AI 계정 연결과 DB 연결 테스트가 `ui/src/ui/ConnectSteps.tsx`의 단계 줄과 `ConnectButton`을 공유한다. 기존 StatusMark의 pending → running → done/failed 전환을 사용하며, 회전은 진행 중에만 발생하고 reduced-motion에서는 공용 단계 줄의 반복 애니메이션을 제거한다. 공용 스타일은 `ConnectSteps.css`로 이동했고 기존 AI 단계 aria-label과 DB 테스트 버튼의 aria-label/test ID를 유지했다.

DB 폼 하단에 Connect with 헤더, D5의 `DriverLogo(iconKey, paradigm)` WebP 로고, 단계별 이벤트 시간과 실패 원인 안내를 표시한다. 네트워크 설정상 불필요한 SSH·터널 및 서버가 skipped로 보고한 단계는 숨기고 TLS는 실제 단계 이벤트가 도착할 때 표시한다. 실패 후 남은 단계는 대기 상태로 멈추며 기존 안전한 오류 코드 기반 AI 진단과 로그인 필요 흐름을 유지한다. 성공 버튼은 `연결됨 ✓`, 실행 중에는 `연결 중…`가 된다. 입력 수정과 재시도는 이전 결과를 초기화한다.

진입 시 Connections.tsx와 ConnectionStatus.tsx는 이미 수정된 상태였고 ProfileForm.tsx는 병행 작업에서 분리된 미추적 파일이었다. 따라서 실제 폼 구현인 ProfileForm.tsx의 관련 부분만 수정하고 Connections.tsx의 분리/로고 변경을 보존했다. w7.css의 기존 병행 변경도 보존했다. 드라이버 로고·manifest·백엔드·사용자 DB는 수정하지 않았다.

## 검증

- `pnpm --dir ui exec tsc --noEmit`: 최종 통과. 중간 병행 테스트 변경의 process 타입 오류는 담당 작업 수정 후 해소됨.
- `node ui/connect-test-capture.mjs`: 통과. 브라우저 요청 가로채기로 mock 이벤트만 주입하며 실제 폼 → API → Channel callback → 상태 렌더 경로를 사용한다. 두 테마의 진행/성공/실패, 실패 후 재시도, 미사용 단계 숨김, 전체 6단계, 종료 후 애니메이션 0, reduced-motion, 입력 수정 시 결과 초기화를 확인했다.
- `AX_UI_URL=http://127.0.0.1:51986 node ui/w7-connect-e2e.mjs`: 통과. Claude 코드 붙여넣기와 ChatGPT 폴링 및 두 테마를 확인했다.
- `pnpm --dir ui test`: 전체 통과 아님. 최초 35 suites/138 tests 통과, branched-menu beforeAll timeout; D5의 browser suite cache 격리 수정 후 재검증은 35 suites/135 tests 통과, extra.test.ts beforeAll timeout으로 실패했다. 마지막 로그에는 react/jsx-dev-runtime 미해결(WhatsNew.tsx)과 Preact __H 오류가 있으며 해당 병행 작업 담당에 전달했다. timeout 증가나 실패 은폐는 하지 않았다.
- `git diff --check` (수정된 추적 파일): 통과.

캡처: `docs/design-shots/connect-test-{running,done,failed}-{midnight,daylight}.png` 6장. 기존 `w7-connect-{claude,chatgpt}-{midnight,daylight}.png` 4장도 재생성했다. Daylight 완료와 Midnight 실패 캡처를 직접 확인했다.

실DB·SSH·TLS·공급자 로그인·네이티브 앱 증거가 아닌 mock UI 검증이다. 요청대로 Cargo/Tauri 빌드, 사용자 GUI 조작, commit/push는 수행하지 않았다.
