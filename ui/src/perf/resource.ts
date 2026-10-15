import { invoke } from '@tauri-apps/api/core';
import { api } from '../api';
import type { DriverManifest } from '../contract';
import type { BenchConfig, BenchConnection } from './benchConfig';
import { frame, perf, readGraphProbes } from './index';

const pause = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
async function waitFor(test: () => boolean) {
  const until = performance.now() + 60000;
  while (!test()) { if (performance.now() > until) throw new Error('Resource benchmark UI timeout'); await pause(20); }
}
function input(element: HTMLInputElement | HTMLSelectElement, value: string) {
  if (!element) throw new Error('Missing resource benchmark input');
  element.value = value; element.dispatchEvent(new Event('input', { bubbles: true })); element.dispatchEvent(new Event('change', { bubbles: true }));
}
export async function resourceBench(leak: boolean) {
  let checkpoint = 0;
  const report: Record<string, unknown> = { target: 'resource', leak, userAgent: navigator.userAgent };
  async function sample(stage: string, iteration = 0) {
    if (stage !== 'startup') { await frame(); await frame(); }
    Object.assign(report, { checkpoint: ++checkpoint, stage, iteration, at: Date.now(), graphWorkers: readGraphProbes().length, viewport: { width: innerWidth, height: innerHeight, scale: devicePixelRatio } });
    await invoke('viewer_bench_write', { report });
    await invoke('viewer_bench_checkpoint', { checkpoint });
  }
  const app = document.getElementById('app')!;
  const active = () => app.querySelector<HTMLElement>('.tab-body:not([hidden])')!;
  const close = async () => {
    for (const button of app.querySelectorAll<HTMLButtonElement>('.workspace-tab > button:last-child')) { button.click(); await pause(0); }
    await waitFor(() => !app.querySelector('.workspace-tab'));
    await frame(); await frame();
    if (readGraphProbes().length) throw new Error('Graph worker owner survived tab close');
  };
  const profiles: string[] = [];
  const sessions: string[] = [];
  const label = `ResourceP1_${Date.now()}`;
  report.fixtureLabel = label;
  let graphSession = '';
  const req = (text: string) => ({ text, params: {}, row_limit: 100000, graph_node_limit: 300, timeout_ms: 60000 });
  try {
    await sample('startup'); await pause(Math.max(0, 10000 - performance.now())); await sample('idle-10');
    await pause(Math.max(0, 60000 - performance.now())); await sample('idle-60');
    const manifests = await api('drivers_list', {}), config = await invoke<BenchConfig>('viewer_bench_config');
    async function connect(driver: DriverManifest, settings: BenchConnection) {
      const name = `P1 ${driver.paradigm}`;
      app.querySelector<HTMLButtonElement>('[aria-label="Add connection"]')!.click();
      await waitFor(() => !!app.querySelector('.profile-form form'));
      const form = app.querySelector<HTMLFormElement>('.profile-form form')!;
      const field = (name: string) => [...form.querySelectorAll('label')].find(l => l.firstChild?.textContent?.trim().replace(/\s*\*$/, '') === name)?.querySelector<HTMLInputElement | HTMLSelectElement>('input,select');
      input(field('Name')!, name); input(field('Driver')!, driver.id); await frame(); await frame();
      for (const f of driver.connection_fields) if (f.secret || settings.fields[f.key] !== undefined) input(field(f.label)!, f.secret ? settings.secret : settings.fields[f.key]);
      await frame(); form.querySelector<HTMLButtonElement>('[type=submit]')!.click();
      const connection = () => [...app.querySelectorAll<HTMLElement>('.connection')].find(c => c.querySelector('strong')?.textContent === name);
      await waitFor(() => !!connection()); connection()!.querySelector<HTMLButtonElement>('.connection-actions button')!.click();
      await waitFor(() => !!app.querySelector('input[type=password][autocomplete=off]'));
      const credential = app.querySelector<HTMLInputElement>('input[type=password][autocomplete=off]')!;
      input(credential, settings.secret); await frame(); credential.closest('form')!.querySelector<HTMLButtonElement>('[type=submit]')!.click();
      await waitFor(() => !!connection()!.querySelector('.tree-node'));
      const list = await window.axdb.agent!.callTool('list_connections', {}) as { id: string; name: string; session_id: string }[];
      const saved = list.find(c => c.name === name)!; profiles.push(saved.id); sessions.push(saved.session_id); return saved.session_id;
    }
    async function open(sessionId: string, text: string) {
      const before = app.querySelectorAll('.workspace-tab').length;
      const result = await window.axdb.agent!.callTool('open_query_tab', { session_id: sessionId, text }) as { error?: unknown };
      if (result.error) throw new Error(JSON.stringify(result.error));
      await waitFor(() => app.querySelectorAll('.workspace-tab').length === before + 1);
      await frame(); await frame();
    }
    async function run(expected: number, graph = false) {
      const tab = active();
      input(tab.querySelector<HTMLInputElement>('[aria-label="Row limit"]')!, '100000'); await frame();
      const since = performance.now(); tab.querySelector<HTMLButtonElement>('[aria-label="Run query"]')!.click();
      await frame();
      await waitFor(() => tab.querySelector('.results-wrap')?.getAttribute('aria-busy') === 'false');
      report.lastQuery = { rows: expected, wall_ms: performance.now() - since };
      await frame(); await frame();
      if (tab.querySelector('.db-error')) throw new Error(tab.querySelector('.db-error')!.textContent!);
      await waitFor(() => Number(tab.querySelector('.result-status > span:nth-child(2) .metric-number')?.textContent?.replaceAll(',', '')) === expected);
      if (!graph) {
        const grid = tab.querySelector('[role=grid][aria-label="Query results"]');
        if (Number(grid?.getAttribute('aria-rowcount')) !== expected + 1 || grid?.querySelector('[role=gridcell]')?.textContent !== (expected === 100000 ? '9007199254740994' : '1')) throw new Error('Loaded table rows or exact first value did not match the query');
      }
    }
    const tableDriver = manifests.find(d => d.paradigm === 'Relational' && d.connection_fields.some(f => f.key === 'host'))!;
    const tableSession = await connect(tableDriver, config.table); await close();
    const sql = `SELECT ${Array.from({ length: 10 }, (_, i) => i % 2 ? `'row ' || i::text AS c${i}` : `9007199254740993::bigint+i AS c${i}`).join(',')} FROM generate_series(1,100000) i`;
    await sample('prepare-table'); await open(tableSession, sql); await run(100000); await sample('table-100000');
    await sample('prepare-graph');
    graphSession = await connect(manifests.find(d => d.paradigm === 'Graph')!, config.graph);
    await api('execute', { sessionId: graphSession, runId: crypto.randomUUID(), req: req(`UNWIND range(0,299) AS i CREATE (n:${label} {i:i,name:'Node '+toString(i)}) WITH collect(n) AS nodes UNWIND range(0,299) AS i WITH nodes[i] AS a,nodes[(i+1)%300] AS b CREATE (a)-[:RESOURCE_LINK]->(b)`) });
    await open(graphSession, `MATCH (a:${label})-[r:RESOURCE_LINK]->(b:${label}) RETURN a,r,b`); perf.clear(); await run(300, true);
    await waitFor(() => !!active().querySelector('canvas[data-ready=true]'));
    await waitFor(() => perf.marks().some(m => m.name === 'graph-stable'));
    if (readGraphProbes()[0]?.positions.length !== 600) throw new Error('Expected 300 graph nodes');
    await sample('graph-300');
    await sample('prepare-tabs');
    while (app.querySelectorAll('.workspace-tab').length < 10) { await open(tableSession, 'SELECT i FROM generate_series(1,1000) i'); await run(1000); }
    app.querySelector<HTMLButtonElement>('.workspace-tab [role=tab]')!.click(); await frame(); await frame(); await run(100000);
    await sample('tabs-10'); await sample('prepare-close'); const closedAt = performance.now(); await close(); await sample('tabs-closed'); await pause(Math.max(0, closedAt + 10000 - performance.now())); await sample('tabs-closed-10');
    if (leak) {
      await sample('prepare-repeat-query');
      await open(tableSession, sql);
      for (let i = 1; i <= 50; i++) { await run(100000); if (i === 1 || i % 5 === 0) await sample('repeat-query', i); }
      await close(); await sample('query-tab-closed');
      await sample('prepare-repeat-tabs');
      for (let i = 1; i <= 50; i++) { await open(tableSession, 'SELECT i FROM generate_series(1,1000) i'); await run(1000); await close(); if (i === 1 || i % 5 === 0) await sample('repeat-tabs', i); }
    }
    report.complete = true;
  } catch (error) { report.complete = false; report.error = error instanceof Error ? error.message : JSON.stringify(error); }
  finally {
    const cleanupErrors: string[] = [];
    const cleanup = async (action: () => Promise<unknown>) => { try { await action(); } catch (error) { cleanupErrors.push(error instanceof Error ? error.message : JSON.stringify(error)); } };
    await cleanup(close);
    if (graphSession) await cleanup(() => api('execute', { sessionId: graphSession, runId: crypto.randomUUID(), req: req(`MATCH (n:${label}) DETACH DELETE n`) }));
    for (const sessionId of sessions) await cleanup(() => api('session_close', { sessionId }));
    for (const profileId of profiles) await cleanup(() => api('profile_delete', { profileId }));
    if (cleanupErrors.length) { report.cleanupError = cleanupErrors; report.complete = false; }
    await invoke('viewer_bench_write', { report });
  }
}
