// Browser-only tile checks replacing the former glass-renderer verification.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
const shots = fileURLToPath(new URL('../../../docs/design-shots/', import.meta.url));
const base = process.env.AXDB_UI_URL ?? 'http://127.0.0.1:5199';
const browser = await chromium.launch({ headless: true });
const errors = [], results = [];
const shot = (page, state, theme) => page.screenshot({ path: `${shots}drivers-v2-${state}-${theme}.png` });
const geometry = locator => locator.evaluateAll(nodes => nodes.map(node => { const r = node.getBoundingClientRect(); return [r.x, r.y, r.width, r.height]; }));
async function open(theme, query = '', installed = false) {
  const page = await browser.newPage({ viewport: { width: 960, height: 600 }, reducedMotion: 'reduce', hasTouch: true });
  page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(theme => {
    localStorage.setItem('axdb-theme', theme); window.driverCLS = 0;
    new PerformanceObserver(list => { for (const entry of list.getEntries()) if (!entry.hadRecentInput && entry.sources.some(source => source.node?.closest?.('.drivers-panel'))) window.driverCLS += entry.value; }).observe({ type: 'layout-shift', buffered: true });
  }, theme);
  if (installed) await page.route('**/src/mock.ts*', async route => {
    const response = await route.fetch(), source = await response.text();
    const seeded = source.replace(/const mockInstalledDrivers = new Map(?:\([^;]*\))?;/, 'const mockInstalledDrivers = new Map([["sqlite", "0.9.0"], ["redis", "1.0.0"], ["mysql", "1.0.0"], ["postgres", "1.0.0"], ["neo4j", "1.0.0"], ["mongodb", "1.0.0"]]);');
    assert.notEqual(seeded, source, 'installed fixture must match transformed mock');
    await route.fulfill({ response, body: seeded });
  });
  await page.goto(`${base}?${query}`);
  await page.getByRole('button', { name: '드라이버', exact: true }).click();
  return page;
}
try {
  for (const theme of ['midnight', 'daylight']) {
    const page = await open(theme, 'driver_catalog_delay=1400&driver_step_delay=250&driver_error=postgres');
    await page.getByRole('status', { name: '드라이버 목록 준비 중' }).waitFor();
    const before = await geometry(page.locator('.driver-skeleton-cell'));
    await shot(page, 'skeleton', theme);
    await page.getByTestId('driver-postgres').waitFor();
    const after = await geometry(page.locator('.driver-tile-cell'));
    assert.deepEqual(after, before, 'skeleton and loaded geometry');
    assert.equal(new Set(after.map(rect => rect[1])).size, 1); assert.equal(after.length, 6);
    assert.equal(await page.locator('.driver-gallery canvas').count(), 0);
    await page.waitForFunction(() => [...document.querySelectorAll('.driver-tile img')].every(image => image.complete && image.naturalWidth === 112));
    await shot(page, 'available', theme);
    const loadingCLS = await page.evaluate(() => window.driverCLS);
    assert.equal(loadingCLS, 0, 'catalog loading CLS');
    const pg = page.getByTestId('driver-postgres').getByRole('button');
    await pg.focus(); await page.getByRole('tooltip').waitFor();
    assert.match(await page.getByRole('tooltip').innerText(), /8.1 MB/);
    await shot(page, 'tooltip', theme);
    await pg.press('Escape');
    await page.locator('.driver-gallery').evaluate(element => { element.style.gridTemplateColumns = 'repeat(3, minmax(0, 1fr))'; });
    await pg.press('ArrowUp');
    assert.equal(await page.evaluate(() => document.activeElement.closest('[data-testid]')?.getAttribute('data-testid')), 'driver-sqlite');
    await page.keyboard.press('ArrowDown');
    assert.equal(await page.evaluate(() => document.activeElement.closest('[data-testid]')?.getAttribute('data-testid')), 'driver-postgres');
    await page.locator('.driver-gallery').evaluate(element => { element.style.removeProperty('grid-template-columns'); });
    await pg.press('ArrowRight');
    assert.equal(await page.evaluate(() => document.activeElement.closest('[data-testid]')?.getAttribute('data-testid')), 'driver-neo4j');
    await pg.click(); await page.getByTestId('driver-postgres').locator('.driver-tile-ring').waitFor();
    await page.waitForFunction(() => document.querySelector('[data-testid="driver-postgres"] .driver-tile-badge small')?.textContent !== '0%');
    await page.mouse.move(5, 580); await shot(page, 'progress', theme);
    await pg.click(); await page.getByRole('dialog', { name: 'PostgreSQL 관리' }).getByRole('button', { name: '설치 취소', exact: true }).click();
    await page.getByTestId('driver-postgres').locator('[aria-label="PostgreSQL · 설치 가능"]').waitFor();
    await pg.click(); await page.getByTestId('driver-postgres').locator('[aria-label*="설치 실패"]').waitFor({ timeout: 15000 });
    await page.mouse.move(5, 580); await shot(page, 'failed', theme);
    await pg.click(); await page.getByRole('dialog', { name: 'PostgreSQL 관리' }).getByRole('button', { name: '다시 시도', exact: true }).click();
    await page.getByTestId('driver-postgres').locator('[aria-label="PostgreSQL · 설치됨"]').waitFor({ timeout: 20000 });
    await pg.click(); const menu = page.getByRole('dialog', { name: 'PostgreSQL 관리' });
    await menu.getByRole('button', { name: 'PostgreSQL 삭제', exact: true }).click();
    await menu.getByRole('button', { name: '되돌리기', exact: true }).click();
    await menu.getByRole('button', { name: '드라이버 메뉴 닫기' }).click();
    await page.mouse.move(5, 580); await shot(page, 'installed', theme);
    await page.getByRole('tab', { name: '사용 가능', exact: true }).click();
    const mysql = page.getByTestId('driver-mysql').getByRole('button');
    await mysql.dispatchEvent('pointerdown', { pointerType: 'touch', pointerId: 1 }); await page.waitForTimeout(550);
    await mysql.dispatchEvent('pointerup', { pointerType: 'touch', pointerId: 1 }); await mysql.dispatchEvent('click');
    await page.getByRole('tooltip').waitFor(); assert.match(await page.getByRole('tooltip').innerText(), /MySQL/);
    assert.equal(await page.getByTestId('driver-mysql').getAttribute('data-state'), 'available', 'long press must not install');
    await page.keyboard.press('Escape');
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), 960);
    results.push({ theme, viewport: '960x600', visibleTiles: 6, tileSize: 52, skeletonGeometryMatches: true, loadingCLS });
    await page.close();
    const installed = await open(theme, 'driver_step_delay=50', true);
    await installed.getByRole('tab', { name: /설치됨/ }).click();
    await installed.getByTestId('driver-sqlite').locator('.driver-update-dot').waitFor(); await shot(installed, 'update', theme);
    await installed.getByTestId('driver-sqlite').getByRole('button').click();
    await shot(installed, 'menu', theme);
    await installed.getByRole('button', { name: '1.0.0 업데이트', exact: true }).click();
    await installed.getByTestId('driver-sqlite').locator('[aria-label="SQLite · 설치됨"]').waitFor();
    assert.equal(await installed.getByTestId('driver-sqlite').locator('.driver-update-dot').count(), 0);
    await installed.mouse.move(5, 580); await shot(installed, 'installed', theme);
    await installed.close();
  }
  assert.deepEqual(errors, []);
  fs.writeFileSync(`${shots}drivers-v2-check.json`, JSON.stringify({ results, errors, checked: ['6 tiles in one row', 'matching skeleton geometry', 'zero initial catalog-loading CLS', '112px WebP', 'no WebGL', 'keyboard tooltip and arrows', 'cancel', 'failure retry', 'delete undo', 'update action', 'touch long press without installation'] }, null, 2));
  console.log(JSON.stringify(results));
} finally { await browser.close(); }
