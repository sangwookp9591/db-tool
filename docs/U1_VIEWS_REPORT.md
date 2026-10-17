# U1 Views report

## Changes

- Edit mode uses the virtual browse grid's initial column-width calculation. Numeric headings and values align right; the fixed-width row-action column stays sticky, uses a delete icon header, and retains existing row-button names.
- FK cells reserve button space, ellipsize, and provide full value text in `title`. Filtered browse titles are rebuilt from target key fields and the returned row's complete typed values, avoiding the PostgreSQL driver's 80-character `ResultSet.title` truncation. SQL result titles keep the original full string for the single-line ellipsis/title.
- Edit-table focus outlines can paint at cell edges. Result flex containers shrink at short viewport sizes. A final 39px view-toolbar height adjustment addresses the probe's 38px content measurement.
- ERD nodes are rounded cards with aligned name/type fields and a PK marker; ERD relationship labels have a surface chip and the search prompt names tables. Graph rendering keeps canvas labels at scale >= 0.6 and fits after the initial worker layout settles. `fitGraph` now permits scale up to 4 (was 2) so small graphs use more of the canvas; no per-frame DOM work was added.

## Screenshot and probe artifacts

The audit screenshots remain the before evidence in `scratchpad/ui-audit/screenshots/`. In `scratchpad/u1-views/after/`, valid mock captures were made for browse, edit-many-columns, long SQL, and ERD at 1280×800, 1024×700, and 800×600 in Daylight and Midnight. The long-SQL mock capture shows the submitted SQL in the editor, but the result header is hard-coded to `People`, so it cannot verify SQL-title ellipsis. The mock has no FK child fixture; `fk-child-unavailable-mock-comparison-*` is the ordinary people browse table and is not FK evidence. A final graph capture after raising the fit scale did not retain the loaded graph state, so the graph screenshot matrix needs a clean rerun.

The reusable probe ran on the available mock views. In the last valid graph-state run, graph/table result-area viewport-overflow findings F207–F228 were absent; the graph render showed node labels. F002 focus-ring-clipped was absent in the valid browse/edit probe runs. The probe still reported F196: view-toolbar scroll height 38px versus client height 35px in browse/edit; the final 39px CSS correction was applied after those runs and remains unverified. Statusbar overflows (F203/F204) and editor/workspace-tab clipping also remain in some states and are outside this views ownership.

The mock does not expose native FK navigation values or SQL-echo result titles. The filtered-title code path reconstructs FK predicates from the matched target row's exact value, but the `9223372036854775807` case still needs a native or purpose-built fixture to visually verify.

## Verification

- `pnpm --dir ui exec tsc --noEmit` passed.
- `pnpm --dir ui test` passed after the FK-title change: 15 files, 73 tests.
- Scratch Vite build passed at `/private/tmp/claude-501/-Users-psw-Projects-db-tool/4da095e2-8b36-4d6c-9840-879187baad6e/scratchpad/u1-views/build`.
- The tests/build predate the final CSS height adjustment; the toolbar correction and graph scale change have not received a fresh visual/probe/performance run.
