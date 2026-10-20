import { afterAll, beforeAll, expect, it } from 'vitest';
import { chromium, type Browser, type Page } from 'playwright';
import { createServer, type ViteDevServer } from 'vite';
let server: ViteDevServer, browser: Browser, page: Page, sessionId: string;
declare global { interface Window { __ownedGraphWorkers: (Worker & { terminated: boolean })[]; __ownedGraphCanvas: HTMLCanvasElement } }
beforeAll(async () => {
  server = await createServer({ configFile: false, cacheDir: `node_modules/.vite-graph-lifecycle-${crypto.randomUUID()}`, optimizeDeps: { rolldownOptions: { transform: { jsx: { runtime: 'automatic', importSource: 'preact' } } } }, server: { host: '127.0.0.1', port: 0, hmr: false }, logLevel: 'error' }); await server.listen();
  browser = await chromium.launch({ headless: true }); page = await browser.newPage({ reducedMotion: 'reduce', viewport: { width: 960, height: 600 } });
  await page.addInitScript(() => { const Original = window.Worker; window.__ownedGraphWorkers = []; window.Worker = class extends Original { terminated = false; constructor(...args: ConstructorParameters<typeof Worker>) { super(...args); window.__ownedGraphWorkers.push(this); } override terminate() { this.terminated = true; super.terminate(); } }; });
  await page.goto(server.resolvedUrls!.local[0]);
  await page.getByRole('button', { name: 'Workspace settings', exact: true }).waitFor();
  await page.evaluate(`(async () => { await (await import('/src/drivers/api.ts')).changeDriver('install', 'neo4j'); })()`);
  await page.getByRole('button', { name: 'Connect or disconnect Graph playground', exact: true }).click();
  await page.getByRole('button', { name: 'Run query', exact: true }).click(); await page.locator('canvas[data-ready=true]').waitFor();
  sessionId = await page.evaluate(async () => {
    const connected = await window.axdb.agent!.callTool('list_connections', {}) as { driver_id: string; session_id: string }[];
    return connected.find(profile => profile.driver_id === 'neo4j')!.session_id;
  });
  await page.evaluate(() => { window.__ownedGraphCanvas = document.querySelector<HTMLCanvasElement>('canvas[data-ready=true]')!; });
}, 30000);
afterAll(async () => { await browser?.close(); await server?.close(); }, 30000);
it('preserves hidden graph resources and releases worker handlers and canvas backing when the tab closes', async () => {
  await page.evaluate(async session => { const opened = await window.axdb.agent!.callTool('open_query_tab', { session_id: session, text: 'RETURN 1' }) as { error?: unknown }; if (opened.error) throw new Error(JSON.stringify(opened.error)); }, sessionId);
  await page.waitForFunction(() => !!window.__ownedGraphCanvas.closest('[hidden]'));
  expect(await page.evaluate(() => window.__ownedGraphCanvas.width > 0 && window.__ownedGraphWorkers.length > 0 && window.__ownedGraphWorkers.every(w => !w.terminated))).toBe(true);
  await page.locator('.workspace-tab').first().locator('button').last().click();
  await page.waitForFunction(() => !window.__ownedGraphCanvas.isConnected);
  expect(await page.evaluate(() => ({ width: window.__ownedGraphCanvas.width, height: window.__ownedGraphCanvas.height, workersReleased: window.__ownedGraphWorkers.every(w => w.terminated && w.onmessage === null && w.onerror === null) }))).toEqual({ width: 0, height: 0, workersReleased: true });
});
