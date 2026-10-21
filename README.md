<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/banner-dark.png">
    <img src="docs/assets/banner-light.png" alt="AxDB — AX DB Tool" width="900">
  </picture>
</p>

<p align="center">
  <b>A light, fast desktop DB tool for working with many databases in one window</b><br>
  Relational, graph, document, and key-value data each appear in their own shape. You install only the drivers you need, and AI agents work alongside you within the permissions you set.
</p>

<p align="center">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-1.99.1-B7410E?logo=rust&logoColor=white">
  <img alt="Preact" src="https://img.shields.io/badge/Preact-11-673AB8?logo=preact&logoColor=white">
  <img alt="MCP" src="https://img.shields.io/badge/MCP-scoped%20tokens-8B7BFF">
  <img alt="Tests" src="https://img.shields.io/badge/tests-329%20Rust%20%C2%B7%20207%20UI-34D399">
  <img alt="License" src="https://img.shields.io/badge/license-MIT-lightgrey">
</p>

<p align="center">
  <img src="docs/assets/screens/pg-table-midnight.png" alt="PostgreSQL result table (Midnight theme)" width="880">
</p>

---

## Why AxDB

| | |
|---|---|
| **A view that fits each DB** | AxDB does not force every database into SQL and a grid. Relational databases show schemas, tables, and ERDs. Graphs show nodes, relationships, and paths. Documents show collections and a JSON tree. Key-value stores show a key hierarchy and per-type tables. |
| **Install only the drivers you need** | The app starts with no drivers. Install only the ones you use from PostgreSQL, MySQL/MariaDB, Neo4j, SQLite, MongoDB, and Redis. Each driver runs as a separate process, and its signature and SHA-256 are verified at install time. |
| **Safe writes** | Nothing is written automatically. You first check a change with a **preview** that runs it and always rolls back. Editing a cell or adding or deleting a row in a grid also goes through **edit → preview → apply**, and the apply is one transaction. Per-connection **read-only** is enforced by the host. |
| **Works with agents** | Each agent that connects over MCP gets per-connection permissions (none / read / preview / write-with-approval), a hashed token, and an audit log. |
| **AI that knows your schema** | It reads your schema and working context and suggests queries. It proposes SQL, file changes, and tool calls, and you review them. Execution goes through the connection and agent permissions and the approval path. You can use it with a Claude login inside the app. |

## Install

Download the file for your operating system from the [**latest GitHub Releases version**](https://github.com/sangwookp9591/db-tool/releases/latest).

Read the [0.3.3 release notes](docs/release-notes/v0.3.3.md) or browse the full [changelog](CHANGELOG.md).

| OS | File | Install |
|---|---|---|
| macOS (Apple Silicon · Intel) | `AxDB_<version>_universal.dmg` | Open the dmg and drag AxDB into the Applications folder. |
| Windows (x64) | `AxDB_<version>_x64-setup.exe` | Run the installer. If the WebView2 runtime is missing, it is downloaded during setup. |

The Windows installer is not code-signed. If SmartScreen shows a warning on first launch, click **More info → Run anyway**.

Version 0.1.x had no update feature, so 0.2.0 has to be installed once from the installer. Later versions update inside the app ([Auto-update](#auto-update)).

## First launch

The first launch shows a 7-scene walkthrough. You can open it again from the menu.

1. Welcome
2. **DB drivers**: pick several drivers and install them right away.
3. Connect and browse
4. ERD and viewers
5. Permissions and safety
6. MCP connection
7. File export and AI

<table>
  <tr>
    <td width="50%"><img src="docs/design-shots/onboarding-2-midnight.png" alt="Onboarding: choosing DB drivers"><br><sub><b>Onboarding scene 2</b> · Pick the drivers you need and install them</sub></td>
    <td width="50%"><img src="docs/design-shots/onboarding-6-midnight.png" alt="Onboarding: MCP connection"><br><sub><b>Onboarding scene 6</b> · Copy the MCP client config and issue a scoped token</sub></td>
  </tr>
</table>

## Installing drivers

Install drivers from **Drivers** in the side rail. If a driver is missing when you add a connection, you are taken to the same install screen. An existing connection whose driver is not installed shows a "Driver required · Install" button.

- The screen has two tabs, **Installed** and **Available**. Each card shows the version, size, and description, with install, update, and remove buttons. You can undo a removal within 5 seconds.
- **Install steps** are download → verify → check signature → install → check connection. The screen shows progress, transfer speed, and time remaining. You can cancel midway or retry after a failure. If you pick several drivers, they install in order.
- **Signature verification:** AxDB checks the Ed25519 signature and the executable's SHA-256 with the official public key built into the app. Extraction finishes in a temporary folder and is then renamed into place, so an install interrupted midway leaves the existing version intact.
- **Install from file:** you can install a downloaded `.axdriver` file directly. A package that is unsigned or signed with an unknown key installs only after you read the "Unverified driver" warning and agree.
- The app makes no network calls at startup. It reads installed drivers only from the app data folder, and it refreshes only one signed list file (`index.json`) when you open the Drivers screen.
- If a driver crashes, the app keeps running and the driver restarts on the next request.

<table>
  <tr>
    <td width="50%"><img src="docs/design-shots/drivers-glass-midnight.png" alt="Drivers screen"><br><sub><b>Drivers</b> · Available drivers shown as cards</sub></td>
    <td width="50%"><img src="docs/design-shots/drivers-tabs-daylight.png" alt="Drivers screen (Daylight)"><br><sub><b>Installed · Available tabs</b> · Daylight theme</sub></td>
  </tr>
</table>

> The Drivers and update screenshots were taken with mock data for development.

To build a driver for a new DB, see the [plugin guide](docs/PLUGIN_GUIDE.md).

## Connect and browse

- The connection form is drawn from the fields the driver declares. Passwords are stored only in the OS keychain, never in the profile file (permissions 0600) or in logs.
- The sidebar has two areas, **Connections** and **Explorer**. Each can be collapsed or expanded, and you can adjust the height between them. A compact view is also available.
- The explorer loads one level at a time. The driver decides what a node does (open a table, open a query, open a diagram).
- There is a ⌘K command palette and a run history, and every run supports cancel and timeout.

## Viewers

Only the views present in a result are shown, as tabs.

| Viewer | Contents |
|---|---|
| **Table** | A grid with virtualized rows and columns. Sort and filter, type headers, and exact display of int8, numeric, jsonb, and timestamptz |
| **Graph** | A Canvas graph. Expand neighbors, search, a legend by label, and selection synced with the table |
| **ERD** | Draws PG foreign keys, SQLite, the Neo4j schema, and Mongo sample-based reference inference in the same diagram |
| **Document** | A virtualized JSON tree. BSON-specific types such as ObjectId, Decimal128, and Date are shown as they are |
| **Key-value** | The Redis key hierarchy (SCAN-based) and per-type tables, TYPE and TTL |

You can also follow a foreign key from a cell to the row it references, and there is navigation history.

<table>
  <tr>
    <td width="50%"><img src="docs/assets/screens/neo4j-graph-midnight.png" alt="Neo4j graph"><br><sub><b>Graph</b> · Switch a Cypher result between graph and table, and expand neighbors</sub></td>
    <td width="50%"><img src="docs/assets/screens/mongo-documents-midnight.png" alt="MongoDB document tree"><br><sub><b>Document</b> · A virtualized JSON tree that shows BSON-specific types as they are</sub></td>
  </tr>
  <tr>
    <td><img src="docs/assets/screens/erd-daylight.png" alt="ERD"><br><sub><b>ERD</b> · Relational, graph, and document data in the same diagram</sub></td>
    <td><img src="docs/assets/screens/connection-midnight.png" alt="Add connection"><br><sub><b>Add connection</b> · The form is drawn from the fields the driver declares</sub></td>
  </tr>
</table>

<details>
<summary>Daylight (light) theme</summary>
<img src="docs/assets/screens/pg-table-daylight.png" alt="Daylight theme">
</details>

## Supported databases

| DB | Paradigm | Browse | Results | Preview | Table editing | Read-only | ERD |
|---|---|---|---|---|---|---|---|
| **PostgreSQL** | Relational | Schemas · tables · columns · indexes | Virtualized table, exact int8 · numeric · jsonb · timestamptz | ✅ Rollback | ✅ Primary key | ✅ DB level | ✅ FK |
| **MySQL · MariaDB** | Relational | DBs · tables · columns · indexes | Table | ✅ InnoDB rollback | ✅ Primary key | ✅ | ✅ FK |
| **Neo4j** | Graph | Labels · relationships · properties | Graph ↔ table, paths, mixed results | ✅ Rollback | — | ✅ READ mode | ✅ |
| **MongoDB** | Document | DBs · collections | JSON tree, ObjectId · Decimal128 · Date | Field-update dry-run | ✅ find, one document | ✅ | ✅ Reference inference |
| **Redis** | Key-value | SCAN-based key hierarchy | Per-type tables, TYPE · TTL | — | — | ✅ Allow list | — |
| **SQLite** | Relational | Tables · columns | Table | ✅ Rollback | ✅ Primary key | ✅ query_only | ✅ |

The shared UI draws screens only from the capabilities a driver declares. That is why adding MongoDB and Redis did not change a single line of `ui/` or `crates/dbcore`.

## Change preview

- **Query preview:** runs the statement and always rolls back. It shows affected row counts, before and after samples, and a schema diff. A statement that cannot be rolled back (such as a transaction-control statement) is not run, and AxDB tells you why. PostgreSQL, Neo4j, and SQLite support it.
- **Table editing:** collect cell edits, row additions, and row deletions in the browse table, then apply them in one transaction after a preview (PostgreSQL, MySQL/MariaDB, and SQLite; primary key required). If an update or delete does not affect exactly 1 row, the whole change rolls back. You cannot apply without a preview, and read-only connections and tables without a primary key cannot be edited.
- **Editing query results:** if the original column metadata of a single SELECT confirms a single table and the full primary key, you can turn on **Edit** in the result toolbar. Computed columns and primary keys are locked, only cell edits are supported in results, and a result above the edit grid's 1,000-row limit is read-only. AxDB compares the original values together with the primary key to detect conflicts, and after you approve and apply, it re-runs the same SELECT. JOINs, views, aggregates, DISTINCT, GROUP BY, truncated results, multiple statements, read-only connections, and agent results are read-only, and the disabled button shows the reason. Functions and subqueries are excluded conservatively until they can be classified safely.
- **MongoDB find editing:** in a find that returns only original fields, you edit the fields of one document at a time, for documents that have an `_id`. The edit goes through a no-write updateOne preview and your approval, and the apply runs atomically only if the original values still match. Aggregations and views are read-only. For the verification scope and limits, see the [D10 report](docs/D10_REPORT.md).

## Permissions and read-only

- **Per-connection read-only:** the host enforces it for UI execution too. If a driver cannot guarantee DB-level read-only, AxDB falls back to the approval flow.
- **Agent permissions:** for each client and each connection, you set none / read / preview / write-with-approval. Even write-with-approval needs your approval inside the app every time.
- Queries an agent runs are DB-level read-only by default.

## MCP connection

Turn on the **MCP bridge** in the status bar (off by default) and an MCP server opens on localhost. Copy the client config JSON and paste it into an HTTP MCP client such as Claude.

- Tokens are scoped tokens issued per client and stored as hashes. You can rotate or revoke them, and calls are recorded in the audit log.
- The tool list and input formats are in [AGENT.md](docs/AGENT.md). `list_drivers` returns only installed drivers.
- The copied JSON contains a token. If you restart the bridge, copy the URL and token again.

<table>
  <tr>
    <td width="50%"><img src="docs/assets/screens/agent-tools-daylight.png" alt="Agent clients"><br><sub><b>Agents</b> · The beam pulses only during a real query or MCP call, and tool calls are recorded</sub></td>
    <td width="50%"><img src="docs/design-shots/sidebar-1280-compact.png" alt="Sidebar compact view"><br><sub><b>Sidebar</b> · Collapse and expand Connections and Explorer, or switch to the compact view (mock data)</sub></td>
  </tr>
</table>

## AI assistant

Off by default. It turns on when you choose an authentication method in settings.

- **Claude login (ant CLI):** click the "Claude login" button in the panel and the login finishes inside the app.
- **API key:** stored in the OS keychain.

In the right-side docked panel (⌘J), you chat with the context of the active tab: its schema, query, selection, and latest error. In the editor, ⌘L or **Ask panel** attaches your selection to the question. You can resize or detach the panel, and conversations are kept in per-connection-session memory. Clear them with **Clear AI chat history** in settings; they also disappear when the app quits.

SQL in an answer is reviewed with **Apply / View diff / New tab / Copy / Create .sql file**. Apply replaces the selection or inserts at the cursor, and a single ⌘Z reverts it. You save a file yourself through the save dialog and ⌘S. The AI never saves or runs anything automatically. Row data is sent only with a separate check and a row-number selection. Earlier versions of a regenerated answer and thumbs up/down are kept only for this session.

When a reply proposes several SQL files, you expand each file's diff and accept or reject it, or review them all at once. Accepting changes only the editor and leaves the tab dirty, and you choose where a new file goes. A per-file edit is undone with one ⌘Z, and a bulk accept right after is undone with the card's undo. If the original changed during review, AxDB does not overwrite it.

For a query or MCP tool call, you check the command, the reason, the target connection, and the permission mode, then approve. Only read tools can skip re-confirmation for the rest of this connection session. Writes always need a rollback preview and an extra execution approval, and destructive SQL moves on to the preview only after the FuseButton's 4-second cancel window ends. During that time you can cancel with the button or Esc, and you can pause the fuse by hovering. The host also blocks writes in read-only mode. The call history collapses to row counts and elapsed time, and the audit log keeps only the SQL hash and the decision.

You choose a model from the model and Effort menus on the right of the composer and from the connection status in the header. ⇧⌘M searches models, and ⇧⌘. cycles the supported reasoning levels. If you choose to save as default, the choice is stored in the same place as the settings screen. If you do not, it applies only to the current connection session.

<p align="center">
  <img src="docs/assets/screens/ai-midnight.png" alt="AI assistant" width="640"><br>
  <sub><b>AI assistant</b> · Shows the evidence it checked (syntax → schema → EXPLAIN) as signals and suggests alternative queries</sub>
</p>

## Export

Use the export button in the result header to save the values currently loaded.

| Target | Format |
|---|---|
| Table | CSV (UTF-8 BOM, Excel-compatible), JSON (large integers and decimals kept as strings), Markdown |
| Document | JSON, NDJSON |
| Graph | `nodes`/`edges` JSON, PNG |
| ERD | SVG, PNG (with the current theme background, or transparent) |

A table is saved in the order after filter and sort. If the result was truncated or has a next page, AxDB tells you how many rows are loaded before saving. The save path must be one you chose in a save dialog opened by the app.

<p align="center">
  <img src="docs/design-shots/export-erd-dialog.png" alt="ERD image export dialog" width="720">
</p>

## Auto-update

Since 0.2.0, the app updates itself.

- It checks for a new version 10 seconds after launch and every 24 hours after that. When one is available, a badge appears in the status bar.
- Click the badge to see the current version → new version and the release notes. **Update now** downloads, installs, and restarts. **Later** and **Skip this version** are also available.
- The update file is installed only after its signature is verified. The update feed is `https://github.com/sangwookp9591/db-tool/releases/latest/download/latest.json`.
- You can turn off automatic checks in settings and check manually. On Windows, the app quits and reopens during the install.

<p align="center">
  <img src="docs/design-shots/update-notes-midnight.png" alt="Update release notes dialog" width="720"><br>
  <sub>Release notes dialog (mock data)</sub>
</p>

## Performance

Only measured values are listed. Environment, method, and limits are in each report, and the numbers are not generalized to other environments.

| Item | Measured value | Source |
|---|---|---|
| Cost of running drivers as separate processes (100k rows, 7 paired runs against the built-in approach on the same harness) | First UI channel row +1.01%, full completion +11.63% (debug build, within the +20% budget) | [P1 report](docs/P1_PLUGIN_REPORT.md) |
| Idle RSS of a driver process (connected) | 6–17 MiB (Redis 6.14 · SQLite 6.36 · PostgreSQL 8.59 · Neo4j 11.84 · MongoDB 16.75); closing the session gives 0 CPU increase over 10 seconds | [P1 report](docs/P1_PLUGIN_REPORT.md) |
| Host memory after repeating the same query 50 times | 696 → **63 MiB** (found that the macOS default allocator does not return freed memory to the OS, and fixed it with mimalloc) | [RESOURCE_OPT](docs/RESOURCE_OPT.md) |
| Add-connection flow (steps that do not touch the network) | All p95 77 ms or less; Connect (including first tree display) p50 134 ms | [VERIFICATION](docs/VERIFICATION.md) |
| Drivers screen list display (Chromium mock environment) | 2.7–18.7 ms with a cache; 5.8 ms skeleton without one | [P3 report](docs/P3_DRIVERS_UX_REPORT.md) |
| Viewers (Chromium mock environment) | 300-node graph pan/zoom p95 16.7 ms; 100k-row cold first display 83.0 ms (misses the 50 ms target); jitter and CLS 0 | [D1 report](docs/D1_DESIGN_REPORT.md), [VIEWER_PERF](bench/VIEWER_PERF.md) |
| Idle CPU (native, 60.8 s window) | 0.0987%, zero idle animations | [D1 report](docs/D1_DESIGN_REPORT.md) |

Raw data and measurement methods are in [VERIFICATION](docs/VERIFICATION.md), [RESOURCE_OPT](docs/RESOURCE_OPT.md), and [VIEWER_PERF](bench/VIEWER_PERF.md).

## Security

- Passwords and tokens are stored only in the OS keychain. They are not kept in the profile file (permissions 0600) or in logs.
- The app does not write automatically. Queries an agent runs are DB-level read-only by default, and writes need your approval inside the app.
- TLS was verified against real servers. For PG, `sslmode` (require / verify-ca / verify-full) and a CA file; for Neo4j, `+s` and `+ssc`; for Mongo, `tls` and `tlsCAFile`.
- Driver packages are verified with an Ed25519 signature from the official public key and SHA-256. Passwords reach the driver process only in the connect request, never in command-line arguments or environment variables.

## Design

- **Themes:** three options, Midnight (dark), Daylight (light), and follow the system setting.
- **Fonts:** Pretendard for the UI, JetBrains Mono for code and result values. Both are bundled with the app, so they look the same offline.
- **Micro-interactions:** components from React Bits (JellyRadio, SwipeRow, CometDial, WakeSlider, CodeSlots, LatticeLoader, BorderGlow, SpringCheck, FuseButton, CallChip, GlideSelect, BranchedMenu, and GlassIcons on the Drivers screen), plus the ⌘K command palette, toasts, tooltips, and the Integration Beam.
- **Motion principle:** things move only in response to input, and a resting screen has zero animation. AxDB follows `prefers-reduced-motion`, and there are no interactions inside the result grid or the graph canvas.
- **Wording:** the UI is in Korean. Technical terms such as Host, Port, TLS, and SQL, and the original error text returned by the DB, are left as they are.
- **Window size:** minimum 960×600.

Detailed rules are in the [design system](docs/DESIGN.md).

## Development

You need Rust 1.99.1 (pinned in `rust-toolchain.toml`), Node 22 or later, and pnpm. The workspace uses the Rust 2024 edition and a minimum of Rust 1.99, and it does not change your global rustup default.

### Build and run

```sh
pnpm --dir ui install
pnpm --dir ui build                              # creates ui/dist (embedded by Tauri)

# Dev build: build the driver executables first and point AXDB_DRIVER_DIR at them.
CARGO_TARGET_DIR=target-dev cargo build --workspace --bins
CARGO_TARGET_DIR=target-dev TAURI_CONFIG='{"build":{"devUrl":null}}' \
AXDB_DRIVER_DIR="$PWD/target-dev/debug" cargo run -p axdb

# Installer bundles (.app / .dmg / .exe). Drivers are not included.
pnpm dlx @tauri-apps/cli@2 build --config src-tauri/tauri.conf.json

# UI only, in a browser (mock backend)
pnpm --dir ui dev                                # http://127.0.0.1:5173
```

The app does not bundle drivers, so with no drivers installed the driver list is empty. During development, `AXDB_DRIVER_DIR` (read only in debug builds) registers the `axdb-driver-*` executables found there.

| Environment variable | Purpose |
|---|---|
| `AXDB_DRIVER_DIR` | Directory a dev build reads driver executables from (debug only). Tests use this path too |
| `AXDB_CONFIG_DIR` | Changes where settings, history, and logs go (for test isolation) |
| `AXDB_AGENT_BRIDGE=1` | Turns on the localhost MCP bridge (off by default). See [AGENT.md](docs/AGENT.md) |

### Tests

```sh
# Test DBs kept separate from your own DBs
docker run -d --name axdb-test-pg -e POSTGRES_PASSWORD=testpw -e POSTGRES_DB=axdb_test -p 127.0.0.1:55432:5432 postgres:16-alpine
docker run -d --name axdb-test-neo4j -e NEO4J_AUTH=neo4j/testpassword -p 127.0.0.1:57687:7687 neo4j:5-community

CARGO_TARGET_DIR=target-dev cargo build --workspace --bins
AXDB_DRIVER_DIR="$PWD/target-dev/debug" CARGO_TARGET_DIR=target-dev \
AXDB_TEST_PG='host=127.0.0.1 port=55432 user=postgres password=testpw dbname=axdb_test' \
AXDB_TEST_NEO4J='bolt://127.0.0.1:57687|neo4j|testpassword' \
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm --dir ui test && pnpm --dir ui exec tsc --noEmit

# Real-app E2E (through the MCP bridge)
node bench/e2e-bridge.mjs && node bench/preview-e2e.mjs && node bench/permissions-e2e.mjs
```

If a test DB variable is not set, the matching real-DB tests are skipped. Tests accept only isolated test servers.

| Variable | Target |
|---|---|
| `AXDB_TEST_PG` | PostgreSQL (port 55432) |
| `AXDB_TEST_NEO4J` | Neo4j (port 57687) |
| `AXDB_TEST_MONGO` | `mongodb://axdb:testmongo@127.0.0.1:57017/?authSource=admin` |
| `AXDB_TEST_REDIS` | `redis://:testredis@127.0.0.1:56379/0` |
| `AXDB_TEST_PG_TLS`, `AXDB_TEST_NEO4J_TLS`, `AXDB_TEST_MONGO_TLS`, `AXDB_TEST_TLS_CA`, `AXDB_TEST_TLS_WRONG_CA` | TLS verification servers and CA files |

### Writing a driver

Create a crate and implement `Driver`, build an executable with the SDK's `serve`, package and sign it with `cargo xtask package-drivers`, and install it in the app. The contract, the capabilities rules, and the protocol are in the [plugin guide](docs/PLUGIN_GUIDE.md), and the design background is in [ADR-0003](docs/adr/0003-installable-drivers.md).

### Release and distribution

GitHub Releases is the only distribution channel. Users get the installer from the [latest release](https://github.com/sangwookp9591/db-tool/releases/latest), and an installed app updates itself by reading `latest.json` from the same place. Drivers are uploaded separately from the app, to `drivers-*` releases.

**One-time setup** (see [RELEASE_SIGNING](docs/RELEASE_SIGNING.md) for the screens to use)

| What | Where | Purpose |
|---|---|---|
| Developer ID Application certificate | Login keychain | Code-signing the macOS app and drivers |
| Notary profile `axdb-notary` (App Store Connect API key) | Keychain | Submitting to Apple notarization |
| Update signing key `~/.tauri/axdb-updater.key`; its password is in the keychain as `axdb-updater` | Local file + keychain | Signing app auto-updates. **If you lose it, you can no longer send updates to installed apps, so back it up** |
| Driver signing key (`cargo xtask gen-signing-key`) | Keychain | Signing `.axdriver` packages. The public key is built into `crates/driver-package` |
| Windows cross-build tools | `rustup target add x86_64-pc-windows-msvc`, `brew install llvm nsis`, `cargo install cargo-xwin` | Building the `.exe` on macOS |

**App release steps**

1. **Bump the version:** make the version the same in `Cargo.toml` (workspace), `src-tauri/tauri.conf.json`, and `ui/package.json`.
2. **Release notes and changelog:** write `docs/release-notes/v<version>.md` with `<!-- date: YYYY-MM-DD -->` metadata and update `CHANGELOG.md`. Use `## Title {new|improved|fixed}` to group changes; each bullet becomes an entry in the app's "What's new" timeline. Its current release marker follows `ui/package.json`. Category tags are stripped automatically in the update feed.
3. **Verify:** `cargo test --workspace` (with the test DBs and `AXDB_DRIVER_DIR`), `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm --dir ui test`, `pnpm --dir ui exec tsc --noEmit`.
4. **If drivers changed, package them first:**
   ```sh
   APPLE_SIGNING_IDENTITY="Developer ID Application: …" \
   cargo xtask package-drivers --target aarch64-apple-darwin --target x86_64-apple-darwin \
     --target x86_64-pc-windows-msvc --version <version> --notarize
   ```
   This creates signed `.axdriver` files and `index.json` in `dist/drivers/`, and the app build embeds that list as an offline snapshot.
5. **Build, sign, and notarize the app:**
   ```sh
   scripts/release.sh <version>            # only builds artifacts into dist/release/<version>/ (no upload)
   ```
   It produces the universal `.dmg` (Developer ID signing → notarization → staple), the Windows NSIS `.exe`, the update archive and its signature (`.sig`), and `latest.json`. Check notarization progress with `xcrun notarytool history --keychain-profile axdb-notary`.
6. **Check the install:** install the dmg in a different location and run it, and confirm that `spctl -a -vv -t open --context context:primary-signature <dmg>` reports `Notarized Developer ID`. Run through driver install, connect, and update once.
7. **Publish (public, hard to undo):**
   ```sh
   scripts/publish-drivers.sh dist/drivers   # if drivers changed. drivers-* releases are not marked latest
   scripts/release.sh <version> --publish    # makes the v<version> release latest and uploads the dmg, exe, and latest.json
   ```
   Driver releases are uploaded with `--latest=false`, so `releases/latest` always points at an app release. The app's update feed and external install links follow that address.
8. **After release:** confirm that the update badge shows in the status bar of an older installed app, then commit and push your changes.

**Windows note:** the installer is not code-signed, so SmartScreen shows a warning on first launch (**More info → Run anyway**). To remove the warning, connect an OV/EV code-signing certificate to `bundle > windows > signCommand`.

## Structure

```
crates/dbcore            Driver contract: Driver·Session, Manifest·Capabilities, typed Value, polymorphic ResultSet
crates/driver-protocol   Driver process protocol (length-prefixed + MessagePack frames)
crates/driver-sdk        axdb_driver_sdk::serve — turns a Driver into a plugin executable
crates/driver-package    .axdriver packages and the signed list (index.json): create, verify, safe extraction
crates/driver-*          postgres · neo4j · mongodb · redis · sqlite (+ example-kv minimal example). Each builds an axdb-driver-<id> executable
xtask                    cargo xtask package-drivers (build, package, sign), gen-signing-key
src-tauri                Host: profiles and keychain, plugin_host (driver processes), driver install, run/cancel/timeout, preview, permissions, MCP bridge, AI, export, updater
ui                       Preact UI · CodeMirror 6 · virtual table · Canvas graph (d3-force Worker) · onboarding · Drivers screen
scripts                  release.sh (app release), publish-drivers.sh (driver upload), release-manifest.py
bench                    Measurement and E2E scripts and raw data
docs                     ARCHITECTURE · PLUGIN_GUIDE · AGENT · DESIGN · VERIFICATION · RELEASE_SIGNING · ADR · work reports
```

The list of documents is in [docs/README.md](docs/README.md).

## License and notices

AxDB is MIT-licensed. The third-party material it uses is:

- **Design token system:** token names and semantic rules taken from an external design convention and adapted (Apache-2.0). The source and full license text are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
- **React Bits:** interactions ported from it (MIT + Commons Clause).
- **neo4rs:** vendored with one patch applied (MIT).
- **Fonts:** Pretendard and JetBrains Mono (OFL).
- **Icons:** Lucide (ISC).

The full notices are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
