import { afterAll, afterEach, beforeAll, expect, it } from 'vitest';
import { chromium, type Browser, type Page } from 'playwright';
import { createServer, type ViteDevServer } from 'vite';

let server: ViteDevServer, browser: Browser, page: Page;
const PROBE = `
import { h, render } from 'preact';
import LibraryPanel from '/src/library/LibraryPanel.tsx';
import { seedHistory } from '/src/mock.ts';
seedHistory(10000);
window.__calls = []; window.__used = []; window.__toasts = [];
addEventListener('ax:mock-command', e => window.__calls.push(e.detail.command));
const host = { profiles: [{ id: 'demo-0', name: 'Analytics sandbox' }], insert: (text) => window.__used.push(['insert', text]), openTab: (text) => window.__used.push(['tab', text]), notify: (m) => window.__toasts.push(m), report: (e) => window.__toasts.push('ERR ' + e.message) };
const t0 = performance.now();
render(h('section', { class: 'history library', style: 'height:320px;width:1000px' }, h(LibraryPanel, { host, open: true, focus: { nonce: 1 }, draft: null, onClose: () => { window.__closed = true; } })), document.getElementById('root'));
window.__mount = () => performance.now() - t0;
`;
beforeAll(async () => {
  server = await createServer({ configFile: false, plugins: [{
    name: 'library-probe',
    configureServer(s) { s.middlewares.use('/library-probe', (_req, res) => { res.setHeader('Content-Type', 'text/html'); res.end('<html><body style="margin:0"><div id="root"></div><script type="module" src="/@id/virtual:library-probe"></script></body></html>'); }); },
    resolveId(id) { if (id === 'virtual:library-probe') return '\0' + id; },
    load(id) { if (id === '\0virtual:library-probe') return PROBE; },
  }], optimizeDeps: { rolldownOptions: { transform: { jsx: { runtime: 'automatic', importSource: 'preact' } } } }, cacheDir: `node_modules/.vite-library-${crypto.randomUUID()}`, server: { host: '127.0.0.1', port: 0, hmr: false }, logLevel: 'error' });
  await server.listen();
  browser = await chromium.launch({ headless: true });
}, 30000);
afterEach(async () => { await page?.close(); });
afterAll(async () => { await browser?.close(); await server?.close(); }, 30000);

async function open() {
  page = await browser.newPage({ reducedMotion: 'reduce', viewport: { width: 1100, height: 700 } });
  page.setDefaultTimeout(8000);
  page.on('pageerror', e => console.error('pageerror', e.message));
  await page.goto(server.resolvedUrls!.local[0] + 'library-probe');
  await page.locator('.lib-scroll [role=option]').first().waitFor();
}

it('renders only the visible rows of 10k history entries and pages on scroll', async () => {
  await open();
  const rows = await page.locator('.lib-scroll [role=option]').count();
  expect(rows).toBeGreaterThan(5); expect(rows).toBeLessThan(40);
  const mount = await page.evaluate(() => (window as any).__mount() as number);
  const scroll = await page.evaluate(async () => {
    const box = document.querySelector('.lib-scroll') as HTMLElement, times: number[] = [];
    for (let i = 0; i < 40; i++) { const t = performance.now(); box.scrollTop += 28 * 30; await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r))); times.push(performance.now() - t); }
    return { max: Math.max(...times), mean: times.reduce((a, b) => a + b, 0) / times.length, rows: document.querySelectorAll('.lib-scroll [role=option]').length, calls: (window as any).__calls.filter((c: string) => c === 'history_search').length, count: Number(box.getAttribute('aria-rowcount')) };
  });
  const out = (globalThis as unknown as { process: { env: Record<string, string | undefined>; getBuiltinModule(n: 'fs'): { writeFileSync(p: string, d: string): void } } }).process;
  if (out.env.LIB_BENCH_OUT) out.getBuiltinModule('fs').writeFileSync(out.env.LIB_BENCH_OUT, JSON.stringify({ mountMs: mount, ...scroll }));
  console.info(`library 10k: first paint ${mount.toFixed(0)}ms, scroll frame mean ${scroll.mean.toFixed(1)}ms max ${scroll.max.toFixed(1)}ms, DOM rows ${scroll.rows}, search calls ${scroll.calls}, known rows ${scroll.count}`);
  expect(scroll.rows).toBeLessThan(40); expect(scroll.calls).toBeGreaterThan(1); expect(scroll.count).toBeGreaterThan(1000);
}, 40000);

it('debounces typing into a single search and navigates and inserts by keyboard', async () => {
  await open();
  const input = page.getByRole('combobox', { name: '이력 검색' });
  await input.focus();
  const before = await page.evaluate(() => (window as any).__calls.length);
  await input.pressSequentially('id = 7', { delay: 30 });
  await page.waitForTimeout(400);
  const searches = await page.evaluate(n => (window as any).__calls.slice(n).filter((c: string) => c === 'history_search').length, before);
  expect(searches).toBe(1);
  expect(await page.locator('.lib-scroll [role=option]').first().textContent()).toContain('id = 7');
  await input.press('ArrowDown');
  const second = await page.locator('.lib-scroll [role=option]').nth(1).getAttribute('id');
  expect(await input.getAttribute('aria-activedescendant')).toBe(second);
  await input.press('Enter');
  await page.waitForFunction(() => (window as any).__used.length === 1);
  expect(await page.evaluate(() => (window as any).__used[0][0])).toBe('insert');
  await input.press('Escape');
  expect(await page.evaluate(() => (window as any).__closed)).toBe(true);
}, 40000);

it('uses the shared combobox menus for history filters, memo filters, and memo scope', async () => {
  await open();
  for (const [label, option] of [['연결 필터', 'Analytics sandbox'], ['결과 필터', '오류만'], ['종류 필터', '미리보기']]) {
    await page.getByRole('combobox', { name: label }).click();
    await page.getByRole('option', { name: option, exact: true }).click();
  }
  await page.getByRole('tab', { name: '메모', exact: true }).click();
  await page.getByRole('combobox', { name: '연결 필터' }).click();
  await page.getByRole('option', { name: 'Analytics sandbox' }).click();
  await page.getByRole('button', { name: '새 메모' }).click();
  await page.getByRole('combobox', { name: '연결 범위' }).click();
  await page.getByRole('option', { name: 'Analytics sandbox' }).click();
}, 40000);

it('saves a history entry as a memo and inserts it from the memo tab', async () => {
  await open();
  await page.getByRole('button', { name: '메모로 저장' }).click();
  await page.getByRole('textbox', { name: '제목' }).fill('주문 조회');
  await page.getByRole('button', { name: /^저장/ }).click();
  await page.locator('.lib-scroll [role=option]').first().waitFor();
  expect(await page.locator('.lib-scroll [role=option]').first().textContent()).toContain('주문 조회');
  await page.getByRole('button', { name: '편집기에 넣기' }).click();
  await page.waitForFunction(() => (window as any).__used.some((u: string[]) => u[0] === 'insert'));
  expect(await page.evaluate(() => (window as any).__used.at(-1)[1])).toContain('SELECT * FROM orders');
}, 40000);
