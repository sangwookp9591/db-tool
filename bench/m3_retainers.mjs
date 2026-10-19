import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from '../ui/node_modules/playwright/index.mjs';
import { createServer } from '../ui/node_modules/vite/dist/node/index.js';

const root = fileURLToPath(new URL('..', import.meta.url));
process.chdir(resolve(root, 'ui'));
// Dedicated mock UI: no credentials, database connections or user browser profile.
const name = process.argv[2] ?? 'baseline';
assert.match(name, /^[a-z0-9-]+$/);
const motion = process.argv[3] ?? 'reduce';
assert.ok(['reduce','no-preference'].includes(motion));
const output = resolve(root, 'target-m3', name);
mkdirSync(output, { recursive: false });
const server = await createServer({ root: process.env.AXDB_M3_UI_ROOT ?? resolve(root, 'ui'), cacheDir: resolve(output,'vite-cache'), configFile: false, oxc: { jsx: { importSource: 'preact' } }, resolve: { dedupe: ['preact'] }, optimizeDeps: { rolldownOptions: { transform: { jsx: { importSource: 'preact' } } } }, server: { host: '127.0.0.1', port: 0, hmr: false, fs: { allow: [root] } }, logLevel: 'error' });
await server.listen();
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 768 }, deviceScaleFactor: 2, reducedMotion: motion });
page.setDefaultTimeout(60000);
const errors = [];
page.on('pageerror', error => errors.push(error.message));
const cdp = await page.context().newCDPSession(page);
await cdp.send('HeapProfiler.enable');
const report = { browser: browser.version(), viewport: {width:1280,height:768,scale:2}, backend:'mock', motion, complete:false, points:[], errors };
async function snapshot(stage, gc = false) {
  if (gc) await cdp.send('HeapProfiler.collectGarbage');
  const chunks = [];
  const receive = ({chunk}) => chunks.push(chunk);
  cdp.on('HeapProfiler.addHeapSnapshotChunk', receive);
  await cdp.send('HeapProfiler.takeHeapSnapshot', { reportProgress: false, captureNumericValue: true, exposeInternals: true });
  cdp.off('HeapProfiler.addHeapSnapshotChunk', receive);
  const raw = chunks.join('');
  writeFileSync(`${output}/${stage}.heapsnapshot`, raw);
  const metrics = await cdp.send('Runtime.getHeapUsage');
  const dom = await cdp.send('Memory.getDOMCounters');
  report.points.push({stage,gc,snapshotBytes:Buffer.byteLength(raw),...metrics,...dom});
  writeFileSync(`${output}/measurements.json`, JSON.stringify(report,null,2));
  console.log(stage, JSON.stringify(report.points.at(-1)));
}
async function open(session, text) {
  const before = await page.locator('.workspace-tab').count();
  await page.evaluate(async ({session,text}) => { const result = await window.axdb.agent.callTool('open_query_tab',{session_id:session,text}); if(result?.error) throw new Error(JSON.stringify(result.error)); }, {session,text});
  await page.waitForFunction(count => document.querySelectorAll('.workspace-tab').length === count, before + 1);
  await page.locator('.tab-body:not([hidden]) .cm-content').waitFor();
}
async function run(rows) {
  const tab = page.locator('.tab-body:not([hidden])');
  await tab.getByLabel('Execution options',{exact:true}).click();
  await tab.getByLabel('Row limit',{exact:true}).fill(String(rows));
  await tab.getByLabel('Execution options',{exact:true}).click();
  await tab.getByRole('button',{name:'Run query',exact:true}).click();
  await tab.locator('.results-wrap[aria-busy=false]').waitFor();
  await tab.locator(`.result-status [aria-label="Rows: ${rows.toLocaleString('en-US')}"]`).waitFor();
}
async function close() {
  while (await page.locator('.workspace-tab').count()) await page.locator('.workspace-tab').last().locator('button').last().click();
  await page.waitForFunction(() => !document.querySelector('.tab-body'));
}
try {
  await page.goto(server.resolvedUrls.local[0]);
  await page.getByRole('button',{name:'Workspace settings',exact:true}).waitFor();
  await snapshot('startup', true);
  await page.evaluate(`(async () => { const {changeDriver}=await import('/src/drivers/api.ts'); await changeDriver('install','postgres'); await changeDriver('install','neo4j'); })()`);
  await page.getByRole('button',{name:'Connect or disconnect Analytics sandbox',exact:true}).click();
  await page.locator('.cm-content').waitFor();
  await close();
  let connections = await page.evaluate(() => window.axdb.agent.callTool('list_connections',{}));
  const tableSession = connections.find(c=>c.driver_id==='postgres').session_id;
  await open(tableSession, 'SELECT large;'); await run(100000);
  assert.equal(await page.locator('[role=gridcell]').first().textContent(), '9007199254740993');
  await snapshot('table-100000');
  await page.getByRole('button',{name:'Connect or disconnect Graph playground',exact:true}).click();
  await page.waitForFunction(() => document.querySelectorAll('.tab-body').length === 2);
  await page.locator('.tab-body:not([hidden]) .cm-content').waitFor();
  await page.waitForTimeout(100);
  connections = await page.evaluate(() => window.axdb.agent.callTool('list_connections',{}));
  const graphSession = connections.find(c=>c.driver_id==='neo4j').session_id;
  await open(graphSession, 'MATCH (n)-[r]->(m) RETURN n,r,m LIMIT 300;'); await run(300);
  await page.locator('canvas[data-ready=true]').waitFor();
  await page.waitForFunction(() => window.axdb.perf.marks().some(m=>m.name==='graph-stable'));
  assert.equal(await page.evaluate(async()=> (await import('/src/perf/index.ts')).readGraphProbes()[0].positions.length),600);
  await snapshot('graph-300');
  while (await page.locator('.workspace-tab').count() < 10) { await open(tableSession, 'SELECT large;'); await run(1000); }
  await page.locator('.workspace-tab [role=tab]').first().click(); await run(100000);
  await snapshot('tabs-10');
  await close(); await page.waitForTimeout(10000);
  await snapshot('closed-10');
  await snapshot('closed-gc', true);
  report.closed = await page.evaluate(async () => ({state:await window.axdb.agent.callTool('get_ui_state',{}),graphProbes:(await import('/src/perf/index.ts')).readGraphProbes().length}));
  assert.equal(report.closed.graphProbes,0);
  assert.deepEqual(errors,[]);
  report.complete = true;
} catch (error) { report.error = String(error); throw error; }
finally { writeFileSync(`${output}/measurements.json`,JSON.stringify(report,null,2)); await browser.close(); await server.close(); }
