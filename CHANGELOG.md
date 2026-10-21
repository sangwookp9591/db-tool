# Changelog

Detailed release notes are in [`docs/release-notes`](docs/release-notes). The app's **What's new** timeline reads those files, and its current release marker follows `ui/package.json`.

## [0.3.4](docs/release-notes/v0.3.4.md) — 2026-10-21

### Added

- Execution history search and a SQL notepad (saved snippets) with title/tags, pin, filters, Korean and identifier infix search, keyset paging, and a virtualised, lazily loaded panel (`Cmd/Ctrl+Shift+H`). A "save as memo" action sends the editor selection or whole query to the notepad.
- Code policy: workspace Rust lints, `clippy.toml`, `rustfmt.toml`, `deny.toml`, `scripts/check-policy.sh`, and a clippy baseline. See [`docs/CODE_POLICY.md`](docs/CODE_POLICY.md).

### Improved

- History moved from a rewrite-the-whole-file JSONL log (capped at 1,000 rows) to an embedded SQLite store with FTS5: constant-cost appends, bounded searches, and retention of 50,000 rows / 180 days. `history.jsonl` is migrated once (corrupt lines skipped) and kept as `history.jsonl.migrated`. Secret redaction is unchanged.
- AI chat errors show an error icon, Korean guidance, collapsible raw text, and a retry action; bubbles, context chips, and the composer are aligned.
- AX workbench forms use one shared `Select` and `FilePicker` (Korean labels, drag and drop). Client map layout no longer overlaps.
- Design review with the app-screen rules (inspector on the connected working screen, 1440px: 5 FAIL → 0): `BorderGlow` and `SpotlightCard` no longer draw a pointer-following gradient or run `requestAnimationFrame` on every `pointermove` (this was the real cause of the editor hover animation; it also removed the conic-gradient border from every dialog); brand buttons are a flat accent fill instead of the brand gradient; all 9–10px text (96 declarations, 319 elements) is now 11px; connection tiles lose their tinted square; the "ask the panel" action moved into the first toolbar row (the editor starts 40px higher and the results area is 41px taller at 1440×900); result toolbar actions are grouped on the right; empty workspace and query screens have an h1.
- Not changed on purpose: the brand violet accent (`#5B45E0`) and the loading/skeleton shimmers; 98% of the text is under 13px, which is kept for a dense database tool.
- Tokio features were reduced from `full` to the set actually used.

### Fixed

- Reading an execution plan no longer leaves the connection dead or looping. A failed `CancelRequest` used to abort a healthy PostgreSQL connection; a cancellation grace timeout killed the shared driver process and every sibling session. Both are isolated now.
- Automatic lint no longer runs an uncancellable `EXPLAIN` on every debounced edit; superseded plan requests are ignored. As a consequence the `large_table_scan` lint rule (which depended on that `EXPLAIN`) is no longer produced while typing; the same warning still appears in the plan view.
- Editor lint underlines are kept in editor state and follow the text through edits; a fixed decoration set used to throw in CodeMirror when the document became shorter than a stale range (regression test: `ui/src/Editor.browser.test.ts`).
- Editor analysis messages (lint diagnostics, quick-fix titles, refactor errors) are now Korean.
- "Schema context timed out": PostgreSQL and MySQL/MariaDB loaded columns once per table (N+1). Columns are now read per schema, the AI schema context is read once, cached for 60 seconds, and returns a partial result instead of an error on timeout.

### Verification

- UI: `tsc` clean; 235 passed, 1 skipped (56 files). Rust workspace unit tests: 211 passed, 0 failed.
- PostgreSQL integration with a local server: 242 passed, 0 failed, 9 ignored, including 200 cancellation races and both failed-cancel explain modes ([evidence](docs/verification/release-0.3.4/verification.json)).
- History store benchmark (Apple M4, 103,000 rows, synthetic ~120-byte SQL, warm cache, LTO off): insert p95 0.76 ms including dispatch, 100-row page p95 0.26 ms, search p95 0.75–4.1 ms, peak RSS growth 8.4 MB, 50,000-line migration 1.0 s, unchanged idle CPU. Worst single insert 135 ms. Results: [`bench/library-bench-results.json`](bench/library-bench-results.json).
- Not verified: the exact production trigger of the reported disconnect, real TLS, the MariaDB metadata query against a live MariaDB server (a failure falls back to the previous per-table path), and the visual result of UI changes in the running app. Pinned snippets are not yet sorted first and history has no date filter.
- Notarization for this release was not run when this entry was written.

## [0.3.3](docs/release-notes/v0.3.3.md) — 2026-10-21

### Added

- Per-client MCP connection guides for Codex, Claude Code, and other MCP apps, with commands and configuration copy actions.
- Code-based UX/function scenarios, browser regressions, isolated DB fixtures, and a native macOS verification harness.

### Improved

- User/role administration with role selection, schema/object filters, bounded permission pages, and reviewed changes that remain available across pages.
- Connection forms with validation that opens the relevant step, local test errors, and draft SSH/proxy credentials that are saved only after profile save succeeds.
- Compact MCP tool permissions with Korean labels and aligned checkbox indicators.
- The in-app changelog uses the package version instead of a fixed release number.

### Fixed

- Missing tables/views and SQL table-name completion after connecting to databases with nested or non-public schemas.
- Stale explorer results and network status updates from unrelated connections.
- Context menus clipped by scrolling containers or window edges, keyboard dismissal/focus issues, and incorrect selection in long dropdown lists.
- Connection progress covering workspace settings.
- PostgreSQL privilege queries exceeding 50,000 cells. The paged role API requires an updated PostgreSQL driver.

### Verification

- UI: 207 passed, 1 skipped; Rust workspace: 329 passed, 7 ignored.
- Additional WebKit regressions, live SSH/proxy/TLS tests, native PostgreSQL editing, and MCP HTTP checks are documented in the [QA report](docs/QA_REPORT.md).
- The macOS universal app passed Apple notarization, ticket validation, and Gatekeeper assessment. See the [notarization record](docs/verification/release-0.3.3/result.json).

## [0.3.1](docs/release-notes/v0.3.1.md) — 2026-10-20

- Added the docked AI chat panel, query context, file-change review, model/effort selection, and approval cards.
- Added editable single-table query results with preview, approval, and conflict checks.
- Improved connection progress, workspace search, and AI panel layout.

## [0.3.0](docs/release-notes/v0.3.0.md) — 2026-10-19

- Added MySQL/MariaDB drivers, SSH/proxy/IAM connection options, and staged connection diagnostics.
- Added query plans, risk warnings, refactoring, import, schema comparison, migrations, backup, and permission management.
- Expanded AI accounts, execution plans, and per-client MCP permissions.

## [0.2.0](docs/release-notes/v0.2.0.md) — 2026-10-19

- Introduced installable signed drivers, automatic updates, onboarding, and result export.
