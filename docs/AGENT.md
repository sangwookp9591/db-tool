# Agent interface

The app exposes automation tools and does not call an LLM. Tool definitions live only in `ui/src/agent/registry.ts`; WebMCP, JavaScript automation, and the optional desktop MCP bridge use this registry. UI decisions use manifest capabilities, never database IDs.

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
| `get_history` | optional `limit` (1–1000, default 100) | Recent entries, with `source: ui/agent`; excludes parameter values |
| `get_perf_metrics` | `{}` | Bounded viewer timing metrics in milliseconds (W4 integration) |
| `get_ui_state` | `{}` | Open connections, active tab connection and current editor text, last result summary |
| `open_query_tab` | `session_id`, optional `text` | Opens and focuses a visible editor tab without executing |
| `browse` | `session_id`, `node_id`, optional `offset` (default 0), `limit` (1–1000, default 100) | Registered only while an open session supports `paged_browse` |
| `expand_node` | `session_id`, `element_id`, optional `limit` (1–200, default 200) | Registered only while an open session supports `graph_expand` |

```js
const tools = window.dbtool.agent.listTools();
const connections = await window.dbtool.agent.callTool('list_connections', {});
const { session_id } = await window.dbtool.agent.callTool('open_connection', {
  profile_id: connections[0].id
});
await window.dbtool.agent.callTool('open_query_tab', { session_id, text: 'SELECT 1' });
const result = await window.dbtool.agent.callTool('run_query', {
  session_id, text: 'SELECT 1', read_only: true
});
```

`run_query`, `browse`, and `expand_node` return `{results: [...]}`. Errors return `{error: {kind, message}}`. Row counts describe the underlying result; `truncated` signals omitted content. Returned table rows, graph nodes and documents are each capped at 200 across the response; graph relations are filtered to retained endpoints and capped at 200 per result. Typed `Value` tags and integer/decimal/JSON strings are preserved. This is a count limit, not a byte limit on individual values.

## WebMCP

When available, tools register on `document.modelContext`, falling back to `navigator.modelContext`. Re-registration aborts the prior registration's AbortController when manifests or open sessions change. The `window.dbtool.agent` API remains available in browsers without WebMCP, including the browser mock backend.

The [15 October 2026 WebMCP draft](https://webmachinelearning.github.io/webmcp/) is a Community Group draft, not a stable standard. The app implements its imperative tool registration subset, not declarative form annotations or browser agent discovery. Its JavaScript API adds minimal schema validation and JSON serialization checks; execution returns JSON objects. No claim is made that WKWebView itself implements WebMCP or that external agents can directly access its modelContext.

## Desktop localhost MCP bridge

The bridge is **off by default**. Enable the `MCP bridge` checkbox in the header, or launch:

```sh
DBTOOL_AGENT_BRIDGE=1 DBTOOL_CONFIG_DIR=/absolute/private/config target/debug/db-tool
```

The toggle persists `agent_bridge` in `settings.json`, preserving other settings. The environment variable enables the bridge at app startup even if the persisted toggle was off. It listens on an OS-selected `127.0.0.1` port and writes `{url, token}` to `agent-bridge.json` with mode 0600 on Unix. Normal app exit or disabling the bridge removes the discovery file; after a crash it can remain stale, and the next start replaces it with a fresh token and port. Do not share this file or put it in version control.

Configure an HTTP MCP client using the current values from that file:

```json
{
  "mcpServers": {
    "db-tool": {
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
DBTOOL_CONFIG_DIR=/absolute/private/config node bench/agent-bridge-smoke.mjs
```

The smoke script checks authorization, Origin rejection, initialization, discovery, connection opening, exploration, exact typed SELECT, DB-level write rejection, approval timeout, lack of persisted writes and agent history. During its final write request, leave the approval dialog untouched so it expires after approximately 30 seconds.

## Safety model and limits

- Every bridge request requires the random bearer token. Any Origin header is rejected (403), including same-origin-looking values, and no CORS permission is advertised. The server binds only loopback, limits JSON bodies to 1 MiB and pending requests to 64, and never logs tokens or parameter values.
- `run_query` defaults to read-only. Drivers declaring `read_only_exec` must enforce that mode themselves. PostgreSQL uses `BEGIN READ ONLY`, acquires an initial snapshot to prevent switching to READ WRITE, then always rolls back, including errors/cancellation. Transaction/session control is refused, and execution inside a user-owned transaction is refused with `Unsupported`. PostgreSQL read-only semantics still permit some temporary-object changes and nonpersistent side effects; use a restricted DB account when stronger isolation is needed.
- Explicit writes and drivers without enforced read-only support require the in-app dialog. It displays the query text, connection name, agent origin, approve and deny controls. Escape, denial, overlapping approval requests, and deadline expiration fail closed with `UserDenied`. Approval is per call, never cached. For unsupported enforcement, approval permits execution without the read-only flag.
- A bridge request expires at query timeout plus 30 seconds, or 60 seconds by default. Expiration aborts pending approval and cancels an executing query. A late approval cannot start an expired query. Credential prompts for opening a connection remain user-managed; an expired open request may still open a session if the user later submits its credential.
- Data-bearing tools carry `untrustedContentHint`; database content, query text, history and profile names must never be treated as agent instructions. Hints are metadata, not a sandbox.
- Profile secrets stay in the keychain and are excluded from connection listings. History keeps query text and redacts the current connection credential; arbitrary secrets deliberately embedded in SQL cannot be inferred automatically. Query results can contain sensitive data by design, so grant the bridge token only to a trusted local client.
- The header shows agent call activity and pulses during execution, with reduced-motion support. Main connection, tree, execution, result-view, paging and approval controls expose accessible names and test IDs.
