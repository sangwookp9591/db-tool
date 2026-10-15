# W1 backend implementation and verification

## Delivered

- Cargo workspace: `dbcore`, `driver-postgres`, and Tauri 2 host `db-tool`.
- Shared serde contract, tagged values, exact integer/decimal/JSON text, registry, redacted connection Debug, row-limit clamp and defaults.
- PostgreSQL native TLS connector with disable/prefer/require; lazy schema/table/view/column/index discovery; quoted identifiers; primary-key/ctid browsing and limit+1 paging.
- PostgreSQL SQL splitter supports doubled quotes, E strings, dollar quotes, line comments, nested block comments, and rejects unterminated input before executing any statement.
- Row-returning SELECT/WITH/VALUES/TABLE uses text protocol and server FETCH limit+1; explicit BEGIN/COMMIT/ROLLBACK state is retained across commands. Cursor errors roll back only host-created transactions.
- Server cancellation, cancellation during protocol boundaries, timeout, connection/auth/query classification, PG message/detail/position, and exact OID-based value mapping.
- All architecture commands implemented. Profile secrets are excluded from JSON and assigned to OS keyring service `db-tool`, account `profile:{id}`. Each session and run has its own identity, duplicate active run IDs are rejected, and connection errors mark sessions dead.
- Atomic owner-only profile/history file writes, 1,000-line history, secret redaction, tracing to app log file, and `DBTOOL_CONFIG_DIR` override for configuration/data/logs.
- `docs/contract-examples.json` was generated using actual Rust serializers and a live isolated PostgreSQL query/error via `cargo run -p driver-postgres --example contract`.

## Decisions and wire details

- Enum variants retain Rust PascalCase; fields use snake_case. `FieldKind::Select` and `NodeActionKind::OpenQuery` use serde external tags, while cell Value uses `{t,v}` and Null has no `v`.
- `FloatValue` represents finite JSON numbers or special strings (`NaN`, `Infinity`, `-Infinity`).
- `ExecRequest.params` uses `serde_json::Map<String, serde_json::Value>` for JSON object input: serde_json does not implement serialization for `Map<String, dbcore::Value>`. PostgreSQL rejects nonempty params because its capability is false.
- `Profile`: id, name, driver_id, fields, save_secret. `profile_save(profile, secret?)` returns the sanitized profile; blank id gets a UUID. `session_open(profile_id, secret?)` accepts a new secret when save_secret is false. Tauri argument names follow the normal camelCase convention.
- `FieldSpec.placeholder` is a string and history timestamp is RFC3339 to align with the UI worker's contract.
- Logs contain driver id and non-secret field names only, avoiding accidental connection-URI or credential logging. The file subscriber accepts only app targets, excluding dependency SQL/server NOTICE output that may contain secrets.
- Native TLS uses the platform trust store and validates certificates; no insecure certificate bypass was added.

## Verification

- `cargo build --workspace`: passed (debug binary and all workspace crates).
- `DBTOOL_TEST_PG=... cargo test --workspace`: passed, 10 tests, 0 failed. This includes real driver and host command paths against the isolated server, not a mocked database.
- Real driver checks: exact int8/numeric/NULL/timestamptz/jsonb/bytea/UUID/array/NaN values; 1,000,000-row source with a two-row cursor cap; trailing line comments; SQL error details/position; 500ms timeout and 200ms cancellation of pg_sleep(10); eight cancellations at 1ms protocol boundaries; SELECT 1 after cancellation; concurrent browse during cancellation; wrong password; closed loopback port; server-side backend termination; explorer, PK/index information and paging; DML RETURNING truncation and affected count; explicit transaction rollback.
- Host checks: actual connection test/open/execute/timeout/cancel/reuse/explorer/close commands; host deadline waits for driver cancellation; run-map cleanup; secret/parameter omission from history; 1,000-line history cap; profile sanitization and owner-only files; dependency server NOTICE messages excluded from the file log.
- `cargo clippy --workspace -- -D warnings`: passed on the final source.
- `cargo fmt --all -- --check`: passed.
- `cargo build -p db-tool --release`: passed on the final source, `Finished release profile [optimized] in 3m 56s`; output `target/release/db-tool` (macOS arm64).

## Limits and unverified evidence

- PostgreSQL does not allow DECLARE cursors for INSERT/UPDATE/DELETE RETURNING, SHOW, or EXPLAIN. Those statements use bounded text-stream retention with draining: result memory is bounded, but server execution/output is not bounded by FETCH. Data-modifying CTEs under WITH can be rejected by PostgreSQL's cursor rules.
- Session::browse has no cancellation-token argument in the contract. Cancelling a browse closes its transport and marks the session dead; reconnect is required. Query cancellation preserves session usability outside a user transaction; a user transaction aborted by PostgreSQL requires the user's ROLLBACK.
- Views without a primary key use ORDER BY 1 because ordinary views have no ctid. Paging follows normal OFFSET snapshot semantics and is not stable against concurrent writes.
- Native OS keychain interaction, TLS against a TLS-enabled remote server, desktop GUI/IPC interaction, app bundling/signing, and other OS targets have not been verified by the automated checks here.
- History records query text as required and redacts the current connection secret. Arbitrary secrets deliberately written into SQL that differ from the connection secret cannot be inferred and scrubbed automatically.
- Only the isolated PostgreSQL endpoint 127.0.0.1:55432 was used for data access; the closed-port negative test reserves then releases a fresh loopback port. No connections were made to ports 5432, 5439, or 32802.
- No commit/push or AI feature was added. UI source is owned by the other worker; the only UI write was the explicitly allowed absent-only temporary `ui/dist/index.html`, subsequently replaced by the UI build.
