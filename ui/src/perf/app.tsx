import { render } from 'preact';
import { invoke } from '@tauri-apps/api/core';
import { api } from '../api';
import type { ExecRequest, ResultSet } from '../contract';
import { Results } from '../Results';
import { executeQuery } from './stream';
import { finish, frame, painted, perf, sampleFrames, start } from './index';
async function waitFor(test: () => boolean, timeout = 60000) { const until = performance.now() + timeout; while (!test()) { if (performance.now() > until) throw new Error('Native benchmark timed out'); await frame(); } }
async function run() {
  const root = document.createElement('div'); root.style.cssText = 'position:fixed;inset:0;z-index:10000;display:flex;background:var(--surface)'; document.body.append(root);
  const manifests = await api('drivers_list', {});
  const drivers = [manifests.find(d => d.paradigm === 'Relational' && d.connection_fields.some(f => f.key === 'host')), manifests.find(d => d.paradigm === 'Graph')];
  const output: Record<string, unknown> = { environment: { userAgent: navigator.userAgent, mode: 'Tauri debug / WKWebView', ports: [55432, 57687], at: new Date().toISOString() }, results: {} };
  const results = output.results as Record<string, unknown>;
  let last: ResultSet[] = []; let viewerSession = '';
  const label = `DbtP_${Date.now()}`; output.fixtureLabel = label;
  const req = (text: string, limit = 100000): ExecRequest => ({ text, params: {}, row_limit: 100000, graph_node_limit: limit, timeout_ms: 60000 });
  const mount = (r: ResultSet[], limit = 5000) => render(<Results results={r} active sessionId={viewerSession} canExpand={r.some(result => !!result.graph)} graphLimit={limit} rowLimit={100000} onError={e => { output.error = String(e); }} />, root);
  async function clear() { render(null, root); await frame(); perf.clear(); }
  async function query(sessionId: string, request: ExecRequest, mode: string) {
    viewerSession = sessionId; await clear(); const since = start('native-execute'); let first: Promise<number> | undefined;
    const update = (r: ResultSet[]) => { last = r; mount(r); if (!first && r.some(r => r.table?.rows.length || r.graph?.nodes.length)) first = painted('native-first', since); };
    if (mode === 'stream') await executeQuery({ sessionId, runId: crypto.randomUUID(), req: request }, update);
    else update(await api('execute', { sessionId, runId: crypto.randomUUID(), req: request }));
    const loaded = finish('native-loaded', since); return { first: await first, loaded };
  }
  try {
    for (const driver of drivers) {
      if (!driver) throw new Error('Required test driver unavailable');
      const graph = driver.paradigm === 'Graph';
      const profile = await api('profile_save', { profile: { id: crypto.randomUUID(), name: graph ? 'Viewer test graph' : 'Viewer test table', driver_id: driver.id, fields: { ...Object.fromEntries(driver.connection_fields.filter(f => !f.secret && f.default !== null).map(f => [f.key, f.default!])), ...(graph ? { uri: 'bolt://127.0.0.1:57687' } : { host: '127.0.0.1', port: '55432', database: 'dbtool_test', sslmode: 'disable' }) }, save_secret: false }, secret: null });
      let sessionId: string | undefined;
      try {
        sessionId = await api('session_open', { profileId: profile.id, secret: graph ? 'testpassword' : 'testpw' });
        if (graph) {
          await api('execute', { sessionId, runId: crypto.randomUUID(), req: req(`UNWIND range(0,4999) AS i CREATE (:${label} {i:i, name:'Node '+toString(i)})`) });
          await api('execute', { sessionId, runId: crypto.randomUUID(), req: req(`MATCH (n:${label}) WITH n ORDER BY n.i WITH collect(n) AS nodes UNWIND range(0,4999) AS i WITH nodes[i] AS a, nodes[(i+1)%5000] AS b, nodes[(i+7)%5000] AS c CREATE (a)-[:DbtP_LINK]->(b), (a)-[:DbtP_LINK]->(c)`) });
          await api('execute', { sessionId, runId: crypto.randomUUID(), req: req(`MATCH (a:${label} {i:0}) UNWIND range(5000,5049) AS i CREATE (a)-[:DbtP_EXPAND]->(:${label} {i:i, name:'Expansion '+toString(i)})`) });

        }
        for (const mode of ['legacy', 'stream']) {
          const scenarios = graph ? [100, 300, 1000, 5000].map(n => [n, 0]) : [[1000, 10], [10000, 10], [100000, 10], [10000, 200]];
          for (const [count, width] of scenarios) {
            const samples = [];
            for (let repeat = 0; repeat < 3; repeat++) {
              const text = graph ? `MATCH (a:${label})-[r:DbtP_LINK]->(b:${label}) WHERE a.i < ${count} AND b.i < ${count} RETURN a,r,b LIMIT ${count * 2}` : `SELECT ${Array.from({ length: width }, (_, c) => c % 2 ? `'row ' || i::text AS c${c}` : `9007199254740993::bigint+i AS c${c}`).join(',')} FROM generate_series(1,${count}) i`;
              const sample: Record<string, unknown> = await query(sessionId, req(text, graph ? count : 300), mode);
              if (graph) {
                await waitFor(() => perf.marks().some(m => m.name === 'graph-stable'));
                const canvas = root.querySelector('canvas')!;
                sample.layout = perf.marks().find(m => m.name === 'graph-stable')?.duration;
                sample.frames = await sampleFrames(i => canvas.dispatchEvent(new WheelEvent('wheel', { deltaY: i % 2 ? 20 : -20, clientX: 500, clientY: 300, bubbles: true, cancelable: true })));
                (root.querySelector('.graph-accessible summary') as HTMLElement).click(); await waitFor(() => !!root.querySelector('.graph-accessible button'));
                const click = performance.now(); ([...root.querySelectorAll<HTMLButtonElement>('.graph-accessible button')].find(b => b.textContent === 'Node 0') ?? root.querySelector<HTMLButtonElement>('.graph-accessible button')!).click(); await waitFor(() => !!root.querySelector('.properties')); sample.click = await painted('native-properties', click);
                const originalCount = last[0].graph!.nodes.length;
                (root.querySelector('.properties > button') as HTMLButtonElement).click();
                await waitFor(() => perf.marks().some(m => m.name === 'expand-frame'));
                sample.merge = perf.marks().find(m => m.name === 'expand-merge')?.duration;
                sample.expand = perf.marks().find(m => m.name === 'expand-frame')?.duration;
                const total = root.querySelector('.warning')?.textContent?.match(/^(\d+)/)?.[1] ?? canvas.getAttribute('aria-label')!.match(/Graph: (\d+)/)![1];
                sample.addedNodes = Number(total) - originalCount;
              } else { const el = root.querySelector<HTMLElement>('.virtual-scroll')!; sample.frames = await sampleFrames(i => { el.scrollTop = i / 59 * el.scrollHeight; el.scrollLeft = i / 59 * el.scrollWidth; }); }
              samples.push(sample);
            }
            results[`${graph ? 'graph' : 'table'}-${count}-${width}-${mode}`] = samples;
            await invoke('viewer_bench_write', { report: output });
          }
        }
      } finally {
        if (sessionId) await api('session_close', { sessionId }).catch(() => {});
        if (graph) {
          try {
            const cleanup = await api('session_open', { profileId: profile.id, secret: 'testpassword' });
            try { await api('execute', { sessionId: cleanup, runId: crypto.randomUUID(), req: req(`MATCH (n:${label}) DETACH DELETE n`) }); }
            finally { await api('session_close', { sessionId: cleanup }); }
          } catch (error) { output.cleanupError = JSON.stringify(error); }
        }
        await api('profile_delete', { profileId: profile.id });
      }
    }
  } catch (error) { output.error = error instanceof Error ? error.message : JSON.stringify(error); }
  finally { render(null, root); root.remove(); output.complete = !output.error && !output.cleanupError; await invoke('viewer_bench_write', { report: output }); }
}
if (await invoke<boolean>('viewer_bench_enabled')) await run();
