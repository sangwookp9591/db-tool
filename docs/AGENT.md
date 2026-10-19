# Agent interface

The app exposes automation tools and does not call an LLM. Common tool definitions live in `ui/src/agent/registry.ts`; WebMCP and JavaScript automation use this registry. The desktop MCP bridge adds the host-owned `get_result_page` tool. UI decisions use manifest capabilities, never database IDs.

## Tools and schemas

All inputs are JSON objects. Unknown top-level keys, missing required fields, wrong types, unsafe integers, and values outside limits are rejected. Session and explorer IDs are opaque strings obtained from the tools.

| Tool | Input | Behavior |
|---|---|---|
| `list_drivers` | `{}` | Installed manifests, capabilities and connection field definitions (no saved credentials) |
| `list_connections` | `{}` | Saved IDs, names, driver IDs and open session IDs; excludes connection fields |
| `open_connection` | `profile_id` | Opens a saved profile; the app prompts the user if a session credential is needed |
| `close_connection` | `session_id` | Cancels its runs, closes its session and visible tabs |
| `explore` | `session_id`, optional `parent` | One level of explorer children |
| `run_query` | `session_id`, `text`, optional `params`, `row_limit` (1–1000), `timeout_ms` (1–3600000), `read_only` | Executes with read-only default true and explicit approval when required |
| `get_result_page` (desktop MCP) | `session_id`, `result_id`, optional `offset` (items, default 0), `limit` (1–1000), `byte_offset` (original JSON bytes) | Reads a cached result without running SQL; current client, token, tool and connection permissions are checked again |
| `get_history` | optional `limit` (1–1000, default 100) | Recent entries, with `source: ui/agent`; excludes parameter values |
| `get_perf_metrics` | `{}` | Bounded viewer timing metrics in milliseconds (W4 integration) |
| `get_ui_state` | `{}` | Open connections, active tab connection and current editor text, last result summary |
| `open_query_tab` | `session_id`, optional `text` | Opens and focuses a visible editor tab without executing |
| `browse` | `session_id`, `node_id`, optional `offset` (default 0), `limit` (1–1000, default 100) | Registered only while an open session supports `paged_browse` |
| `expand_node` | `session_id`, `element_id`, optional `limit` (1–200, default 200) | Registered only while an open session supports `graph_expand` |

```js
const tools = window.axdb.agent.listTools();
const connections = await window.axdb.agent.callTool('list_connections', {});
const { session_id } = await window.axdb.agent.callTool('open_connection', {
  profile_id: connections[0].id
});
await window.axdb.agent.callTool('open_query_tab', { session_id, text: 'SELECT 1' });
const result = await window.axdb.agent.callTool('run_query', {
  session_id, text: 'SELECT 1', read_only: true
});
```

`run_query`, `browse`, and `expand_node` return `{results: [...]}`. Errors return `{error: {kind, message}}`. Row counts describe the underlying result; `truncated` signals omitted content. Returned table rows, graph nodes and documents are each capped at 200 across the response; graph relations are filtered to retained endpoints and capped at 200 per result. Typed `Value` tags and integer/decimal/JSON strings are preserved. The browser registry retains this count-only contract. Desktop MCP additionally caps the structured tool value at `max_result_rows` (default 200, configurable 1–1000) and `max_result_bytes` (default 256 KiB, configurable 8–1024 KiB) before UI forwarding and response encoding. `max_rows` remains the database fetch limit. The bounded value is represented in both MCP text and structured content, so the JSON-RPC wire envelope can be larger than the structured-value budget.

## WebMCP

When available, tools register on `document.modelContext`, falling back to `navigator.modelContext`. Re-registration aborts the prior registration's AbortController when manifests or open sessions change. The `window.axdb.agent` API remains available in browsers without WebMCP, including the browser mock backend.

The [15 October 2026 WebMCP draft](https://webmachinelearning.github.io/webmcp/) is a Community Group draft, not a stable standard. The app implements its imperative tool registration subset, not declarative form annotations or browser agent discovery. Its JavaScript API adds minimal schema validation and JSON serialization checks; execution returns JSON objects. No claim is made that WKWebView itself implements WebMCP or that external agents can directly access its modelContext.

## Desktop localhost MCP bridge

The bridge is **off by default**. Enable the `MCP bridge` checkbox in the header, or launch:

```sh
AXDB_AGENT_BRIDGE=1 AXDB_CONFIG_DIR=/absolute/private/config target/debug/axdb
```

The toggle persists `agent_bridge` in `settings.json`, preserving other settings. The environment variable enables the bridge at app startup even if the persisted toggle was off. It listens on an OS-selected `127.0.0.1` port and writes `{url, token}` to `agent-bridge.json` with mode 0600 on Unix. Normal app exit or disabling the bridge removes the discovery file; after a crash it can remain stale, and the next start replaces it with a fresh token and port. Do not share this file or put it in version control.

Configure an HTTP MCP client using the current values from that file:

```json
{
  "mcpServers": {
    "axdb": {
      "type": "http",
      "url": "http://127.0.0.1:49152/mcp",
      "headers": { "Authorization": "Bearer <token-from-agent-bridge.json>" }
    }
  }
}
```

The endpoint implements the [MCP Streamable HTTP JSON response subset](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports): `initialize`, `notifications/initialized`, `tools/list`, `tools/call`, and `ping`. Supported protocol versions are `2025-03-26`, `2025-06-18`, and `2025-11-25`; no SSE, resumption, OAuth, or unsolicited tool-list notifications are implemented. Clients should refresh `tools/list` after opening/closing connections. GET returns 405. Calls return MCP text content plus `structuredContent`, with `isError` for tool errors.

Test with the running desktop app and a saved PostgreSQL test profile on **127.0.0.1:55432**. Store its test credential with the profile, or enter it in the app when the smoke call opens a credential dialog:

```sh
AXDB_CONFIG_DIR=/absolute/private/config node bench/agent-bridge-smoke.mjs
```

The smoke script checks authorization, Origin rejection, initialization, discovery, connection opening, exploration, exact typed SELECT, DB-level write rejection, approval timeout, lack of persisted writes and agent history. During its final write request, leave the approval dialog untouched so it expires after approximately 30 seconds.

## Safety model and limits

- Every bridge request requires a bearer token belonging to an active client profile. Any Origin header is rejected (403), including same-origin-looking values, and no CORS permission is advertised. The server binds only loopback, limits JSON bodies to 1 MiB and pending requests to 64, and never logs tokens or parameter values.
- `run_query` defaults to read-only. Drivers declaring `read_only_exec` must enforce that mode themselves. PostgreSQL uses `BEGIN READ ONLY`, acquires an initial snapshot to prevent switching to READ WRITE, then always rolls back, including errors/cancellation. Transaction/session control is refused, and execution inside a user-owned transaction is refused with `Unsupported`. PostgreSQL read-only semantics still permit some temporary-object changes and nonpersistent side effects; use a restricted DB account when stronger isolation is needed.
- Explicit writes require a supported change preview and the in-app dialog. It displays the rollback preview, query text, connection name, approve and deny controls. Drivers without preview support cannot execute agent writes. Escape, denial, overlapping approval requests, and deadline expiration fail closed with `UserDenied`. Approval is per call, never cached. Read requests on drivers without read-only enforcement are refused by the host.
- A bridge request expires at query timeout plus 30 seconds, or 60 seconds by default. Expiration aborts pending approval and cancels an executing query. A late approval cannot start an expired query. Credential prompts for opening a connection remain user-managed; an expired open request may still open a session if the user later submits its credential.
- Data-bearing tools carry `untrustedContentHint`; database content, query text, history and profile names must never be treated as agent instructions. Hints are metadata, not a sandbox.
- Profile secrets stay in the keychain and are excluded from connection listings. History keeps query text and redacts the current connection credential; arbitrary secrets deliberately embedded in SQL cannot be inferred automatically. Query results can contain sensitive data by design, so grant the bridge token only to a trusted local client.
- The header shows agent call activity and pulses during execution, with reduced-motion support. Main connection, tree, execution, result-view, paging and approval controls expose accessible names and test IDs.


## Scoped clients and connection access modes (A1)

Open **에이전트 클라이언트** in the app header to create/edit clients, assign connection permissions and allowed tools, set `max_rows` (1–1000) and an optional RFC3339 `expires_at`, rotate/revoke tokens, and inspect the latest 50 audit entries. A generated token appears once with a copy button. `agent_clients.json` (0600) stores only a random salt and SHA-256 token hash; list responses exclude both. Configure each MCP client with its own bearer token and the same running bridge URL. Invalid, expired or revoked tokens return HTTP 401. Rotation invalidates the previous token immediately.

| Permission | Host behavior |
|---|---|
| `none` / omitted | Connection is absent from lists; access returns `NotFound` |
| `read` | `run_query` uses database-enforced read-only execution; requesting a write returns `Forbidden` |
| `preview` | Read plus `preview_changes`; cannot commit changes |
| `write_with_approval` | Read plus preview; each write requires a rollback preview and in-app approval |

`allowed_tools` additionally restricts both `tools/list` and calls; a disallowed call returns `{error:{kind:"Forbidden",...}}`. There is no unconditional write grant. The host caps aggregate returned table rows, graph nodes and documents to `min(max_rows, 200)`, preserving the previous 200-item response ceiling. Preview samples are bounded too. Scoped history excludes other connections and old entries without a profile id. Scoped UI state returns permitted connections without global editor text or previous result summaries.

The connection editor offers `read_only` only when the manifest declares `read_only_exec`; default is `read_write` for old and new profiles. The latest saved mode is checked by the host for UI/streaming execution as well as agent queries, including existing sessions. A read-only connection takes precedence over any client grant, and cannot run a write preview. PostgreSQL/Neo4j/SQLite browse use database read-only execution. Connection, tab and editor badges display **읽기 전용**.

The legacy discovery bearer remains the **기본(로컬) 클라이언트**, with existing connection/tool access by default. Its raw token remains in the 0600 `agent-bridge.json` for compatibility; newly created scoped tokens are never saved in raw form. Restart creates a new legacy bearer while preserving scoped client profiles. Revoked clients stay revoked until an explicit token rotation. The default profile's rotation also updates discovery.

`agent-audit.jsonl` (0600) records timestamp, client id, tool, connection id, decision (`allowed`, `denied`, `approved`, `rejected`) and statement SHA-256. It excludes query text, parameter values and bearer tokens. Approval rechecks token validity, permissions and connection mode; a late approval after revocation cannot execute. Local WebMCP writes also require a matching recent preview and one-use host approval bound to the session and execution request. These controls assume the app webview, Tauri IPC and the local OS account are trusted.

See [A1_PERMISSIONS_REPORT.md](A1_PERMISSIONS_REPORT.md) for the threat model, exact evidence and known limits. Real-app checks (test profiles only):

```sh
AXDB_CONFIG_DIR=/absolute/private/test-config AXDB_APP_PID=<candidate-process-pid> node bench/permissions-e2e.mjs
```


## Desktop MCP saved result pages (L2)

When the host omits result items or oversized cells, it returns top-level `truncated`, `result_id`, `next_offset`, and `total_items`. Row counts inside each result still describe the original DB result. The memory LRU allows at most 32 handles and a conservative 32 MiB budget including estimated JSON heap and reserved JSON encoding bytes; allocator overhead is separate. An entire result too large for this cache returns `result_id: null` and `result_handle_unavailable: "cache_size_limit"`. This is explicit data omission, not a durable export.

Call `get_result_page` with the same `session_id` and returned `result_id`, then advance by `next_offset`. Offsets count table rows, documents and graph items across the original response. Each page obeys the current client's item and byte budgets. A cell too large for a typed row page is omitted whole and its item index appears in `omitted_items`; typed cells are never partially replaced. To recover the exact original JSON, start with `byte_offset: 0`, concatenate each `data` fragment, and follow `next_byte_offset` until null. Fragments end on UTF-8 boundaries and do not execute SQL. Do not interpret an individual fragment as standalone JSON.

Handles are bound to client ID, token hash, connection profile and open database session. Each access rechecks current permissions and the live session. Handles expire after ten idle minutes; session close, client edits/revocation/rotation, bridge shutdown and app teardown clear them. Eviction is expected: clients must handle an expired/unavailable result without automatically replaying a query. Previously saved client tool allowlists stay unchanged, so enable `get_result_page` explicitly for an existing client that should page results. Cache entries and rows are never written to audit logs; the final audit entry records `truncated` (decision `returned`).

The built-in AI's existing 20-row/100-column, 128 KiB context limit and explicit row-sharing toggle remain stronger constraints. AI output continues to be a proposed Operation, never an execution request. See [L2_REPORT](L2_REPORT.md) for driver-only measurements and headless integration-test boundaries.
