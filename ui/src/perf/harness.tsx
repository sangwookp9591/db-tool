import { api } from '../api';
import { readGraphProbes } from './index';
import { render } from 'preact';
import { Results } from '../Results';
import { mergeGraph } from '../graph';
import { fixtureTable, fixtureGraph, fixtureResult } from './fixtures';
import { finish, frame, painted, perf, registerScenario, sampleFrames, start } from './index';
import '../style.css';
import { fixtureStream } from './fixtureStream';
const root = document.getElementById('app')!;
let benchmarkSession = '';
async function graphSession() { if (benchmarkSession) return; const drivers = await api('drivers_list', {}), profiles = await api('profiles_list', {}); const profile = profiles.find(p => drivers.some(d => d.id === p.driver_id && d.capabilities.graph_expand))!; benchmarkSession = await api('session_open', { profileId: profile.id, secret: null }); }
const error = (e: unknown) => { throw e; };
function mount(results: ReturnType<typeof fixtureResult>[], limit = 5000) { render(<Results results={results} sessionId={benchmarkSession} canExpand={!!benchmarkSession} graphLimit={limit} rowLimit={100000} onError={error} />, root); }
async function clear() { render(null, root); await frame(); perf.clear(); }
async function waitFor(selector: string) { const deadline = performance.now() + 30000; while (!document.querySelector(selector)) { if (performance.now() > deadline) throw new Error(`Timeout: ${selector}`); await frame(); } }
for (const [rows, columns] of [[1000, 10], [10000, 10], [100000, 10], [10000, 200]]) registerScenario(`table-${rows}-${columns}`, async () => {
  await clear(); const since = start('execute');
  let first = 0, firstPaint: Promise<number> | undefined;
  const loading = fixtureStream(rows, columns, results => {
    mount(results);
    if (!firstPaint && results[0]?.table?.rows.length) firstPaint = painted('first-row', since).then(duration => first = duration);
  });
  await loading; const loaded = finish('last-chunk', since); await firstPaint;
  const scroll = document.querySelector<HTMLElement>('.virtual-scroll')!;
  const frames = await sampleFrames(i => { scroll.scrollTop = i / 59 * scroll.scrollHeight; scroll.scrollLeft = i / 59 * scroll.scrollWidth; });
  return { first, loaded, frames, domCells: document.querySelectorAll('[role=gridcell]').length, heap: (performance as Performance & { memory?: { usedJSHeapSize: number } }).memory?.usedJSHeapSize ?? null };
});
for (const count of [100, 300, 1000, 5000]) registerScenario(`graph-${count}`, async () => {
  await clear(); await graphSession(); const graph = fixtureGraph(count); const since = start('graph-bench'); mount([fixtureResult(fixtureTable(10, 10), graph)]);
  await waitFor('canvas[data-ready=true]'); await painted('graph-visible', since);
  const layoutFrames = await sampleFrames(() => {});
  const deadline = performance.now() + 60000; while (!perf.marks().some(m => m.name === 'graph-stable')) { if (performance.now() > deadline) throw new Error('Layout failed to stabilize'); await frame(); }
  const canvas = document.querySelector('canvas')!;
  const frames = await sampleFrames(i => { canvas.dispatchEvent(new WheelEvent('wheel', { deltaY: i % 2 ? -20 : 20, clientX: 600, clientY: 300, bubbles: true, cancelable: true })); });
  const search = document.querySelector<HTMLInputElement>('[aria-label="Search graph"]')!;
  search.value = 'Node 0'; search.dispatchEvent(new Event('input', { bubbles: true })); await frame(); await frame();
  const rect = canvas.getBoundingClientRect();
  const clickStart = start('node-click'); canvas.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, clientX: rect.left + rect.width / 2, clientY: rect.top + rect.height / 2 })); await waitFor('.properties'); const click = await painted('properties', clickStart);
  const expandStart = start('expand-bench');
  (document.querySelector('.properties > button') as HTMLButtonElement).click();
  const expandDeadline = performance.now() + 60000; while (!perf.marks().some(m => m.name === 'expand-frame')) { if (performance.now() > expandDeadline) throw new Error('Expand timed out'); await frame(); }
  const merge = perf.marks().find(m => m.name === 'expand-merge')!.duration, expand = perf.marks().find(m => m.name === 'expand-frame')!.duration;
  return { first: perf.marks().find(m => m.name === 'graph-visible')?.duration, layout: perf.marks().find(m => m.name === 'graph-stable')?.duration ?? null, layoutFrames, frames, click, merge, expand, metrics: perf.marks() };
});
registerScenario('transitions', async () => {
  await clear(); const graphStart = start('first-graph-pane'); mount([fixtureResult(fixtureTable(1000, 10), fixtureGraph(300)), fixtureResult(fixtureTable(1000, 10))]); await waitFor('canvas[data-ready=true]');
  const samples: { name: string; duration: number }[] = [{ name: 'first-Graph', duration: await painted('first-graph-pane', graphStart) }];
  for (let i = 0; i < 10; i++) for (const name of ['Table', 'Graph']) {
    const since = start('view-switch'); const button = [...document.querySelectorAll<HTMLButtonElement>('[aria-label="Result views"] button')].find(b => b.textContent === name)!; button.click(); samples.push({ name: `${i === 0 && name === 'Table' ? 'first' : 'return'}-${name}`, duration: await painted('view-switch', since) });
  }
  const resultSwitch = [];
  for (let i = 0; i < 10; i++) { const since = performance.now(); (document.querySelectorAll<HTMLButtonElement>('.result-set:not([hidden]) [aria-label="Result sets"] button, .results > [aria-label="Result sets"] button')[(i + 1) % 2]).click(); resultSwitch.push(await painted('result-switch', since)); }
  return { samples, resultSwitch };
});
window.dispatchEvent(new Event('bench-ready'));
registerScenario('codec', async () => {
  const { packTable, unpackTable } = await import('./columnar');
  const table = fixtureTable(100000, 10), packed = packTable(table), raw = JSON.stringify(table), compact = JSON.stringify(packed);
  const rawParse = [], packedParse = [];
  for (let i = 0; i < 5; i++) { let t = performance.now(); JSON.parse(raw); rawParse.push(performance.now() - t); t = performance.now(); unpackTable(JSON.parse(compact)); packedParse.push(performance.now() - t); }
  return { rawBytes: new TextEncoder().encode(raw).length, packedBytes: new TextEncoder().encode(compact).length, rawParse, packedParse };
});
registerScenario('techniques', async () => {
  const { hitIndex } = await import('./spatial'); const { valueText } = await import('../values'); const { seedPositions } = await import('../graph');
  const points = Array.from({ length: 5000 }, (_, i) => ({ id: String(i), x: i % 100 * 50, y: Math.floor(i / 100) * 50 }));
  let t = performance.now(); const hit = hitIndex(points); const build = performance.now() - t;
  t = performance.now(); for (let i = 0; i < 10000; i++) { const p = points[i % 5000]; points.find(n => (n.x - p.x) ** 2 + (n.y - p.y) ** 2 < 21 ** 2); } const linear = performance.now() - t;
  t = performance.now(); for (let i = 0; i < 10000; i++) { const p = points[i % 5000]; if (hit(p.x, p.y, 21)?.id !== p.id) throw new Error('Incorrect quadtree hit'); } const quadtree = performance.now() - t;
  const value = { t: 'Json' as const, v: '{"large":9007199254740993123,"nested":[1,2,3,4,5],"name":"Long value"}' };
  t = performance.now(); for (let i = 0; i < 10000; i++) valueText(value); const uncached = performance.now() - t;
  const cache = new WeakMap(); t = performance.now(); for (let i = 0; i < 10000; i++) { if (!cache.has(value)) cache.set(value, valueText(value)); cache.get(value); } const cached = performance.now() - t;
  const graph = fixtureGraph(300), previous = graph.nodes.map((n, i) => ({ id: n.id, x: i, y: i }));
  const merged = mergeGraph(graph, fixtureGraph(50, 300, 'n0')); t = performance.now(); const seeded = seedPositions(merged, previous); const seedMs = performance.now() - t;
  return { hitTest: { linear, quadtree, build, queries: 10000 }, format: { uncached, cached, calls: 10000 }, incremental: { seedMs, movedExisting: seeded.slice(0, 300).filter((n, i) => n.x !== previous[i].x || n.y !== previous[i].y).length, newNodes: seeded.length - previous.length } };
});

registerScenario('graph-idle-300', async () => {
  await clear(); mount([fixtureResult(null, fixtureGraph(300))], 300); await waitFor('canvas[data-ready=true]'); while (!perf.marks().some(m => m.name === 'graph-stable')) await frame(); await frame(); await frame();
  let previous = readGraphProbes()[0], movement = 0, comparisons = 0; const startFrames = previous.frames, until = performance.now() + 5000;
  while (performance.now() < until) { await frame(); const next = readGraphProbes()[0]; for (let i = 0; i < previous.positions.length; i += 2) { movement += Math.hypot(previous.positions[i] - next.positions[i], previous.positions[i + 1] - next.positions[i + 1]); comparisons++; } previous = next; }
  return { duration: 5000, nodes: previous.positions.length / 2, averageMovementPx: comparisons ? movement / comparisons : 0, renderFrames: previous.frames - startFrames };
});

registerScenario('graph-motion-300', async () => {
  await clear(); mount([fixtureResult(null, fixtureGraph(300))], 300);
  let previous: ReturnType<typeof readGraphProbes>[number] | undefined, movement = 0, samples = 0;
  const until = performance.now() + 5000;
  while (performance.now() < until) { await frame(); const next = readGraphProbes()[0]; if (previous?.ready && next?.ready && previous.positions.length === next.positions.length) for (let i = 0; i < previous.positions.length; i += 2) { movement += Math.hypot(previous.positions[i] - next.positions[i], previous.positions[i + 1] - next.positions[i + 1]); samples++; } previous = next; }
  return { duration: 5000, averageVisibleMovementPx: samples ? movement / samples : 0, renderFrames: previous?.frames ?? 0 };
});

registerScenario('reset', async () => { await clear(); return {}; });
registerScenario('graph-probe', async () => readGraphProbes().map(p => ({ ...p, positions: Array.from(p.positions) })));
registerScenario('graph-limit', async () => { await clear(); await graphSession(); mount([fixtureResult(null, fixtureGraph(300))], 300); await waitFor('canvas[data-ready=true]'); });

for (const count of [300, 1000, 5000]) registerScenario(`graph-panzoom-${count}`, async () => {
  await clear(); mount([fixtureResult(null, fixtureGraph(count))], count); await waitFor('canvas[data-ready=true]'); await frame(); await frame();
  const { graphInteraction } = await import('./graphInteraction');
  return { ...(await graphInteraction(document.querySelector('canvas')!)), metrics: perf.marks() };
});
