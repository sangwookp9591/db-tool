import { chromium } from 'playwright';
import assert from 'node:assert/strict';
const base = process.env.AXDB_UI_URL ?? 'http://127.0.0.1:5195';
const browser = await chromium.launch({ headless: true });
const errors = [];
try {
  for (const theme of ['midnight', 'daylight']) {
    for (const offline of [false, true]) {
      const page = await browser.newPage({ viewport: { width: 1200, height: 800 } });
      page.on('pageerror', error => errors.push(error.message));
      const url = new URL(base);
      if (offline) {
        url.searchParams.set('driver_catalog_warning', 'HTTP status client error (404 Not Found) for url (https://github.com/sangwookp9591/db-tool/releases/download/drivers-index/index.json)');
        url.searchParams.set('driver_catalog_empty', '1');
      }
      await page.goto(url.href);
      await page.getByRole('button', { name: '드라이버', exact: true }).click();
      await page.evaluate(theme => { document.documentElement.dataset.theme = theme; window.dispatchEvent(new Event('axdb-theme-change')); }, theme);
      const tabs = page.getByRole('tablist', { name: '드라이버 목록' });
      await tabs.waitFor();
      const installed = tabs.getByRole('tab', { name: /설치됨/ });
      await installed.click();
      await installed.press('ArrowRight');
      await page.waitForFunction(() => !document.querySelector('.drivers-skeleton'));
      assert.equal(await tabs.getByRole('tab', { name: '사용 가능' }).getAttribute('aria-selected'), 'true');
      assert.equal(await page.locator('.drivers-toolbar .sliding-indicator__line').count(), 0);
      assert.ok((await tabs.boundingBox()).height <= 34);
      if (offline) {
        await page.getByRole('heading', { name: '드라이버 저장소에 연결할 수 없습니다' }).waitFor();
        const details = page.locator('.drivers-catalog-status details');
        assert.equal(await details.getAttribute('open'), null);
        await details.locator('summary').click();
        assert.equal(await details.locator('pre').evaluate(el => el.scrollWidth <= el.clientWidth), true);
        await details.locator('summary').click();
        await page.getByRole('button', { name: '다시 시도', exact: true }).click();
        await page.waitForFunction(() => !document.querySelector('.drivers-skeleton') && !document.querySelector('.drivers-catalog-status button')?.disabled);
      } else {
        await page.getByTestId('driver-postgres').waitFor();
        await page.waitForFunction(() => document.querySelector('.glass-icons')?.dataset.animating === 'false');
      }
      await page.getByRole('heading', { name: '드라이버', exact: true }).click();
      assert.equal(await page.locator('.drivers-panel').evaluate(el => el.scrollWidth <= el.clientWidth), true);
      await page.screenshot({ path: `../docs/design-shots/drivers-${offline ? 'offline' : 'tabs'}-${theme}.png` });
      await page.close();
    }
  }
  const page = await browser.newPage({ viewport: { width: 960, height: 600 }, reducedMotion: 'reduce' });
  for (const [warning, expected] of [
    ['network offline', '인터넷 연결을 확인해 주세요.'],
    ['request timed out', '서버 응답 시간이 초과되었습니다.'],
    ['Untrusted catalog signing key', '목록의 공식 서명을 확인하지 못했습니다.'],
  ]) {
    const url = new URL(base);
    url.searchParams.set('driver_catalog_warning', warning);
    await page.goto(url.href);
    await page.getByRole('button', { name: '드라이버', exact: true }).click();
    await page.getByText(expected, { exact: false }).waitFor();
    await page.getByTestId('driver-postgres').waitFor();
    assert.equal(await page.locator('.drivers-panel').evaluate(el => el.scrollWidth <= el.clientWidth), true);
    assert.equal(await page.locator('.jelly-radio--segmented').evaluate(el => getComputedStyle(el, '::before').transitionDuration), '0s');
  }
  await page.close();
  assert.deepEqual(errors, []);
  console.log('Four theme/state captures passed: keyboard tabs, 34px track, collapsed/wrapped details, retry, no overflow or page errors.');
} finally { await browser.close(); }
