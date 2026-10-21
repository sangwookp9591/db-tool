import { afterAll, afterEach, beforeAll, expect, it } from 'vitest';
import { chromium, type Browser, type Page } from 'playwright';
import { createServer, type ViteDevServer } from 'vite';

let server: ViteDevServer, browser: Browser, page: Page;
// The editor keeps the previous lint result visible while the next one is computed. Those underlines must follow
// the text through edits: a fixed decoration set keeps absolute positions and made CodeMirror throw once the
// document became shorter than a stale range ("Position 7 is out of range", "No tile at position").
const PROBE = `
import { h, render } from 'preact';
import { Editor } from '/src/Editor.tsx';
window.__handle = null; window.__errors = [];
const diagnostics = [{ rule_id: 'select_star', start: 7, end: 8, severity: 'warning', message: 'm', quick_fix: null }];
const show = list => render(h(Editor, { text: 'SELECT * FROM people;', mode: 'sql', diagnostics: list, onChange: () => {}, onRun: () => {}, onReady: handle => { window.__handle = handle; }, onError: e => window.__errors.push(String(e)) }), document.getElementById('root'));
// Like the app, diagnostics arrive after the editor view exists (the lint request is asynchronous).
show([]);
setTimeout(() => show(diagnostics), 150);
`;
beforeAll(async () => {
  server = await createServer({ configFile: false, plugins: [{
    name: 'editor-probe',
    configureServer(s) { s.middlewares.use('/editor-probe', (_req, res) => { res.setHeader('Content-Type', 'text/html'); res.end('<html><body style="margin:0"><div id="root" style="height:300px;width:900px"></div><script type="module" src="/@id/virtual:editor-probe"></script></body></html>'); }); },
    resolveId(id) { if (id === 'virtual:editor-probe') return '\0' + id; },
    load(id) { if (id === '\0virtual:editor-probe') return PROBE; },
  }], optimizeDeps: { rolldownOptions: { transform: { jsx: { runtime: 'automatic', importSource: 'preact' } } } }, cacheDir: `node_modules/.vite-editor-${crypto.randomUUID()}`, server: { host: '127.0.0.1', port: 0, hmr: false }, logLevel: 'error' });
  await server.listen();
  browser = await chromium.launch({ headless: true });
}, 30000);
afterEach(async () => { await page?.close(); });
afterAll(async () => { await browser?.close(); await server?.close(); }, 30000);

async function open() {
  page = await browser.newPage({ reducedMotion: 'reduce', viewport: { width: 1000, height: 500 } });
  page.setDefaultTimeout(20000);
  const pageErrors: string[] = [];
  page.on('pageerror', e => pageErrors.push(e.message));
  await page.goto(server.resolvedUrls!.local[0] + 'editor-probe');
  await page.locator('.ax-lint-underline').first().waitFor();
  return pageErrors;
}

it('keeps stale lint underlines on their text when characters are inserted before them', async () => {
  const pageErrors = await open();
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+Home');
  await page.keyboard.type('  -- ');
  await expect.poll(() => page.locator('.ax-lint-underline').allTextContents(), { timeout: 15000, interval: 100 }).toEqual(['*']);
  expect(pageErrors).toEqual([]);
}, 40000);

it('does not throw when the document becomes shorter than a stale lint range', async () => {
  const pageErrors = await open();
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+A');
  await page.keyboard.type('x');
  await page.keyboard.type('y');
  await expect.poll(() => page.locator('.cm-content').textContent(), { timeout: 15000, interval: 100 }).toBe('xy');
  expect(await page.locator('.ax-lint-underline').count()).toBe(0);
  expect(pageErrors).toEqual([]);
}, 40000);
