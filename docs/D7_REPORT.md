# D7 — AI 우측 패널과 사용자 피드백

2026-10-20, main / 0.3.0 이후. 시작 시 작업 트리는 깨끗했다. ADR-0004, ADR-0003, ARCHITECTURE와 README를 확인했다. 사용자 앱·계정·DB·SSH 호스트, dist/, target/, target-rel은 변경하지 않았고 commit/push하지 않았다. 모든 빌드와 임베드 UI는 target-d7 아래에 두었다. 전체 Rust 테스트가 갱신한 기존 W4B 벤치 결과는 원래 내용으로 복원했다.

저장 경로는 기존 `ai_configure → ai_settings` 뒤에 사용자 피드백이 없었다. 이제 저장 중/저장됨 ✓, 성공·실패 토스트와 즉시 읽기 반영을 제공한다. AI가 꺼져 있으면 전송 범위를 설명하고 동의 후 켜고 저장한다. 실제 연결은 `session_open_progress`의 Channel로 기존 ConnectSteps/StatusMark를 사용하며, 성공 표시 뒤 닫히고 실패 단계·원인·다시 시도를 남긴다. 재연결 뒤 남던 네트워크 끊김 표시도 성공 이벤트로 해제한다. 기존 `session_open` IPC는 유지했다.

AI 도우미는 지연 로드되는 우측 도킹 패널로 바뀌었다. ⌘J, 너비 조절, 떠 있는 패널, 세션별 대화 목록/새 대화/삭제, 활성 탭·쿼리·선택·스키마·최근 오류 맥락, ⌘L/패널에 묻기, 50ms 스트림 묶음, 완성된 Markdown 블록 캐시, 메시지 가상화와 중지를 구현했다. SQL에는 CodeMirror 하이라이트와 적용/diff/새 탭/복사/파일 만들기를 제공한다. 적용은 단일 트랜잭션과 dirty 표시를 사용하고 자동 저장·DB 실행을 하지 않는다. diff 도중 탭 또는 내용이 바뀌면 적용을 거부한다. SQL 파일은 기존 일회용 저장 경로 토큰과 원자적 쓰기를 재사용하며 ⌘S도 저장 대화상자를 거친다.

사용자 제공 ThinkingLoader/MessageActions를 Preact와 토큰 CSS로 옮겼다. 기존 Button/ToolCallGroup을 재사용했고 출처를 THIRD_PARTY_NOTICES에 기록했다. 단계는 실제 호스트 이벤트로 바뀌며 숨긴 패널/문서에서는 로더를 렌더하지 않는다. 재생성 이전 버전과 좋아요·싫어요 사유는 메모리에만 보존한다. Kokonut 참고 컴포저, / 명령, @ 맥락 후보, 대화 예산 표시와 로컬 발췌 요약을 추가했다. 예산은 앱의 32,000-byte 제한이며 공급자 컨텍스트 한도라고 표시하지 않는다. 행 포함 응답은 후속 대화 맥락에서 제외하며, 호스트도 대화 역할/20개/UTF-8 32,000-byte 한도와 기존 비밀 redaction을 강제한다.

추가 패딩 피드백에 따라 상단 모델 선택기를 없애고 컴포저를 한 줄 툴바로 정리했다. 권한 모드는 방패·상태 점·팝오버로 표시하며, 읽기 전용 연결은 쓰기 선택을 비활성화한다. 사용자가 고른 읽기 전용 제안 모드는 호스트 context의 쓰기 권한도 낮춘다. 행 전송 토글은 한 줄 맥락 스트립으로 옮겼고, 비어 있는 선택·오류 칩은 숨겼다. 패널 16px/컴포저·SQL 카드 12px/툴바 간격 8px을 적용하고 작업 시간 0초는 1초 미만으로 표시했다. [권한 팝오버 Midnight](design-shots/d7-chat-permission-midnight.png), [Daylight](design-shots/d7-chat-permission-daylight.png).

## 검증

- `cargo build --workspace --bins`, `CARGO_TARGET_DIR=target-d7`: 통과. 플러그인은 `AXDB_DRIVER_DIR=target-d7/debug`로 사용했다.
- `cargo test -p axdb`: 200 통과, 7 ignored. 승인된 PG/Neo4j/Mongo/Redis fixture 환경을 사용했다. 환경이 없는 MySQL·별도 Neo4j 복원 대상 등은 실연결 증거가 아니다.
- 최종 호스트 lib 재검증: 133 통과, 4 ignored. `cargo clippy -p axdb --all-targets -- -D warnings` 통과. 기존 vendor/neo4rs의 deprecated 경고는 남아 있다.
- `pnpm --dir ui exec tsc --noEmit`, 전체 `pnpm --dir ui test`: 149 통과, 1 skipped. 모의 브라우저에서 Tauri Channel 생성으로 연결이 멈추던 회귀를 실제 재현하고 기존 모의 채널 패턴으로 고친 뒤 통과했다.
- 별도 debug 앱/WKWebView + PG 테스트 DB + debug 전용 `AXDB_AI_FIXTURE`: 저장 성공·실패, 자격 증명 입력, 실제 단계 이벤트, 인증 실패와 재시도, 스트리밍/중지, SQL 적용/dirty/한 번의 undo/diff, 재생성 버전, 새 탭 맥락 갱신, 자동 실행 없음, 유휴 무한 애니메이션 0, 1,000개 메시지에서 DOM 20개 이하를 확인했다. [검증 JSON](design-shots/d7-native-result.json), [체크 요약](design-shots/d7-checks.json), [Midnight](design-shots/d7-chat-midnight.png), [Daylight](design-shots/d7-chat-daylight.png).

재현은 `pnpm --dir ui exec vite build --outDir ../target-d7/ui`, `CARGO_TARGET_DIR=target-d7 TAURI_CONFIG='{"build":{"devUrl":null,"frontendDist":"/Users/iron/Project/psw/db-tool/target-d7/ui"},"identifier":"dev.axdb.d7probe"}' cargo build -p axdb`, `python3 bench/d7_native.py` 순서다. runner는 별도 임시 설정과 fixture를 만들고 자신이 띄운 PID의 창만 캡처한 뒤 종료한다. 초기 네이티브 검증의 오래된 임베드 UI와 비활성 버튼/짧은 스트림 상태를 놓치는 하네스 문제는 수정했으며, timeout을 늘리거나 실패를 숨기지 않았다.

## 남은 범위와 한계

실제 공급자 계정 로그인·과금 호출과 OS 파일 저장 대화상자 조작 전체는 검증하지 않았다. 파일 만들기/⌘S 구현은 기존 저장 대화상자와 토큰 검증 경로를 사용하지만 네이티브 화면 검증은 SQL 편집/dirty/undo까지다. `/Applications/AxDB.app`의 이전 바이너리를 직접 조작한 재현 증거는 없다.

후속 메시지로 추가된 여러 .sql 파일의 일괄 수락/거절/되돌리기, 임의 MCP 도구 실행 승인 카드 및 destructive 길게 누르기, '이 도구에 다시 묻지 않기'는 구현하지 않았다. 현재 패널은 제안과 편집만 제공하며 DB 실행은 기존 미리보기·승인 경로에 남는다. @ 후보는 현재 탭·연결·스키마 식별자의 경량 목록이고, compact는 로컬 발췌 요약이며 모델 기반 의미 요약은 아니다. 따라서 원래 피드백 1–4와 추가 요청 전체의 완료를 구분해야 한다.
