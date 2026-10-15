# W7 fixes report

Branch/HEAD at final handoff: `main` / `3e09c8287416b2de232d306876c6344dca2f5791` (shared working tree; W7 changes uncommitted, concurrent owners advanced HEAD).

Scope: defects 1, 5, 6, 7, 8 in `docs/tasks/W7-fixes.md`, plus defect 9 received from coordinator (`msg_17f04dc6d507`). Defects 2/3/4 and the SQLite driver remain with their owners. No commits or app replacement were performed.

| Defect | Cause and implemented fix | Regression evidence |
| --- | --- | --- |
| 1 | Sorting Row.keys lost RETURN order; upstream Row stores fields in a HashMap and has no public stream-header accessor. Vendor pinned neo4rs 0.9.0-rc.10 and add only `DetachedRowStream::fields()`, then initialize columns from Bolt RUN SUCCESS before fetching rows. | Actual Neo4j integration asserts [z,a,m] and zero rows with headers [x,y]. |
| 5 | Agent registry read the initial empty UI snapshot while manifest IPC was still loading. `list_drivers` now awaits host `drivers_list`, whose registry is initialized before the app handles commands. | Deferred host-response test with an empty UI snapshot. |
| 6 | Closing relied on conditional unmount to discard state and left no explicit draft/DOM cleanup. Close, Cancel, Escape and successful Save now clear DOM inputs, secret entries and draft state; profile keys separate form identities and autocomplete is off. | Chromium browser regression covers all four paths, reopening with empty name/password. |
| 7 | Path formatter used internal node IDs. Reuse existing nodeCaption (name, title, first text property, first label, ID fallback). | Unit test asserts (Alice)-[:ACTED_IN]->(Matrix) and first-text/label fallbacks. |
| 8 | Logger used OpenOptions default creation mode. Open with Unix 0600 and apply descriptor permissions to repair existing 0644 files too. | Unix regression checks new-file mode and reopening an existing 0644 file. |
| 9 | tokio-postgres cancel_query flushes/shuts down its socket without waiting for server processing, so late cancellation can reach the next statement. Await cancellation socket EOF using the same TLS connector/mode, prefer the completed execution before sending another cancel, then fence under the session lock with bounded 57014 absorption. | 200 short-query/cancel races followed immediately by SELECT 42; existing timeout, cancel and read-only tests retained. |

## Verification

- `npm --prefix ui test`: PASS, 6 files / 30 tests.
- `npm --prefix ui run build`: PASS (includes `tsc --noEmit`).
- `ego-browser nodejs < bench/w7-form-regression.mjs`: PASS, four form termination/reopen checks against built UI preview on localhost:4178, mock API only.
- `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target-w7 cargo test --workspace` with both `DBTOOL_TEST_PG` and `DBTOOL_TEST_NEO4J` set to the isolated databases: PASS, 28 tests; actual Neo4j and PostgreSQL integration tests ran, including the new 200-iteration cancellation race and log-permission regression.
- `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target-w7 cargo clippy --workspace -- -D warnings`: PASS, exit 0.
- Validation logs: `/tmp/dbtool-w7-pass-tests.log` and `/tmp/dbtool-w7-pass-clippy.log` (ephemeral local evidence).
- Host load caused long initial delays; compile-time API/lifetime errors found during validation were corrected before the final passing commands. No UI/dist rebuild was run after the coordinator imposed the W4 measurement freeze.
- Vendor source comparison: only `src/stream.rs` differs from the published package (other additions are provenance documentation).
- `git diff --check`: PASS.
- UI driver-ID conditional grep: no matches.

Protocol basis for defect 9: [PostgreSQL cancellation flow](https://www.postgresql.org/docs/current/protocol-flow.html#PROTOCOL-FLOW-CANCELING-REQUESTS) specifies that the server processes CancelRequest before closing its separate connection. The EOF wait therefore synchronizes dispatch; the existing execution future and the fence consume the original connection's responses.

## Boundaries and assumptions

Only isolated test endpoints 127.0.0.1:55432 and bolt://127.0.0.1:57687 are used by integration tests. Vendor sources are copied from the installed crates.io package; `vendor/neo4rs/PATCHES.md` records provenance and the single upstream source change. The upstream published package declares MIT but does not include a standalone LICENSE file; an added LICENSE-MIT preserves those standard terms and Cargo author attribution, documented transparently in PATCHES.md. The ADR reuse table records this vendored patch.

Secret cleanup drops reachable UI state and empties DOM fields; JavaScript strings cannot promise physical memory zeroization. The real desktop app was not restarted or replaced, so coordinator MCP E2E 21/21 against the final integrated binary remains NOT RUN here. Browser checks are Chromium/mock-API evidence, not WKWebView/live-provider proof. Actual encrypted PostgreSQL cancellation has not been exercised against a TLS-enabled server; the isolated test database uses its existing TLS configuration. Defect 4 timing is owned by W4 and is not claimed here.
