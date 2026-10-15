# A1-permissions-mcp: 사용자·에이전트별 권한, 읽기 전용 모드, 범위가 있는 MCP

이 작업은 반드시 끝까지 완료한다. 합리적인 가정으로 계속 진행하고, 결정한 내용은 마지막 보고에 적는다. 사용자 지시가 skill 지침보다 우선한다.

## 배경
이미 있는 것은 재사용한다:
- W3: 에이전트 실행의 기본 읽기 전용(DB 수준), 승인 모달, 단일 bearer 토큰 브리지
- C1: 변경 미리보기
- W7b: ready 핸드셰이크

## Change
1. **권한 모델(호스트에서 강제한다. UI에서만 막는 것은 무효):**
   - **연결별 사용자 접근 모드.** 값은 `read_only`와 `read_write`이고 기본은 read_write다.
     - read_only 연결에서는 UI 실행과 browse를 포함한 모든 실행이 DB 수준 읽기 전용(`read_only_exec`)으로 강제된다.
     - 드라이버가 `read_only_exec`를 지원하지 않으면 이 모드를 고를 수 없다. UI가 capability를 보고 숨긴다.
     - UI 배지("읽기 전용")를 표시하고, 탭과 에디터에도 표시한다.
   - **에이전트 클라이언트 프로필**(`agent_clients.json`, 0600). 프로필 필드:
     - `id`, `name`
     - 토큰 해시(argon2 또는 sha256+salt). 토큰 원문은 생성할 때 한 번만 보여 주고 저장하지 않는다.
     - `connections: {profile_id → none | read | preview | write_with_approval}`
     - `allowed_tools`
     - `max_rows`
     - `expires_at`
   - 권한 단계의 의미:
     - `none`: 그 연결이 목록에 보이지 않는다.
     - `read`: DB 수준 읽기 전용으로만 실행한다.
     - `preview`: 읽기와 `preview_changes`만 쓸 수 있다.
     - `write_with_approval`: 쓰기를 하려면 미리보기를 보여 주는 앱 내 승인이 필요하다.
     - 무조건 쓰기 권한은 두지 않는다.
   - **우선순위.** 사용자 연결 모드가 에이전트 권한보다 우선한다. 예를 들어 read_only 연결이면 에이전트도 쓰지 못한다.
2. **MCP 브리지:**
   - 기존 단일 토큰은 "기본(로컬) 클라이언트" 프로필로 이전한다. 하위 호환을 유지한다.
   - 요청마다 토큰으로 클라이언트 프로필을 판별한다.
     - `tools/list`는 해당 프로필이 허용하는 도구만 돌려준다.
     - `list_connections`와 `open_connection`은 허용된 연결만 다룬다.
     - `run_query`는 프로필 권한과 연결 모드를 적용하고, 결과 행을 `max_rows`로 자른다.
   - 권한이 없으면 MCP 오류 `{kind:"Forbidden"}`을 돌려준다. 존재 여부를 노출하지 않도록 none 연결은 NotFound로 처리한다.
   - **감사 로그** `agent-audit.jsonl`(0600): 시각, 클라이언트 id, 도구, 연결 id, 결정(allowed/denied/approved/rejected), 문장 해시(문장 원문과 파라미터 값은 저장하지 않는다)
   - 토큰 회전과 폐기를 지원한다.
3. **UI 설정 화면(최소):**
   - 연결 편집에 접근 모드를 둔다.
   - "에이전트 클라이언트" 패널: 목록, 생성(토큰 1회 표시와 복사), 연결별 권한 표, 허용 도구, 폐기
   - 감사 로그 최근 50건을 보여 준다.
4. **테스트:**
   - 호스트 단위 테스트: 권한 매트릭스 전 조합
   - 실제 앱 브리지 테스트(`bench/permissions-e2e.mjs`): 클라이언트 A(PG read, Neo4j none), 클라이언트 B(PG preview), 기본 클라이언트(기존 동작)를 만들고 다음을 확인한다.
     - 목록 필터링
     - 금지된 도구와 연결
     - A의 쓰기는 DB 수준에서 거부
     - B의 미리보기는 허용되지만 커밋하지 않음
     - read_only 연결에서는 UI 경로의 쓰기도 거부
     - 감사 로그 기록
     - 토큰 폐기 후 401
   - 토큰과 비밀번호가 로그와 감사 로그에 남지 않는지 grep으로 확인한다.

## Constraints
- 기존 E2E 21/21을 유지한다. 기본 클라이언트의 동작은 바뀌지 않아야 한다.
- 테스트 DB만 쓴다.
- UI에서 드라이버 id로 분기하지 않는다.
- `CARGO_TARGET_DIR=target-a1`을 쓴다.
- 동시 작업 가능성: V1(뷰어). 공유 파일은 수정 직전에 다시 읽는다.
- git commit을 하지 않는다. 완료할 때 `--files-modified`를 정확히 적는다.

## Observable acceptance
- cargo test, clippy, UI tsc, vitest, build 통과
- `bench/permissions-e2e.mjs`가 실제 앱에서 통과
- 보고서 `docs/A1_PERMISSIONS_REPORT.md`(위협 모델 요약 포함). `docs/AGENT.md` 갱신
