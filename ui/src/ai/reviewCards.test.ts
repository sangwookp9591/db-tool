import {afterAll,beforeAll,expect,it} from 'vitest';
import {chromium,type Browser,type Page} from 'playwright';
import {createServer,type ViteDevServer} from 'vite';
let server:ViteDevServer,browser:Browser,page:Page;
beforeAll(async()=>{
 server=await createServer({configFile:false,plugins:[{name:'review-test',enforce:'pre',configureServer(server){server.middlewares.use('/review-test',(_req,res)=>{res.setHeader('Content-Type','text/html');res.end('<html><body></body></html>');});},resolveId(id,importer){if(id==='virtual:review-test')return id;if(id==='../api'&&importer?.includes('/ai/ToolApproval.tsx'))return 'virtual:tool-api';},load(id){if(id==='virtual:tool-api')return `export async function api(command,args){window.calls.push({command,args});if(command==='ai_tool_policy')return {allowed:!args.readOnly,read_only:window.isRead,destructive:!window.isRead,connection:'Test',mode:args.readOnly?'read_only':'write_with_approval',reason:args.readOnly?'읽기 전용 연결':undefined};return window.isRead?{row_count:3,elapsed_ms:7}:{preview:{statements:[],notes:[],committed:false},elapsed_ms:7};}`;if(id==='virtual:review-test')return `import {h,render} from 'preact';import {useState} from 'preact/hooks';import {Editor} from '/src/Editor.tsx';import {FileReview} from '/src/ai/FileReview.tsx';import {ToolApproval} from '/src/ai/ToolApproval.tsx';window.calls=[];window.isRead=true;window.handles={};window.remembered=new Set();const root=document.createElement('div');document.body.replaceChildren(root);function App(){const [files,setFiles]=useState([1,2,3].map(i=>({id:String(i),path:i+'.sql',after:'SELECT '+i,before:'original '+i,decision:'pending'}))),[tool,setTool]=useState({id:'read',session:'s',request:{name:'run_query',arguments:{text:'SELECT 1'}},reason:'check',status:'pending',readOnly:false}),[locked,setLocked]=useState(false);window.setLocked=setLocked;window.setTool=setTool;return h('div',{},files.map(f=>h(Editor,{key:f.id,text:f.before,mode:'sql',onChange:()=>{},onReady:handle=>window.handles[f.id]=handle,onRun:()=>{},onError:()=>{}})),h(FileReview,{files,session:'s',onChange:setFiles,actions:{apply:async(_s,f)=>{window.handles[f.id].replace(f.after);return {...f,decision:'accepted'};},undo:(_s,f)=>{window.handles[f.id].undo();return {...f,decision:'pending'};}}}),h(ToolApproval,{tool,readOnly:locked,remembered:window.remembered,onChange:patch=>setTool(t=>({...t,...patch}))}));}render(h(App),root);`;}}],optimizeDeps:{rolldownOptions:{transform:{jsx:{runtime:'automatic',importSource:'preact'}}}},cacheDir:`node_modules/.vite-review-${crypto.randomUUID()}`,server:{host:'127.0.0.1',port:0,hmr:false},logLevel:'error'});
 await server.listen();browser=await chromium.launch({headless:true});page=await browser.newPage();await page.goto(server.resolvedUrls!.local[0]+'review-test');await page.addScriptTag({type:'module',url:'/@id/virtual:review-test'});await page.locator('.ai-file-review').waitFor();
},30000);
afterAll(async()=>{await browser?.close();await server?.close();});
it('rejects one file, accepts the remaining files in single editor transactions, and undoes both',async()=>{
 await page.locator('.ai-file').nth(0).getByRole('button',{name:'거절',exact:true}).click();await page.getByRole('button',{name:'전체 수락',exact:true}).click();
 await page.waitForFunction(()=>document.querySelectorAll('.ai-file')[2].getAttribute('data-decision')==='accepted');
 expect(await page.evaluate(()=>Object.values((window as any).handles).map((h:any)=>h.text()))).toEqual(['original 1','SELECT 2','SELECT 3']);
 await page.getByRole('button',{name:'수락 되돌리기',exact:true}).click();await page.waitForFunction(()=>![...document.querySelectorAll('.ai-file')].some(f=>f.getAttribute('data-decision')==='accepted'));
 expect(await page.evaluate(()=>Object.values((window as any).handles).map((h:any)=>h.text()))).toEqual(['original 1','original 2','original 3']);
});
it('routes native history actions to the focused query editor',async()=>{
 await page.evaluate(()=>{const handles=(window as any).handles;handles['3'].replace('native history');Object.defineProperty(document,'hasFocus',{configurable:true,value:()=>false});window.dispatchEvent(new CustomEvent('ax:editor-history',{detail:'undo'}));delete (document as any).hasFocus;});
 expect(await page.evaluate(()=>Object.values((window as any).handles).map((h:any)=>h.text()))).toEqual(['original 1','original 2','original 3']);
});
it('remembers only reads and cancels a destructive fuse before execution',async()=>{
 await page.getByRole('checkbox',{name:'이 세션에서 이 도구 다시 묻지 않기'}).check();await page.getByRole('button',{name:'허용',exact:true}).click();await page.locator('.tool-group').waitFor();
 expect(await page.evaluate(()=>(window as any).remembered.size)).toBe(1);
 await page.evaluate(()=>{(window as any).isRead=false;(window as any).setTool({id:'delete',session:'s',request:{name:'run_query',arguments:{text:'DELETE FROM t'}},reason:'remove',status:'pending',readOnly:false});});
 const allow=page.getByRole('button',{name:'허용',exact:true});await allow.waitFor();expect(await page.getByRole('checkbox').count()).toBe(0);
 const calls=await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.command==='ai_tool_decide').length);
 await allow.click();await page.getByRole('button',{name:'취소',exact:true}).waitFor();await page.keyboard.press('Escape');await allow.waitFor();
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.command==='ai_tool_decide').length)).toBe(calls);
 await page.evaluate(()=>(window as any).setLocked(true));await page.waitForFunction(()=>document.querySelector('.invalid')?.textContent?.includes('읽기 전용')===true);
 expect(await allow.isDisabled()).toBe(true);
 await page.getByRole('button',{name:'거부',exact:true}).click();await page.waitForFunction(()=>document.querySelector('.tool-group')?.textContent?.includes('도구 1개')===true);
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.command==='ai_tool_decide').at(-1).args.decision)).toBe('deny');
});
