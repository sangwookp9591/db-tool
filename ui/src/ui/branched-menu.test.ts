// BranchedMenu: pure fold/path/keyboard helpers, plus gallery and mock-app DOM behavior in the existing Chromium test dependency.
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { chromium, type Browser, type Page } from 'playwright';
import { createServer, type ViteDevServer } from 'vite';
import { activePath, reachIndex, reachState, toggleOpen, treeKey, type BranchNode } from './BranchedMenu';

const tree: BranchNode[] = [
  { value: 'tables', label: 'Tables', children: [{ value: 'people', label: 'people', children: [{ value: 'people.id', label: 'id' }] }, { value: 'orders', label: 'orders' }] },
  { value: 'views', label: 'Views', children: [{ value: 'v1', label: 'v1' }] },
  { value: 'leaf', label: 'Leaf' },
];

describe('fold state', () => {
  it('toggles one key without mutating the previous set', () => {
    const start = new Set(['a']);
    const opened = toggleOpen(start, 'b');
    expect([...opened].sort()).toEqual(['a', 'b']);
    expect([...start]).toEqual(['a']);
    expect([...toggleOpen(opened, 'a')]).toEqual(['b']);
  });
});

describe('active path', () => {
  it('lists every ancestor down to the active row', () => {
    expect(activePath(tree, 'people.id')).toEqual(['tables', 'people', 'people.id']);
    expect(activePath(tree, 'v1')).toEqual(['views', 'v1']);
    expect(activePath(tree, 'leaf')).toEqual(['leaf']);
    expect(activePath(tree, 'missing')).toEqual([]);
  });
  it('reaches the first sibling on the path; rows above it pass the trunk', () => {
    const path = new Set(activePath(tree, 'people.id'));
    const reached = reachIndex(['people', 'orders'], path);
    expect(reached).toBe(0);
    expect(reachIndex(['x', 'orders', 'people'], new Set(['people']))).toBe(2);
    expect(reachIndex(['x'], path)).toBe(-1);
    expect([0, 1, 2].map(index => reachState(index, 1))).toEqual(['pass', 'to', undefined]);
    expect(reachState(0, -1)).toBeUndefined();
  });
});

describe('keyboard', () => {
  const row = (over: Partial<Parameters<typeof treeKey>[1]> = {}) => ({ index: 2, count: 5, hasChildren: false, open: false, parentIndex: 0, ...over });
  it('moves with arrows and clamps at the ends', () => {
    expect(treeKey('ArrowDown', row())).toEqual({ type: 'focus', index: 3 });
    expect(treeKey('ArrowDown', row({ index: 4 }))).toEqual({ type: 'focus', index: 4 });
    expect(treeKey('ArrowUp', row({ index: 0 }))).toEqual({ type: 'focus', index: 0 });
    expect(treeKey('Home', row())).toEqual({ type: 'focus', index: 0 });
    expect(treeKey('End', row())).toEqual({ type: 'focus', index: 4 });
  });
  it('opens, enters, closes, and leaves groups', () => {
    expect(treeKey('ArrowRight', row({ hasChildren: true }))).toEqual({ type: 'open' });
    expect(treeKey('ArrowRight', row({ hasChildren: true, open: true }))).toEqual({ type: 'focus', index: 3 });
    expect(treeKey('ArrowRight', row())).toBeNull();
    expect(treeKey('ArrowLeft', row({ hasChildren: true, open: true }))).toEqual({ type: 'close' });
    expect(treeKey('ArrowLeft', row())).toEqual({ type: 'focus', index: 0 });
    expect(treeKey('ArrowLeft', row({ parentIndex: -1 }))).toBeNull();
    expect(treeKey('Enter', row())).toBeNull();
    expect(treeKey('ArrowDown', row({ count: 0 }))).toBeNull();
  });
});

let server: ViteDevServer;
let browser: Browser;
let page: Page;
let base: string;
beforeAll(async () => {
  server = await createServer({ configFile: false, server: { host: '127.0.0.1', port: 0, hmr: false }, logLevel: 'error' });
  await server.listen();
  base = server.resolvedUrls!.local[0];
  browser = await chromium.launch({ headless: true });
  page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
}, 30000);
afterAll(async () => { await browser?.close(); await server?.close(); }, 30000);

describe('gallery BranchedMenu', () => {
  it('folds sections, moves the accent line and the rail marker, and selects leaves', async () => {
    await page.goto(`${base}?gallery`);
    const demo = page.getByTestId('bm-demo');
    await demo.waitFor();
    const heads = demo.locator('.bm-root > .bm-item > .bm-row > .bm-button');
    expect(await heads.first().getAttribute('aria-expanded')).toBe('true');
    expect(await demo.locator('.bm-item[data-section-active]').count()).toBe(1);
    expect(await demo.locator('.bm-item[data-reach=to] .bm-button').first().innerText()).toBe('orders');
    expect(await demo.locator('.bm-item[data-reach=pass]').count()).toBe(1);
    await demo.getByRole('button', { name: 'order_items' }).click();
    expect(await page.getByTestId('bm-selected').innerText()).toBe('items');
    expect(await demo.locator('.bm-item[data-reach=to] .bm-button').first().innerText()).toBe('order_items');
    // Folding closes the grid row, then removes the rows once the animation is done.
    await heads.first().click();
    expect(await heads.first().getAttribute('aria-expanded')).toBe('false');
    await page.waitForFunction(() => !document.querySelector('[data-testid=bm-demo] .bm-fold'), null, { timeout: 2000 });
    await heads.nth(1).click();
    await demo.getByRole('button', { name: 'name', exact: true }).waitFor();
    expect(await demo.locator('.bm-item[data-section-active]').count()).toBe(0);
  });
  it('draws lines with CSS boxes, not SVG paths', async () => {
    await page.goto(`${base}?gallery`);
    await page.getByTestId('bm-demo').waitFor();
    expect(await page.locator('[data-testid=bm-demo] svg:not(.lucide)').count()).toBe(0);
  });
});

describe('sidebar explorer (mock app)', () => {
  const live = (label: string) => page.evaluate(sel => (document.querySelector(sel) as HTMLElement).click(), `button[aria-label="Connect or disconnect ${label}"]`);
  beforeAll(async () => {
    await page.goto(base);
    await page.evaluate(() => { try { localStorage.clear(); } catch { /* ignore */ } });
    await page.goto(base);
    const sidebar = await page.locator('.app').getAttribute('data-sidebar-open');
    if (sidebar !== 'true') await page.getByLabel('Connections panel').click();
    await live('Graph playground');
    await page.getByLabel('Expand Labels').waitFor();
    await live('Analytics sandbox');
    await page.getByLabel('Expand public').waitFor({ state: 'attached' });
  }, 30000);

  it('keeps one explorer visible and separate scroll regions', async () => {
    expect(await page.locator('.explorer-region:not([hidden])').count()).toBe(1);
    expect(await page.locator('.explorer-region').count()).toBe(2);
    const overflow = await page.evaluate(() => ['.connections', '.connection-list', '.explorer-scroll'].map(sel => getComputedStyle(document.querySelector(sel)!).overflowY));
    expect(overflow).toEqual(['hidden', 'auto', 'auto']);
    // Selecting the other connection's card swaps which explorer is shown.
    await page.locator('.connection', { hasText: 'Graph playground' }).click();
    expect(await page.locator('.explorer-region:not([hidden]) [aria-label="Expand Labels"]').count()).toBe(1);
    await page.locator('.connection', { hasText: 'Analytics sandbox' }).click();
    expect(await page.locator('.explorer-region:not([hidden]) [aria-label="Expand public"]').count()).toBe(1);
  });

  it('loads lazily, reaches the active row, and supports deep levels and the keyboard', async () => {
    await page.getByLabel('Expand public').click();
    await page.getByLabel('Expand people').waitFor();
    expect(await page.getByTestId('tree-item').count()).toBeGreaterThan(3);
    await page.getByLabel('Expand people').click();
    await page.getByRole('button', { name: 'created_at', exact: true }).click();
    expect(await page.locator('.bm-item[data-reach=to] [aria-current=true]').count()).toBe(1);
    expect(await page.locator('.explorer .bm-root > .bm-item[data-section-active]').count()).toBe(1);
    // Keyboard: Left on a leaf goes to its parent, Left again closes it, Right reopens, Down moves on.
    await page.getByRole('button', { name: 'id', exact: true }).focus();
    await page.keyboard.press('ArrowLeft');
    expect(await page.evaluate(() => document.activeElement?.getAttribute('aria-label'))).toBe('people');
    await page.keyboard.press('ArrowLeft');
    expect(await page.getByLabel('Expand people').count()).toBe(1);
    await page.keyboard.press('ArrowRight');
    expect(await page.getByLabel('Collapse people').count()).toBe(1);
    await page.keyboard.press('ArrowDown');
    expect(await page.evaluate(() => document.activeElement?.getAttribute('aria-label'))).toBe('id');
    // Browse and Query stay reachable from the tree.
    await page.getByLabel('Actions for people').click();
    expect(await page.getByRole('menuitem').allInnerTexts()).toEqual(expect.arrayContaining(['Browse', 'Open query']));
    await page.keyboard.press('Escape');
  });

  it('pages long groups instead of rendering hundreds of rows', async () => {
    await page.getByLabel('Expand Archive').click();
    await page.getByLabel('Show more in Archive').waitFor();
    expect(await page.locator('[data-parent="public.archive"]').count()).toBe(300);
    await page.getByLabel('Show more in Archive').click();
    expect(await page.locator('[data-parent="public.archive"]').count()).toBe(450);
    expect(await page.locator('.explorer svg:not(.lucide)').count()).toBe(0);
  });
});
