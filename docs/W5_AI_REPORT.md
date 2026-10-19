# W5 — AI 공급자와 Operation 계획

ADR-0004 D절에 따라 `assistant.rs`의 기존 Claude 로그인·질의 경로를 보존하고 `assistant/provider.rs`, `oauth.rs`, `operation.rs`를 추가했다. `AiProvider`는 로그인, 자격증명 갱신, completion 경계를 제공하고, Anthropic Messages 및 OpenAI Responses의 텍스트·도구·usage를 같은 응답으로 정규화한다. Anthropic API 키와 Claude `ant` 로그인, OpenAI API 키와 ChatGPT 로그인을 지원한다. AI 모듈에는 DB execute/preview/apply 호출이 없고, 자연어 출력은 Operation 후보뿐이다.

## 공식 OpenAI OAuth 판단

2026-10-19 공식 문서를 확인했다. [Quickstart](https://developers.openai.com/siwc/quickstart)는 상용 파트너의 제한적 제공과 별개로 오픈소스 앱의 ChatGPT plan 사용 흐름을 명시하고, [오픈소스 개요](https://developers.openai.com/siwc/token-sharing-open-source)는 로컬 앱용 동적 등록을 설명한다. 따라서 MIT 오픈소스 로컬 앱인 AxDB에는 공식 OSS 흐름을 구현했다. 다른 앱의 client ID·토큰·쿠키·Codex 자격증명 파일을 가져오는 방식은 사용하지 않는다.

[공식 로그인 계약](https://developers.openai.com/siwc/token-sharing-open-source/sign-in)에 따라 설치별 `urn:uuid:` host ID를 저장하고, 최초 `dynamic_agent_client`, `agent_name_hint=AxDB`, 매 시도별 state/nonce/PKCE S256, 먼저 바인딩한 `127.0.0.1` 루프백의 `/auth/callback`을 사용한다. 콜백 중복 매개변수·state·경로·재로그인의 client ID를 확인하고, 발급받은 client ID와 동일한 redirect/resource로 code를 교환한다. 공식 JWKS와 ring RS256으로 ID 토큰 서명, issuer, audience/azp, exp/nbf/iat, nonce, 기존 subject를 검증한 후만 계정을 활성화한다. 계정별 client ID와 검증된 subject를 보존하고 새 로그인 시도는 활성 계정과 분리한다.

[계정·세션 계약](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions)에 따라 access/refresh/ID token과 계정 등록은 OS 키체인 `axdb / ai:openai:oauth`에 저장한다. 만료 60초 전에 refresh하며 같은 설정 디렉터리의 앱 프로세스 간에는 OS 파일 잠금, 프로세스 안에서는 mutex로 회전을 직렬화하고, 새 access/refresh/scope/expiry를 한 키체인 값으로 교체한다. 로그아웃은 공식 OIDC discovery가 알리는 HTTPS auth.openai.com 폐기 endpoint에만 refresh token을 보내고, 네트워크·5xx 실패에는 최대 3회 backoff 후 로컬 토큰을 지운다. 원격 폐기를 확인하지 못하면 `remote_revoked:false`를 반환한다. 로그아웃 후에도 활성 계정의 client mapping을 보존해 재인증에 재사용하며, refresh의 400/401은 토큰을 비우고 재로그인을 요구한다. 같은 이메일의 등록도 client ID가 붙은 라벨로 구분한다. 서로 다른 AXDB_CONFIG_DIR로 실행하면서 같은 사용자 키체인 세션을 공유하는 구성은 지원하지 않는다.

[모델·추론 계약](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference)에 따라 선택된 계정의 `/v1/models`를 조회하고, 공식 `/v1/responses`에 `store:false, stream:true`로 요청한다. `response.completed` 이전 중단·실패·불완전 응답을 성공으로 처리하지 않는다. provider HTTP는 redirect를 허용하지 않고, 오류 메시지는 공급자 본문·토큰·프롬프트를 포함하지 않는다.

Anthropic 도구 호출은 [공식 도구 정의 문서](https://platform.claude.com/docs/en/agents-and-tools/tool-use/define-tools)의 Opus 5.5 등 모델 제한에 맞춰 `tool_choice:auto`와 명시적 도구 사용 지시를 쓴다. 구조화 도구 대신 일반 문장만 반환하면 후보를 만들지 않고 오류로 처리한다. 강제 `any/tool`을 보내 기본 모델에서 HTTP 400이 발생하는 조합을 피한다.

## 호스트·UI 계약

기존 `ai_settings/configure/request`와 Claude `ai_login_*`, `ai_logout`은 유지된다. settings에 `provider:anthropic|openai`가 추가되며 오래된 파일의 기본값은 anthropic이다. auth는 `api_key|claude_login|chatgpt_login`이며 공급자와 맞지 않는 조합을 거부한다. API 키는 공급자별 `axdb / ai:{provider}`에 저장한다. `ai_settings`는 `usage:{anthropic,openai}`와 `accounts:[{id,label,active,logged_in}]`을 함께 반환한다. usage는 완료 응답의 `input_tokens/output_tokens` 누적이며 금액 추정이나 공급자 청구서가 아니다.

| 커맨드 | 인자 및 결과 |
| --- | --- |
| `ai_provider_login_start` | `{provider,accountId?:string|null}` → `{url,waiting_for_code}`. 빈 문자열 accountId는 새 계정 등록, null은 활성 계정 재인증 |
| `ai_provider_login_poll` | `{provider}` → `{state:idle|waiting|done|failed,account?:string}` |
| `ai_provider_login_cancel` | `{provider}` → void |
| `ai_provider_logout` | `{provider}` → `{logged_in:false,remote_revoked?:boolean}` |
| `ai_account_select` | `{accountId}` → void |
| `ai_models` | `{}` → `[{id,name}]` |
| `ai_operation` | `{sessionId,intent,purpose:operation|explain|recommend_indexes,details?:object|null}` → 기존 정규화 응답 + `operations` |
| `ai_diagnose_connection` | `{steps:[{stage,status,elapsed_ms,error_code}]}` → 정규화 응답. 실패 연결에 sessionId를 요구하지 않음 |

Operation은 `{id,kind,connection,params,plan:{reason,evidence,statements},risk,needs_approval}`이다. id와 connection은 호스트가 부여한다. 도구 스키마는 현재 드라이버 Capabilities로 kind를 제한하고, 미지원 kind·알 수 없는 도구·형식 오류·불완전 도구 응답·근거 없는 제안을 거부한다. 모든 SQL/DDL 제안은 최소 write 승인 경로, restore 또는 DROP/TRUNCATE 제안은 destructive로 올린다. 후보에는 승인 토큰이 없고 실행하지 않는다.

자연어 Operation 컨텍스트는 schema, schema_truncated, query_language, paradigm, capabilities만 선택한다. 결과 행·선택 행·이전 쿼리·오류 원문·프로필 필드를 전달하지 않는다. 계획 해설도 PlanTree의 op/target/행 수/비용/children만 선택하므로 filter 조건과 실제 값은 제외된다. 기존 일반 AI 패널의 명시적 행 샘플 동의 동작은 유지된다. 진단은 정해진 단계, 상태, 시간, 오류 코드만 허용하고 원문 로그/message/help_key/자격증명 필드는 거부한다. 프롬프트는 안전한 수동 조치, 세 문장 계획 해설, 인덱스 근거와 쓰기·저장 비용 및 DDL 제안만 요청한다.

W6에 위 커맨드·settings·mock 계약을 메시지로 전달했고 UI는 W6 소유다. MCP 공통 등록 지점(permissions, dispatch, registry)을 W2/W3와 조율하여 W2가 `explain_query/lint_query`, W3가 `schema_diff/plan_import/plan_migration`을 맡는다. `plan_grants/backup_status`는 W4의 역할·백업 능력 구현 후 연결해야 하며 현재 능력이 없는 도구를 노출하지 않는다.

## 검증

재시작 시 기존 W5 코드·fixture·보고서가 남아 있었고 다른 작업자의 공유 변경도 이미 있었다. 이를 보존하면서 이번 실행은 assistant.rs의 갱신 추상화 호출, OAuth 수명주기, Operation의 실 SQLite 검증과 이 보고서를 보완했다. fake ant 테스트와 명시적 실제 Claude 테스트는 test-only mutex를 공유해 전역 CLI 환경 변수 충돌을 막았다. AI 모듈에 TODO나 실행을 가장한 placeholder가 없는지 확인했다. 검증 결과는 아래와 같다. 모든 Rust 산출물은 `target-w5-ai`만 사용했다. `dist`, `target`, `target-rel`, 사용자 앱·GUI·DB·SSH 호스트는 변경하지 않았고 commit/push하지 않았다.

실제 AI 호출은 `AXDB_TEST_ANTHROPIC` 또는 `AXDB_TEST_OPENAI`가 명시될 때만 실행한다. 기본 검증은 합성 프로토콜 fixture와 로컬 mock HTTP 서버를 사용하며, 실제 공급자에서 녹화한 응답 또는 실제 OAuth 로그인 증거로 주장하지 않는다. ID 토큰 fixture는 테스트 전용 임시 RSA 키로 서명하고 공개 키·서명 토큰만 남겼다. 실제 OS 키체인·브라우저 로그인·공급자 청구/갱신은 fixture 통과만으로 검증됐다고 볼 수 없다.

| 검증 | 이 실행에서 확인한 결과 |
| --- | --- |
| `cargo test -p axdb assistant:: --lib` | 23 passed / 0 failed / 1 ignored. 이 중 실제 공급자 opt-in 테스트 2개는 환경 변수 부재로 네트워크 호출 없이 반환했다. 이후 전체 lib에서도 AI 테스트가 전부 통과했다. |
| `cargo build --workspace --bins` | `target-w5-ai`에서 통과. W2 PG temp-schema 경고 수정 후 최신 bin을 다시 빌드했다. |
| 지정 실DB 환경 + `AXDB_DRIVER_DIR=target-w5-ai/debug cargo test -p axdb --test plugin_contracts` | 37 passed / 0 failed. PG·Neo4j·SQLite·Mongo·Redis 플러그인 계약, explain, 관리 경로 포함. |
| 지정 실DB 환경 + `cargo test -p axdb` | 마지막 lib 결과 108 passed / 1 failed / 3 ignored. 새 W1 `net::parse::tests::rejects_invalid_ports_and_non_postgres_psql_targets`가 `postgresql://db.test:0/app` 허용을 발견했다. 담당자에게 전달했고 이 실패 때문에 전체 crate 통과로 보지 않는다. 직전 스냅숏 lib는 104 passed / 0 failed / 3 ignored였고 실제 PG·Neo4j·Mongo 컨텍스트 테스트는 두 실행에서 통과했다. |
| `cargo clippy -p axdb --all-targets -- -D warnings` | 마지막 결과 실패: W1 `src-tauri/src/net/tls.rs:25`의 `nonminimal_bool`. W5 AI 파일 진단은 없었으나 전체 clippy 통과를 주장하지 않는다. |
| `pnpm --dir ui exec tsc --noEmit` | 통과. |
| `pnpm --dir ui test` | W6 변경 후 마지막 실행 26 files / 120 tests 통과. 앞선 실행은 116 passed / 2 timeout(CallChip·Button)이었으며 timeout 증대나 retry-to-pass 수정은 하지 않았다. |
| AI 파일 `rustfmt --check`, `git diff --check` | 통과. production Operation 모듈에 execute/apply/preview/승인 요청 호출이 없는 것도 확인했다. |

실 SQLite 테스트는 실제 테이블에 비공개 sentinel 값을 넣고, 행 샘플을 포함하도록 만든 기존 컨텍스트를 두 공급자의 요청 본문에서 제거하는지 확인한다. `DROP TABLE` 도구 fixture를 후보로 바꾼 뒤에도 원래 테이블과 행, 호스트 승인·미리보기 저장소가 변하지 않는지 실제 조회로 확인했다. 콜백 state/PKCE/client binding, RS256 ID token 서명·issuer/audience/nonce/expiry, scope 없는 refresh 거부·토큰 회전·로컬 삭제, discovery를 통한 외부 호스트 토큰 전송 거부, SSE 중단·실패·도구·usage와 로컬 HTTP 헤더/오류 redaction도 검사했다.

병렬 변경 중 Mongo PlanTree 이름/프로토콜 의존성 graph 불일치와 PG temp-schema 통계 누락을 발견해 W2와 조율했다. PG 실패는 W2 수정 후 37개 플러그인 재검증으로 해결을 확인했다. 디스크 부족으로 중단된 빌드·clippy는 코디네이터의 공간 확보 후 재실행했고, 이 워커는 자신의 `target-w5-ai/debug/incremental`만 정리했다. 마지막에 남은 전체 lib/clippy의 W1 실패는 별도 후속 검증이 필요하다.

실제 AI 공급자 호출, 브라우저 OAuth 로그인, 실제 OS 키체인 저장·refresh·revocation, 공급자에서 녹화한 응답은 미검증/미확보다. 합성 fixture의 통과를 그 증거로 대신하지 않는다. `AXDB_TEST_OPENAI`와 `AXDB_TEST_ANTHROPIC`가 없으므로 API 키 생성·재사용 확인을 요구하거나 외부 AI를 호출하지 않았다. 사용자 앱·GUI, 사용자 DB·SSH 호스트는 쓰지 않았고 commit/push도 하지 않았다.
