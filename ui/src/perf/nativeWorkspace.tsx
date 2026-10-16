import type { BenchConfig } from './benchConfig';
import { render } from 'preact';
import { invoke } from '@tauri-apps/api/core';
import { api } from '../api';
import type { DriverManifest } from '../contract';
import { Results } from '../Results';
import { frame, painted, perf } from './index';
async function waitFor(test: () => boolean) { const end = performance.now() + 30000; while (!test()) { if (performance.now() > end) throw new Error('Native workspace benchmark timed out'); await frame(); } }
function input(element: HTMLInputElement | HTMLSelectElement, value: string) { element.value = value; element.dispatchEvent(new Event('input', { bubbles: true })); element.dispatchEvent(new Event('change', { bubbles: true })); }
function button(element: ParentNode, text: string) { const found = [...element.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent?.trim() === text); if (!found) throw new Error(`Missing benchmark control: ${text}`); return found; }
export async function nativeWorkspace(manifests: DriverManifest[], config: BenchConfig) {
  const app = document.getElementById('app')!;
  const manifest = manifests.find(d => d.paradigm === 'Relational' && d.connection_fields.some(f => f.key === 'host'))!;
  const name = `Viewer workspace ${Date.now()}`;
  const values = config.table.fields;
  const output = { uptime: await invoke<string | null>('viewer_bench_load'), connect: [] as number[], newTab: [] as number[], tab: [] as number[], addConnection: [] as number[], resultSet: [] as number[], view: [] as { view: string; first: boolean; duration: number }[] };
  let profileId: string | undefined, connection: HTMLElement | undefined;
  const panel = document.createElement('div'); panel.style.cssText = 'position:fixed;inset:0;display:flex;background:var(--surface);z-index:10000';
  try {
    (app.querySelector('[aria-label="Add connection"]') as HTMLButtonElement).click();
    await waitFor(() => !!document.querySelector('.profile-form form'));
    const form = document.querySelector<HTMLFormElement>('.profile-form form')!;
    const field = (label: string) => [...form.querySelectorAll<HTMLInputElement | HTMLSelectElement>('input,select')].find(input => input.getAttribute('aria-label') === label) ?? [...form.querySelectorAll('label')].find(l => l.firstChild?.textContent?.trim() === label)?.querySelector<HTMLInputElement | HTMLSelectElement>('input,select');
    input(field('Name')!, name); input(field('Driver')!, manifest.id); await frame(); await frame();
    for (const f of manifest.connection_fields) if (values[f.key] !== undefined) input(field(f.label)!, values[f.key]);
    await frame(); const added = performance.now(); form.requestSubmit();
    await waitFor(() => [...app.querySelectorAll('.connection')].some(c => c.querySelector('strong')?.textContent === name));
    connection = [...app.querySelectorAll<HTMLElement>('.connection')].find(c => c.querySelector('strong')?.textContent === name)!;
    async function connect() {
      const since = performance.now(); (connection!.querySelector('.connection-actions button') as HTMLButtonElement).click();
      await waitFor(() => !!document.querySelector('input[type=password][autocomplete=off]'));
      const credential = document.querySelector<HTMLInputElement>('input[type=password][autocomplete=off]')!; input(credential, config.table.secret); await frame();
      credential.closest('form')!.requestSubmit();
      await waitFor(() => !!connection!.querySelector('.tree-node')); output.connect.push(await painted('native-connection-tree', since));
    }
    await connect(); output.addConnection.push(await painted('native-add-connection', added));
    const connections = await window.axdb.agent!.callTool('list_connections', {}) as { id: string; name: string; session_id: string | null }[];
    const saved = connections.find(c => c.name === name)!; profileId = saved.id;
    for (let i = 0; i < 2; i++) { (connection.querySelector('.connection-actions button') as HTMLButtonElement).click(); await waitFor(() => !connection!.querySelector('.tree-node')); await connect(); }
    for (let i = 0; i < 10; i++) { const since = performance.now(); (connection.querySelector('.new-query') as HTMLButtonElement).click(); output.newTab.push(await painted('native-new-tab', since)); }
    for (let i = 0; i < 10; i++) { const since = performance.now(); (app.querySelectorAll('.workspace-tabs [role=tab]')[i % 2] as HTMLButtonElement).click(); output.tab.push(await painted('native-tab-switch', since)); }
    const open = await window.axdb.agent!.callTool('list_connections', {}) as typeof connections;
    const sessionId = open.find(c => c.id === profileId)!.session_id!;
    const results = await api('execute', { sessionId, runId: crypto.randomUUID(), req: { text: 'SELECT 1 AS one; SELECT 2 AS two', params: {}, row_limit: 1000, graph_node_limit: 300, timeout_ms: 30000 } });
    document.body.append(panel); render(<Results results={results} sessionId={sessionId} canExpand={false} graphLimit={300} rowLimit={1000} onError={e => { throw e; }} />, panel);
    await waitFor(() => panel.querySelectorAll('.result-set:not([hidden]) .result-tabs button').length === 2);
    for (let i = 0; i < 10; i++) { const since = performance.now(); (panel.querySelectorAll('.result-set:not([hidden]) .result-tabs button')[(i + 1) % 2] as HTMLButtonElement).click(); output.resultSet.push(await painted('native-result-switch', since)); }
    const graphDriver = manifests.find(d => d.paradigm === 'Graph')!;
    const graphProfile = await api('profile_save', { profile: { id: crypto.randomUUID(), name: `${name} graph`, driver_id: graphDriver.id, fields: config.graph.fields, save_secret: false }, secret: null });
    const label = `DbtP_WS_${Date.now()}`;
    const request = (text: string) => ({ text, params: {}, row_limit: 1000, graph_node_limit: 300, timeout_ms: 30000 });
    let graphSession: string | undefined;
    try {
      graphSession = await api('session_open', { profileId: graphProfile.id, secret: config.graph.secret });
      const graphResults = await api('execute', { sessionId: graphSession, runId: crypto.randomUUID(), req: request(`UNWIND range(0,299) AS i CREATE (n:${label} {i:i,name:'Node '+toString(i)}) WITH collect(n) AS nodes UNWIND range(0,299) AS i WITH nodes[i] AS a,nodes[(i+1)%300] AS b,nodes[(i+7)%300] AS c CREATE (a)-[r:DbtP_WS_LINK]->(b),(a)-[s:DbtP_WS_LINK]->(c) RETURN a,r,b,s,c`) });
      perf.clear(); const since = performance.now();
      render(<Results results={graphResults} sessionId={graphSession} canExpand={false} graphLimit={300} rowLimit={1000} onError={e => { throw e; }} />, panel);
      await waitFor(() => !!panel.querySelector('canvas[data-ready=true]')); output.view.push({ view: 'Graph', first: true, duration: await painted('native-first-graph-view', since) });
      await waitFor(() => perf.marks().some(m => m.name === 'graph-stable'));
      for (let i = 0; i < 10; i++) for (const view of ['Table', 'Graph']) { const t = performance.now(); button(panel.querySelector('.view-switch')!, view).click(); output.view.push({ view, first: i === 0 && view === 'Table', duration: await painted('native-view-switch', t) }); }
    } finally {
      render(null, panel);
      if (graphSession) await api('session_close', { sessionId: graphSession });
      const cleanup = await api('session_open', { profileId: graphProfile.id, secret: config.graph.secret });
      try { await api('execute', { sessionId: cleanup, runId: crypto.randomUUID(), req: request(`MATCH (n:${label}) DETACH DELETE n`) }); }
      finally { await api('session_close', { sessionId: cleanup }); await api('profile_delete', { profileId: graphProfile.id }); }
    }
    return output;
  } finally {
    render(null, panel); panel.remove();
    if (connection?.querySelector('.tree-node')) { (connection.querySelector('.connection-actions button') as HTMLButtonElement).click(); await waitFor(() => !connection!.querySelector('.tree-node')); }
    if (profileId) await api('profile_delete', { profileId });
  }
}
