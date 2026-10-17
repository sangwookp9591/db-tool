# U2 CallChip report

## Implementation

- Added an independent Preact/CSS CallChip using lucide-preact and Midnight/Daylight tokens. The fill transitions to a 90% park while running, the live ms display uses `requestAnimationFrame` only while running, completion shows a green Check wash, and errors tint and shake unless `shake={0}` or reduced motion applies. A retry button is rendered only for an error with `onRetry`; the status message includes the tool, argument, state, and final duration.
- Added recent MCP activity chips to `AgentPanel` with mapped status, connection name, row count, and the backend reported `duration_ms`. The event currently contains no query text or arguments, so the chip uses this safe short metadata preview. Added the AI context CallChip above the existing `ToolCallGroup`; expandable details remain unchanged.
- Added four interactive states to the micro gallery and a browser test for transitions, stopped idle/done frames, and retry visibility. Added the React Bits table row to `DESIGN.md`.

## Verification

- `pnpm --dir ui exec tsc --noEmit`: passed.
- `pnpm --dir ui test`: passed on rerun. The first run hit the Chromium/Vite `beforeAll` 30-second timeout while the shared workspace was busy; isolated `src/ui/extra.test.ts` then passed 6/6, and the full suite passed after.
- `pnpm --dir ui exec vite build --outDir /private/tmp/claude-501/-Users-psw-Projects-db-tool/4da095e2-8b36-4d6c-9840-879187baad6e/scratchpad/u2-callchip`: passed.
- Ego browser and `overlap-probe.js`: AI dialog had no overlap, overflow, or clipping findings at 1280×800, 1024×700, and 960×600 in both themes. The CallChip gallery had no chip clipping or viewport overflow at those sizes in either theme; its retry overlay intentionally covers the chip as the click target, which the generic probe reports as overlap.
- Screenshots use the browser mock at 1280×800 and were scaled with `sips -Z 1600`: [AI Midnight](design-shots/u2-callchip-ai-panel-1280x800-midnight.png), [AI Daylight](design-shots/u2-callchip-ai-panel-1280x800-daylight.png), [feed demo running Midnight](design-shots/u2-callchip-agent-feed-running-1280x800-midnight.png), [running Daylight](design-shots/u2-callchip-agent-feed-running-1280x800-daylight.png), [done Midnight](design-shots/u2-callchip-agent-feed-done-1280x800-midnight.png), [done Daylight](design-shots/u2-callchip-agent-feed-done-1280x800-daylight.png), [error Midnight](design-shots/u2-callchip-agent-feed-error-1280x800-midnight.png), and [error Daylight](design-shots/u2-callchip-agent-feed-error-1280x800-daylight.png).

The browser mock does not emit native `agent-call` events or return an AI context reply, so the agent-state screenshots show the interactive CallChip gallery fixture, and the AI screenshots show the real mock assistant dialog in its available settings state. There was no pre-existing CallChip UI to capture as a before image. No commit was made.
