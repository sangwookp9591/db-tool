# W6 AxDB rename verification

Base: `e3e1dc4e054b515826fe62d574f0d068a527d821`. No commits made.

Changed the host package/binary to `axdb`, Tauri product/window title to `AxDB`, identifier to `dev.axdb.app`, keychain service and log filename to `axdb`/`axdb.log`, environment variables to `AXDB_*`, and UI global API to `window.axdb`. Updated connected tests, benchmark fixtures/scripts and active documentation; no compatibility aliases remain. The Rust tracing filter also uses the renamed crate target, preserving the dependency-log exclusion. README documents that old keychain entries are not migrated.

## Checks

- `CARGO_TARGET_DIR=target-w6 cargo build --workspace`: PASS.
- `AXDB_TEST_PG='host=127.0.0.1 port=55432 user=postgres password=testpw dbname=axdb_test' AXDB_TEST_NEO4J='bolt://127.0.0.1:57687|neo4j|testpassword' CARGO_TARGET_DIR=target-w6 cargo test --workspace`: PASS, 33 tests, including live host commands, PostgreSQL and Neo4j contracts and SQLite integration.
- `CARGO_TARGET_DIR=target-w6 cargo clippy --workspace -- -D warnings`: PASS.
- `pnpm --dir ui build`: PASS, including TypeScript validation.
- `pnpm --dir ui test`: PASS, 34 tests across 6 files.
- All `bench/*.mjs` scripts passed `node --check`.
- Tauri identity assertions and `git diff --check`: PASS.
- Protected `vendor/`, `docs/tasks/`, historical report patterns and `bench/results-*.json`: no changed paths.
- Rust test output: `/tmp/axdb-w6-cargo-test.log`; clippy output: `/tmp/axdb-w6-clippy.log`.

## Remaining-name search

The task's recursive grep, excluding target directories, node_modules, .git and dist, found only these preserved categories (counts are matching lines): task specifications 38; existing historical reports 27; vendor 1; ignored `.remember` session history 59; untracked `ui/.playwright` browser snapshots 2. The source, active documentation and scripts in the editable tracked scope have zero matches, including a separate hidden-file-aware scan for the Rust crate spelling.

## Test environment and handoff

Created the empty `axdb_test` database in the already-running isolated PostgreSQL test container on port 55432; preserved the existing test database and container names. Neo4j used the existing isolated test container on port 57687. No user database was accessed.

NOT RUN: desktop app launch, observed window title, real OS keychain read/write and running-app MCP bridge smoke. The task explicitly assigns app/keychain execution to the coordinator because macOS may prompt. Built binary: `target-w6/debug/axdb`; configure it with `AXDB_CONFIG_DIR` and `AXDB_AGENT_BRIDGE=1`, prepare profiles/credentials in the new keychain service, then run `AXDB_CONFIG_DIR=<private-dir> node bench/e2e-bridge.mjs`. Existing credentials are not migrated; old environment-variable and JavaScript-global aliases are intentionally absent.
