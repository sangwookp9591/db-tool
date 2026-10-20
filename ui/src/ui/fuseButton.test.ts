import {afterAll,beforeAll,expect,it} from 'vitest';
import {chromium,type Browser,type Page} from 'playwright';
import {createServer,type ViteDevServer} from 'vite';
let server:ViteDevServer,browser:Browser,page:Page;
beforeAll(async()=>{
 server=await createServer({configFile:false,plugins:[{name:'fuse-test',resolveId(id){if(id==='virtual:fuse-test')return id;},configureServer(server){server.middlewares.use('/fuse-test',(_req,res)=>{res.setHeader('Content-Type','text/html');res.end('<html><body></body></html>');});},load(id){if(id==='virtual:fuse-test')return `import {h,render} from 'preact';import {useState} from 'preact/hooks';import {FuseButton} from '/src/ui/FuseButton.tsx';window.events={cancel:0,finish:0,locked:0,reduced:0,hover:0};function App(){const [locked,setLocked]=useState(true);window.setLocked=setLocked;return h('div',{},['cancel','finish','locked','reduced','hover'].map(id=>h('div',{id},h(FuseButton,{label:'허용',undoLabel:'취소',doneLabel:'실행됨',undoWindow:id==='hover'?1000:300,commitOn:'fuseEnd',fuse:'outline',pauseOnHover:true,settle:'stay',size:'sm',disabled:id==='locked'&&locked,onCommit:()=>window.events[id]++}))));}render(h(App),document.body);`;}}],optimizeDeps:{rolldownOptions:{transform:{jsx:{runtime:'automatic',importSource:'preact'}}}},cacheDir:`node_modules/.vite-fuse-${crypto.randomUUID()}`,server:{host:'127.0.0.1',port:0,hmr:false},logLevel:'error'});
 await server.listen();browser=await chromium.launch({headless:true});page=await browser.newPage();await page.goto(server.resolvedUrls!.local[0]+'fuse-test');await page.addScriptTag({type:'module',url:'/@id/virtual:fuse-test'});await page.locator('#cancel .fuse-button').waitFor();
},30000);
afterAll(async()=>{await browser?.close();await server?.close();});
it('arms from the keyboard, keeps cancel focused, and Escape prevents the commit',async()=>{
 expect(await page.evaluate(()=>document.getAnimations().filter(a=>a.playState==='running').length)).toBe(0);
 await page.locator('#cancel .fuse-button__idle').focus();await page.keyboard.press('Enter');await page.waitForFunction(()=>document.querySelector('#cancel .fuse-button')?.getAttribute('data-phase')==='armed');
 expect(await page.locator('#cancel .fuse-button__undo').evaluate(el=>el===document.activeElement)).toBe(true);
 await page.keyboard.press('Escape');await page.waitForFunction(()=>document.querySelector('#cancel .fuse-button')?.getAttribute('data-phase')==='idle');
 await page.waitForTimeout(350);expect(await page.evaluate(()=>(window as any).events.cancel)).toBe(0);
});
it('commits once only at fuseEnd and stays settled',async()=>{
 await page.locator('#finish .fuse-button__idle').click();expect(await page.evaluate(()=>(window as any).events.finish)).toBe(0);
 await page.waitForFunction(()=>(window as any).events.finish===1);expect(await page.locator('#finish .fuse-button').getAttribute('data-phase')).toBe('settled');
 await page.waitForTimeout(350);expect(await page.evaluate(()=>(window as any).events.finish)).toBe(1);
});
it('pauses after leaving and re-entering with the pointer, then resumes',async()=>{
 await page.locator('#hover .fuse-button__idle').click();await page.mouse.move(700,400);await page.locator('#hover .fuse-button__undo').hover();
 await page.waitForFunction(()=>document.querySelector('#hover .fuse-button')?.getAttribute('data-paused')==='true');
 await page.waitForTimeout(1100);expect(await page.evaluate(()=>(window as any).events.hover)).toBe(0);
 await page.mouse.move(700,400);await page.waitForFunction(()=>(window as any).events.hover===1);
});
it('disables read-only approval and replaces reduced-motion fuse animation with remaining seconds',async()=>{
 expect(await page.locator('#locked .fuse-button__idle').isDisabled()).toBe(true);await page.locator('#locked .fuse-button__idle').evaluate((el:HTMLButtonElement)=>el.click());expect(await page.evaluate(()=>(window as any).events.locked)).toBe(0);
 await page.emulateMedia({reducedMotion:'reduce'});await page.waitForFunction(()=>document.querySelector('#reduced .fuse-button')?.getAttribute('data-reduced')==='true');
 await page.locator('#reduced .fuse-button__idle').click();expect(await page.locator('#reduced .fuse-button__countdown').textContent()).toContain('초');expect(await page.locator('#reduced .fuse-button__rim').count()).toBe(0);await page.waitForFunction(()=>(window as any).events.reduced===1);
});
