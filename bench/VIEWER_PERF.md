# Viewer performance — 2026-10-15

All durations are milliseconds; cells show p50 / p95 unless stated otherwise. Three repetitions per scenario, 60 rAF intervals per interaction, and nearest-rank percentiles. A three-sample p95 is the maximum, not a population guarantee.

## Environment and method

- {"os":"darwin 27.0.0","cpu":"Apple M5","arch":"arm64","browser":"156.0.8078.4","mode":"Vite dev / headless Chromium","viewport":"1440x1000"}
- Native: Tauri debug binary, embedded Vite production assets, macOS WKWebView; only PG 127.0.0.1:55432 and Neo4j bolt://127.0.0.1:57687. Native hardware is the same host.
- Paint is a double-rAF proxy requested by the spec, not a compositor timestamp. On this 60 Hz environment it normally costs about 33 ms, making the strict <16 ms switch target unattainable by that definition.
- Mock baseline generates, serializes and parses the whole deterministic fixture on the main thread before mounting. Final mock streams the identical fixture from a Worker, first 200 rows then 2,000, using columnar JSON. Native compares legacy execute vs real Tauri Channel on the same optimized viewer. These measure different layers and are reported separately.
- Baseline/source starts at UI before W4. Immutable temporary UI copies isolated baseline and candidate from other workers' HMR. Development builds and shared-machine background load limit causal attribution of small deltas; no hardware-wide speed guarantee.
- JS heap is Chromium usedJSHeapSize, after loading/scroll; it is not process RSS or a peak. WKWebView does not expose that heap API.

## Table: mock baseline → final

| Rows × columns | First before | First after | Load before | Load after | Scroll p95 before → after | Final heap MiB p50/p95 | Cells before → after |
|---|---:|---:|---:|---:|---:|---:|---:|
| table-1000-10 | 33.3 / 51.3 | 47.6 / 251.0 | 4.8 / 6.0 | 23.0 / 236.3 | 16.8 / 16.8 → 16.8 / 16.8 | 4.6 / 4.7 | 330 → 288 |
| table-10000-10 | 66.7 / 67.0 | 65.4 / 90.2 | 43.4 / 45.1 | 57.4 / 95.7 | 16.7 / 16.8 → 16.7 / 16.8 | 9.7 / 11.0 | 330 → 288 |
| table-100000-10 | 449.8 / 588.1 | 67.2 / 84.8 | 438.7 / 560.7 | 387.7 / 419.1 | 16.7 / 16.8 → 16.8 / 50.1 | 66.8 / 72.9 | 330 → 288 |
| table-10000-200 | 1086.1 / 1190.6 | 66.8 / 67.1 | 942.9 / 1044.7 | 740.4 / 752.5 | 283.4 / 300.0 → 16.7 / 16.8 | 116.0 / 116.0 | 6600 → 288 |

## Graph: mock baseline → final

| Nodes (~2× relationships) | First before → after | Stable before → after | Pan/zoom frame p50 | Pan/zoom frame p95 | Properties after | +50 pure merge after | +50 merge→paint after |
|---|---:|---:|---:|---:|---:|---:|---:|
| 100 | 49.0 / 347.1 → 56.1 / 349.1 | 998.9 / 1002.5 → 1263.4 / 1742.2 | 16.7 / 16.7 | 16.7 / 16.8 | 50.9 / 54.9 | 0.4 / 0.8 | 33.1 / 90.8 |
| 300 | 50.1 / 52.9 → 59.6 / 64.0 | 1003.6 / 1007.3 → 1522.5 / 1545.8 | 16.7 / 16.7 | 16.7 / 16.8 | 50.8 / 51.2 | 0.5 / 1.1 | 34.6 / 36.2 |
| 1000 | 78.2 / 122.5 → 66.4 / 246.5 | 1956.1 / 2800.2 → 2775.5 / 3039.8 | 16.7 / 16.7 | 16.7 / 16.8 | 78.9 / 83.3 | 1.0 / 1.6 | 33.9 / 34.8 |
| 5000 | 657.6 / 905.4 → 733.0 / 748.9 | 12190.1 / 14887.4 → 12888.4 / 13258.5 | 16.7 / 16.7 | 16.7 / 16.8 | 207.4 / 207.7 | 4.8 / 5.9 | 35.0 / 42.1 |

## Transitions

| Operation | p50 / p95 |
|---|---:|
| first-Table | 35.8 / 36.4 |
| first-Graph | 33.4 / 35.9 |
| return-Table | 32.7 / 36.4 |
| return-Graph | 33.4 / 36.0 |
| Result set switch | 33.3 / 35.5 |
| Workspace baseline: connect | 128.9 / 128.9 |
| Workspace baseline: newTab | 64.4 / 67.1 |
| Workspace baseline: tab | 64.2 / 67.8 |
| Workspace baseline: addConnection | 93.0 / 94.1 |

Query tabs already kept their components mounted before W4. W4 adds active propagation so hidden graph Workers and rAF painting stop; view/result-set panes now mount on first use and remain mounted. DOM verification asserts table scroll and canvas identity survive re-entry.

## Technique ledger

| Technique | Measured evidence | Decision |
|---|---|---|
| First-200/next-2000 columnar mock Worker stream | 100k×10 first 449.8 / 588.1 → 67.2 / 84.8 | Keep combined pipeline; native evidence below isolates transport |
| Uniform tag + value vectors, mixed-column tagged fallback | 36,644,862 → 19,945,132 bytes; JSON parse vs parse+decode 113.4 / 173.0 → 70.4 / 130.6 | Keep; exact Int/Decimal/JSON strings preserved |
| Both-axis virtualization + transform row pool + rAF batching | Wide fixture 6,600 → final visible cell count above; intermediate table-only results in results-viewer-table.json | Keep as a combined rendering change; separate speed attribution to pooling and rAF is not established |
| Visible-cell formatting cache (bounded at 4,096 entries) | 10,000 repeated JSON formats 6.9 / 8.7 → 0.2 / 0.5 | Keep; microbenchmark, not whole-table latency |
| Quadtree picking | 10,000 picks at 5,000 nodes 172.4 / 254.6 → 7.2 / 10.3; build 2.2 / 5.6 | Keep; coincident-node/empty/nearest regression tests |
| Incremental seeding and fixed old nodes | +50 nodes seeded in 0.3 / 0.6; existing node positions moved: 0, 0, 0 | Keep, shared mergeGraph remains unchanged |
| d3-force Worker + Float32 transfer + culling/LOD/color batching | First/stable/frame numbers above; positions move off main thread, Canvas handles paint and index updates | Keep combined renderer; large graph stable time remains a bottleneck |
| Fixed 16ms Worker yield | results-viewer-worker16.json compared with final; slows small-layout settling | Removed fixed delay; yield with zero-delay tasks |
| WebGL | Not implemented or benchmarked | Canvas retained; no unsupported claim that WebGL is faster |

## Native application (real database + WKWebView)

Environment: {"at":"2026-10-15T04:52:13.698Z","mode":"Tauri debug / WKWebView","ports":[55432,57687],"userAgent":"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)"}. Complete: false. {"detail":null,"kind":"Connection","message":"Session disconnected; reconnect required","position":null}

| Scenario / transport | First p50/p95 | Loaded p50/p95 | Stable p50/p95 | Interaction frame p50 | Interaction frame p95 | Properties p50/p95 | Expand request→paint | Pure merge | Added nodes |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| table-1000-10-legacy | 66.0 / 101.0 | 44.0 / 61.0 | unavailable | 33.0 / 33.0 | 37.0 / 49.0 | unavailable | unavailable | unavailable | —,—,— |
| table-1000-10-stream | 264.0 / 330.0 | 389.0 / 420.0 | unavailable | 33.0 / 33.0 | 42.0 / 44.0 | unavailable | unavailable | unavailable | —,—,— |
| table-10000-10-legacy | 516.0 / 532.0 | 481.0 / 511.0 | unavailable | 33.0 / 33.0 | 34.0 / 34.0 | unavailable | unavailable | unavailable | —,—,— |
| table-10000-10-stream | 214.0 / 336.0 | 562.0 / 653.0 | unavailable | 33.0 / 33.0 | 35.0 / 36.0 | unavailable | unavailable | unavailable | —,—,— |
| table-10000-200-legacy | 23319.0 / 34367.0 | 23206.0 / 34108.0 | unavailable | 34.0 / 36.0 | 145.0 / 305.0 | unavailable | unavailable | unavailable | —,—,— |
| table-10000-200-stream | 1784.0 / 1817.0 | 7639.0 / 10288.0 | unavailable | 33.0 / 33.0 | 35.0 / 45.0 | unavailable | unavailable | unavailable | —,—,— |
| table-100000-10-legacy | 3167.0 / 15546.0 | 3121.0 / 15479.0 | unavailable | 33.0 / 33.0 | 43.0 / 52.0 | unavailable | unavailable | unavailable | —,—,— |
| table-100000-10-stream | 1531.0 / 1583.0 | 6901.0 / 8725.0 | unavailable | 33.0 / 33.0 | 37.0 / 40.0 | unavailable | unavailable | unavailable | —,—,— |

## Target accounting

| Target | Result |
|---|---|
| 100k first row incl mock generation/transport <150ms | 84.80 ms, **met** |
| Scroll p95 frame <16.7ms | 50.10 ms, **NOT met** |
| Table↔Graph re-entry double-rAF <16ms | 36.40 ms, **NOT met** |
| 300-node first frame <100ms | 64.00 ms, **met** |
| +50 pure graph merge <50ms | 5.90 ms, **met** |

## Limits, failures, and verification

- The host currently chunks completed driver results. It reduces IPC/parse/paint blocking, but first-row latency still includes database execution and collection. No database-level sink/FETCH streaming was added; PG keeps its existing bounded FETCH implementation.
- Sort/filter operates on loaded row indices without copying original rows; expensive filtering still runs on the main thread. Integers and finite decimals sort without JS-number conversion.
- Baseline results contain the pre-existing unmount ResizeObserver null-element error. Final results must have no page errors; the callback now checks that its container still exists. Invalid hot-reload attempts are retained as diagnostic JSON and excluded from final comparison.
- Graph captions/arrows are suppressed below zoom .6; viewport culling and same-color paths reduce drawing. The default node limit remains 300 and UI maximum 5,000. +50 at the 5,000 display ceiling merges data but preserves the display cap.
- No browser result proves native/provider performance. Native app-mode test uses generated PG rows without writes and run-specific DbtP_ labels for Neo4j, cleaned in finally; it creates/removes only its temporary profiles. No user database ports 5432/5439/32802 were used.
- No commits were created by W4. W3-owned agent/history/security changes are preserved.

Commands: pnpm --dir ui build; pnpm --dir ui exec tsc --noEmit; pnpm --dir ui test; CARGO_TARGET_DIR=target-w4 cargo test --workspace (isolated test DB environment); CARGO_TARGET_DIR=target-w4 cargo clippy --workspace -- -D warnings; node bench/verify-viewer.mjs. See final worker handoff for the actual outcomes, rather than treating this command list as proof.

Reproduce: start pnpm --dir ui dev --port 1424; node bench/viewer-bench.mjs final; node bench/viewer-workspace.mjs after. Native: build UI and cargo build -p db-tool with CARGO_TARGET_DIR=target-w4; DBTOOL_BINARY=target-w4/debug/db-tool node bench/viewer-bench-app.mjs. Reports are written to a private temporary config directory and copied to results-viewer-app.json. get_perf_metrics reads window.dbtool.perf.marks(); run(scenario) is exercised in viewer-bench.html.

Primary API references: [Tauri channels](https://v2.tauri.app/develop/calling-frontend/#channels), [d3-force simulation](https://d3js.org/d3-force/simulation).
