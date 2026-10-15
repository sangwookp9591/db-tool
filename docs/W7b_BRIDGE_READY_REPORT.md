# W7b bridge readiness — 2026-10-15

## Change

The webview sends `agent_bridge_ready` only after manifests have loaded and both request/cancellation listeners have registered. The host publishes its private discovery file only after that signal, and retains up to 64 pending requests while waiting up to 10 seconds for readiness. Main-webview navigation clears readiness and removes discovery; the newly loaded webview repeats the handshake.

Host forwarding failure, a dropped/expired webview response, and bridge shutdown return `Unavailable`. A request cancelled before execution returns `Timeout`; actual approval rejection/expiry remains `UserDenied`.

Owned changes: `src-tauri/src/agent_bridge.rs`, the readiness hook/command registration in `src-tauri/src/lib.rs`, the readiness prop in `ui/src/App.tsx`, the typed command in `ui/src/api.ts`, `ui/src/agent/AgentPanel.tsx`, `ui/src/agent/registry.ts`, its regression tests, `bench/agent-bridge-startup.mjs`, and the report/evidence artifacts. Existing W4 changes were preserved. No commit was made, and shared `ui/dist` was not rebuilt.

## Verification

- `CARGO_TARGET_DIR=target-w7b cargo test -p db-tool`: **13 passed**, 0 failed (final rerun also 13 passed).
- `CARGO_TARGET_DIR=target-w7b cargo clippy -p db-tool --all-targets -- -D warnings`: **passed**.
- `pnpm --dir ui exec tsc --noEmit`: **passed**.
- `pnpm --dir ui test`: **32 passed**, 6 test files.
- Private UI copy build: **passed**; changed runtime UI files match the workspace files, and built assets contain `agent_bridge_ready`.
- `git diff --check` and `node --check bench/agent-bridge-startup.mjs`: **passed**.

Host regression checks cover queued requests before readiness, delivery after readiness, waiting again after reload, missing readiness, unavailable/dropped transport replies, preserved explicit denial, discovery publication/removal/0600 permissions, and shutdown releasing queued requests.

## Real desktop startup

Built a real macOS debug application with `CARGO_TARGET_DIR=target-w7b cargo build -p db-tool` and `TAURI_CONFIG` overriding only `build.frontendDist` to `/private/tmp/dbtool-w7b-ui-_31xwy9t/dist`. This leaves shared `ui/dist` untouched. The startup script creates five separate temporary configurations and fresh processes, polls discovery every 5ms, then immediately calls `tools/call list_drivers` with a 2-second deadline.

| Fresh app | Request to manifest response |
| --- | ---: |
| 1 | 117ms |
| 2 | 14ms |
| 3 | 36ms |
| 4 | 109ms |
| 5 | 52ms |

**5/5 passed**; every response contained the real Neo4j, PostgreSQL and SQLite manifest array. No database connection is required by this startup test.

## Coordinator E2E

**21/21 passed** with the unchanged `bench/e2e-bridge.mjs`, the same candidate binary/UI, and the isolated config `/private/tmp/dbtool-w7b-e2e-2mmawexi`. Per coordinator direction, profiles used `save_secret=false`; test credentials were supplied only through the app's native session-credential dialog via AX `set-value` with stdin. Synthetic keyboard typing did not populate the secure input correctly; the eventual passing run used AX values and the existing FormData submit path. No inline secret was persisted to profile fields, settings, or history.

The script's static test label `pg open (credential from keychain)` was not changed; it does **not** describe this run's credential source. **keychain 경로는 W7b 미변경, 이번 실행에서는 미검증(unsigned debug 바이너리의 키체인 partition 프롬프트 때문)**. The coordinator will recheck keychain access with the final build. Initial keychain and synthetic-input attempts were superseded by the passing session-only run.

A literal `rg -l -F -f -` scan (patterns supplied through stdin) checked both fixture secrets across 21 owned logs/config/history files and returned no matches. The test PostgreSQL password was independently checked against the container fixture without printing it, and a test-container TCP `SELECT 1` succeeded.

## Evidence and limits

Logs: `/private/tmp/dbtool-w7b-tests-final.log`, `/private/tmp/dbtool-w7b-clippy.log`, `/private/tmp/dbtool-w7b-ui-tests.log`, `/private/tmp/dbtool-w7b-ui-build.log`, `/private/tmp/dbtool-w7b-app-build.log`, `/private/tmp/dbtool-w7b-startup.log`. Passing E2E output is `/private/tmp/dbtool-w7b-e2e-ax.log`. Durable startup timings and all 21 E2E assertions are saved in `bench/results-w7b-bridge.json`.

Native webview reload was covered by host state-transition tests, not a separate GUI reload run. The five startup measurements prove this local macOS debug environment; no release-build, Windows or Linux runtime claim is made. Only test database profiles at PG 55432 / Neo4j 57687 (plus deliberately closed port 1) are in the E2E configuration.
