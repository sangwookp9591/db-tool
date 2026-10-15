// Run with: ego-browser nodejs < bench/w7-form-regression.mjs (UI preview on 127.0.0.1:4178).
const task = await taskSpace('db-tool W7 connection form regression');
const page = task.page('p1');
await page.goto('http://127.0.0.1:4178');
await page.waitForSelector('button[aria-label="Add connection"]');
for (const action of ['cancel', 'close', 'escape', 'save']) {
  await page.click('button[aria-label="Add connection"]');
  await page.fill('dialog form label:first-child input', 'W7 draft');
  await page.fill('dialog input[type="password"]', 'w7-disposable-test-secret');
  if (action === 'cancel') await page.click('dialog button[type="button"]:last-of-type');
  if (action === 'close') await page.click('button[aria-label="Close connection form"]');
  if (action === 'escape') await page.keyboard.press('Escape');
  if (action === 'save') await page.click('dialog button[type="submit"]');
  await page.waitForFunction(() => !document.querySelector('dialog'));
  await page.click('button[aria-label="Add connection"]');
  const clean = await page.evaluate(() => {
    const form = document.querySelector('dialog form');
    return form.querySelector('label:first-child input').value === '' && form.querySelector('input[type="password"]').value === '';
  });
  if (!clean) throw new Error(`Draft survived ${action}`);
  await page.click('button[aria-label="Close connection form"]');
  console.log(`PASS connection form reset after ${action}`);
}
await task.finish({ keep: [] });
