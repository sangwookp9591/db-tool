import { chromium } from '../ui/node_modules/playwright/index.mjs';
import { writeFile } from 'node:fs/promises';
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
const errors = []; page.on('pageerror', e => errors.push(e.message));
await page.goto(process.env.BENCH_URL ?? 'http://127.0.0.1:1424');
await page.waitForLoadState('networkidle');
await page.locator('.connection-actions button').first().waitFor();
const results = { connect: [], newTab: [], tab: [], addConnection: [] };
async function measured(action, selector) {
  await page.evaluate(() => window.__benchStart = performance.now()); await action();
  if (selector) await page.locator(selector).last().waitFor();
  return page.evaluate(async () => { await new Promise(requestAnimationFrame); await new Promise(requestAnimationFrame); return performance.now() - window.__benchStart; });
}
for (let i = 0; i < 3; i++) {
  results.connect.push(await measured(() => page.locator('.connection-actions button').first().click(), '.tree-node'));
  if (i < 2) { await page.locator('.connection-actions button').first().click(); await page.locator('.tree-node').waitFor({ state: 'detached' }); }
}
for (let i = 0; i < 10; i++) results.newTab.push(await measured(() => page.getByRole('button', { name: '＋ Query · SQL', exact: true }).click(), '.workspace-tab'));
for (let i = 0; i < 10; i++) results.tab.push(await measured(() => page.locator('.workspace-tabs [role=tab]').nth(i % 2).click()));
for (let i = 0; i < 3; i++) {
  await page.getByRole('button', { name: 'Add connection', exact: true }).click();
  await page.getByLabel('Name', { exact: true }).fill(`Bench ${i}`);
  await page.evaluate(() => window.__benchStart = performance.now());
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  const connection = page.locator('.connection').filter({ hasText: `Bench ${i}` });
  await connection.locator('.connection-actions button').first().click(); await connection.locator('.tree-node').first().waitFor();
  results.addConnection.push(await page.evaluate(async () => { await new Promise(requestAnimationFrame); await new Promise(requestAnimationFrame); return performance.now() - window.__benchStart; }));
}
await writeFile(new URL(`./results-viewer-workspace-${process.argv[2] ?? 'after'}.json`, import.meta.url), JSON.stringify(results, null, 2));
await browser.close();
if (errors.length) throw new Error(errors.join('\n'));
