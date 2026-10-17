# C2 Row Edit Report

## Backend

The fixed contract in `docs/tasks/C2-row-edit.md` is implemented additively: `EditTarget`, optional/defaulted `ResultSet.edit_target`, tagged `RowEdit`, defaulted `EditRequest`, `EditResult`, `ErrorKind::Conflict`, and default-Unsupported `Session::preview_edits` / `apply_edits`. Old result JSON without `edit_target` still deserializes; a round-trip/default test covers the additions. `docs/ARCHITECTURE.md` records the contract and host commands.

PostgreSQL and SQLite advertise `edit_rows: true`. Only catalog-backed tables with declared primary keys expose an edit target; views, arbitrary query results, and tables without primary keys do not. PostgreSQL INCLUDE columns are excluded from primary-key identity. Physical locators (`ctid`, `rowid`) are deliberately not edit identities: they are storage details, do not supply stable logical identity, and are unnecessary when declared primary keys are available. Existing browse ordering may still use its pre-existing physical fallback for non-editable tables. Neo4j, MongoDB, Redis, and the KV example remain unchanged with editing disabled.

A shared planner validates complete primary-key predicates, known/non-duplicate columns, non-key update sets, and the 1,000-edit limit before generating SQL. Every edit value uses bound parameters. PostgreSQL resolves real column types and binds text through casts to catalog-derived, quoted schema/type names. SQLite binds native null/integer/real/text/blob values and explicitly addresses `main` so a temporary object cannot shadow the validated table. Invalid scalar conversions return Query errors naming the zero-based edit index. Generated statements and history retain placeholders, never SQL-interpolated edit values.

Each apply uses one transaction; PostgreSQL uses a savepoint inside an existing user transaction. SQLite rejects edits/previews inside user transactions, preserving C1's protection against SQLite interruption/conflict handling rolling back outer work. Update/Delete must affect exactly one row; zero or multiple rows return Conflict naming the edit index and roll back the complete batch. Preview executes the same parameterized DML, collects bounded before/after samples, drains complete RETURNING results for accurate counts, and rolls back. Existing C1 cancellation/session-lease wrappers drive both runners; interruption and errors roll back, and failed cleanup discards the connection. Sequence advances and external trigger/function side effects remain outside rollback guarantees, as in C1.

`edit_preview` and `edit_apply` are registered Tauri commands with the existing run/cancel/timeout/observe lifecycle. Both reject read-only connections with Auth and unsupported capabilities with Unsupported. Apply requires a successful preview of the exact serialized request on the same session within 60 seconds; the approval is removed before applying and cannot be replayed. Edit fingerprints are domain-separated from SQL-preview approvals while reusing `host.previews`. Changed requests, expiry, replay, and missing previews are rejected. History kinds are `edit_preview` and `edit_apply`, with generated parameterized statements and credential redaction; pre-planning failures have a target/count description because no SQL exists yet. No agent edit tool was added.

### Backend verification

Only the isolated PostgreSQL instance at `127.0.0.1:55432/axdb_test`, Neo4j at `127.0.0.1:57687`, and SQLite temporary files are used. Rust commands use `CARGO_TARGET_DIR=target-c2 CARGO_BUILD_JOBS=6`, Rust 1.94.0.

The five new real-driver integration tests (2 PostgreSQL, 3 SQLite) passed. They cover Update/Insert/Delete with re-query, preview non-persistence and typed samples, exact storage of injection/quote/backslash/Unicode strings, complete rollback for zero-row and multi-row conflicts, unknown tables/columns and invalid conversions, oversized batches, PostgreSQL user savepoints, SQLite user-transaction refusal, cancel/timeout followed by successful session reuse, primary-key INCLUDE columns, quoted catalog identifiers, and main/temp table shadowing. Multi-row conflicts are exercised using PostgreSQL inherited rows and SQLite nullable composite primary keys rather than weakening the primary-key-only contract.

Final gates passed:

```sh
AXDB_TEST_PG='host=127.0.0.1 port=55432 user=postgres password=testpw dbname=axdb_test' \
AXDB_TEST_NEO4J='bolt://127.0.0.1:57687|neo4j|testpassword' \
CARGO_TARGET_DIR=target-c2 CARGO_BUILD_JOBS=6 cargo test --workspace
CARGO_TARGET_DIR=target-c2 CARGO_BUILD_JOBS=6 cargo clippy --workspace --all-targets -- -D warnings
```

The complete workspace test harness reported **94 passed / 0 failed / 2 ignored**, including **8 new C2 tests** (1 contract, 5 real-driver, 2 host). The two existing ignored tests are the manual OS-keychain round trip and the opt-in paid Anthropic request. The host tests additionally cover read-only and unsupported capability denial, missing/changed/expired/replayed preview approval, and redacted parameterized history. All-target workspace clippy completed without warnings, including MongoDB. `git diff --check` passed for backend-owned files.

Optional MongoDB (`AXDB_TEST_MONGO`, `AXDB_TEST_MONGO_TLS`), Redis (`AXDB_TEST_REDIS`), and TLS-specific database fixtures were unset; their integration functions returned early and therefore appear as passing harness entries rather than ignored tests. PostgreSQL, Neo4j, and SQLite real-database paths ran. An initial workspace build failed because a parallel UI rebuild removed a font while Tauri embedded `ui/dist`; after the coordinator stabilized that directory, the full workspace retry passed. Logs: `/tmp/c2-workspace-tests-final.log`, `/tmp/c2-clippy.log`. No commit was created; UI source ownership was preserved.
