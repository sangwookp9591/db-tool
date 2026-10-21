# Backend verification — 2026-10-21

## Results

- `cargo test --workspace`: passed, 63 test harnesses; 329 passed, 0 failed, 7 ignored. Full output is in [cargo-test-final.log](cargo-test-final.log).
- `cargo clippy --workspace --all-targets -- -D warnings`: passed. Output is in [cargo-clippy-final.log](cargo-clippy-final.log).
- The suite exercised isolated PostgreSQL, MySQL, MariaDB, MongoDB, Neo4j, Redis, and SQLite fixtures. PostgreSQL's large role matrix regression also passed independently: 800 tables produce 5,600 privilege cells for the selected role, returned as bounded pages of at most 100 cells with an exact total. See [roles-page-regression.log](roles-page-regression.log).
- The Tauri/plugin protocol round trip for `roles_page` passed. See [roles-page-plugin-contract.log](roles-page-plugin-contract.log).

The PostgreSQL role API preserves the legacy full matrix for small databases. Once its projected matrix exceeds 50,000 cells, it returns role metadata and `role_required` without building the cross product. `roles_page` requires a role selection and supports namespace/object substring filters, offset, total, and `has_more`; page size is capped at 100. Other drivers adapt their existing role snapshot through the default paging implementation.

## Isolated fixtures

Use `scripts/qa-backend-fixtures.sh start` to create only the `axdb-qa-20261021-*` containers after checking that all reserved ports are free. Run `scripts/qa-backend-fixtures.sh env` to print test-only connection variables. `stop` removes only these named containers. The fixtures were retained through native UI/MCP verification and then removed using the scoped stop command.

The database test runner must use the built driver executables, for example:

```sh
AXDB_DRIVER_DIR="$PWD/target-u4b/debug" CARGO_TARGET_DIR=target-u4b cargo test --workspace
```

## Coverage limits

Seven tests are explicitly ignored in the test suite (including provider calls, manual performance cases, and OS keychain coverage). Conditional live checks also require external setup and were not claimed as exercised: OpenAI/Anthropic credentials; TLS connection URIs and CA files for PostgreSQL, MongoDB, and Neo4j; `AXDB_TEST_NEO4J_RESTORE` for destructive Neo4j restore coverage. MySQL TLS was run without an optional CA fixture. These requirements are environment-specific; no cloud account or user profile was used.

Additional build, fixture, UI/native evidence is recorded alongside this report. The generated tracked benchmark result was restored after capture; the QA copy remains at [w4b-streaming-plugin.json](w4b-streaming-plugin.json).

The separate final network run populated `AXDB_TEST_NETWORK_FIXTURE` and passed 8 tests, including SSH/proxy draft credentials and tunneled PostgreSQL TLS. See `network-draft-secrets.log`; these overlap workspace cases and are not added to its total.
