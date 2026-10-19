import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import { chromium, type Browser, type Page } from 'playwright';
import { createServer, type ViteDevServer } from 'vite';

let server: ViteDevServer;
let browser: Browser;
let page: Page;
let url: string;
const capture = import.meta.env.VITE_ONBOARDING_SHOTS === '1';
beforeAll(async () => {
  server = await createServer({ configFile: false, optimizeDeps: { rolldownOptions: { transform: { jsx: { runtime: 'automatic', importSource: 'preact' } } } }, cacheDir: `node_modules/.vite-onboarding-${crypto.randomUUID()}`, server: { host: '127.0.0.1', port: 0, hmr: false }, logLevel: 'error' });
  await server.listen(); url = server.resolvedUrls!.local[0];
  browser = await chromium.launch({ headless: true });
}, 30000);
beforeEach(async () => {
  page = await browser.newPage({ reducedMotion: 'reduce', viewport: { width: 960, height: 600 } });
  await page.goto(url);
  await page.getByRole('button', { name:'Workspace settings', exact:true }).waitFor();
}, 30000);
afterEach(async () => { await page?.close(); });
afterAll(async () => { await browser?.close(); await server?.close(); }, 30000);
async function openGuide() {
  await page.getByRole('button', { name:'Workspace settings', exact:true }).click();
  await page.getByRole('button', { name:'사용법 다시 보기', exact:true }).click();
  await page.getByRole('dialog', { name:'AxDB 사용법' }).waitFor();
}

it('navigates by keyboard and persists skipping for the mock session', async () => {
  await page.goto(`${url}?onboarding`);
  const dialog = page.getByRole('dialog', { name: 'AxDB 사용법' });
  await dialog.waitFor();
  await page.locator('.ob-content').focus();
  await page.keyboard.press('ArrowRight');
  await page.waitForFunction(() => document.querySelector('.ob-eyebrow')?.textContent?.includes('DB 드라이버'));
  await page.keyboard.press('ArrowLeft');
  await page.waitForFunction(() => document.querySelector('.ob-eyebrow')?.textContent?.includes('환영'));
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => document.querySelector('.ob-eyebrow')?.textContent?.includes('DB 드라이버'));
  expect(await page.locator('.ob-eyebrow').textContent()).toContain('DB 드라이버');
  await page.keyboard.press('Escape');
  await dialog.waitFor({state:'detached'});
  const status = await page.evaluate(`(async () => {
    const host = await import('/src/onboarding/host.ts');
    return host.shouldShowOnboarding(await host.onboardingStatus());
  })()`);
  expect(status).toBe(false);
});

it('uses the rendered step when the next key arrives before passive effects', async () => {
  await openGuide();
  await page.locator('.ob-content').focus();
  await page.keyboard.press('ArrowRight');
  await page.waitForFunction(() => document.querySelector('.ob-eyebrow')?.textContent?.includes('DB 드라이버'));
  await page.waitForTimeout(200); // Settle the previous handler before exercising the next render boundary.
  // Dispatch at the DOM boundary, before a passive effect can replace an old handler.
  await page.evaluate(() => {
    const observer = new MutationObserver(() => {
      if (!document.querySelector('.ob-eyebrow')?.textContent?.includes('환영')) return;
      observer.disconnect();
      document.querySelector('.ob-content')!.dispatchEvent(new KeyboardEvent('keydown', {key:'Enter', bubbles:true}));
    });
    observer.observe(document.querySelector('.ob-content')!, {childList:true, subtree:true});
  });
  await page.keyboard.press('ArrowLeft');
  await page.waitForFunction(() => document.querySelector('.ob-eyebrow')?.textContent?.includes('DB 드라이버'));
  expect(await page.locator('.ob-eyebrow').textContent()).toContain('02 / 07');
});

it('reopens from settings', async () => {
  const dialog = page.getByRole('dialog', { name: 'AxDB 사용법' });
  await page.getByRole('button', { name: 'Workspace settings', exact:true }).click();
  await page.getByRole('button', { name: '사용법 다시 보기', exact:true }).click();
  await dialog.waitFor();
  expect(await page.locator('.ob-eyebrow').textContent()).toContain('환영');
  await dialog.getByRole('button',{name:'건너뛰기',exact:true}).click();
  await dialog.waitFor({state:'detached'});
});

it('reopens from the command palette', async () => {
  const dialog = page.getByRole('dialog', { name:'AxDB 사용법' });
  await page.keyboard.press('Meta+k');
  await page.getByRole('option').filter({hasText:'사용법 다시 보기'}).click();
  await dialog.waitFor();
  await page.keyboard.press('Escape');
  await dialog.waitFor({state:'detached'});
});

it('defaults mock automation to no modal and copies no fake credential', async () => {
  await page.getByRole('button', { name:'Workspace settings',exact:true }).waitFor();
  expect(await page.getByRole('dialog', {name:'AxDB 사용법'}).count()).toBe(0);
  await openGuide();
  await page.getByRole('button',{name:'6. MCP 연결',exact:true}).click();
  await page.getByRole('button',{name:'먼저 켜기',exact:true}).click();
  expect(await page.locator('.ob-feedback').textContent()).toContain('먼저 MCP 브리지를 켜세요');
  await page.getByRole('checkbox',{name:'온보딩 MCP 브리지',exact:true}).check();
  await page.getByRole('button',{name:'클라이언트 설정 JSON 복사',exact:true}).click();
  await page.waitForFunction(() => document.querySelector('.ob-feedback')?.textContent?.includes('모의 화면에서는 실제 MCP 설정'));
  expect(await page.locator('.ob-masked').textContent()).toContain('••••••••');
  await page.keyboard.press('Escape');
});

it('fits both themes at minimum size, respects reduced motion, and releases animations', async () => {
  await page.emulateMedia({reducedMotion: capture ? 'no-preference' : 'reduce'});
  const errors: string[] = [];
  page.on('pageerror', error=>errors.push(error.message));
  await openGuide();
  await page.getByRole('dialog',{name:'AxDB 사용법'}).waitFor();
  for (const theme of ['midnight','daylight']) {
    await page.evaluate(theme => { document.documentElement.dataset.theme = theme; }, theme);
    for (let i=0;i<7;i++) {
      await page.locator('.ob-dots button').nth(i).click();
      if (capture) await page.waitForTimeout(1800);
      const fit = await page.locator('.onboarding').evaluate(node => {
        const rect = node.getBoundingClientRect();
        const body = node.querySelector('.dialog-body')!;
        return { inside: rect.x>=0 && rect.y>=0 && rect.right<=innerWidth && rect.bottom<=innerHeight, clipped: body.scrollHeight>body.clientHeight+1 };
      });
      expect(fit).toEqual({ inside:true, clipped:false });
      if (capture) await page.screenshot({path:`../docs/design-shots/onboarding-${i+1}-${theme}.png`});
    }
  }
  if (capture) {
  await page.evaluate(() => { document.documentElement.dataset.theme='midnight'; });
  await page.locator('.ob-dots button').first().click();
  // Pause actual entry animations at deterministic times to capture their motion frames.
  await page.locator('.ob-illustration').evaluate(node => node.getAnimations({subtree:true}).forEach(animation=>animation.pause()));
  for (const [index,time] of [0,400,1200].entries()) {
    await page.locator('.ob-illustration').evaluate((node,time)=>node.getAnimations({subtree:true}).forEach(animation=>{animation.currentTime=time;}),time);
    await page.screenshot({path:`../docs/design-shots/onboarding-entry-${index+1}.png`});
  }
  }
  await page.emulateMedia({reducedMotion:'reduce'});
  await page.locator('.ob-dots button').nth(5).click();
  expect(await page.locator('.onboarding').evaluate(node=>node.getAnimations({subtree:true}).length)).toBe(0);
  await page.keyboard.press('Escape');
  await page.getByRole('dialog',{name:'AxDB 사용법'}).waitFor({state:'detached'});
  for (let i=0;i<6;i++) { await page.waitForTimeout(500); expect(await page.evaluate(()=>document.getAnimations().map(animation=>({state:animation.playState,target:(animation.effect as KeyframeEffect)?.target?.outerHTML})))).toEqual([]); }
  await page.emulateMedia({reducedMotion:'no-preference'});
  expect(errors).toEqual([]);
}, 45000);

it('releases the MCP scene immediately when skipped', async () => {
  await page.emulateMedia({reducedMotion:'no-preference'});
  await openGuide();
  await page.getByRole('dialog',{name:'AxDB 사용법'}).waitFor();
  await page.waitForFunction(() => !document.querySelector('.empty-workspace')?.getAnimations({subtree:true}).length);
  await page.getByRole('button',{name:'6. MCP 연결',exact:true}).click();
  await page.keyboard.press('Escape');
  await page.getByRole('dialog',{name:'AxDB 사용법'}).waitFor({state:'detached'});
  for (let i=0;i<6;i++) {
    await page.waitForTimeout(500);
    expect(await page.evaluate(()=>document.getAnimations().map(animation=>({state:animation.playState,target:(animation.effect as KeyframeEffect)?.target?.outerHTML})))).toEqual([]);
  }
},10000);


it('installs a selected driver in place and locks navigation until it finishes', async () => {
  await openGuide();
  await page.getByRole('button', {name:'2. DB 드라이버',exact:true}).click();
  await page.getByRole('checkbox', {name:'PostgreSQL 선택',exact:true}).check();
  await page.getByRole('button', {name:'선택한 드라이버 1개 설치',exact:true}).click();
  await page.getByRole('button', {name:'설치 중…',exact:true}).waitFor();
  expect(await page.getByRole('button', {name:'다음',exact:true}).isDisabled()).toBe(true);
  await page.keyboard.press('Escape');
  expect(await page.getByRole('dialog', {name:'AxDB 사용법'}).isVisible()).toBe(true);
  await page.getByText('선택한 드라이버를 설치했습니다.', {exact:true}).waitFor();
  expect(await page.getByRole('checkbox', {name:'PostgreSQL 선택',exact:true}).isDisabled()).toBe(true);
  expect(await page.getByRole('button', {name:'다음',exact:true}).isEnabled()).toBe(true);
  const installed = await page.evaluate(`(async () => (await (await import('/src/api.ts')).api('drivers_list', {})).map(driver => driver.id))()`);
  expect(installed).toContain('postgres');
});


it('guides users to a connection and sample query when no export is available', async () => {
  await openGuide();
  await page.getByRole('button', {name:'7. 파일과 AI',exact:true}).click();
  await page.getByRole('button', {name:'결과 내보내기 보여 주기',exact:true}).click();
  expect(await page.locator('.ob-feedback').textContent()).toContain('샘플 쿼리를 실행');
  expect(await page.getByRole('button', {name:'샘플 쿼리 열기',exact:true}).isVisible()).toBe(true);
  await page.getByRole('button', {name:'이미지 내보내기 보여 주기',exact:true}).click();
  expect(await page.locator('.ob-feedback').textContent()).toContain('ERD 또는 그래프');
});

describe('existing export toolbar', () => {
  beforeEach(async () => {
    await page.evaluate(`(async () => { await (await import('/src/drivers/api.ts')).changeDriver('install', 'neo4j'); })()`);
    await page.getByRole('button', {name:'Connect or disconnect Graph playground',exact:true}).click();
    await page.getByTestId('run-query').click();
    await page.getByRole('button', {name:'이미지 내보내기',exact:true}).waitFor();
  }, 30000);
  it.each(['결과', '이미지'])('focuses the real %s export button without exporting automatically', async kind => {
    await openGuide();
    await page.getByRole('button', {name:'7. 파일과 AI',exact:true}).click();
    await page.getByRole('button', {name:`${kind} 내보내기 보여 주기`,exact:true}).click();
    await page.getByRole('dialog', {name:'AxDB 사용법'}).waitFor({state:'detached'});
    expect(await page.evaluate(()=>document.activeElement?.getAttribute('aria-label'))).toBe(`${kind} 내보내기`);
    expect(await page.locator('.ob-export-target').count()).toBe(1);
    expect(await page.getByRole('dialog').count()).toBe(0);
    await page.getByRole('button', {name:'Workspace settings',exact:true}).focus();
    expect(await page.locator('.ob-export-target').count()).toBe(0);
  });
});
