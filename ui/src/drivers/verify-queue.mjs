import { chromium } from 'playwright';
import assert from 'node:assert/strict';
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 960, height: 600 }, reducedMotion: 'reduce', colorScheme: 'dark' });
const errors = [];
page.on('pageerror', error => errors.push(error.message));
try {
  await page.goto(`${process.env.AXDB_UI_URL ?? 'http://127.0.0.1:5194'}?onboarding`);
  await page.getByRole('button', { name: '2. DB 드라이버', exact: true }).click();
  await page.getByRole('checkbox', { name: 'SQLite 선택', exact: true }).check();
  await page.getByRole('checkbox', { name: 'Redis 선택', exact: true }).check();
  await page.getByRole('button', { name: '선택한 드라이버 2개 설치', exact: true }).click();
  await page.getByRole('button', { name: '설치 중…', exact: true }).waitFor();
  assert.equal(await page.getByRole('button', { name: '다음', exact: true }).isDisabled(), true);
  assert.match(await page.locator('.install-queue').innerText(), /0\/2 완료/);
  await page.screenshot({ path: '../docs/design-shots/drivers-v2-onboarding-midnight.png' });
  await page.getByText('선택한 드라이버를 설치했습니다.', { exact: true }).waitFor();
  assert.equal(await page.getByRole('checkbox', { name: 'SQLite 선택', exact: true }).isDisabled(), true);
  assert.equal(await page.getByRole('checkbox', { name: 'Redis 선택', exact: true }).isDisabled(), true);
  await page.evaluate(() => { document.documentElement.dataset.theme = 'daylight'; window.dispatchEvent(new Event('axdb-theme-change')); });
  await page.screenshot({ path: '../docs/design-shots/drivers-v2-onboarding-daylight.png' });
  await page.getByRole('button', { name: '건너뛰기', exact: true }).click();
  await page.getByRole('button', { name: '드라이버', exact: true }).click();
  await page.getByRole('tab', { name: '설치됨 2', exact: true }).click();
  assert.equal(await page.getByTestId('driver-sqlite').getAttribute('data-state'), 'installed');
  assert.equal(await page.getByTestId('driver-redis').getAttribute('data-state'), 'installed');
  assert.deepEqual(errors, []);
  console.log('PASS: two driver selections, sequential queue, navigation lock, actual installed list, no page errors');
} finally { await browser.close(); }
