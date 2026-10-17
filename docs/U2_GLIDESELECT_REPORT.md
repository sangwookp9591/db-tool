# U2 GlideSelect report

## Changes

- Added an independent Preact GlideSelect with token-based styling, `ChevronDown`/`Check`, `sm`/`md`/`lg` sizes, tags, selection highlight, keyboard navigation, typeahead, and press-drag selection.
- Menus use fixed viewport coordinates and close/reposition on outside pointer, scrolling, and resize. Menus in modal dialogs portal into the open dialog's top layer; other menus portal to `document.body`. Placement flips against both viewport edges and the dialog header/footer bounds.
- Replaced the app's native selects in connection fields, driver/access mode, page size, appearance settings, and client permissions. Named form selects render a required-capable form value input for `FormData` and validity checks.
- Corrected connection form and client dialog primary buttons to `brand`, removed the duplicate connection section chevron, and allowed client connection names to wrap while retaining their title.
- Added the React Bits row in `docs/DESIGN.md`, gallery coverage, and GlideSelect behavior helper tests. The mock now keeps agent client edits in in-memory state for `agent_client_save` and `agent_clients_list`; desktop credential commands and real `api.ts` behavior are unchanged.
- Fixed a portal styling issue found during mouse verification by applying the size-specific menu row token directly to the portal menu.

## Verification

- `pnpm --dir ui exec tsc --noEmit`: passed.
- `pnpm --dir ui test`: 75 tests passed across 15 files on the serial retry. The first two parallel runs timed out in the unrelated CallChip browser suite under heavy concurrent load; the serial run passed.
- `pnpm --dir ui exec vite build --outDir /private/tmp/claude-501/-Users-psw-Projects-db-tool/4da095e2-8b36-4d6c-9840-879187baad6e/scratchpad/u2-glideselect-build`: passed after the portal size token and mock-only client persistence changes.
- `rg -n '<select' ui/src`: no native selects remain. `bench/w7-form-regression.mjs` only reads the first name input and password input; no benchmark or `ui/src/agent` code uses `selectOption` or assigns `.value` on these selects.
- Ego browser mock: connection access-mode menu and settings appearance menu opened in Midnight and Daylight at 1280×800, 1024×700, and 960×600. The connection menu flipped above the trigger and stayed within the viewport; GlideSelect menu clipping/overflow findings were zero on these screens.
- The connection dialog screenshot confirmed its menu is visible above the dialog footer after moving the portal into the modal top layer.
- Page size: in Midnight and Daylight at 1280×800, 1024×700, and 960×600, selected 500 and 1000 with alternating mouse and keyboard input. The unchanged `Page size` accessible name remained present, and the toolbar reloaded to `0 – 500` or `0 – 1,000`. The overlap probe found no GlideSelect menu clipping/overflow.
- Client permissions: changed Analytics sandbox from `읽기` to `읽기 + 미리보기`, saved, reopened the editor, and confirmed the value persisted with the unchanged `Permission Analytics sandbox` name. The open menu fit inside the dialog and above its footer; the overlap probe found no menu clipping. Probe occlusions were limited to the open menu overlapping permission triggers in lower table rows.

## Screenshots

Screenshots are in [design-shots](design-shots/) and were resized with `sips -Z 1600`.

- Connection dialog and menu: `u2-glideselect-connection-<width>x<height>-<theme>.png`
- Settings and menu: `u2-glideselect-settings-<width>x<height>-<theme>.png`
- Page-size menu: `u2-glideselect-pagesize-<width>x<height>-<theme>.png`
- Client permission editor and menu: `u2-glideselect-clients-edit-<width>x<height>-<theme>.png`
- Selected permission after editing: `u2-glideselect-clients-edit-selected-1280x800-midnight.png`
