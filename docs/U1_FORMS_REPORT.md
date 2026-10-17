# U1 Forms/UI Fixes Report

## Changes

- Connection dialogs now hold a consistent `min(716px, 85vh - 4px)` shell height, leaving the existing dialog body scroll container, scroll hints, and footer intact. Required driver fields are grouped in the first Connection section before any inferred optional Authentication/Advanced sections; this places SQLite's required `Database file` there without driver-id branching. The SQLite read-only checkbox remains in Connection alongside the app access-mode control.
- Connection-form and parameter disclosure controls already use lucide chevrons; no text `▼` or `▶` markers were present in those current components.
- The command palette disables autocorrect, autocapitalization, and spellcheck; displays kind icons for connections, table actions, and history queries; and deduplicates identical recent query text while preserving the newest entry.
- Agent client names and audit client/connection names have native `title` tooltips. Client list names truncate safely, audit tables use fixed layout, and command-palette rows constrain long labels.
- Neo4j `server_info` now extracts the version from nested list values and includes edition when present. Output is `Neo4j <version>` with an optional parenthesized edition. Unit coverage validates nested extraction and the real integration test rejects the former debug format.

## Verification

- `pnpm --dir ui exec tsc --noEmit` — passed after the concurrent views edit was resolved.
- `pnpm --dir ui exec vitest run src/ui/micro.test.ts` — passed (9 tests), including duplicate-recent-query coverage.
- `pnpm --dir ui test` — passed (15 files, 73 tests).
- `pnpm --dir ui exec vite build --outDir /private/tmp/claude-501/-Users-psw-Projects-db-tool/4da095e2-8b36-4d6c-9840-879187baad6e/scratchpad/u1-forms/dist` — passed.
- `AXDB_TEST_NEO4J='bolt://127.0.0.1:57687|neo4j|testpassword' CARGO_TARGET_DIR=target-u1c cargo test -p driver-neo4j` — passed (4 unit, 3 Neo4j integration, 1 TLS test).
- `CARGO_TARGET_DIR=target-u1c cargo clippy -p driver-neo4j -- -D warnings` — passed.

## Visual verification status

- After screenshots: 18 connection dialog screenshots (PostgreSQL, Neo4j, MongoDB × 3 viewport sizes × 2 themes), plus 6 each for the command palette, agent clients, and AI panel, and two additional scrolled client-list screenshots. An additional PostgreSQL advanced-open scroll screenshot is included in `/private/tmp/claude-501/-Users-psw-Projects-db-tool/4da095e2-8b36-4d6c-9840-879187baad6e/scratchpad/u1-forms/`.
- Before screenshots are the native audit artifacts in `docs/design-shots/`: connection dialogs (PostgreSQL, Neo4j, MongoDB, Redis, SQLite), palette (`ui-audit-20`), clients (`ui-audit-22`), and AI (`ui-audit-21`). The audit baseline only captured 1280×800, predominantly one theme, so it does not provide before images at every requested size/theme.
- The mock provides only PostgreSQL, Neo4j, and MongoDB manifests. SQLite and Redis were not available for after screenshots; the SQLite required-field placement is handled generically using `required` metadata.
- The overlap probe was run on connection form, palette, clients, and AI states across all 3 requested sizes and both themes. Summary output is in `scratchpad/u1-forms/overlap-findings.json`; connection dialog checks found no major finding targeting the dialog shell. Remaining probe observations include known modal-layer occlusion/viewport heuristics and a focus-ring-clipped report on the native dialog close button (the dialog is top-layer rendered above the status bar).
- Visual spot checks showed the PostgreSQL password and keychain option visible at 800×600; opening Advanced exposes TLS, TLS CA, and access-mode fields within the internal scroll area. Palette kind icons are visible, and the AI/clients dialogs fit their available viewport with their body areas scrolling.

## Scoped control styling review

- Connections: action buttons use explicit `morph={false}` or icon/button primitives; the relevant selectors have token-scoped sizing overrides already present.
- Dialog: the header close control has a token-scoped override and keeps its existing 28px hit target.
- Command palette and agent clients: no new text glyph icons or bare-button style regressions were introduced.
