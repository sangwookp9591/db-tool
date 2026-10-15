# AI1 — schema and work context assistance

## Implementation

- `src-tauri/src/context.rs` is the shared builder for MCP/WebMCP and the Anthropic assistant. It reads manifest language/paradigm and `schema_graph`, falling back to bounded explorer traversal. No driver-ID dispatch was added to the UI.
- Schema output has a conservative 6,000 UTF-8 byte ceiling (at most about 6,000 byte-fallback tokens), preserves UTF-8, ranks matching entity names first and includes field types, PKs and available relationships/FKs. `schema_truncated` marks omitted metadata. Explorer fallback is limited to 100 visits and schema collection to 15 seconds.
- Work context includes current editor text, retained execution text/error kind/message, latest successful result column/row-count metadata, connection mode, and effective MCP client grants. The latest 20 run summaries stay in memory only; disconnect removes that session's summaries. `run_context_expired` distinguishes unavailable summaries. Full result rows are not copied into this store.
- Tools: `get_schema_context(session_id, focus?)`, `get_work_context()`, `explain_error(run)`. `run_query` now returns a `run` ID on execution success/failure. A1 filters tool lists and connection visibility, checks permissions before data access and rechecks context permissions before returning. Existing client allowlists are intentionally unchanged; users must explicitly grant the new tools.
- Optional assistant: default off; raw reqwest HTTP to the fixed Anthropic Messages endpoint, TLS certificate verification, redirects disabled, 120-second timeout, bounded SSE parsing, incremental UI text, explicit refusal handling. Default model `claude-opus-5-5`, adaptive thinking, configurable effort (default `medium`), fallback header/body as specified. Stable instructions and schema precede changing work context/question; the schema system block owns `cache_control: ephemeral`.
- API key uses OS keychain service `axdb`, account `ai:anthropic`. `ai.json` stores only enabled/model/effort using the existing private atomic writer. No prompts, API responses or API keys are added to query history or logging. Errors omit provider bodies and sensitive headers.
- Suggestions open a new query tab without execution. That tab's Run uses `execute_agent` with DB-enforced read-only mode. Writes require the existing C1 rollback preview followed by a separate visible approval action, A1's matching single-use preview token and `execute_agent`. Unsupported preview drivers cannot use this write path. Preview approval expires after the existing 60-second host lifetime.

## Data boundary

| Data | External MCP client | Anthropic (only enabled + key + explicit send) |
|---|---|---|
| Schema entities/types/PK/FK | Yes, bounded and permission filtered | Yes, cached system block |
| Manifest language/paradigm/access | Yes | Yes |
| Active editor / retained execution text | Work/error context only | Yes, up to 8,000 bytes each |
| Error kind/message | Retained run, message up to 2,000 bytes | Same |
| Result columns / row count / affected / truncated | Metadata only | Metadata only by default |
| Connection profile fields / password | Not included; known secret values redacted | Not included; known secret values redacted |
| Anthropic key | Never | Authentication header only; sensitive header, never a prompt |
| Selected rows | These context tools always exclude them | Explicit opt-in and selected 1-based row numbers; maximum 20 rows from the first available table |
| Masked/sensitive columns | No row data | Excluded case-insensitively, including user-entered masked column names |
| JSON/list/map/graph/byte cell values | No row data | Omitted (null), even with opt-in |

Sensitive-name filtering covers password/passwd/secret/token/api_key/apikey/credential/authorization/private_key/ssn/credit_card. This is not a general DLP classifier: arbitrary secrets embedded in query literals, error messages, schema names or ordinary scalar columns cannot all be inferred. The panel discloses query/error transmission before sending; selected context over 128 KB is rejected. No parameter values are independently exported. Requests do not execute generated text, even if schema/error content contains hostile instructions.

## Verification

All Rust commands use `CARGO_TARGET_DIR=target-ai1 CARGO_BUILD_JOBS=4`. An initial debug-info build exhausted disk space. Only this worker's `target-ai1` was cleaned (2.9 GiB); subsequent commands use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`. No unrelated build caches were deleted.

| Gate | Evidence / status |
|---|---|
| UI unit tests | `pnpm --dir ui test`: 9 files, 44 tests passed |
| Type check + production UI | `pnpm --dir ui build` (includes `tsc --noEmit`): passed |
| Rust lints | `cargo clippy --workspace --all-targets -- -D warnings`: passed |
| Native build | `cargo build -p axdb`: passed |
| Local host checks | `cargo test -p axdb --lib -- --skip postgres_real_context --skip neo4j_real_context --skip real_host_commands`: 28 passed, 2 ignored, 3 explicitly filtered live-DB tests |
| DB contracts / SQLite | `cargo test -p dbcore -p driver-sqlite -p driver-example-kv`: 16 tests passed, including real in-memory/file SQLite behavior |
| Anthropic HTTP mock | Request headers/model/adaptive effort/cache placement; mixed CRLF/LF and byte-fragmented UTF-8 SSE; refusal and incomplete/error streams; generic credential-free error messages: passed |
| Native MCP + SQLite | [six checks](../bench/results-ai1-native.json) via [repro script](../bench/ai1-context-smoke.mjs): actual app/host/bridge/schema, work editor, error run, read/none grants and disallowed row arguments |
| Browser UI + mocked Tauri IPC | [seven checks](../bench/results-ai1-browser.json): key guidance/default off, streaming, no automatic execution, read-only Run, separate preview/approval, refusal removes insert action. This is Chromium mock evidence, not native/provider proof |
| Full live workspace | `cargo test --workspace`: **66 passed, 2 ignored**, with actual PostgreSQL, Neo4j, MongoDB and SQLite tests; [raw final output](../bench/results-ai1-validation.txt) |

The first live run failed at PG/Neo4j connection setup while the shared OrbStack socket was absent. After the shared runtime recovered (without this worker restarting it), the final full workspace command passed with `AXDB_TEST_PG=host=127.0.0.1 port=55432 user=postgres password=testpw dbname=axdb_test`, `AXDB_TEST_NEO4J=bolt://127.0.0.1:57687|neo4j|testpassword` and `AXDB_TEST_MONGO=mongodb://axdb:testmongo@127.0.0.1:57017/?authSource=admin`. An intermediate run used the wrong Mongo variable name; it is not the final evidence. The final log includes the live Mongo test (0.73 s) and PG/Neo4j/SQLite context checks.

## Remaining verification and limits

- **Actual Anthropic call unverified**: no real key/call used. `assistant::tests::real_anthropic_once` is ignored and requires `AXDB_TEST_ANTHROPIC=1` plus a key in the named OS keychain. It makes one paid request when explicitly run. Real model entitlement, fallback availability, usage/billing and cache hits remain unverified.
- OS keychain write/delete/persistence and native assistant SSE delivery have not been exercised with a real key; mock UI proves only the UI action flow.
- PG/Neo4j/SQLite live context tests passed. Existing full GUI E2E/permissions E2E remain unrun in this task; the focused native SQLite permission checks are separate evidence.
- [Coordinator external-agent scenario](../bench/ai-agent-scenario.md) is ready for the coordinator to execute directly. No production DB was accessed.
- Graph/document nested row payloads are intentionally omitted, not recursively guessed safe. Scalar table rows are the supported opt-in path. No new row-selection system was added to the viewers; the panel selects explicit row numbers.
- Schema collection follows each driver's default `schema_graph(None)` scope (for PostgreSQL, its default public schema); `focus` prioritizes entities within returned metadata, not cross-schema discovery. Connection errors require a live/reconnected session for schema collection.
- No performance improvement claim or P1 measurement was made. No commits/pushes, deployments or messages to external channels were made.

Reference: provided raw-HTTP/prompt-caching guides, [Anthropic streaming](https://platform.claude.com/docs/en/build-with-claude/streaming), [refusal events](https://platform.claude.com/docs/en/test-and-evaluate/strengthen-guardrails/handle-streaming-refusals), and [reqwest redirect policy](https://docs.rs/reqwest/latest/reqwest/redirect/struct.Policy.html).

## Workspace ownership

The task started on a shared dirty tree containing A1, V1, M1 and P1 work. Changes were limited to the listed AI1 files and integration hunks; existing changes were preserved, no reset/commit was used, and only the AI1 native app/Vite processes and its build cache were stopped/cleaned. The native candidate is `target-ai1/debug/axdb` (debug information disabled for disk pressure); performance measurement remains P1-owned.
