# Viewer performance — 2026-10-15

All durations are milliseconds; cells show p50 / p95 unless stated otherwise. Three repetitions per scenario, 60 rAF intervals per interaction, and nearest-rank percentiles. Frame triples use median run p50, worst run p95 and worst max. A three-sample p95 is the maximum, not a population guarantee.

## Environment and method

- {"os":"darwin 27.0.0","cpu":"Apple M5","arch":"arm64","browser":"156.0.8078.4","mode":"Vite dev / headless Chromium","viewport":"1440x1000"}
- Native: Tauri debug binary, embedded Vite production assets, macOS WKWebView; only PG 127.0.0.1:55432 and Neo4j bolt://127.0.0.1:57687. Native hardware is the same host.
- Paint is a double-rAF proxy requested by the spec, not a compositor timestamp. On this 60 Hz environment it normally costs about 33 ms, making the strict <16 ms switch target unattainable by that definition.
- Mock baseline generates, serializes and parses the whole deterministic fixture on the main thread before mounting. Final mock streams the identical fixture from a Worker, first 200 rows then 2,000, using columnar JSON. Native compares legacy execute vs real Tauri Channel on the same optimized viewer. These measure different layers and are reported separately.
- Baseline/source starts at UI before W4. Immutable temporary UI copies isolated baseline and candidate from other workers' HMR. Development builds and shared-machine background load limit causal attribution of small deltas; no hardware-wide speed guarantee.
- JS heap is Chromium usedJSHeapSize, after loading/scroll; it is not process RSS or a peak. WKWebView does not expose that heap API.

## Table: mock baseline → final

| Rows × columns | First before | First after | Load before | Load after | Scroll frames p50 / p95 / max before → after | Final heap MiB p50/p95 | Cells before → after |
|---|---:|---:|---:|---:|---:|---:|---:|
| table-1000-10 | 33.3 / 51.3 | 32.8 / 167.3 | 4.8 / 6.0 | 26.0 / 165.9 | 16.7 / 16.8 / 33.3 → 16.7 / 16.8 / 33.4 | 4.7 / 4.8 | 330 → 288 |
| table-10000-10 | 66.7 / 67.0 | 50.5 / 97.8 | 43.4 / 45.1 | 64.5 / 110.4 | 16.7 / 16.8 / 16.8 → 16.7 / 16.8 / 16.8 | 9.8 / 11.0 | 330 → 288 |
| table-100000-10 | 449.8 / 588.1 | 50.2 / 115.9 | 438.7 / 560.7 | 446.6 / 473.4 | 16.7 / 16.8 / 16.8 → 16.7 / 16.7 / 50.0 | 67.0 / 72.6 | 330 → 288 |
| table-10000-200 | 1086.1 / 1190.6 | 74.4 / 116.9 | 942.9 / 1044.7 | 920.1 / 1001.1 | 133.3 / 300.0 / 333.3 → 16.7 / 16.8 / 49.9 | 115.7 / 115.8 | 6600 → 288 |

## Graph: mock baseline → final

| Nodes (~2× relationships) | First before → after | Stable before → after | Pan/zoom frame p50 | Pan/zoom frame p95 | Properties after | +50 pure merge after | +50 merge→paint after |
|---|---:|---:|---:|---:|---:|---:|---:|
| 100 | 49.0 / 347.1 → 49.7 / 355.4 | 998.9 / 1002.5 → 430.6 / 522.5 | 16.7 / 16.7 | 16.7 / 16.8 | 49.4 / 51.2 | 0.4 / 0.7 | 34.3 / 34.7 |
| 300 | 50.1 / 52.9 → 50.8 / 51.0 | 1003.6 / 1007.3 → 722.0 / 729.3 | 16.7 / 16.7 | 16.7 / 16.7 | 52.1 / 52.2 | 0.5 / 0.6 | 31.3 / 33.1 |
| 1000 | 78.2 / 122.5 → 48.4 / 50.9 | 1956.1 / 2800.2 → 2085.0 / 2355.7 | 16.7 / 16.7 | 16.8 / 16.8 | 49.9 / 50.1 | 0.8 / 2.0 | 34.4 / 35.5 |
| 5000 | 657.6 / 905.4 → 105.5 / 112.8 | 12190.1 / 14887.4 → 12189.0 / 12321.0 | 16.7 / 16.7 | 16.7 / 16.7 | 49.8 / 50.7 | 4.2 / 4.4 | 32.7 / 33.5 |

| Nodes | Frame p95 during active layout, before → after |
|---|---:|
| 100 | 16.8 / 16.8 → 16.8 / 16.8 |
| 300 | 16.8 / 16.8 → 16.7 / 16.8 |
| 1000 | 50.0 / 50.0 → 16.8 / 16.8 |
| 5000 | 216.7 / 250.0 → 16.8 / 33.3 |

## Transitions

| Operation | p50 / p95 |
|---|---:|
| first-Table | 33.5 / 34.3 |
| first-Graph | 49.4 / 63.7 |
| return-Table | 33.0 / 36.3 |
| return-Graph | 33.5 / 35.9 |
| Result set switch | 33.2 / 36.4 |
| Workspace baseline: connect | 189.0 / 338.1 |
| Workspace baseline: newTab | 65.8 / 171.4 |
| Workspace baseline: tab | 63.0 / 255.8 |
| Workspace baseline: addConnection | 163.8 / 338.5 |
| Workspace after: connect | 259.9 / 507.7 |
| Workspace after: newTab | 81.5 / 175.1 |
| Workspace after: tab | 64.6 / 265.0 |
| Workspace after: addConnection | 275.1 / 313.7 |

Query tabs already kept their components mounted before W4. W4 adds active propagation so hidden graph Workers and rAF painting stop; view/result-set panes now mount on first use and remain mounted. DOM verification asserts table scroll and canvas identity survive re-entry.

## Technique ledger

| Technique | Measured evidence | Decision |
|---|---|---|
| First-200/next-2000 columnar mock Worker stream | 100k×10 first 449.8 / 588.1 → 50.2 / 115.9 | Keep combined pipeline; native evidence below isolates transport |
| Uniform tag + value vectors, mixed-column tagged fallback | 36,644,862 → 19,945,132 bytes; JSON parse vs parse+decode 128.4 / 191.9 → 84.5 / 156.0 | Keep; exact Int/Decimal/JSON strings preserved |
| Both-axis virtualization + transform row pool + rAF batching | Wide fixture 6,600 → final visible cell count above; intermediate table-only results in results-viewer-table.json | Keep as a combined rendering change; separate speed attribution to pooling and rAF is not established |
| Visible-cell formatting cache (bounded at 4,096 entries) | 10,000 repeated JSON formats 6.8 / 8.5 → 0.2 / 0.4 | Keep; microbenchmark, not whole-table latency |
| Quadtree picking | 10,000 picks at 5,000 nodes 68.7 / 170.6 → 4.8 / 7.3; build 0.8 / 1.7 | Keep; coincident-node/empty/nearest regression tests |
| Incremental seeding and fixed old nodes | +50 nodes seeded in 0.2 / 0.9; existing node positions moved: 0, 0, 0 | Keep, shared mergeGraph remains unchanged |
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
| 100k first row incl mock generation/transport <150ms | 115.90 ms, **met** |
| Scroll p95 frame <16.7ms | 16.80 ms, **NOT met** |
| Table↔Graph re-entry double-rAF <16ms | 36.30 ms, **NOT met** |
| 300-node first frame <100ms | 51.00 ms, **met** |
| +50 pure graph merge <50ms | 4.40 ms, **met** |

## Limits, failures, and verification

- The host currently chunks completed driver results. It reduces IPC/parse/paint blocking, but first-row latency still includes database execution and collection. No database-level sink/FETCH streaming was added; PG keeps its existing bounded FETCH implementation.
- Sort/filter operates on loaded row indices without copying original rows; expensive filtering still runs on the main thread. Integers and finite decimals sort without JS-number conversion.
- Baseline results contain the pre-existing unmount ResizeObserver null-element error. Final results must have no page errors; the callback now checks that its container still exists. Invalid hot-reload attempts are retained as diagnostic JSON and excluded from final comparison.
- Graph captions/arrows are suppressed below zoom .6; viewport culling and same-color paths reduce drawing. The default node limit remains 300 and UI maximum 5,000. +50 at the 5,000 display ceiling merges data but preserves the display cap.
- The mock goal uses the dedicated benchmark fixture Worker; the ordinary demo backend still constructs its legacy fixture before host-style chunk delivery. The report does not claim that the demo SELECT large has the same first-row latency.
- Native adds 52 nodes at 100/300/1,000 (50 fixture nodes plus two existing incoming neighbors outside the loaded graph), and 50 at 5,000. Mock expansion is exactly +50. Node selection uses graph search outside timing then a canvas pointer-up through the hit-test into properties.
- First-Graph means first pane mount in that run; its JavaScript module may already be cached from earlier scenarios. Native workspace-tab/create/connection-form transitions were not separately automated; the full workspace transition matrix above is Chromium mock.
- No browser result proves native/provider performance. Native app-mode test uses generated PG rows without writes and run-specific DbtP_ labels for Neo4j, cleaned in finally; it creates/removes only its temporary profiles. No user database ports 5432/5439/32802 were used.
- No commits were created by W4. W3-owned agent/history/security changes are preserved.

Commands: pnpm --dir ui build; pnpm --dir ui exec tsc --noEmit; pnpm --dir ui test; CARGO_TARGET_DIR=target-w4 cargo test --workspace (isolated test DB environment); CARGO_TARGET_DIR=target-w4 cargo clippy --workspace -- -D warnings; node bench/verify-viewer.mjs. See final worker handoff for the actual outcomes, rather than treating this command list as proof.

Reproduce: start pnpm --dir ui dev --port 1424; node bench/viewer-bench.mjs final; node bench/viewer-workspace.mjs after. Native: build UI and cargo build -p db-tool with CARGO_TARGET_DIR=target-w4; DBTOOL_BINARY=target-w4/debug/db-tool node bench/viewer-bench-app.mjs. Reports are written to a private temporary config directory and copied to results-viewer-app.json. get_perf_metrics reads window.dbtool.perf.marks(); run(scenario) is exercised in viewer-bench.html.

Primary API references: [Tauri channels](https://v2.tauri.app/develop/calling-frontend/#channels), [d3-force simulation](https://d3js.org/d3-force/simulation).
