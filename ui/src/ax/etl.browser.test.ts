import { afterAll, beforeAll, expect, it } from 'vitest';
import { chromium, type Browser, type Page } from 'playwright';
import { createServer, type ViteDevServer } from 'vite';
declare const process: { env: Record<string, string | undefined> };
let server: ViteDevServer, browser: Browser, base: string;
beforeAll(async () => {
  server = await createServer({ configFile: false, optimizeDeps: { rolldownOptions: { transform: { jsx: { runtime: 'automatic', importSource: 'preact' } } } }, cacheDir: `node_modules/.vite-etl-${crypto.randomUUID()}`, server: { host: '127.0.0.1', port: 0, hmr: false }, logLevel: 'error' });
  await server.listen(); base = server.resolvedUrls!.local[0];
  browser = await chromium.launch({ headless: true });
}, 30000);
afterAll(async () => { await browser?.close(); await server?.close(); });
const shots = process.env.AXDB_ETL_SHOTS;
async function shot(page: Page, name: string) { if (shots) await page.screenshot({ path: `${shots}/${name}.png`, fullPage: true }); }
async function choose(page: Page, label: string, option: string) {
  await page.getByRole('combobox', { name: label, exact: true }).click();
  await page.getByRole('option', { name: option }).first().click();
}
async function start(page: Page) {
  page.setDefaultTimeout(6000); await page.goto(base);
  await page.getByRole('button', { name: '드라이버', exact: true }).click(); await page.getByRole('tab', { name: '사용 가능', exact: true }).click();
  await page.getByRole('button', { name: 'PostgreSQL · 설치 가능', exact: true }).click(); await page.getByRole('button', { name: 'PostgreSQL · 설치됨', exact: true }).waitFor();
  await page.getByRole('button', { name: '드라이버 화면 닫기' }).click();
  await page.getByLabel('Connect or disconnect Analytics sandbox', { exact: true }).click();
  await page.getByRole('button', { name: 'people', exact: true }).waitFor(); await page.getByRole('button', { name: 'AX 워크벤치', exact: true }).click();
  await page.getByRole('tab', { name: '데이터 파이프라인', exact: true }).click();
  await page.getByLabel('파이프라인 이름', { exact: true }).waitFor();
}
it('blocks preview and run while the spec is incomplete', async () => {
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 }, reducedMotion: 'reduce' });
  try {
    await start(page);
    await page.getByLabel('파이프라인 이름', { exact: true }).fill('   ');
    await page.getByLabel('소스 쿼리', { exact: true }).fill('');
    await page.getByRole('button', { name: '다음', exact: true }).click();
    await choose(page, '추가할 단계', '형 변환');
    await page.getByRole('button', { name: '단계 추가', exact: true }).click();
    for (let i = 0; i < 2; i++) await page.getByRole('button', { name: '다음', exact: true }).click();
    expect(await page.getByRole('button', { name: '미리보기 실행', exact: true }).isDisabled()).toBe(true);
    // Every validation problem is listed, not only the first.
    for (const text of ['파이프라인 이름은 1..200바이트여야 합니다', '소스 쿼리는 1바이트 이상 256 KiB 이하여야 합니다', '단계 1: 컬럼 이름을 입력하세요', '대상 연결과 테이블을 선택하세요'])
      expect(await page.getByText(text, { exact: false }).count(), text).toBeGreaterThan(0);
    await page.getByRole('button', { name: '다음', exact: true }).click();
    expect(await page.getByRole('button', { name: '실행', exact: true }).isDisabled()).toBe(true);
  } finally { await page.close(); }
}, 60000);
for (const [theme, width] of [['daylight', 1280], ['midnight', 1280], ['daylight', 960]] as const) it(`etl pipeline preview and run ${theme} ${width}`, async () => {
  const page = await browser.newPage({ viewport: { width, height: 900 }, reducedMotion: 'reduce' });
  const errors: string[] = []; page.on('pageerror', e => errors.push(String(e)));
  try {
    await page.addInitScript(t => localStorage.setItem('axdb-theme', t), theme); await start(page);
    await shot(page, `${theme}-${width}-1-source`);
    await page.getByLabel('파이프라인 이름', { exact: true }).fill('사용자 복사');
    await page.getByLabel('소스 쿼리', { exact: true }).fill('SELECT id, name, team FROM people');
    await page.getByRole('button', { name: '다음', exact: true }).click();
    await choose(page, '추가할 단계', '텍스트 변환');
    await page.getByRole('button', { name: '단계 추가', exact: true }).click();
    await page.locator('.etl-step input, .etl-steps input').first().fill('name');
    await shot(page, `${theme}-${width}-2-steps`);
    await page.getByRole('button', { name: '다음', exact: true }).click();
    await choose(page, '대상 연결', 'Analytics sandbox');
    await choose(page, '대상 테이블', 'people');
    await shot(page, `${theme}-${width}-3-target`);
    await page.getByRole('button', { name: '다음', exact: true }).click();
    const run = page.getByRole('button', { name: '실행', exact: true });
    await shot(page, `${theme}-${width}-4a-before-preview`);
    await page.getByRole('button', { name: '미리보기 실행', exact: true }).click();
    await page.getByRole('region', { name: '변환 결과 표본' }).waitFor();
    expect(await page.getByRole('region', { name: '변환 결과 표본' }).locator('tbody tr').count()).toBeGreaterThan(0);
    await shot(page, `${theme}-${width}-4-preview`);
    await page.getByRole('button', { name: '다음', exact: true }).click();
    expect(await run.isDisabled()).toBe(false);
    await run.click();
    await page.getByText(/완료: \d+행을 \d+개 배치로 반영했습니다/).waitFor();
    // The receipt is single use: run is disabled again until a new preview.
    expect(await run.isDisabled()).toBe(true);
    await page.getByText('미리보기 승인은 한 번만 쓸 수 있습니다', { exact: false }).waitFor();
    await shot(page, `${theme}-${width}-5-run`);
    const overflow = await page.locator('.ax-workbench').evaluate(root => root.scrollWidth > root.clientWidth);
    expect(overflow).toBe(false);
    expect(errors).toEqual([]);
  } finally { await page.close(); }
}, 60000);
