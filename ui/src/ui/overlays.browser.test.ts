import { afterAll, beforeAll, expect, it } from 'vitest';
import { chromium, webkit, type Browser, type Page } from 'playwright';
import { createServer, type ViteDevServer } from 'vite';
import { menuPosition } from './FloatingMenu';

declare const process: { env: Record<string,string|undefined> };
let server: ViteDevServer, browser: Browser, base: string;
beforeAll(async()=>{
  server=await createServer({configFile:false,plugins:[{
    name:'overlay-probe',configureServer(s){s.middlewares.use('/overlay-probe',(_req,res)=>{res.setHeader('Content-Type','text/html');res.end('<html><body><main id="root"></main></body></html>');});},
    resolveId(id){if(id==='virtual:overlay-probe')return id;},
    load(id){if(id==='virtual:overlay-probe')return `import {h,render} from 'preact';import {useState} from 'preact/hooks';import {GlideSelect} from '/src/ui/GlideSelect.tsx';import {Dialog} from '/src/Dialog.tsx';import '/src/style.css';
      function Probe(){const [value,setValue]=useState(''),[count,setCount]=useState(0);return h(Dialog,{label:'선택 검증',onClose:()=>window.closedDialog=true,footer:h('button',null,'저장')},h('form',{onSubmit:e=>e.preventDefault()},h(GlideSelect,{ariaLabel:'많은 옵션',name:'choice',required:true,value,onChange:v=>{setValue(v);setCount(n=>n+1)},options:Array.from({length:50},(_,i)=>'항목 '+i)}),h('button',{type:'submit'},'검증'),h('output',null,value+' / '+count)));}render(h(Probe),document.querySelector('#root'));`;},
  }],optimizeDeps:{rolldownOptions:{transform:{jsx:{runtime:'automatic',importSource:'preact'}}}},cacheDir:`node_modules/.vite-overlays-${crypto.randomUUID()}`,server:{host:'127.0.0.1',port:0,hmr:false},logLevel:'error'});
  await server.listen();base=server.resolvedUrls!.local[0];
  browser=await (process.env.AXDB_QA_BROWSER==='webkit'?webkit:chromium).launch({headless:true,...(process.env.AXDB_QA_WEBKIT_PATH?{executablePath:process.env.AXDB_QA_WEBKIT_PATH}:{})});
},30000);
afterAll(async()=>{await browser?.close();await server?.close();});

it('places a tall menu above the lower right corner and clamps negative points',()=>{
  expect(menuPosition({left:955,top:590,bottom:590},{width:220,height:190},{width:960,height:600})).toEqual({left:732,top:396});
  expect(menuPosition({left:-10,top:-20,bottom:-20},{width:220,height:190},{width:960,height:600})).toEqual({left:8,top:8});
});

async function reachable(page:Page,selector:string){
  const result=await page.locator(selector).evaluate(el=>{
    const r=el.getBoundingClientRect();
    return {bounds:r.left>=0&&r.top>=0&&r.right<=innerWidth&&r.bottom<=innerHeight,items:[...el.querySelectorAll('button')].map(button=>{const b=button.getBoundingClientRect(),hit=document.elementFromPoint(b.left+b.width/2,b.top+b.height/2);return {visible:button.contains(hit),label:button.textContent,hit:hit?.outerHTML.slice(0,250)};})};
  });
  expect(result.bounds,selector).toBe(true);expect(result.items.filter(item=>!item.visible),selector).toEqual([]);
}

for(const theme of ['daylight','midnight'])for(const [width,height] of [[960,600],[1280,800],[1920,1080]]){
  it(`connection/context/refactor menus remain clickable, ${theme} ${width}x${height}`,async()=>{
    const page=await browser.newPage({viewport:{width,height},reducedMotion:'reduce'});page.setDefaultTimeout(6000);
    try{
      await page.addInitScript(theme=>localStorage.setItem('axdb-theme',theme),theme);
      await page.goto(base);
      await page.getByRole('button',{name:'드라이버',exact:true}).click();
      await page.getByRole('tab',{name:'사용 가능',exact:true}).click();
      await page.getByRole('button',{name:'PostgreSQL · 설치 가능',exact:true}).click();
      await page.getByRole('button',{name:'PostgreSQL · 설치됨',exact:true}).waitFor();
      await page.getByRole('button',{name:'드라이버 화면 닫기'}).click();
      if(await page.locator('.app').getAttribute('data-sidebar-open')!=='true')await page.getByLabel('Connections panel').click();
      const trigger=page.getByRole('button',{name:'Connection actions for Analytics sandbox',exact:true});
      await trigger.click();await reachable(page,'.connection-menu');
      // Delete below the sidebar boundary is reachable; dismissing cancels the delayed action.
      await page.getByLabel('Delete Analytics sandbox',{exact:true}).click();
      await page.keyboard.press('Escape');
      await page.locator('.connection-menu').waitFor({state:'detached'});
      expect(await trigger.evaluate(el=>el===document.activeElement)).toBe(true);
      await trigger.click();expect(await page.getByLabel('Delete Analytics sandbox',{exact:true}).count()).toBe(1);
      await page.getByRole('menuitem',{name:'편집',exact:true}).click();
      await page.getByRole('dialog',{name:'Edit connection',exact:true}).waitFor();
      await reachable(page,'dialog[open] .dialog-footer');
      await page.getByRole('button',{name:'3. 인증',exact:true}).click();
      if(process.env.AXDB_QA_SCREENSHOTS)await page.screenshot({path:`${process.env.AXDB_QA_SCREENSHOTS}/form-${theme}-${width}.png`});
      await page.getByLabel('Close connection form',{exact:true}).click();
      await trigger.click();
      await page.getByRole('menuitem',{name:'연결하기',exact:true}).click();
      await page.getByRole('button',{name:'people',exact:true}).waitFor();
      await page.evaluate(()=>window.dispatchEvent(new CustomEvent('ax:mock-network',{detail:{event:'network',profile_id:'unrelated-profile',state:'disconnected',attempt:0}})));
      expect(await page.locator('.statusbar').innerText()).not.toContain('네트워크 연결 끊김');
      await page.getByRole('button',{name:'people',exact:true}).click({button:'right'});
      if(process.env.AXDB_QA_SCREENSHOTS)await page.screenshot({path:`${process.env.AXDB_QA_SCREENSHOTS}/context-${theme}-${width}.png`});
      await reachable(page,'.context-menu');await page.keyboard.press('End');
      expect(await page.locator('.context-menu button').last().evaluate(el=>el===document.activeElement)).toBe(true);
      await page.keyboard.press('Escape');
      await page.getByLabel('Actions for people',{exact:true}).click();
      await page.getByRole('menuitem',{name:'쿼리 열기'}).click();
      await page.locator('.cm-content:visible').waitFor();
      await page.getByRole('button',{name:/리팩터링/}).first().click();
      await reachable(page,'.ax-refactor-menu');
      if(process.env.AXDB_QA_SCREENSHOTS)await page.screenshot({path:`${process.env.AXDB_QA_SCREENSHOTS}/menus-${theme}-${width}.png`});
      await page.keyboard.press('Escape');
      const native=await page.locator('.cm-content:visible').evaluate(el=>el.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true})));
      expect(native).toBe(true);
      expect(await page.locator('.ax-refactor-menu').count()).toBe(0);
      await page.locator('summary[aria-label="Execution options"]:visible').click();await reachable(page,'.execution-options:visible');
      await page.locator('summary[aria-label="Execution options"]:visible').click();
      await page.getByRole('button',{name:'Workspace settings',exact:true}).click();
      await reachable(page,'.settings-popover');
      await page.getByRole('combobox',{name:'Appearance',exact:true}).click();
      const selectBounds=await page.locator('.glide-select__menu').boundingBox();
      expect(selectBounds!.x+selectBounds!.width).toBeLessThanOrEqual(width);expect(selectBounds!.y).toBeGreaterThanOrEqual(0);
      await page.keyboard.press('Escape');await page.getByLabel('Close settings',{exact:true}).click();
    }finally{await page.close();}
  },30000);
}

it('selects the clicked row after scrolling a long modal list, once, and preserves the dialog on Escape',async()=>{
  const page=await browser.newPage({viewport:{width:960,height:600},reducedMotion:'reduce'});
  try{
    await page.goto(base+'overlay-probe');await page.addScriptTag({type:'module',url:'/@id/virtual:overlay-probe'});
    await page.getByRole('button',{name:'검증',exact:true}).click();
    const combo=page.getByRole('combobox',{name:'많은 옵션'});
    expect(await combo.getAttribute('aria-invalid')).toBe('true');
    expect(await combo.evaluate(el=>el===document.activeElement)).toBe(true);
    await combo.click();await page.getByRole('option',{name:'항목 45',exact:true}).scrollIntoViewIfNeeded();
    await page.getByRole('option',{name:'항목 45',exact:true}).click();
    expect(await page.locator('output').innerText()).toBe('항목 45 / 1');
    await combo.press('ArrowDown');await combo.press('End');
    await page.getByRole('option',{name:'항목 49',exact:true}).waitFor();
    await combo.press('Enter');expect(await page.locator('output').innerText()).toBe('항목 49 / 2');
    await combo.click();await combo.press('Escape');
    expect(await page.getByRole('dialog').count()).toBe(1);
    expect(await page.getByRole('listbox').count()).toBe(0);
  }finally{await page.close();}
},30000);
