# Changelog

Detailed release notes are in [`docs/release-notes`](docs/release-notes). The app's **What's new** timeline reads those files, and its current release marker follows `ui/package.json`.

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
