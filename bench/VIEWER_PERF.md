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
- Source grep for database IDs in ui/src excluding mock/tests returns0. Benchmark fixture values live in bench/fixtures.json and are delivered by the gated host command. Native harness is dynamically imported only when AXDB_BENCH=1; the ordinary app does not auto-run it. Forced always-on-top/focus was removed after the user observed the visible automated transitions.
- Host chunking begins after the driver finishes collecting; server execution/collection remains in first-row latency. PG already uses bounded FETCH, but no new dbcore sink or database-level result streaming was added. Large arrays/JSON construction and serializing2million wide-table cells remain expensive.
- Main-thread index filtering and Canvas painting at5000nodes remain limits. Converged-only first frame deliberately adds layout latency to avoid visible jitter. Small-load/graph first and scrolling targets are reported honestly above. Expansion full-paint includes actual database roundtrip; pure merge is separately measured.
- No commits or production database connections were made by W4. Final native retry made no progress for over120s with app CPU0, consistent with WKWebView rAF suspension when occluded, but that cause was not independently proven; it was stopped instead of repeatedly stealing focus. Its results-viewer-app-matrix-interrupted.json is explicitly incomplete, excluded from final comparisons, and had not reached graph fixture creation. Re-run final native matrix with a continuously visible dedicated test window to verify the last table geometry change. Historical failed/superseded/regression JSON files remain diagnostic and are excluded from final paired comparisons. Early test runs encountered the W7a-owned PG cancellation race and incomplete vendor snippet checkout; the final workspace run is authoritative. Coordinator’s21/21 bridge E2E is separate evidence; W4 does not infer it from these viewer benchmarks.

Reproduce browser: prepare immutable before/after UI trees on1425/1426 with the same harness and old/new viewer implementations, then node bench/viewer-bench-paired.mjs. The baseline starts from original viewer components in b36cafe with measurement hooks and the unmount observer guard; final graph/table/limit runs use OUTPUT_SUFFIX=final-graph/final-table/final-limit and SCENARIOS subsets, merged explicitly by summarize-viewer.mjs. Older current-load raw files supply codec/technique measurements; no failed sample is silently mixed in. Main-app DOM: UI dev on1424, node bench/verify-viewer.mjs. Basic mock runner: node bench/viewer-bench.mjs NAME.

Native: pnpm --dir ui build; TAURI_CONFIG='{"build":{"devUrl":null}}' CARGO_TARGET_DIR=target-w4 cargo build -p axdb; AXDB_BINARY=target-w4/debug/axdb node bench/viewer-bench-app.mjs [matrix|small|graph|workspace|idle]. No argument runs the complete matrix, separate processes are serialized by a lock. AXDB_CONFIG_DIR points to a private temporary directory and bridge is disabled. Each exact DbtP_ label and temporary profile is removed in finally. Reports retain uptime and raw samples; native graphics tests require a visible app window.

Implementation references: [Tauri Channels](https://v2.tauri.app/develop/calling-frontend/#channels), [d3-force simulation](https://d3js.org/d3-force/simulation).

## W4b 팬·줌

2026-10-15. 시작 기준은 `8c0bb73`, 전체 시작 상태는 [viewer-w4b-baseline.txt](viewer-w4b-baseline.txt)에 보존했다. 실행 중 외부에서 HEAD가 `d92c751`로 이동했으며 이 워커는 커밋하지 않았다. before GraphView는 `d92c751`의 W4 정적 뷰어와 SHA-256이 일치한다. 기존 변경과 외부 문서 삭제는 보존했다.

### 프로파일 원인과 적용 범위

- 먼저 최적화 없이 단계별 `performance.measure`를 넣었다. [최초 프로파일](results-viewer-w4b-profile-before.json)에서 5,000노드 곡선 계산·culling p95 2.7ms, 전체 JS 그리기 호출 p95 4.5ms인데 프레임 간격 p95는 183.4ms였다. 측정한 기본 fit 배율에서는 라벨·화살표 분기가 실행되지 않았다. quadtree는 원래 좌표가 바뀔 때만 생성되며 팬·줌 경로에서 생성하지 않는다. 색상별 노드 일괄 그리기도 이미 유지되고 있었다.

- [Chromium before trace](viewer-w4b-trace-before.json.gz)에서 1,000노드 `LayerTreeHost::DoUpdateLayers`는 67회 합계 3,266.8ms, 최대 102.15ms였다. [after trace](viewer-w4b-trace-after-trace.json.gz)는 69회 합계 329.1ms, 최대 45.00ms였다. 벡터 Canvas의 표면 갱신 경로가 주된 병목이라는 판단을 뒷받침한다. trace는 진단용 개별 실행이며 아래 교대 비교와 표본을 섞지 않았다. [집계 및 소스 해시](results-viewer-w4b-summary.json), [after 단계 프로파일](results-viewer-w4b-profile-after-trace.json).

- **적용:** 입력 중 전체 장면을 한 번 그린 비트맵을 카메라 변환으로 표시하고, 마지막 입력 120ms 뒤 정밀 벡터 프레임을 한 번 그린다. 노드와 곡선 제어점의 전체 bounds로 캐시하므로 직전 viewport 밖으로 팬할 때 이전 화면의 잘린 스냅샷을 사용하지 않는다. 장면·좌표·검색·테마·크기 변경 시 캐시를 무효화하고 숨김·정리 시 타이머를 취소한다. 첫 표시와 expand는 기존 벡터 경로를 사용한다.

- **추가 확인 후 적용:** 검색의 카메라·강조 갱신을 `useLayoutEffect`에서 처리한다. 검색 재그리기가 속성 클릭 프레임에 겹쳤던 1,000노드 집중 비교에서 클릭 시간이 before 66.7~86.2ms, after 37.8~48.7ms였다([before](results-viewer-before-w4b-search.json), [after](results-viewer-after-w4b-search.json)). 최종 전체 측정은 이 변경을 포함한다. `fitGraph`는 많은 곡선 bounds도 인자 펼침 한도를 넘지 않도록 순회로 계산하며 200,000개 좌표 회귀 검사를 추가했다.

- **되돌린 최적화는 없음.** Path2D 캐시, 추가 LOD, 텍스트 폭 캐시는 도입하지 않았다. 곡선 계산·텍스트·quadtree가 측정된 프레임 지연의 주된 원인이 아니었고, 비트맵만으로 active-frame 목표가 충족됐다. 기존 culling·색상별 batching을 유지한다.

### 최종 Chromium 교대 실행

환경: `{"os": "darwin 27.0.0", "cpu": "Apple M5", "arch": "arm64", "browser": "156.0.8078.4", "viewport": "1440x1000", "mode": "Vite dev / headless Chromium / alternating paired runs"}`. DPR 1, 동일 deterministic mock graph와 동일 harness를 사용했다. 고정 before/after UI 복사본을 1425/1426에서 제공하고 매 반복의 순서를 before→after, after→before, before→after로 바꿨다. 60개 교대 wheel 입력의 rAF 간격을 쓰는 W4 방법이며, 이 숫자는 wheel/zoom proxy다. 실제 포인터 pan·드래그·hit-test는 별도 기능 검사로 확인했다. 각 원자료에 실행 직전 load average와 uptime을 기록했다.

[최종 before 원자료](results-viewer-before-w4b-accepted.json) · [최종 after 원자료](results-viewer-after-w4b-accepted.json). 아래 표는 이 두 complete=true 파일만 사용한다. 각 latency 셀은 3회 p50 / p95이고 p95는 3개 중 최대다.

| 노드 | 실행별 팬·줌 p95 before → after | 목표 | 판정 |
|---|---|---:|---|
| 300 | 33.4 / 33.4 / 33.3 → 16.8 / 16.8 / 16.7 | ≤16.8ms | 충족 |
| 1000 | 50.1 / 50.1 / 50.1 → 16.7 / 16.8 / 16.8 | ≤20ms | 충족 |
| 5000 | 150.1 / 150.0 / 166.7 → 16.8 / 16.8 / 16.8 | ≤50ms | 충족 |

| 노드 | 첫 표시 before → after | 속성 클릭 before → after | 순수 merge before → after | expand→첫 merged paint before → after |
|---|---|---|---|---|
| 300 | 195.00 / 229.70 → 180.80 / 200.10 | 50.00 / 50.00 → 48.80 / 49.00 | 0.20 / 0.20 → 0.20 / 0.20 | 52.40 / 58.40 → 52.80 / 53.50 |
| 1000 | 517.00 / 521.00 → 489.80 / 575.30 | 66.60 / 99.90 → 38.30 / 38.40 | 0.20 / 0.20 → 0.30 / 0.30 | 124.80 / 173.60 → 107.20 / 107.40 |
| 5000 | 2080.40 / 2221.60 → 2198.50 / 2217.90 | 168.10 / 202.00 → 41.80 / 42.10 | 1.20 / 1.40 → 1.00 / 1.20 | 312.60 / 351.70 → 301.60 / 331.40 |

**비회귀 전체 조건은 미달이다.** 1,000노드 첫 표시 p95가 521.0→575.3ms로 늘었고, 5,000노드 첫 표시 p50도 2,080.4→2,198.5ms로 늘었다. 1,000노드 순수 merge는 계측의 0.1ms 단위에서 0.2→0.3ms다. 가장 느린 1,000노드 after 표본의 Worker workMs는 360.5ms(before 같은 반복 308.9ms), layout wall은 505.8ms(before 437.1ms)였다. Worker 코드는 변경하지 않았지만 공유 호스트의 부하 변동만으로 회귀를 배제할 수 없으므로 무회귀라고 주장하지 않는다. 속성 클릭과 expand p95는 모든 크기에서 개선됐다.

**남은 상호작용 제한:** 5,000노드 after active-frame 최대는 150.0ms다. 첫 제스처의 캐시 생성 한 프레임과 입력 종료 후 정밀 rasterization 비용은 남아 있다. 깊게 줌하는 동안 fitted bitmap의 세부 정보가 일시적으로 흐려질 수 있고, 120ms idle 이후 정밀 그리기로 복원된다. p95 충족을 모든 프레임 60fps나 첫 제스처 지연 제거로 확대 해석하지 않는다.

### 흔들림·유휴·CLS와 실제 입력

- 최종 before/after 각 3회: 초기 표시 이후 보이는 노드 이동 평균 **0px**, 수렴 후 5초 이동 **0px**, 유휴 render frames **0**, CLS **0**, recent-input을 포함한 전체 layout-shift 합 **0**. 추가 `graph-panzoom-300/1000/5000`의 입력 종료 후 이동 및 유휴 render도 모두 **0**이었다.

- [실제 입력 검증](results-viewer-w4b-verification.json)은 complete=true, 페이지 오류 0, 7개 검사 통과다. trusted pan 좌표 보존, trusted 노드 드래그 후 고정, 클릭 선택, expand 병합, 테마·resize·DPR 2 갱신, 후속 유휴 0을 확인했다. DPR 2에서 delta=0 wheel을 거쳐 정밀 프레임으로 돌아왔을 때 픽셀 SHA-256이 기존 프레임과 일치했다. 이는 해당 경로의 픽셀 복원을 확인한 것으로 모든 줌 배율의 시각 품질 보장은 아니다. [before](viewer-w4b-before.png), [pan](viewer-w4b-pan.png), [dark](viewer-w4b-dark.png).

### WKWebView 및 CPU: 최종 검증 미완료

- Rust 파일 범위를 늘리지 않기 위해 기존 `AXDB_BENCH_TARGET=idle`에 300/1,000노드 각 3회 시나리오를 추가했다. 앱 내 `fixtureGraph`를 렌더하므로 **WKWebView의 실제 UI/Canvas 측정**이며 DB transport나 실제 DB 그래프의 성능 증거는 아니다. 이 타깃은 드라이버 세션이나 DB 연결을 생성하지 않는다. 환경은 Tauri debug의 임베드 production UI, `tauri://localhost`, WebKit/605.1.15, 1280×800 창이다.

- 검색 동기화 조정 전 [완료된 예비 실행](results-viewer-app-idle-w4b.json): 300노드 실행별 p95 **34 / 39 / 34ms**, 1,000노드 **34 / 34 / 34ms**. load average는 표본별 원자료에 있다. 300노드 12초 유휴 이동 0px, render 0이었으나 기존 `ps %cpu` 평균은 **2.62%**였다.

- macOS `man ps`에 따르면 `%cpu`는 최대 1분 전 활동의 감쇠 평균이다. 앞선 제스처가 섞일 수 있어 누적 `ps TIME`의 실제 구간 차분을 추가했다. [중간 CPU 재측정](results-viewer-app-idle-w4b-interval.json)의 차분 값은 0.69%, 기존 감쇠 평균은 0.05%였지만 CPU 측정 시간이 14.49초로 그래프 유휴 보유 12초를 넘었다. **이 차분 값을 CPU 충족 증거에서 제외한다.**

- 최종 harness는 그래프 유휴 보유를 30초로 늘리고, CPU 구간이 그 안에 완전히 들어갔는지 assert한다. 기존 `meanPercent` 의미는 유지하고 `intervalMeanPercent`에 차분 CPU를 기록한다. parser assert 2개와 JS 문법 검사는 통과했으나 최신 네이티브 실측은 이 검사까지 도달하지 못했다.

- 최신 바이너리 [불완전 재실행](results-viewer-app-idle-w4b-accepted-interrupted.json)은 300노드 2개 표본 이후 120초 넘게 진행이 멈췄다. 앱 CPU 0.0%, focused UI 없음, window screenIndex null이 관측됐다. WKWebView 가시성/occlusion에 따른 rAF 중단이 의심되지만 원인을 확정하지 못했다. 이 프로세스를 종료했고 runner는 `Native app exited before benchmark completion`으로 실패했다. **최신 WKWebView 전체 300/1,000노드 비교와 최종 유휴 CPU <1%는 검증 미완료**다. 앞선 성공 실행이나 멈춘 앱의 CPU 0%를 최종 성공으로 사용하지 않는다. 안정적으로 보이는 전용 창에서 재실행해야 한다.

### 검사·재현·판정

- `pnpm --dir ui exec tsc --noEmit` 통과, Vitest **34/34**, `pnpm --dir ui build` 통과. `TAURI_CONFIG='{"build":{"devUrl":null}}' CARGO_TARGET_DIR=target-w4b cargo build -p axdb` 통과. 실제 포인터 브라우저 검사 7/7 통과. `git diff --check` 통과. UI의 DB driver id 리터럴 분기 grep은 mock/tests 제외 **0**이다. 전체 Rust/DB 통합 테스트는 실행하지 않았다.

- Chromium: `/tmp/axdb-w4b/before-ui`와 `after-ui`는 동일 harness이며 before는 W4 GraphView/graph.ts, after는 최종 renderer다. 두 Vite 서버를 1425/1426에 실행하고 `SCENARIOS=graph-300,graph-1000,graph-5000,graph-panzoom-300,graph-panzoom-1000,graph-panzoom-5000,graph-idle-300,graph-motion-300 REPEATS=3 OUTPUT_SUFFIX=w4b-accepted node bench/viewer-bench-paired.mjs`를 실행한다. source 해시는 [집계 파일](results-viewer-w4b-summary.json)에 있다. 기본 W4 `graph-N` 시나리오를 유지했고 별도 전후 immutable 트리로 비교했다.

- trace: `TRACE=1 PORT=1426 PHASE=after-trace node bench/viewer-w4b-profile.mjs`; before 포트 1425에서도 trace를 재수집할 수 있다. `graphProfile` 쿼리는 최종 GraphView의 단계별 계측을 켠다. 기능 검사: `node bench/verify-viewer-w4b.mjs`(1426, DPR 2).

- 네이티브: 위 build 뒤 `AXDB_BINARY=target-w4b/debug/axdb OUTPUT_SUFFIX=w4b-accepted node bench/viewer-bench-app.mjs idle`. 창을 계속 보이게 유지해야 한다. 프로필은 private temp dir, bridge는 꺼짐이며 이 idle 타깃에서 DB를 사용하지 않는다. `OUTPUT_SUFFIX`는 기존 W4 결과를 덮어쓰지 않도록 추가했다. `w4b`, `w4b-final`, `w4b-search`, `profile-bitmap` 원자료는 중간 진단이며 최종 Chromium 표에 섞지 않았다.

결과: Chromium active-frame 목표 3개와 이동·유휴·CLS 0은 확인했다. 첫 표시의 완전한 비회귀와 최신 네이티브/유휴 CPU 조건은 남아 있다.
