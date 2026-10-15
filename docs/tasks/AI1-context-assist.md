# AI1-context-assist: 스키마·작업 맥락 기반 AI 지원

이 작업은 반드시 끝까지 완료한다. 합리적인 가정으로 계속 진행하고, 결정한 내용은 마지막 보고에 적는다. 사용자 지시가 skill 지침보다 우선한다.

## 원칙
- AI는 권한(A1)과 미리보기(C1)를 **우회하지 않는다**. AI가 만든 쿼리는 에디터에 제안만 할 뿐, 자동으로 실행하지 않는다. 쓰기 문장은 미리보기와 승인 절차를 거친다.
- 기본값은 꺼짐이다. 행 데이터는 기본적으로 외부로 보내지 않는다. 보내는 내용은 스키마 요약, 쿼리 텍스트, 오류 메시지다. 사용자가 "선택한 행 포함"을 켰을 때만 행을 최대 20개까지 보내고, 이때 secret과 마스킹 대상 컬럼은 제외한다.
- 단일 원천: 맥락을 만드는 builder 하나를 MCP와 앱 내 도우미가 함께 쓴다.

## Change
1. **맥락 빌더(호스트, Rust).** `context::build(session_id, scope)`를 만든다.
   - 스키마 요약: V1의 schema_graph가 있으면 그것을, 없으면 explorer를 써서 엔티티, 필드, 타입, PK/FK를 간결한 텍스트로 만든다. 토큰 상한(예: 6k)을 두고, 넘으면 관련 엔티티 우선으로 자른다.
   - 작업 맥락: 활성 탭의 에디터 텍스트, 쿼리 언어(manifest.query_language), 최근 오류(kind와 메시지), 마지막 결과 요약(컬럼, 행 수), 연결 접근 모드와 권한
   - **드라이버 id로 분기하지 않는다.** 언어와 패러다임은 manifest 필드를 쓴다.
2. **MCP와 WebMCP 도구** (`ui/src/agent/registry.ts`와 브리지):
   - `get_schema_context(session_id, focus?)`
   - `get_work_context()`
   - `explain_error(run)`: 오류와 관련 스키마를 묶어 돌려준다
   - 기존 권한 필터를 그대로 적용한다. A1 프로필에서 해당 연결이 none이면 숨긴다.
   - 외부 에이전트(Claude Code 등)가 앱 맥락을 이해하는 주 경로다.
3. **앱 내 AI 도우미 패널 (선택 기능, BYO 키):**
   - 공급자: Anthropic Messages API를 raw HTTP(reqwest)로 호출한다. 사용 방법은 이 skill 문서를 읽고 따른다:
     - `/private/tmp/claude-501/bundled-skills/2.1.300/d12f0239f848833c88e7fc86f30ff2e0/claude-api/curl/examples.md`
     - 같은 디렉터리의 `shared/prompt-caching.md`
   - 모델은 기본 `claude-opus-5-5`로 하고, 설정에서 바꿀 수 있게 한다.
   - `thinking: {type:"adaptive"}`, `output_config.effort`는 명시적으로 `medium`(설정 가능)
   - 스트리밍(SSE)
   - 거부 대비: `anthropic-beta: server-side-fallback-2026-07-01` 헤더와 `fallbacks: "default"`. `stop_reason == "refusal"`이면 사용자에게 알린다.
   - 스키마 맥락은 system 블록에 `cache_control: {type:"ephemeral"}`로 둔다. 안정적인 접두부를 앞에, 변하는 질문을 뒤에 둔다.
   - API 키는 OS 키체인(서비스 `axdb`, 계정 `ai:anthropic`)에 저장한다. 로그와 이력에 남기지 않는다.
   - 기능:
     - "이 스키마에서 ~ 쿼리 작성": 결과를 에디터에 삽입하고 실행하지 않는다
     - "이 오류 설명/수정 제안"
     - "결과 요약": 기본은 컬럼과 통계만 쓰고, 행은 동의한 경우에만 보낸다
   - 출력 쿼리 언어는 연결의 query_language를 따른다(SQL, Cypher, Mongo 명령 JSON).
   - 키가 없으면 패널에 설정 안내만 보여 준다. 기본은 꺼짐이다.
4. **테스트:**
   - 맥락 빌더 단위 테스트: PG, Neo4j, SQLite 실제 DB에서 요약에 엔티티와 관계가 포함되는지, 토큰 상한, secret과 행 데이터가 빠지는지
   - 도우미 요청 조립 테스트: mock HTTP 서버로 헤더, 모델, `cache_control` 위치, 스트리밍 파싱, refusal 처리, 키가 로그에 안 남는지
   - 실제 Anthropic 호출: 이 머신에는 키가 없다. **실제 호출은 미검증으로 명시한다.** 키가 있으면 `AXDB_TEST_ANTHROPIC=1`로 1회 실행하는 ignored 테스트를 둔다.
   - 코디네이터 실검증 경로: 외부 에이전트 시나리오 문서 `bench/ai-agent-scenario.md`를 쓴다. 내용: MCP 클라이언트가 `get_schema_context` → 쿼리 작성 → `preview_changes`/`run_query(read)`를 수행하는 절차. 코디네이터(Claude)가 직접 실행한다.

## Constraints
- 테스트 DB만 쓴다. UI에서 드라이버 id로 분기하지 않는다. 기존 E2E와 권한 E2E를 유지한다.
- `CARGO_TARGET_DIR=target-ai1`을 쓴다. git commit을 하지 않는다. 완료할 때 `--files-modified`를 정확히 적는다.

## Observable acceptance
- cargo test, clippy, UI tsc, vitest, build 통과
- 보고서 `docs/AI1_REPORT.md`: 데이터 경계(무엇이 외부로 나가는지 표), 검증 범위, 미검증 항목
