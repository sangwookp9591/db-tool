import { chromium } from 'playwright';
import assert from 'node:assert/strict';
const browser = await chromium.launch({headless:true});
const base = process.env.AX_UI_URL ?? 'http://127.0.0.1:51986';
const output = new URL('../docs/design-shots/',import.meta.url).pathname;
const errors = [];
try {
  for (const theme of ['midnight','daylight']) {
    const context = await browser.newContext({viewport:{width:1280,height:960}});
    await context.addInitScript(theme=>localStorage.setItem('axdb-theme',theme),theme);
    const page = await context.newPage();
    page.on('pageerror',error=>errors.push(error.message));
    // Browser-only fixture: keep the real form/API/event path, with no DB or provider calls.
    await page.route('**/src/ax/connectionContract.ts', async route=>{
      const response = await route.fetch();
      const source = await response.text();
      const marker = 'export async function connectionMock(command, args) {';
      assert.ok(source.includes(marker));
      await route.fulfill({response,body:source.replace(marker,marker+`
        if(command === 'connection_test_steps') {
          for(const stage of ['dns','ssh','tunnel','tls','db_auth','permissions']) {
            const emit = (status, elapsed_ms, code=null) => args.onEvent.onmessage({event:'stage',stage,status,elapsed_ms,code,help_key:null});
            if(['ssh','tunnel','tls'].includes(stage) && !window.__allStages) {emit('skipped',0);continue;}
            emit('running',0);
            if(stage==='db_auth') await new Promise(resolve=>window.__finishConnection=resolve);
            if(stage==='db_auth' && window.__failConnection) {emit('failed',184,'db_auth_failed');throw new Error('DB_AUTH_FAILED');}
            emit('passed',stage==='db_auth'?184:12);
          }
          return 'Mock database · browser demo';
        }
      `)});
    });
    await page.route('**/src/mock.ts', async route=>{
      const response=await route.fetch();
      const source=await response.text();
      const marker='const mockInstalledDrivers = new Map();';
      assert.ok(source.includes(marker));
      await route.fulfill({response,body:source.replace(marker,'const mockInstalledDrivers = new Map([["postgres","1.0.0"]]);')});
    });
    await page.goto(base);
    await page.getByTestId('add-connection').click();
    await page.getByLabel('Name',{exact:true}).fill('Design mock · PostgreSQL');
    await page.getByRole('button',{name:'3. 인증',exact:true}).click();
    await page.getByLabel('User',{exact:true}).fill('demo');
    await page.getByLabel('Password',{exact:true}).fill('mock-only');
    await page.getByRole('button',{name:'1. 기본',exact:true}).click();
    const button = page.getByTestId('test-connection-form');
    const timeline = page.getByLabel('단계별 연결 테스트',{exact:true});
    for(const failed of [false,true]) {
      await page.evaluate(failed=>{window.__failConnection=failed;window.__finishConnection=null;},failed);
      await button.click();
      await page.waitForFunction(()=>typeof window.__finishConnection==='function');
      await timeline.scrollIntoViewIfNeeded();
      assert.equal(await timeline.locator('li').count(),3);
      assert.equal(await timeline.locator('li[data-status="running"]').count(),1);
      assert.equal(await button.textContent(),'연결 중…');
      if(!failed) await page.screenshot({path:`${output}connect-test-running-${theme}.png`});
      await page.evaluate(()=>window.__finishConnection());
      await page.waitForFunction(()=>!document.querySelector('[data-testid="test-connection-form"]').disabled);
      await page.waitForTimeout(650);
      assert.equal(await timeline.locator('[data-status="running"]').count(),0);
      assert.equal(await timeline.evaluate(el=>el.getAnimations({subtree:true}).filter(a=>a.playState==='running').length),0);
      if(failed) {
        assert.equal(await timeline.locator('li[data-status="failed"]').count(),1);
        assert.equal(await timeline.locator('li[data-status="pending"]').count(),1);
        await timeline.getByText(/사용자 이름, 키체인/).waitFor();
        await timeline.getByRole('button',{name:/AI로 진단|로그인 필요/}).waitFor();
      } else {
        assert.equal(await timeline.locator('li[data-status="done"]').count(),3);
        assert.equal(await button.textContent(),'연결됨 ✓');
      }
      await timeline.scrollIntoViewIfNeeded();
      await page.screenshot({path:`${output}connect-test-${failed?'failed':'done'}-${theme}.png`});
    }
    await page.emulateMedia({reducedMotion:'reduce'});
    await page.evaluate(()=>{window.__failConnection=false;window.__allStages=true;window.__finishConnection=null;});
    await button.click();await page.waitForFunction(()=>typeof window.__finishConnection==='function');
    assert.equal(await timeline.locator('li').count(),6);
    assert.equal(await timeline.evaluate(el=>el.getAnimations({subtree:true}).filter(a=>a.effect?.getTiming().iterations===Infinity).length),0);
    await page.evaluate(()=>window.__finishConnection());
    await page.waitForFunction(()=>!document.querySelector('[data-testid="test-connection-form"]').disabled);
    await page.getByLabel('Name',{exact:true}).fill('Changed mock');
    assert.equal(await timeline.count(),0);
    assert.equal(await button.textContent(),'연결 테스트');
    await context.close();
  }
  assert.deepEqual(errors,[]);
  console.log('PASS: 6 mock captures; running/done/failed, retry, omitted stages, all 6 stages, stopped animation, reduced motion, edited-form reset.');
} finally {await browser.close();}
