# Viewer performance — 2026-10-15

All times are milliseconds. Each latency cell is p50 / p95; nearest-rank percentiles, three repetitions unless stated otherwise. Three-sample p95 is the maximum, not a population guarantee. Frame cells contain run-p50 p50/p95; run-p95 p50/p95; absolute max (60 intervals per interaction).

## Environment and measurement limits

- Chromium: {"os":"darwin 27.0.0","cpu":"Apple M5","arch":"arm64","browser":"156.0.8078.4","viewport":"1440x1000","mode":"Vite dev / headless Chromium / alternating paired runs"}.
- Native: {"at":"2026-10-15T06:31:46.408Z","href":"tauri://localhost","mode":"Tauri debug / WKWebView","target":"all","targets":["127.0.0.1:55432","bolt://127.0.0.1:57687"],"userAgent":"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)"}. Embedded Vite production assets in a Tauri debug binary, macOS WKWebView, same host. WKWebView heap is unavailable.
- Browser before/after uses two immutable temporary UI copies, alternates order every repetition, unmounts each scene between samples, and records uptime/load before every sample. Native legacy/adaptive comparisons also alternate. The complete native matrix/workspace/idle precede the final layout-effect and absolute-column refinements. The focused small-query run verifies the effect refinement; final Chromium table/graph runs verify final UI geometry. A subsequent final native matrix stopped making progress after the first100k legacy sample when not forced to the foreground; its interrupted partial data is excluded. Final native transport/frame deltas after the column geometry refinement are NOT verified.
- The host ran other workloads. Actual load averages are retained per sample; small differences are not attributable solely to this change. Mock data is deterministic and uses a dedicated fixture Worker. The ordinary demo SELECT-large path still constructs its old fixture and is not covered by that first-row claim.
- Paint is a requested double-rAF proxy, not a compositor timestamp. At 60 Hz it generally needs ~33ms, so <16ms re-entry is incompatible with this measurement definition. Background/throttled WKWebView can need longer.
- Mock baseline builds/serializes/parses the full fixture before mounting; after streams first200/next2000 rows from a Worker using columnar JSON. Native compares two transports on the same optimized viewer; it is not a full old/new app comparison.
- Test endpoints only: PG127.0.0.1:55432 and Neo4j127.0.0.1:57687; generated PG SELECTs and unique DbtP_ fixture labels. Native reports complete=true only after cleanup succeeds.

## Table: paired Chromium before → after

| Scenario | First before → after | Full load before → after | Scroll frames before → after | Heap after MiB | Visible cells before → after |
|---|---:|---:|---|---:|---:|
| table-1000-10 | 24.00 / 46.80 → 41.00 / 90.10 | 5.20 / 5.80 → 15.40 / 75.00 | 16.70 / 16.70; 16.70 / 16.80; 16.80 → 16.70 / 16.70; 16.70 / 16.80; 33.40 | 5.81 / 6.11 | 330 → 288 |
| table-10000-10 | 50.50 / 58.10 → 32.90 / 39.70 | 34.00 / 40.40 → 38.80 / 42.90 | 16.70 / 16.70; 16.80 / 16.80; 16.80 → 16.70 / 16.70; 16.70 / 16.80; 16.80 | 10.17 / 11.02 | 330 → 288 |
| table-100000-10 | 396.10 / 575.50 → 41.40 / 48.90 | 386.60 / 557.00 → 270.30 / 294.40 | 16.70 / 16.70; 16.70 / 16.80; 33.40 → 16.70 / 16.70; 16.70 / 16.70; 50.00 | 69.97 / 75.72 | 330 → 288 |
| table-10000-200 | 807.50 / 943.10 → 56.90 / 67.10 | 734.30 / 867.60 → 539.40 / 749.90 | 83.40 / 116.60; 150.00 / 166.70; 333.20 → 16.70 / 16.70; 16.80 / 33.30; 66.70 | 114.34 / 259.56 | 6600 → 288 |

## Graph: paired Chromium before → after

First after means a converged, static graph is visible. Before exposed intermediate force positions. Stable time starts at layout setup; first includes mounting/module loading. All graphs have about twice as many relationships.

| Nodes | First before → after | Stable before → after | Pan/zoom frames before → after | Properties after | Pure +50 merge after | Expand request→first merged paint after |
|---|---:|---:|---|---:|---:|---:|
| 100 | 81.20 / 110.60 → 99.40 / 118.50 | 998.60 / 1033.40 → 40.90 / 47.60 | 16.70 / 16.70; 16.70 / 16.80; 50.00 → 16.70 / 16.70; 16.70 / 16.80; 16.80 | 49.20 / 50.40 | 0.10 / 0.10 | 49.80 / 51.30 |
| 300 | 81.20 / 83.80 → 164.60 / 168.10 | 1001.60 / 1154.50 → 110.40 / 118.30 | 16.70 / 16.70; 16.70 / 16.80; 16.80 → 16.70 / 16.70; 33.30 / 33.40; 33.40 | 49.40 / 50.00 | 0.10 / 0.10 | 53.50 / 55.80 |
| 1000 | 124.30 / 132.40 → 483.00 / 494.60 | 1501.90 / 1543.70 → 417.50 / 425.40 | 16.70 / 16.70; 16.70 / 16.70; 16.80 → 50.00 / 50.00; 50.10 / 50.10; 66.70 | 67.30 / 68.10 | 0.30 / 0.30 | 108.40 / 112.40 |
| 5000 | 921.20 / 1357.30 → 3075.70 / 3544.10 | 12452.10 / 13086.30 → 2650.10 / 3179.20 | 16.70 / 16.70; 33.30 / 33.40; 33.40 → 166.70 / 166.70; 200.00 / 216.60; 233.40 | 199.80 / 199.90 | 2.10 / 2.90 | 391.40 / 418.20 |

The first paint of expansion starts a200ms ease-out for new nodes only. Existing nodes stay fixed. Drag release pins the node. Layout ticks are synchronous inside a Worker, yielded in8ms batches, alphaDecay.08/alphaMin.01/velocityDecay.7; only the converged Float32 position array is transferred. Initial layout is hidden behind a fixed placeholder.

## Idle, motion and layout stability

| Measurement | Before | After |
|---|---:|---:|
| Converged300nodes: average movement over5s (px) | 0.00 / 0.00 | 0.00 / 0.00 |
| Converged300nodes: render frames over5s | 0.00 / 0.00 | 0.00 / 0.00 |
| First5s after mount: average visible movement per sampled node/frame(px) | 6.85 / 6.87 | 0.00 / 0.00 |
| First5s after mount: render frames | 62.00 / 62.00 | 1.00 / 1.00 |
| Scenario CLS (excludes recent input), maximum | 0.011779781539351853 | 0 |

Native idle before:300nodes, 12s, mean displacement 0px, render frames 0;10 one-second ps samples of app+new WebKit processes: mean 0.19% CPU, samples [0.1,0,0,0,0,0,0,0,1.8,0]. app plus new WebKit processes since spawn; conservative if another app launched WebKit concurrently. Load: 15:30  up 3 days,  6:13, 22 users, load averages: 35.21 47.42 88.05.

Native idle after:300nodes, 12s, mean displacement 0px, render frames 0;10 one-second ps samples of app+new WebKit processes: mean 0.06% CPU, samples [0.4,0,0,0,0.2,0,0,0,0,0]. app plus new WebKit processes since spawn; conservative if another app launched WebKit concurrently. Load: 15:31  up 3 days,  6:13, 22 users, load averages: 27.01 43.64 84.36.

Both CPU means are below1%; the small reduction is within host noise and is not claimed as causal. After convergence neither implementation continuously repaints; the new prewarm removes visible movement during initial settling. Idle code has no recurring rAF/setInterval; only interaction/resize/data schedules paint. Hidden views, inactive tabs and document.hidden pause Workers and cancel animation. Preliminary wide-table CLS0.504 came from slot-based column recycling; changing keys alone worsened it to30.73 (results-viewer-cls-smoke.json). Final cells/headers keep their absolute column offsets rather than counter-moving a grid with a parent transform, and counters reserve fixed-width left-aligned numeric slots; final table samples are all0. Crossing the graph display limit used to insert a warning row, shrink the canvas and trigger fit-to-view again; the warning now occupies the existing fixed toolbar, with a300→350 node expansion rectangle assertion. Trusted view switches also have0 unexpected CLS. Columns keep absolute offsets while rows use vertical transforms; changing just column keys was insufficient because grid reflow counteracted parent translation. A real Chromium ResizeObserver check additionally verifies query/editor/results dimensions stay unchanged for5s.

## Transitions

| Operation | Before p50/p95 | After p50/p95 |
|---|---:|---:|
| Mock first-Graph | 90.30 / 94.90 | 161.80 / 170.20 |
| Mock first-Table | 31.60 / 33.90 | 33.40 / 33.50 |
| Mock return-Graph | 33.30 / 36.10 | 33.40 / 38.00 |
| Mock return-Table | 33.40 / 36.80 | 33.20 / 33.90 |
| Mock result-set tab | 33.30 / 34.00 | 32.70 / 40.20 |
| Mock workspace connect | 189.00 / 338.10 | 259.90 / 507.70 |
| Mock workspace newTab | 65.80 / 171.40 | 81.50 / 175.10 |
| Mock workspace tab | 63.00 / 255.80 | 64.60 / 265.00 |
| Mock workspace addConnection | 163.80 / 338.50 | 275.10 / 313.70 |
| Native workspace connect (3runs) | — | 367.00 / 501.00 |
| Native workspace newTab (3runs) | — | 66.00 / 74.00 |
| Native workspace tab (3runs) | — | 67.00 / 72.00 |
| Native workspace addConnection (3runs) | — | 569.00 / 598.00 |
| Native workspace resultSet (3runs) | — | 66.00 / 113.00 |
| Native first-Graph | — | 306.00 / 307.00 |
| Native first-Table | — | 63.00 / 67.00 |
| Native return-Graph | — | 66.00 / 68.00 |
| Native return-Table | — | 69.00 / 139.00 |

Query tabs were already kept mounted. Results now lazily mount each view/result-set once and retain it; canvas identity and table scroll survive switching. Headers/status/toolbars reserve fixed heights, counters are tabular/throttled100ms, inspectors overlay, and streamed extra result sets occupy the existing toolbar. Splitter ratio persists per mounted query tab; keyboard/drag resizing respects result minimum240px. Fit/reset computes bounds and padding; reverse/parallel edges receive separated curves, labels/arrows disappear below zoom.6.

## Technique ledger

| Technique | Measured evidence | Decision |
|---|---|---|
| Columnar uniform tags/mixed tagged fallback | 36,644,862 → 19,945,132bytes (45.6% smaller); raw JSON parse 86.80 / 103.80 vs parse+decode 52.20 / 137.20 | Keep; Int/Decimal/JSON remain exact strings |
| Both-axis virtualization, transformed row pool, absolute column positions, rAF scroll/resize, visible cache | Wide fixture cells 6600 → 288; scroll above;10k repeated JSON formats 5.70 / 6.40 → 0.10 / 0.30 | Keep;4096-entry bound, synthetic format microbenchmark; no independent attribution to each rendering change |
| Loaded-row index sorting/filtering | Uint32Array indices, original rows unmodified; exact integer/decimal tests and DOM filter/sort pass | Keep; main-thread scan remains a large-data bottleneck |
| Quadtree picking |10k picks/5knodes linear 58.10 / 136.10 → tree 4.20 / 4.90, build 0.70 / 1.30 | Keep |
| Incremental placement | +50 seed 0.20 / 0.30, existing nodes moved 0,0,0 | Keep; fixed previous positions |
| Worker force, Float32 transfer, viewport culling, LOD, color-batched Canvas | Graph/stability/motion metrics above | Keep combined renderer; culling/LOD independently unisolated |
| Keep-alive and inactive suspension | Transition numbers above; stable re-entry triggers0 new Worker replies | Keep |
| Channel for every result | Diagnostic results-viewer-app-channel-regression.json: graph300legacy~0.2–0.8s vs Channel~2.4–5.3s | Reverted for graphs and all results whose tables≤2000rows; use direct invoke. Larger table-only results use200/2000row Channel chunks |
| Fixed16ms Worker yield | Intermediate results-viewer-worker16.json slowed settling | Reverted; zero-delay yield between8ms CPU batches |
| WebGL | No implementation or benchmark | Canvas retained; no claim that WebGL would improve it |

## Native real database matrix

Three repetitions each. “stream” names the adaptive execute_stream command: direct Vec return for graph/small table, Channel for larger table-only data. Legacy and adaptive use the same optimized rendering. Driver elapsed is execution/collection; it excludes layout/paint. Full load is command/chunk completion and can precede first paint.

| Scenario / transport | First | Full load | Driver elapsed | Stable | Interaction frames | Properties | Expand→paint | Pure merge | Added nodes |
|---|---:|---:|---:|---:|---|---:|---:|---:|---|
| graph-100-0-legacy | 267.00 / 331.00 | 124.00 / 187.00 | 97.00 / 157.00 | 44.00 / 47.00 | 33.00 / 33.00; 37.00 / 39.00; 42.00 | 100.00 / 100.00 | 202.00 / 233.00 | 0.00 / 0.00 | 52,52,52 |
| graph-100-0-stream | 233.00 / 233.00 | 65.00 / 82.00 | 45.00 / 62.00 | 37.00 / 38.00 | 33.00 / 33.00; 38.00 / 40.00; 43.00 | 100.00 / 100.00 | 167.00 / 168.00 | 0.00 / 0.00 | 52,52,52 |
| graph-1000-0-legacy | 1274.00 / 1322.00 | 549.00 / 569.00 | 399.00 / 462.00 | 630.00 / 640.00 | 33.00 / 34.00; 37.00 / 42.00; 86.00 | 100.00 / 101.00 | 172.00 / 233.00 | 1.00 / 1.00 | 52,52,52 |
| graph-1000-0-stream | 1000.00 / 1145.00 | 303.00 / 429.00 | 198.00 / 320.00 | 582.00 / 628.00 | 33.00 / 33.00; 35.00 / 40.00; 44.00 | 98.00 / 100.00 | 135.00 / 137.00 | 1.00 / 1.00 | 52,52,52 |
| graph-300-0-legacy | 471.00 / 916.00 | 258.00 / 626.00 | 217.00 / 520.00 | 130.00 / 176.00 | 33.00 / 33.00; 40.00 / 81.00; 374.00 | 99.00 / 100.00 | 167.00 / 168.00 | 1.00 / 1.00 | 52,52,52 |
| graph-300-0-stream | 333.00 / 333.00 | 106.00 / 116.00 | 70.00 / 79.00 | 103.00 / 107.00 | 33.00 / 33.00; 36.00 / 36.00; 44.00 | 100.00 / 104.00 | 167.00 / 189.00 | 0.00 / 0.00 | 52,52,52 |
| graph-5000-0-legacy | 6098.00 / 6564.00 | 1832.00 / 2435.00 | 1240.00 / 1884.00 | 3546.00 / 4637.00 | 33.00 / 33.00; 39.00 / 40.00; 294.00 | 102.00 / 102.00 | 230.00 / 1132.00 | 2.00 / 2.00 | 50,50,50 |
| graph-5000-0-stream | 5468.00 / 6970.00 | 1422.00 / 3459.00 | 769.00 / 2512.00 | 3380.00 / 3903.00 | 33.00 / 33.00; 37.00 / 39.00; 42.00 | 99.00 / 100.00 | 200.00 / 201.00 | 1.00 / 2.00 | 50,50,50 |
| table-1000-10-legacy | 110.00 / 117.00 | 52.00 / 70.00 | 18.00 / 30.00 | unavailable | 33.00 / 33.00; 40.00 / 49.00; 425.00 | unavailable | unavailable | unavailable | —,—,— |
| table-1000-10-stream | 100.00 / 101.00 | 38.00 / 55.00 | 10.00 / 17.00 | unavailable | 33.00 / 33.00; 36.00 / 37.00; 51.00 | unavailable | unavailable | unavailable | —,—,— |
| table-10000-10-legacy | 656.00 / 783.00 | 582.00 / 755.00 | 112.00 / 129.00 | unavailable | 33.00 / 33.00; 35.00 / 44.00; 81.00 | unavailable | unavailable | unavailable | —,—,— |
| table-10000-10-stream | 201.00 / 239.00 | 486.00 / 669.00 | 93.00 / 121.00 | unavailable | 33.00 / 33.00; 36.00 / 36.00; 41.00 | unavailable | unavailable | unavailable | —,—,— |
| table-10000-200-legacy | 5903.00 / 7649.00 | 5799.00 / 7561.00 | 1563.00 / 2084.00 | unavailable | 33.00 / 33.00; 52.00 / 69.00; 569.00 | unavailable | unavailable | unavailable | —,—,— |
| table-10000-200-stream | 1636.00 / 2212.00 | 8693.00 / 11838.00 | 1427.00 / 1986.00 | unavailable | 33.00 / 34.00; 56.00 / 64.00; 127.00 | unavailable | unavailable | unavailable | —,—,— |
| table-100000-10-legacy | 3556.00 / 4738.00 | 3500.00 / 4656.00 | 1058.00 / 1703.00 | unavailable | 33.00 / 33.00; 35.00 / 41.00; 51.00 | unavailable | unavailable | unavailable | —,—,— |
| table-100000-10-stream | 1141.00 / 2030.00 | 5498.00 / 8676.00 | 1042.00 / 1795.00 | unavailable | 33.00 / 33.00; 35.00 / 44.00; 57.00 | unavailable | unavailable | unavailable | —,—,— |

Native expansion returns52nodes at100/300/1000 (50new fixture nodes plus two existing incoming neighbors outside the loaded graph),50 at5000. At the5000node display ceiling data merges while visible count stays capped. Mock expansion adds exactly50. Click timing uses canvas hit-testing after search centers Node0, then opens the actual properties panel.

## Four-row query: UI vs agent

The measured regression was sending tiny graph results through a multi-event Channel. Adaptive direct invoke removes that transport overhead. Retaining positions for identical node IDs avoids re-layout, and setting up the cached scene in useLayoutEffect avoids deferring display until an after-paint effect. Queries use the same temporary session and identical MATCH returning4nodes; agent execution is read-only.

| Run | Agent driver | Agent wall | UI driver | UI command wall | UI first paint |
|---|---:|---:|---:|---:|---:|
| Cold | 73 | 98 | 98 | 176 | 302 |
| Warm 1 | 31 | 40 | 16 | 25 | 41 |
| Warm 2 | 11 | 20 | 11 | 20 | 66 |
| Warm 3 | 12 | 20 | 10 | 20 | 67 |
| Warm 4 | 27 | 34 | 38 | 50 | 85 |

Warm UI command 20.00 / 50.00; warm paint 66.00 / 85.00. The <100ms requirement for command wall is **met**, and for first paint is **met**. These samples are the focused effect-adjustment run in results-viewer-app-small.json. Earlier full all-matrix raw samples remain in results-viewer-app.json (paint93–180ms). Cold first paint302ms misses100ms. The later final-geometry native retry is interrupted and not used as a success result. These numbers do not assert a same-load1,145ms→X causal improvement against the coordinator’s earlier observation.

## Target accounting (paired mock p95)

| Target | Measured | Verdict |
|---|---:|---|
| 100k first incl mock transport <150ms | 48.90 | **met** |
| All scroll p95 <16.7ms | 33.30 | **NOT met** |
| Table↔Graph re-entry double-rAF <16ms | 37.40 | **NOT met** |
| 300node converged first <100ms | 168.10 | **NOT met** |
| +50 pure merge <50ms | 2.90 | **met** |

## Verification, remaining bottlenecks and reproduction

- UI build/typecheck and32Vitest tests pass. Full Rust workspace tests with both isolated DB environment variables pass; clippy --workspace -- -D warnings passes. Browser verification passes exact large values, index filter/sort, both-axis DOM bound, canvas/scroll keep-alive, no Worker recomputation on stable re-entry, splitter per-tab ratio/minimum, agent perf tool, no page errors, trusted node drag/release pinning,300→350 expansion without canvas resize, trusted-switch CLS0 and5s idle size stability.
- Source grep for database IDs in ui/src excluding mock/tests returns0. Benchmark fixture values live in bench/fixtures.json and are delivered by the gated host command. Native harness is dynamically imported only when DBTOOL_BENCH=1; the ordinary app does not auto-run it. Forced always-on-top/focus was removed after the user observed the visible automated transitions.
- Host chunking begins after the driver finishes collecting; server execution/collection remains in first-row latency. PG already uses bounded FETCH, but no new dbcore sink or database-level result streaming was added. Large arrays/JSON construction and serializing2million wide-table cells remain expensive.
- Main-thread index filtering and Canvas painting at5000nodes remain limits. Converged-only first frame deliberately adds layout latency to avoid visible jitter. Small-load/graph first and scrolling targets are reported honestly above. Expansion full-paint includes actual database roundtrip; pure merge is separately measured.
- No commits or production database connections were made by W4. Final native retry made no progress for over120s with app CPU0, consistent with WKWebView rAF suspension when occluded, but that cause was not independently proven; it was stopped instead of repeatedly stealing focus. Its results-viewer-app-matrix-interrupted.json is explicitly incomplete, excluded from final comparisons, and had not reached graph fixture creation. Re-run final native matrix with a continuously visible dedicated test window to verify the last table geometry change. Historical failed/superseded/regression JSON files remain diagnostic and are excluded from final paired comparisons. Early test runs encountered the W7a-owned PG cancellation race and incomplete vendor snippet checkout; the final workspace run is authoritative. Coordinator’s21/21 bridge E2E is separate evidence; W4 does not infer it from these viewer benchmarks.

Reproduce browser: prepare immutable before/after UI trees on1425/1426 with the same harness and old/new viewer implementations, then node bench/viewer-bench-paired.mjs. The baseline starts from original viewer components in b36cafe with measurement hooks and the unmount observer guard; final graph/table/limit runs use OUTPUT_SUFFIX=final-graph/final-table/final-limit and SCENARIOS subsets, merged explicitly by summarize-viewer.mjs. Older current-load raw files supply codec/technique measurements; no failed sample is silently mixed in. Main-app DOM: UI dev on1424, node bench/verify-viewer.mjs. Basic mock runner: node bench/viewer-bench.mjs NAME.

Native: pnpm --dir ui build; TAURI_CONFIG='{"build":{"devUrl":null}}' CARGO_TARGET_DIR=target-w4 cargo build -p db-tool; DBTOOL_BINARY=target-w4/debug/db-tool node bench/viewer-bench-app.mjs [matrix|small|graph|workspace|idle]. No argument runs the complete matrix, separate processes are serialized by a lock. Both config-dir aliases point to a private temporary directory and bridge is disabled. Each exact DbtP_ label and temporary profile is removed in finally. Reports retain uptime and raw samples; native graphics tests require a visible app window.

Implementation references: [Tauri Channels](https://v2.tauri.app/develop/calling-frontend/#channels), [d3-force simulation](https://d3js.org/d3-force/simulation).
