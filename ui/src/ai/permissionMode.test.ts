import { afterAll, beforeAll, expect, it } from 'vitest';
import { chromium, type Browser, type Page } from 'playwright';
import { createServer, type ViteDevServer } from 'vite';
let server:ViteDevServer,browser:Browser,page:Page;
beforeAll(async()=>{
 server=await createServer({configFile:false,plugins:[{name:'permission-test',configureServer(server){server.middlewares.use('/permission-test',(_req,res)=>{res.setHeader('Content-Type','text/html');res.end('<html><body></body></html>');});},resolveId(id){if(id==='virtual:permission-test')return id;},load(id){if(id==='virtual:permission-test')return `import {h,render} from 'preact';import {PermissionMode} from '/src/ai/PermissionMode.tsx';const root=document.createElement('div');document.body.replaceChildren(root);let readOnly=false;const mount=()=>render(h(PermissionMode,{readOnly,locked:false,connection:'Test',onChange:value=>{readOnly=value;mount();}}),root);mount();`;}}],optimizeDeps:{rolldownOptions:{transform:{jsx:{runtime:'automatic',importSource:'preact'}}}},cacheDir:`node_modules/.vite-permission-${crypto.randomUUID()}`,server:{host:'127.0.0.1',port:0,hmr:false},logLevel:'error'});
 await server.listen();browser=await chromium.launch({headless:true});page=await browser.newPage();
 await page.goto(server.resolvedUrls!.local[0]+'permission-test');
 await page.addScriptTag({type:'module',url:'/@id/virtual:permission-test'});
 await page.locator('.ai-permission').waitFor();
},30000);
afterAll(async()=>{await browser?.close();await server?.close();});
it('opens from keyboard, selects with Enter, cycles arrows, and restores focus on Escape',async()=>{
 const trigger=page.locator('.ai-permission summary');await trigger.focus();await page.keyboard.press('Enter');
 const read=page.getByRole('menuitemradio',{name:'읽기 전용',exact:true}),write=page.getByRole('menuitemradio',{name:'승인 후 쓰기',exact:true});
 await write.waitFor({state:'visible'});await page.waitForFunction(()=>document.activeElement?.getAttribute('aria-label')==='승인 후 쓰기');
 await page.keyboard.press('ArrowUp');expect(await read.evaluate(el=>el===document.activeElement)).toBe(true);
 await page.keyboard.press('Enter');await page.waitForFunction(()=>!document.querySelector('details')?.open);
 await trigger.focus();await page.keyboard.press('ArrowDown');await page.waitForFunction(()=>document.activeElement?.getAttribute('aria-label')==='읽기 전용');
 expect(await read.getAttribute('aria-checked')).toBe('true');await page.keyboard.press('ArrowDown');expect(await write.evaluate(el=>el===document.activeElement)).toBe(true);
 await page.keyboard.press('Escape');expect(await trigger.evaluate(el=>el===document.activeElement)).toBe(true);
 expect(await page.locator('details').getAttribute('open')).toBeNull();
});
