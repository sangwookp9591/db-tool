// Run from ui with a separate mock Vite server: node sidebar-browser.mjs http://127.0.0.1:5191
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { chromium } from 'playwright';
const base = process.argv[2] ?? 'http://127.0.0.1:5191';
const shots = new URL('../docs/design-shots/', import.meta.url).pathname;
await mkdir(shots, { recursive: true });
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 960, height: 600 }, reducedMotion: 'reduce' });
const errors = [];
page.on('pageerror', error => errors.push(error.message));
async function mount(kind = 'sidebar') {
  await page.goto(base);
  await page.waitForSelector('.connections');
  await page.evaluate(async kind => {
    // Use Vite's exact dependency URL so hooks and renderer share one Preact instance.
    const main = await (await fetch('/src/main.tsx')).text();
    const { h, render } = await import(main.match(/from "([^"]*\/preact.js[^\"]*)"/)[1]);
    const { api } = await import('/src/api.ts');
    const { Connections } = await import('/src/Connections.tsx');
    const { ClientsPanel } = await import('/src/agent/ClientsPanel.tsx');
    const drivers = await api('drivers_list', {}), original = (await api('profiles_list', {}))[0];
    const profiles = Array.from({ length: 24 }, (_, i) => ({ ...original, id: `sidebar-${i}`, name: `분석 연결 ${String(i + 1).padStart(2, '0')}` }));
    for (const profile of profiles) await api('profile_save', { profile, secret: null });
    const active = profiles.at(-1);
    const sessions = { [active.id]: await api('session_open', { profileId: active.id, secret: null }) };
    const noop = () => {};
    const props = { profiles, drivers, sessions, dead: new Set(), onProfiles: async () => {}, onConnect: async () => {}, onDisconnect: async () => {}, onQuery: noop, onAction: noop, onError: error => { throw error; } };
    const root = document.getElementById('app');
    if (kind === 'sidebar') {
      render(h('div', { class: 'app' }, h('div', { class: 'workspace' }, h(Connections, props), h('main', { style: 'padding:24px' }, '사이드바 mock 검증 · 연결 24개'))), root);
    } else {
      const calls = Array.from({ length: 30 }, (_, i) => ({ id: String(i), name: 'run_query', status: 'done', client_id: 'local', connection_id: active.id, duration_ms: 10, startedAt: Date.now(), decision: 'allowed', row_count: 1 }));
      const panelProps = { profiles, drivers, sessions, onClose: noop, activity: { running: 0, total: 0 }, queryPulse: null, onProfileSelect: noop, bridgeReply: 0, calls };
      window.rerenderSidebarDialog = () => { panelProps.calls = panelProps.calls.map(call => ({ ...call })); render(h(ClientsPanel, panelProps), root); };
      window.rerenderSidebarDialog();
    }
  }, kind);
  await page.waitForTimeout(250);
}
try {
  await mount();
  assert.equal(await page.locator('.connection').count(), 24);
  assert.equal(await page.locator('.connection').first().getAttribute('data-active'), '');
  await page.getByRole('button', { name: 'Expand public', exact: true }).click();
  await page.getByRole('button', { name: 'Expand people', exact: true }).click();
  const tree = page.locator('.explorer-scroll');
  assert.ok((await tree.boundingBox()).height >= 100, 'tree remains visible at minimum window size');
  assert.equal(await page.evaluate(() => document.getAnimations().filter(a => a.playState === 'running').length), 0);
  await page.screenshot({ path: `${shots}sidebar-960-expanded.png` });
  const splitter = page.getByRole('separator', { name: '연결과 탐색기 높이 조절' });
  await splitter.focus(); await page.keyboard.press('ArrowDown');
  assert.equal(await splitter.getAttribute('aria-valuenow'), '40');
  await page.keyboard.press('ArrowUp');
  assert.equal(await splitter.getAttribute('aria-valuenow'), '35');
  const rect = await splitter.boundingBox();
  await page.mouse.move(rect.x + rect.width / 2, rect.y + rect.height / 2); await page.mouse.down();
  await page.mouse.move(rect.x + rect.width / 2, rect.y + 100); await page.mouse.up();
  const draggedRatio = Number(await splitter.getAttribute('aria-valuenow'));
  assert.ok(draggedRatio > 35);
  assert.ok((await tree.boundingBox()).height >= 80, 'splitter reserves explorer space');
  await page.getByRole('button', { name: '컴팩트 보기', exact: true }).click();
  assert.ok((await page.locator('.connection').first().boundingBox()).height <= 38);
  await page.locator('.connection-list').evaluate(el => { el.scrollTop = el.scrollHeight; });
  const listBox = await page.locator('.connection-list').boundingBox();
  assert.ok(Math.abs((await page.locator('.connection[data-active]').boundingBox()).y - listBox.y) < 2, 'active connection remains pinned');
  await page.screenshot({ path: `${shots}sidebar-960-compact.png` });
  const connectionToggle = page.locator('.sidebar-section-toggle').first();
  const explorerToggle = page.locator('.sidebar-section-toggle').last();
  await connectionToggle.click();
  assert.equal(await connectionToggle.getAttribute('aria-expanded'), 'false');
  assert.ok((await tree.boundingBox()).height > 300);
  await page.screenshot({ path: `${shots}sidebar-960-connections-collapsed.png` });
  await explorerToggle.click();
  assert.equal(await explorerToggle.getAttribute('aria-expanded'), 'false');
  await mount();
  assert.equal(await connectionToggle.getAttribute('aria-expanded'), 'false');
  assert.equal(await explorerToggle.getAttribute('aria-expanded'), 'false');
  await connectionToggle.click();
  assert.equal(await page.getByRole('button', { name: '컴팩트 보기', exact: true }).getAttribute('aria-pressed'), 'true');
  assert.ok((await page.locator('.connection-list').boundingBox()).height > 400);
  await explorerToggle.click();
  assert.equal(Number(await splitter.getAttribute('aria-valuenow')), draggedRatio);
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.screenshot({ path: `${shots}sidebar-1280-compact.png` });
  await page.setViewportSize({ width: 960, height: 600 });
  await mount('dialog');
  const body = page.locator('.dialog-body');
  assert.equal(await body.evaluate(el => getComputedStyle(el).scrollSnapType), 'none');
  assert.equal(await page.locator('.call-history__list').evaluate(el => getComputedStyle(el).overflowY), 'visible');
  await body.hover({ position: { x: 650, y: 150 } });
  const scrollSamples = [];
  for (let i = 0; i < 4; i++) { await page.mouse.wheel(0, 60); await page.waitForTimeout(450); scrollSamples.push(await body.evaluate(el => el.scrollTop)); }
  assert.ok(scrollSamples.every((top, i) => top > (scrollSamples[i - 1] ?? 0)), `small wheel gestures move forward: ${scrollSamples}`);
  const before = await body.evaluate(el => el.scrollTop);
  await page.evaluate(() => window.rerenderSidebarDialog()); await page.waitForTimeout(300);
  assert.equal(await body.evaluate(el => el.scrollTop), before, 'call updates do not reset scrollTop');
  await page.screenshot({ path: `${shots}sidebar-mcp-scroll.png` });
  await body.evaluate(el => { el.scrollTop = el.scrollHeight; });
  await body.hover({ position: { x: 650, y: 150 } }); await page.mouse.wheel(0, 400); await page.waitForTimeout(400);
  assert.equal(await page.evaluate(() => document.scrollingElement.scrollTop), 0);
  assert.ok(await body.evaluate(el => el.scrollTop > 0));
  await page.evaluate(() => localStorage.setItem('axdb.sidebar', '{broken'));
  await mount();
  assert.equal(await connectionToggle.getAttribute('aria-expanded'), 'true');
  assert.equal(await splitter.getAttribute('aria-valuenow'), '35');
  await page.evaluate(() => { Storage.prototype.setItem = () => { throw new Error('storage unavailable'); }; });
  await page.getByRole('button', { name: '컴팩트 보기', exact: true }).click();
  assert.equal(await page.getByRole('button', { name: '컴팩트 보기', exact: true }).getAttribute('aria-pressed'), 'true');
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ passed: true, connections: 24, viewport: '960x600 and 1280x800', scrollSamples, rerenderScrollTop: before, screenshots: 5 }, null, 2));
} finally { await browser.close(); }
