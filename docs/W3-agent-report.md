# W3 agent interface report — 2026-10-15

## Result and ownership

Implemented the W3 specification: additive dbcore read-only contracts, PostgreSQL read-only execution, capability-derived agent tools, WebMCP/global JavaScript adapters, in-app query approval, source-tagged history, accessible controls, and opt-in localhost MCP bridge. No LLM calls were added. This worker did not commit or edit `crates/driver-neo4j` or `src-tauri/src/drivers.rs`.

Work was done on shared `main`; HEAD observed while writing this report was `384e6b6`. W2/W4 changes were preserved, including W4 streaming commands, active-view state, perf namespace and `get_perf_metrics`. Shared files were re-read before edits. A one-line W4 benchmark fixture adjustment now derives default connection fields from the manifest and overrides only isolated test endpoints, removing UI database-ID literals without changing production routing.

## Behavior and decisions

- `ExecRequest.read_only` and `Capabilities.read_only_exec` deserialize missing fields as false. Ordinary UI runs retain their prior behavior. Agent `run_query` defaults to true.
- PostgreSQL uses `BEGIN READ ONLY; SELECT 1` before executing agent SQL, then rolls back on success, error, cancellation or timeout. The initial snapshot prevents switching to READ WRITE through `DO` or `set_config`. SQL transaction/session control is rejected and a user-owned transaction causes `Unsupported`. Failed rollback closes the transport.
- The agent registry is the single tool-definition source. Capabilities plus open sessions determine optional browse/expand registration; target session capability is checked again at call time. Tool names, English descriptions, schemas and annotations are shared across adapters. Query results preserve typed values and cap aggregate rows/nodes/documents at 200.
- Explicit writes or unsupported read-only enforcement require a visible, per-call approval. Denial, Escape, overlapping approvals and deadline expiry return `UserDenied`. Expired approval cannot start a query.
- A separate additive `execute_agent` host command labels history as `agent`, while existing `execute`/`execute_stream` signatures remain unchanged and label history as `ui`. Old persisted history defaults to `ui`.
- Native accessibility input can update a credential field without triggering Preact's input handler. Credential submission now reads the actual `FormData` value; this repaired a reproducible native authentication failure during smoke testing.
- The bridge uses axum with only HTTP1/JSON/Tokio features. It binds `127.0.0.1:0`, uses a fresh random bearer token, rejects every Origin header, limits bodies to 1 MiB and pending calls to 64, forwards tools to the webview, and cancels expired work. Discovery/settings files use existing atomic 0600 storage. UI toggle state is persisted.
- Query approval expires at `timeout_ms + 30000`, or 60000 ms by default; this includes time spent in the dialog. The native smoke sets a 1 ms query timeout so write approval expires after approximately 30 seconds without issuing the write.

## Evidence

| Check | Result |
|---|---|
| `cargo check --workspace` | PASS |
| `DBTOOL_TEST_PG='host=127.0.0.1 port=55432 user=postgres password=testpw dbname=dbtool_test' cargo test --workspace` | PASS, 18 reported tests; PostgreSQL driver/host paths used the actual isolated DB; Neo4j live integration was env-gated and not exercised by this worker |
| Final focused read-only test after isolation/cleanup changes | PASS; SELECT, CREATE/INSERT denial, transaction/session escape denial, DO/set_config escape denial, timeout recovery, and write capability restored afterward |
| `cargo clippy --workspace -- -D warnings` | PASS on final Rust source |
| `pnpm --dir ui build` | PASS, includes `tsc --noEmit` |
| `pnpm --dir ui test` | PASS, latest observed 28 tests across 6 files including 10 registry tests; parallel W4 also added tests |
| `git diff --check` | PASS |
| Requested UI single-quoted DB-ID grep excluding mock | No matches after manifest-derived benchmark fixture cleanup |
| Browser dev, dedicated port 5189 | PASS: global API, optional tools, exact typed values with 200-row cap, visible tab, agent history, actual approval denial, document.modelContext shim and old registration abort |
| Real Tauri debug app, temporary config, `DBTOOL_AGENT_BRIDGE=1` | PASS: HTTP authentication, Origin rejection, initialize/list, open_connection, explore, actual PostgreSQL read-only SELECT/write denial, pending write approval timeout, no table persisted, agent history |
| Native UI toggle | PASS: disabled setting persisted, settings mode 0600, discovery removed; re-enable created a fresh discovery file and persisted true |
| Normal native window exit | PASS: final verification app exited and its discovery file disappeared |

Browser evidence: [`bench/results-agent-ui.json`](../bench/results-agent-ui.json). Native MCP evidence: [`bench/results-agent-bridge.json`](../bench/results-agent-bridge.json). Reproduction: [`bench/agent-bridge-smoke.mjs`](../bench/agent-bridge-smoke.mjs). Usage and safety model: [`AGENT.md`](AGENT.md).

Final cleanup: the worker-owned native app and port-5189 dev server were stopped; the temporary keychain entry was deleted, and no test credential was found in app profile/history/settings/log files. The private temporary configuration directory was retained for evidence without a live discovery token.

The read-only integration test uses pid plus nanosecond suffixes for every created/attempted table. It catches assertion unwinding, closes the test session, and uses a fresh connection to drop its unique objects before rethrowing a failure. The smoke probe also uses a UUID table name. Only PostgreSQL 127.0.0.1:55432 was accessed for this worker's DB verification; no accesses to 5432, 5439 or 32802 were made.

## WebMCP draft differences and remaining limits

This implements imperative registration and AbortSignal lifetimes from the [15 October 2026 Community Group draft](https://webmachinelearning.github.io/webmcp/), not declarative forms or browser agent discovery. The compatibility fallback supports `navigator.modelContext`; `window.dbtool.agent` adds schema/JSON validation. The native WKWebView is reached through Tauri events, not an assumed native WebMCP implementation.

The bridge implements the [MCP Streamable HTTP JSON response subset](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports), without SSE, session resumption, OAuth, or unsolicited tools-list change notifications. Clients must refresh tool lists after session changes. Real Claude Code/client compatibility beyond the provided HTTP configuration and Node smoke was not tested.

Count limits do not impose a byte limit on one large cell or nested path. PostgreSQL read-only semantics allow some temporary-object and nonpersistent side effects; a restricted database role remains appropriate. Arbitrary secrets deliberately placed in SQL/results cannot be inferred and scrubbed. Crash/force-kill can leave stale discovery data until the next startup replaces it.

A credential prompt for open_connection is user-managed; if that request expires and the user later submits its credential, the connection may still open, although no query is executed by that action. Test credentials seeded through the macOS `security` CLI were not readable by the unbundled debug app; the temporary keychain entry was deleted, and the passing native smoke used the app's session credential form. General native keychain ACL behavior, other operating systems, signing/bundling, live external providers and production DBs were not verified.
