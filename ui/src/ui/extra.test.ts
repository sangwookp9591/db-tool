// 사용자 제공 레퍼런스 재구현 · DOM behavior in the existing Chromium test dependency.
import { afterAll, beforeAll, expect, it } from 'vitest';
import { chromium, type Browser, type Page } from 'playwright';
import { createServer, type ViteDevServer } from 'vite';
import { toolPreview } from './ToolCall';

let server: ViteDevServer;
let browser: Browser;
let page: Page;
beforeAll(async () => {
  server = await createServer({ configFile: false, server: { host: '127.0.0.1', port: 0, hmr: false }, logLevel: 'error' });
  await server.listen();
  browser = await chromium.launch({ headless: true });
  page = await browser.newPage();
  await page.addInitScript(() => {
    const active = new Set<number>();
    const start = window.setInterval, stop = window.clearInterval;
    window.setInterval = ((...args: Parameters<typeof start>) => { const id = start(...args); active.add(id); return id; }) as typeof start;
    window.clearInterval = id => { if (id !== undefined) active.delete(id); stop(id); };
    const frames = new Set<number>(), raf = window.requestAnimationFrame, cancel = window.cancelAnimationFrame;
    window.requestAnimationFrame = callback => { let id = 0; id = raf.call(window, time => { frames.delete(id); callback(time); }); frames.add(id); return id; };
    window.cancelAnimationFrame = id => { frames.delete(id); cancel.call(window, id); };
    Object.assign(window, { d3Intervals: active, d3Frames: frames });
  });
  await page.goto(`${server.resolvedUrls!.local[0]}?gallery`);
  await page.getByTestId('d3-run').waitFor();
}, 30000);
afterAll(async () => { await browser?.close(); await server?.close(); }, 30000);

it('CallChip animates running only, settles when idle/done, and exposes retry only for error', async () => {
  const chips = page.locator('#d3-call-chip .call-chip');
  const first = chips.first();
  expect(await first.getAttribute('data-status')).toBe('idle');
  expect(await first.locator('.call-chip__retry').count()).toBe(0);
  const frames = () => page.evaluate(() => (window as unknown as { d3Frames: Set<number> }).d3Frames.size);
  const settled = await frames();
  await page.getByRole('button', { name: 'Start', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('#d3-call-chip .call-chip')?.getAttribute('data-status') === 'running');
  await page.waitForFunction(base => (window as unknown as { d3Frames: Set<number> }).d3Frames.size > base, settled);
  await page.getByRole('button', { name: 'Complete', exact: true }).click();
  await page.waitForTimeout(80);
  const doneFrames = await frames();
  await page.waitForTimeout(80);
  expect(await frames()).toBe(doneFrames);
  await page.getByRole('button', { name: 'Fail', exact: true }).click();
  expect(await first.locator('.call-chip__retry').count()).toBe(1);
  await first.locator('.call-chip__retry').click();
  expect(await first.getAttribute('data-status')).toBe('running');
  await page.getByRole('button', { name: 'Idle', exact: true }).click();
  expect(await first.locator('.call-chip__retry').count()).toBe(0);
});

it('Button preserves native props and blocks loading/disabled clicks', async () => {
  const button = page.getByTestId('d3-run');
  expect(await button.getAttribute('aria-label')).toBe('D3 run');
  expect(await button.getAttribute('type')).toBe('submit');
  expect(await button.getAttribute('name')).toBe('run');
  expect(await button.getAttribute('value')).toBe('query');
  await button.click();
  await page.waitForFunction(() => document.querySelector('[data-testid="d3-clicks"]')?.textContent === '1');
  await page.getByRole('checkbox', { name: 'D3 loading', exact: true }).check();
  expect(await button.isDisabled()).toBe(true);
  expect(await button.getAttribute('aria-busy')).toBe('true');
  await button.evaluate(element => (element as HTMLButtonElement).click());
  expect(await page.getByTestId('d3-clicks').textContent()).toBe('1');
  expect(await page.getByRole('button', { name: 'Disabled', exact: true }).isDisabled()).toBe(true);
  await page.getByRole('checkbox', { name: 'D3 loading', exact: true }).uncheck();
  const bare = page.getByTestId('d3-bare');
  await bare.click();
  expect(await bare.getAttribute('data-ref-passed')).toBe('true');
  expect(await bare.locator('*').count()).toBe(0);
  expect(await bare.getAttribute('class')).toBeNull();
});

it('SuggestionCard chooses alternatives, returns exact accept/undo selection, and dismisses menu', async () => {
  const card = page.getByRole('article', { name: 'Query suggestion' });
  const alternatives = card.getByRole('button', { name: '대안 선택', exact: true });
  await alternatives.click();
  await page.keyboard.press('End'); await page.keyboard.press('Enter');
  await card.getByRole('button', { name: '검증', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('[data-testid="d3-suggestion-result"]')?.textContent === 'Validated count');
  await card.getByRole('button', { name: '새 탭에 삽입', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('[data-testid="d3-suggestion-result"]')?.textContent === 'Inserted count');
  await card.getByRole('button', { name: '실행 취소', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('[data-testid="d3-suggestion-result"]')?.textContent === 'Undid count');
  await alternatives.click(); await page.keyboard.press('Escape');
  expect(await alternatives.getAttribute('aria-expanded')).toBe('false');
  expect(await alternatives.evaluate(element => element === document.activeElement)).toBe(true);
  await alternatives.click(); await page.locator('h1').click();
  expect(await card.getByRole('menu').count()).toBe(0);
});

it('ToolCall opens on error, closes on rerun, honors manual toggle, and disposes timers on completion/unmount', async () => {
  const tool = page.getByTestId('d3-tool');
  const header = tool.locator('> button');
  const intervals = () => page.evaluate(() => (window as unknown as { d3Intervals: Set<number> }).d3Intervals.size);
  const baseline = await intervals();
  await page.getByRole('button', { name: 'Simulate tool start' }).click();
  await page.waitForFunction(base => (window as unknown as { d3Intervals: Set<number> }).d3Intervals.size === base + 1, baseline);
  await page.getByRole('button', { name: 'Simulate tool error' }).click();
  await tool.getByRole('button', { name: '재시도', exact: true }).waitFor();
  expect(await header.getAttribute('aria-expanded')).toBe('true');
  expect(await intervals()).toBe(baseline);
  await tool.getByRole('button', { name: '재시도', exact: true }).click();
  expect(await header.getAttribute('aria-expanded')).toBe('false');
  await header.click();
  await page.getByRole('button', { name: 'Simulate tool success' }).click();
  expect(await header.getAttribute('aria-expanded')).toBe('true');
  expect(await intervals()).toBe(baseline);
  await page.getByRole('button', { name: 'Simulate tool start' }).click();
  await page.waitForFunction(base => (window as unknown as { d3Intervals: Set<number> }).d3Intervals.size === base + 1, baseline);
  await page.getByRole('button', { name: 'Unmount tool' }).click();
  expect(await intervals()).toBe(baseline);
  const denied = page.getByRole('article', { name: 'run_query', exact: true });
  expect(await denied.getByRole('button', { name: '재시도', exact: true }).count()).toBe(0);
  expect(await denied.locator('.tool-call__elapsed').count()).toBe(0);
  expect(await page.locator('.tool-group__header').textContent()).not.toContain('0.0s');
  await page.getByRole('button', { name: 'Simulate tool success' }).click();
  await page.getByRole('button', { name: 'Mount tool' }).click();
});

it('Tool preview bounds deep, circular, oversized, and many-row values', () => {
  const circular: Record<string, unknown> = {}; circular.self = circular;
  expect(toolPreview(circular)).toContain('[circular]');
  const preview = toolPreview(Array.from({ length: 10000 }, (_, id) => ({ id, text: 'x'.repeat(10000) })));
  expect(preview.length).toBeLessThanOrEqual(8000);
  expect(preview.split('\n').length).toBeLessThanOrEqual(20);
  expect(preview).not.toContain('"id": 20');
  expect(toolPreview(1n)).toBe('"1"');
});

it('keeps brand styling, stops all idle motion, and honors reduced motion during loading', async () => {
  const button = page.getByTestId('d3-run');
  expect(await button.evaluate(element => getComputedStyle(element).backgroundImage)).toContain('linear-gradient');
  expect(await button.evaluate(element => getComputedStyle(element).minHeight)).toBe('40px');
  await page.mouse.move(1, 1);
  await page.waitForFunction(() => document.getAnimations().length === 0);
  await page.waitForTimeout(3000);
  expect(await page.evaluate(() => document.getAnimations().length)).toBe(0);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.getByRole('checkbox', { name: 'D3 loading', exact: true }).check();
  await page.getByRole('button', { name: 'Simulate tool start' }).click();
  expect(await page.evaluate(() => document.getAnimations().length)).toBe(0);
  await page.getByRole('checkbox', { name: 'D3 loading', exact: true }).uncheck();
  await page.getByRole('button', { name: 'Simulate tool success' }).click();
});
