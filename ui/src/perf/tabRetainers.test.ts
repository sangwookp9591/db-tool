import { afterAll, beforeAll, expect, it } from 'vitest';
import { chromium, type Browser, type CDPSession, type Page } from 'playwright';
import { createServer, type ViteDevServer } from 'vite';

let server: ViteDevServer, browser: Browser, page: Page, cdp: CDPSession;
beforeAll(async () => {
  server = await createServer({ root: new URL('../../', import.meta.url).pathname, cacheDir: `node_modules/.vite-tab-retainers-${crypto.randomUUID()}`, configFile: false, optimizeDeps: { rolldownOptions: { transform: { jsx: { runtime: 'automatic', importSource: 'preact' } } } }, server: { host: '127.0.0.1', port: 0, hmr: false }, logLevel: 'error' });
  await server.listen(); browser = await chromium.launch({ headless: true });
  page = await browser.newPage({ reducedMotion: 'reduce' }); cdp = await page.context().newCDPSession(page);
  await page.goto(`${server.resolvedUrls!.local[0]}?driver_step_delay=1`);
  await page.getByRole('button', { name: 'Workspace settings', exact: true }).waitFor();
}, 30000);
afterAll(async () => { await browser?.close(); await server?.close(); }, 30000);

it('releases spring callbacks even while callers keep their stop handles', async () => {
  for (const mode of ['immediate', 'completed', 'cancelled']) {
    await page.emulateMedia({ reducedMotion: mode === 'immediate' ? 'reduce' : 'no-preference' });
    await page.evaluate(`(async () => {
      const { spring } = await import('/src/ui/spring.ts');
      const owner = { payload: new Uint8Array(1024) };
      window.__springOwner = new WeakRef(owner);
      window.__springStop = spring(0, 1, () => { owner.payload[0]++; }, 20);
      if (${JSON.stringify(mode)} === 'cancelled') window.__springStop();
    })()`);
    await page.waitForTimeout(100);
    await cdp.send('HeapProfiler.collectGarbage');
    expect(await page.evaluate('window.__springOwner.deref() === undefined'), mode).toBe(true);
    await page.evaluate('delete window.__springOwner; delete window.__springStop');
  }
});

it('collects closed table/graph tabs and empties their maps without clearing live connections', async () => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.evaluate(`(async () => {
    const { changeDriver } = await import('/src/drivers/api.ts');
    await changeDriver('install', 'postgres'); await changeDriver('install', 'neo4j');
  })()`);
  for (const name of ['Analytics sandbox', 'Graph playground']) {
    await page.getByRole('button', { name: `Connect or disconnect ${name}`, exact: true }).click();
    await page.waitForFunction(expected => document.querySelectorAll('.tab-body').length === expected, name === 'Analytics sandbox' ? 1 : 2);
    await page.locator('.tab-body:not([hidden]) .cm-content').waitFor();
  }
  const connections = await page.evaluate(async () => await window.axdb.agent!.callTool('list_connections', {}) as { driver_id: string; session_id: string }[]);
  const table = connections.find(c => c.driver_id === 'postgres')!.session_id;
  const graph = connections.find(c => c.driver_id === 'neo4j')!.session_id;
  for (let index = 2; index < 10; index++) {
    const graphTab = index === 2;
    await page.evaluate(async args => {
      const result = await window.axdb.agent!.callTool('open_query_tab', args) as { error?: unknown };
      if (result.error) throw new Error(JSON.stringify(result.error));
    }, { session_id: graphTab ? graph : table, text: graphTab ? 'MATCH (n) RETURN n LIMIT 300' : 'SELECT large;' });
    const tab = page.locator('.tab-body:not([hidden])'); await tab.locator('.cm-content').waitFor();
    await tab.getByRole('button', { name: 'Run query', exact: true }).click();
    await tab.locator('.results-wrap[aria-busy=false]').waitFor();
    if (graphTab) await tab.locator('canvas[data-ready=true]').waitFor();
  }
  while (await page.locator('.workspace-tab').count()) await page.locator('.workspace-tab').last().locator('button').last().click();
  await page.waitForFunction(() => !document.querySelector('.tab-body'));
  // Allow passive unmount cleanups and transient UI timers to finish before forcing full GC.
  await page.waitForTimeout(500);
  await cdp.send('HeapProfiler.collectGarbage');
  const chunks: string[] = []; const receive = ({ chunk }: { chunk: string }) => chunks.push(chunk);
  cdp.on('HeapProfiler.addHeapSnapshotChunk', receive);
  await cdp.send('HeapProfiler.takeHeapSnapshot', { exposeInternals: true });
  cdp.off('HeapProfiler.addHeapSnapshotChunk', receive);
  const snapshot = JSON.parse(chunks.join(''));
  const { node_fields: nf, edge_fields: ef, node_types: nt, edge_types: et } = snapshot.snapshot.meta;
  expect(nf).toContain('detachedness');
  const nodes: number[] = snapshot.nodes, edges: number[] = snapshot.edges, strings: string[] = snapshot.strings;
  const width = nf.length, edgeWidth = ef.length;
  const name = (offset: number): string => strings[nodes[offset + nf.indexOf('name')]];
  const starts = new Map<number, number>(); let edge = 0, detached = 0;
  for (let offset = 0; offset < nodes.length; offset += width) {
    starts.set(offset, edge); edge += nodes[offset + nf.indexOf('edge_count')] * edgeWidth;
    if (nt[0][nodes[offset + nf.indexOf('type')]] === 'native' && nodes[offset + nf.indexOf('detachedness')] === 2) detached++;
  }
  function links(offset: number) {
    const start = starts.get(offset)!;
    return Array.from({ length: nodes[offset + nf.indexOf('edge_count')] }, (_, index) => {
      const at = start + index * edgeWidth, type = et[0][edges[at + ef.indexOf('type')]];
      return { type, key: ['element', 'hidden'].includes(type) ? '' : strings[edges[at + ef.indexOf('name_or_index')]], to: edges[at + ef.indexOf('to_node')] };
    });
  }
  const empty = new Set<string>();
  for (let offset = 0; offset < nodes.length; offset += width) {
    if (!name(offset).startsWith('system / Context')) continue;
    for (const link of links(offset)) {
      if (!['handles', 'lastRuns', 'graphProbes', 'runs'].includes(link.key)) continue;
      const map = name(link.to) === 'Map' ? link.to : links(link.to).find(e => e.key === 'current')?.to;
      expect(map, link.key).toBeDefined();
      const table = links(map!).find(e => e.key === 'table')?.to;
      expect(table, link.key).toBeDefined();
      expect(links(table!).filter(e => e.key !== 'map'), link.key).toEqual([]);
      empty.add(link.key);
    }
  }
  expect([...empty].sort()).toEqual(['graphProbes', 'handles', 'lastRuns', 'runs']);
  expect(detached).toBe(0);
  const state = await page.evaluate(async () => await window.axdb.agent!.callTool('get_ui_state', {}) as { active_tab: unknown; last_result: unknown[]; connections: unknown[] });
  expect(state.active_tab).toBeNull(); expect(state.last_result).toEqual([]); expect(state.connections).toHaveLength(2);
}, 60000);
