# C2 native E2E verification

Date: 2026-10-17. Target commit: `b65ab15664ea2c70ff6bc602f4714b7e7a02389b`. The test used the native macOS debug AxDB app and the isolated config at `/private/tmp/claude-501/-Users-psw-Projects-db-tool/4da095e2-8b36-4d6c-9840-879187baad6e/scratchpad/c2-config`.

All database access in this run was limited to the documented test services: PostgreSQL container `axdb-test-pg` (`127.0.0.1:55432`, database `axdb_test`) and Neo4j container `axdb-test-neo4j` (`127.0.0.1:57687`) for regression coverage. The native C2 workflow used only PostgreSQL. No other databases or containers were accessed.

## First-run false stop

The previous report stopped at the text **“Mock mode: edits are illustrative and apply only to this browser mock.”** That notice was hardcoded and also appeared in the real native app even though the app invoked the real Tauri `edit_preview` and `edit_apply` commands. The previous run treated the notice as a product limitation and stopped before trying a cell edit. This rerun confirms the native edit and apply path works; the first stop was false.

## Build and launch

Commands used:

```sh
pnpm --dir ui build
. "$HOME/.cargo/env"
TAURI_CONFIG='{"build":{"devUrl":null}}' CARGO_TARGET_DIR=target-c2e2e cargo build -p axdb
AXDB_CONFIG_DIR=/private/tmp/claude-501/-Users-psw-Projects-db-tool/4da095e2-8b36-4d6c-9840-879187baad6e/scratchpad/c2-config \
AXDB_DEV_SECRET_STORE=/private/tmp/claude-501/-Users-psw-Projects-db-tool/4da095e2-8b36-4d6c-9840-879187baad6e/scratchpad/c2-config/dev-secrets.json \
AXDB_AGENT_BRIDGE=1 target-c2e2e/debug/axdb
```

The UI build and debug app build both completed successfully. The isolated config and debug-only test credentials were reused from the previous run. GUI control used `orca computer` following the computer-use skill.

## PostgreSQL baseline

The fixture had `e2e.typed` with primary key `id` and two rows, `e2e.no_pk` with no primary key, and `e2e.big` with 250,000 rows. `e2e.typed` included `raw bytea`.

Baseline command:

```sh
docker exec axdb-test-pg psql -U postgres -d axdb_test -P null='NULL' \
  -c "SELECT id,note,encode(raw,'hex') AS raw_hex,octet_length(raw) AS raw_len FROM e2e.typed ORDER BY id"
```

Observed before native edits:

```text
 id |    note     | raw_hex  | raw_len
----+-------------+----------+--------
  1 | NULL        | 00ffabcd |      4
  2 | 한글 텍스트 |          |      0
(2 rows)
```

## Native C2 workflow

1. Connected to the `e2e-pg` profile and opened `e2e.typed` using the table’s Browse action. The grid showed two rows, a locked `id` primary key, and the `Edit rows` toggle. **No mock-mode notice appeared.** Screenshot: [primary-keyed browse](design-shots/c2-native-browse.png).

2. Turned on Edit rows. Changed row 1 `note` to `O'Brien \\ 한글` (one literal backslash followed by a space and Korean text), changed row 1 `raw` to `0x0102ff`, added row 3 with `id=3` and other fields NULL, and marked row 2 for deletion. The pending counter showed 3 row operations: one update, one insert, and one delete. The UI displayed `raw` as `<3 bytes> 0x0102ff`. Screenshot: [staged native edits](design-shots/c2-native-edit.png).

   PSQL check after staging and before preview:

   ```sh
   docker exec axdb-test-pg psql -U postgres -d axdb_test -P null='NULL' \
     -c "SELECT id,note,encode(raw,'hex') AS raw_hex,octet_length(raw) AS raw_len FROM e2e.typed ORDER BY id"
   ```

   PostgreSQL was unchanged from baseline: two rows remained; row 1 still had NULL note and `00ffabcd` (4 bytes), and row 2 remained.

3. Clicked Preview Changes. The preview showed `UPDATE` affected 1 (setting both `note` and `raw`), `DELETE` affected 1, and `INSERT` affected 1, with the row 1 before/after sample showing the requested text and row 3 containing `id=3`. The preview said it had not committed. Screenshot: [preview](design-shots/c2-native-preview.png).

   PSQL check after preview:

   ```sh
   docker exec axdb-test-pg psql -U postgres -d axdb_test -P null='NULL' \
     -c "SELECT id,note,encode(raw,'hex') AS raw_hex,octet_length(raw) AS raw_len FROM e2e.typed ORDER BY id"
   ```

   Output was identical to the baseline; preview rolled back and changed no data.

4. Clicked Apply and confirmed the in-app dialog (“Apply 3 changes to the database”). The editor returned to its browsed grid showing rows 1 and 3. Screenshot: [applied and re-browsed grid](design-shots/c2-native-applied.png).

   PSQL check after Apply:

   ```sh
   docker exec axdb-test-pg psql -U postgres -d axdb_test -P null='NULL' \
     -c "SELECT id,note,encode(raw,'hex') AS raw_hex,octet_length(raw) AS raw_len FROM e2e.typed ORDER BY id"
   ```

   ```text
    id |      note      | raw_hex | raw_len
   ----+----------------+---------+--------
     1 | O'Brien \ 한글 | 0102ff  |      3
     3 | NULL           | NULL    |   NULL
   (2 rows)
   ```

   The result is exactly one row update, one inserted row, and one deleted row. The binary value has `octet_length(raw)=3` and `encode(raw,'hex')='0102ff'`.

5. Verified stale-preview invalidation. With the editor open, changed row 1 note to `stale check`, previewed it, returned to the grid, and changed the edit set to `stale check 2`. The grid then said preview was required and exposed Preview Changes without Apply. Previewing again showed the Apply action. The test edit was discarded by exiting edit mode without applying. Screenshot: [stale preview guard](design-shots/c2-native-stale-preview.png). A final `psql` query before fixture cleanup confirmed the intended applied state from step 4 was still intact.

   The same `SELECT id,note,encode(raw,'hex'),octet_length(raw)` query used above returned row 1 with `O'Brien \\ 한글` and `0102ff` (3 bytes), plus row 3 with NULLs; no `stale check` value appeared.

6. Opened `e2e.no_pk`. It showed the `value` and `note` columns and no Edit rows toggle. Screenshot: [table without a primary key](design-shots/c2-native-no-pk.png).

7. Edited the `e2e-pg` test profile in the UI and set Advanced settings → Connection access mode to Read only. The open typed tab was labeled “읽기 전용” and had no Edit rows toggle. Screenshot: [read-only profile](design-shots/c2-native-readonly.png). Restored the profile to Read / Write in the UI; the existing typed tab again showed Edit rows. The regression permission suite also confirmed read-only UI Run and preview enforcement.

   These no-primary-key and access-mode checks changed no database rows. The database query after both checks and after discarding the stale edit still returned the step 4 state: rows 1 and 3, row 1 `note=O'Brien \\ 한글`, `raw_hex=0102ff`, `raw_len=3`, and row 3 NULLs.

## Fixture cleanup

After capturing the successful native Apply evidence, restored `e2e.typed` to its original two-row fixture so the regression benches could run against their expected values. This cleanup was performed only in `axdb-test-pg`:

```sql
BEGIN;
DELETE FROM e2e.typed WHERE id=3;
UPDATE e2e.typed SET note=NULL, raw=decode('00ffabcd','hex') WHERE id=1;
INSERT INTO e2e.typed (id,big,amount,note,created,d,doc,raw,uid,tags)
VALUES (2,-9223372036854775808,-0.000000001,'한글 텍스트',
        '2026-10-16 10:00:00+00','2026-10-16','null'::jsonb,decode('','hex'),
        '00000000-0000-0000-0000-000000000002',ARRAY['y']);
COMMIT;
```

The final restored query showed rows 1 and 2 and the baseline values. A first bridge run before cleanup was 20/21 because it expected the deleted id 2 row; after restoring the fixture, the clean rerun passed.

## Regression results

Commands used after fixture cleanup:

```sh
AXDB_CONFIG_DIR=/private/tmp/claude-501/-Users-psw-Projects-db-tool/4da095e2-8b36-4d6c-9840-879187baad6e/scratchpad/c2-config node bench/e2e-bridge.mjs
AXDB_CONFIG_DIR=/private/tmp/claude-501/-Users-psw-Projects-db-tool/4da095e2-8b36-4d6c-9840-879187baad6e/scratchpad/c2-config node bench/preview-e2e.mjs
AXDB_CONFIG_DIR=/private/tmp/claude-501/-Users-psw-Projects-db-tool/4da095e2-8b36-4d6c-9840-879187baad6e/scratchpad/c2-config AXDB_APP_PID=41941 node bench/permissions-e2e.mjs
```

- `bench/e2e-bridge.mjs`: **21/21 passed** (PostgreSQL, Neo4j, SQLite, live app bridge).
- `bench/preview-e2e.mjs`: **5/5 passed** (PostgreSQL UPDATE/DDL/refusal, Neo4j CREATE rollback, SQLite DELETE rollback).
- `bench/permissions-e2e.mjs`: **13/13 passed**, including read-only database enforcement, UI Run and agent preview restrictions, rollback preview and approval behavior.

No product source files were changed, and no git commit was created. The debug app was quit after verification.
