import type { BenchConfig } from './benchConfig';
import { render } from 'preact';
import { invoke } from '@tauri-apps/api/core';
import { api } from '../api';
import type { ExecRequest, ResultSet } from '../contract';
import { Results } from '../Results';
import { executeQuery } from './stream';
import { finish, frame, painted, perf, sampleFrames, start } from './index';
async function waitFor(test: () => boolean, timeout = 60000) { const until = performance.now() + timeout; while (!test()) { if (performance.now() > until) throw new Error('Native benchmark timed out'); await frame(); } }
async function run() {
  const resourceTarget = await invoke<string>('viewer_bench_target');
  if (resourceTarget === 'd9') { await (await import('./d9')).d9Bench(); return; }
  if (resourceTarget === 'd7') { await (await import('./d7')).d7Bench(); return; }
  if (resourceTarget === 'connections') { await (await import('./connections')).connectionBench(); return; }
  if (resourceTarget.startsWith('resource')) { await (await import('./resource')).resourceBench(resourceTarget === 'resource-leak'); return; }
  const root = document.createElement('div'); root.style.cssText = 'position:fixed;inset:0;z-index:10000;display:flex;background:var(--surface)'; document.body.append(root);
  const manifests = await api('drivers_list', {});
  const config = await invoke<BenchConfig>('viewer_bench_config');
  const target = await invoke<string>('viewer_bench_target');
  const drivers = target === 'workspace' || target === 'idle' ? [] : [manifests.find(d => d.paradigm === 'Relational' && d.connection_fields.some(f => f.key === 'host')), manifests.find(d => d.paradigm === 'Graph')].filter(d => !['graph', 'small'].includes(target) || d?.paradigm === 'Graph');
  const output: Record<string, unknown> = { environment: { href: location.href, userAgent: navigator.userAgent, mode: 'Tauri debug / WKWebView', targets: [config.table.fields.host + ':' + config.table.fields.port, config.graph.fields.uri], target, at: new Date().toISOString() }, results: {} };
  const results = output.results as Record<string, unknown>;
  let last: ResultSet[] = []; let viewerSession = '';
  const label = `DbtP_${Date.now()}`; output.fixtureLabel = label;
  const req = (text: string, limit = 100000): ExecRequest => ({ text, params: {}, row_limit: 100000, graph_node_limit: limit, timeout_ms: 60000 });
  const mount = (r: ResultSet[], limit = 5000) => render(<Results results={r} active sessionId={viewerSession} canExpand={r.some(result => !!result.graph)} graphLimit={limit} rowLimit={100000} onError={e => { output.error = String(e); }} />, root);
  async function clear() { render(null, root); await frame(); perf.clear(); }
  async function query(sessionId: string, request: ExecRequest, mode: string) {
    viewerSession = sessionId; await clear(); const since = start('native-execute'); let first: Promise<number> | undefined;
    const update = (r: ResultSet[]) => { last = r; mount(r); if (!first && r.some(r => r.table?.rows.length || r.graph?.nodes.length)) first = r.some(result => result.graph) ? waitFor(() => !!root.querySelector('canvas[data-ready=true]')).then(() => painted('native-first', since)) : painted('native-first', since); };
    if (mode === 'stream') await executeQuery({ sessionId, runId: crypto.randomUUID(), req: request }, update);
    else update(await api('execute', { sessionId, runId: crypto.randomUUID(), req: request }));
    const loaded = finish('native-loaded', since); return { first: await first, loaded, driverElapsed: last.reduce((sum, result) => sum + result.elapsed_ms, 0) };
  }
  output.stage = 'ready'; await invoke('viewer_bench_write', { report: output });
  try {
    for (const driver of drivers) {
      if (!driver) throw new Error('Required test driver unavailable');
      const graph = driver.paradigm === 'Graph';
      const profile = await api('profile_save', { profile: { id: crypto.randomUUID(), name: graph ? 'Viewer test graph' : 'Viewer test table', driver_id: driver.id, fields: { ...Object.fromEntries(driver.connection_fields.filter(f => !f.secret && f.default !== null).map(f => [f.key, f.default!])), ...(graph ? config.graph : config.table).fields }, save_secret: false }, secret: null });
      let sessionId: string | undefined;
      try {
        sessionId = await api('session_open', { profileId: profile.id, secret: (graph ? config.graph : config.table).secret });
        if (graph) {
          await api('execute', { sessionId, runId: crypto.randomUUID(), req: req(`UNWIND range(0,4999) AS i CREATE (:${label} {i:i, name:'Node '+toString(i)})`) });
          await api('execute', { sessionId, runId: crypto.randomUUID(), req: req(`MATCH (n:${label}) WITH n ORDER BY n.i WITH collect(n) AS nodes UNWIND range(0,4999) AS i WITH nodes[i] AS a, nodes[(i+1)%5000] AS b, nodes[(i+7)%5000] AS c CREATE (a)-[:DbtP_LINK]->(b), (a)-[:DbtP_LINK]->(c)`) });
          await api('execute', { sessionId, runId: crypto.randomUUID(), req: req(`MATCH (a:${label} {i:0}) UNWIND range(5000,5049) AS i CREATE (a)-[:DbtP_EXPAND]->(:${label} {i:i, name:'Expansion '+toString(i)})`) });

        }
        if (graph) {
          output.smallQuery = [];
          for (let i = 0; i < 5; i++) {
            const text = `MATCH (n:${label}) WHERE n.i < 4 RETURN n`, request = req(text, 300), load = await invoke<string | null>('viewer_bench_load');
            let t = performance.now(); const agent = await api('execute_agent', { sessionId, runId: crypto.randomUUID(), req: { ...request, read_only: true } }); const agentWall = performance.now() - t;
            if (i === 0) await clear(); else perf.clear(); viewerSession = sessionId; t = performance.now(); const received = await executeQuery({ sessionId, runId: crypto.randomUUID(), req: request }, value => mount(value, 300)); const uiWall = performance.now() - t;
            await waitFor(() => !!root.querySelector('canvas[data-ready=true]')); const firstPaint = await painted('small-query-first', t);
            (output.smallQuery as unknown[]).push({ cold: i === 0, load, rows: received[0].row_count, agentWall, agentDriver: agent[0].elapsed_ms, uiWall, uiDriver: received[0].elapsed_ms, firstPaint });
          }
        }
        const scenarios = target === 'small' ? [] : graph ? [100, 300, 1000, 5000].map(n => [n, 0]) : [[1000, 10], [10000, 10], [100000, 10], [10000, 200]];
        for (const [count, width] of scenarios) for (let repeat = 0; repeat < 3; repeat++) {
          for (const mode of repeat % 2 ? ['stream', 'legacy'] : ['legacy', 'stream']) {
              const text = graph ? `MATCH (a:${label})-[r:DbtP_LINK]->(b:${label}) WHERE a.i < ${count} AND b.i < ${count} RETURN a,r,b LIMIT ${count * 2}` : `SELECT ${Array.from({ length: width }, (_, c) => c % 2 ? `'row ' || i::text AS c${c}` : `9007199254740993::bigint+i AS c${c}`).join(',')} FROM generate_series(1,${count}) i`;
              const load = await invoke<string | null>('viewer_bench_load');
              const sample: Record<string, unknown> = await query(sessionId, req(text, graph ? count : 300), mode);
              sample.load = load;
              if (graph) {
                await waitFor(() => perf.marks().some(m => m.name === 'graph-stable'));
                const canvas = root.querySelector('canvas')!;
                sample.layout = perf.marks().find(m => m.name === 'graph-stable')?.duration;
                sample.frames = await sampleFrames(i => canvas.dispatchEvent(new WheelEvent('wheel', { deltaY: i % 2 ? 20 : -20, clientX: 500, clientY: 300, bubbles: true, cancelable: true })));
                const search = root.querySelector<HTMLInputElement>('[aria-label="Search graph"]')!;
                search.value = 'Node 0'; search.dispatchEvent(new Event('input', { bubbles: true })); await frame(); await frame();
                const rect = canvas.getBoundingClientRect(), click = performance.now();
                canvas.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, clientX: rect.left + rect.width / 2, clientY: rect.top + rect.height / 2 }));
                await waitFor(() => !!root.querySelector('.properties')); sample.click = await painted('native-properties', click);
                const originalCount = last[0].graph!.nodes.length;
                (root.querySelector('.properties > button') as HTMLButtonElement).click();
                await waitFor(() => perf.marks().some(m => m.name === 'expand-frame'));
                sample.merge = perf.marks().find(m => m.name === 'expand-merge')?.duration;
                sample.expand = perf.marks().find(m => m.name === 'expand-frame')?.duration;
                const total = canvas.dataset.totalNodes ?? root.querySelector('.warning')?.textContent?.match(/^(\d+)/)?.[1] ?? canvas.getAttribute('aria-label')!.match(/Graph: (\d+)/)![1];
                sample.addedNodes = Number(total) - originalCount;
              } else { const el = root.querySelector<HTMLElement>('.virtual-scroll')!; sample.frames = await sampleFrames(i => { el.scrollTop = i / 59 * el.scrollHeight; el.scrollLeft = i / 59 * el.scrollWidth; }); }
            const key = `${graph ? 'graph' : 'table'}-${count}-${width}-${mode}`;
            const samples = (results[key] ?? []) as unknown[]; samples.push(sample); results[key] = samples;
            await invoke('viewer_bench_write', { report: output });
          }
        }
      } finally {
        if (sessionId) await api('session_close', { sessionId }).catch(() => {});
        if (graph) {
          try {
            const cleanup = await api('session_open', { profileId: profile.id, secret: config.graph.secret });
            try { await api('execute', { sessionId: cleanup, runId: crypto.randomUUID(), req: req(`MATCH (n:${label}) DETACH DELETE n`) }); }
            finally { await api('session_close', { sessionId: cleanup }); }
          } catch (error) { output.cleanupError = JSON.stringify(error); }
        }
        await api('profile_delete', { profileId: profile.id });
      }
    }
    if (target === 'all' || target === 'workspace') { await clear(); root.remove(); const runs = []; for (let i = 0; i < 3; i++) runs.push(await (await import('./nativeWorkspace')).nativeWorkspace(manifests, config)); output.workspace = runs; }
    if (target === 'idle') {
      const { fixtureGraph, fixtureResult } = await import('./fixtures'), { graphInteraction } = await import('./graphInteraction');
      const samples = [];
      for (const count of [300, 1000]) for (let repeat = 0; repeat < 3; repeat++) {
        await clear(); mount([fixtureResult(null, fixtureGraph(count))], count);
        await waitFor(() => !!root.querySelector('canvas[data-ready=true]')); await frame(); await frame();
        const load = await invoke<string | null>('viewer_bench_load');
        samples.push({ count, repeat, load, ...(await graphInteraction(root.querySelector('canvas')!)) });
        output.graphPanZoom = samples; await invoke('viewer_bench_write', { report: output });
      }
    }
    if (target === 'all' || target === 'idle') {
      document.body.append(root); root.tabIndex = -1; root.focus({ preventScroll: true }); await clear(); mount([(await import('./fixtures')).fixtureResult(null, (await import('./fixtures')).fixtureGraph(300))], 300);
      await waitFor(() => !!root.querySelector('canvas[data-ready=true]')); await waitFor(() => perf.marks().some(m => m.name === 'graph-stable')); await frame(); await frame();
      const { readGraphProbes } = await import('./index'); const begin = readGraphProbes()[0];
      const idleWindowMs = 90000; output.idleReadyAt = Date.now(); output.idleWindowMs = idleWindowMs;
      output.idleReady = true; await invoke('viewer_bench_write', { report: output });
      await new Promise(resolve => setTimeout(resolve, idleWindowMs));
      const end = readGraphProbes()[0]; let movement = 0; for (let i = 0; i < begin.positions.length; i += 2) movement += Math.hypot(begin.positions[i] - end.positions[i], begin.positions[i + 1] - end.positions[i + 1]);
      output.idle = { nodes: end.positions.length / 2, renderFrames: end.frames - begin.frames, averageMovementPx: movement / Math.max(1, end.positions.length / 2), duration: idleWindowMs, activeAnimations: document.getAnimations().length }; output.idleReady = false;
    }
  } catch (error) { output.error = error instanceof Error ? error.message : JSON.stringify(error); }
  finally { render(null, root); root.remove(); output.complete = !output.error && !output.cleanupError; await invoke('viewer_bench_write', { report: output }); }
}
await run();
