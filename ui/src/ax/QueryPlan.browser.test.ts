import { afterAll, beforeAll, expect, it } from 'vitest';
import { chromium, type Browser, type Page } from 'playwright';
import { createServer, type ViteDevServer } from 'vite';
let server:ViteDevServer,browser:Browser,base:string;
beforeAll(async()=>{
 server=await createServer({configFile:false,plugins:[{
  name:'editor-analysis-probe',enforce:'pre',
  configureServer(server){server.middlewares.use('/analysis-probe',(_req,res)=>{res.setHeader('Content-Type','text/html');res.end('<html><body><main id="root"></main></body></html>');});},
  resolveId(id){if(id.startsWith('virtual:analysis-'))return id;if(id==='./api'||id==='../api')return 'virtual:analysis-api';},
  load(id){
   if(id==='virtual:analysis-api')return `export const isMock=true;export function api(command,args){(window.__calls??=[]).push({command,...args});if(command==='ai_settings')return Promise.resolve({settings:{enabled:false,auth:'api_key'},has_key:false});if(command==='cancel')return Promise.resolve();if(command==='explain_query'||command==='lint_query')return new Promise(resolve=>(window.__pending??={})[args.runId??args.query]=resolve);return Promise.resolve([]);}`;
   if(id==='virtual:analysis-probe')return `import {h,render} from 'preact';import {useState} from 'preact/hooks';import {QueryPlan} from '/src/ax/QueryPlan.tsx';import {QueryTab} from '/src/QueryTab.tsx';import '/src/results.css';function Probe(){const [query,setQuery]=useState('SELECT 1');window.__setQuery=setQuery;return window.__tab?h(QueryTab,{initialText:'SELECT * FROM people',sessionId:'fixture',manifest:{id:'postgres',query_language:{editor_mode:'sql',display:'SQL'},capabilities:{lint:true,explain:true,params:false,cancel:'Server'}},graphLimit:100,onRun:()=>{},onReady:h=>window.__editor=h,onHistory:()=>{},onError:()=>{}}):h(QueryPlan,{sessionId:'fixture',query,onInsert:()=>{}});}window.__unmount=()=>render(null,document.querySelector('#root'));render(h(Probe),document.querySelector('#root'));`;
  }
 }],optimizeDeps:{rolldownOptions:{transform:{jsx:{runtime:'automatic',importSource:'preact'}}}},cacheDir:`node_modules/.vite-analysis-${crypto.randomUUID()}`,server:{host:'127.0.0.1',port:0,hmr:false},logLevel:'error'});
 await server.listen();base=server.resolvedUrls!.local[0];browser=await chromium.launch({headless:true});
},30000);
afterAll(async()=>{await browser?.close();await server?.close();});
async function open(tab=false){const page=await browser.newPage();page.setDefaultTimeout(7000);await page.goto(base+'analysis-probe');await page.evaluate(tab=>{(window as any).__tab=tab;},tab);await page.addScriptTag({type:'module',url:'/@id/virtual:analysis-probe'});await page.waitForFunction(()=>!!(window as any).__setQuery);return page;}
async function calls(page:Page,command:string){return page.evaluate(command=>((window as any).__calls??[]).filter((call:any)=>call.command===command),command);}
const plan={op:'Result',target:null,cost:1,est_rows:1,actual_rows:null,warnings:[],children:[]};
it('explains once per click, cancels superseded requests and ignores late results',async()=>{
 const page=await open();try{
  await page.getByRole('button',{name:'실행 계획 읽기',exact:true}).click();const first=(await calls(page,'explain_query'))[0];
  await page.evaluate(()=>{(window as any).__setQuery('SELECT 2');});
  await expect.poll(async()=>calls(page,'cancel')).toHaveLength(1);
  await page.getByRole('button',{name:'실행 계획 읽기',exact:true}).click();const second=(await calls(page,'explain_query'))[1];
  await page.evaluate(({id,plan})=>(window as any).__pending[id](plan),{id:first.runId,plan});
  expect(await page.getByRole('status').count()).toBe(1);expect(await page.locator('.ax-plan-summary').count()).toBe(0);
  await page.evaluate(({id,plan})=>(window as any).__pending[id](plan),{id:second.runId,plan});
  await page.locator('.ax-plan-summary').waitFor();await page.waitForTimeout(850);
  expect(await calls(page,'explain_query')).toHaveLength(2);expect(await calls(page,'lint_query')).toHaveLength(0);
  await page.getByRole('button',{name:'실행 계획 읽기',exact:true}).click();await page.evaluate(()=>(window as any).__unmount());
  await expect.poll(async()=>calls(page,'cancel')).toHaveLength(2);
 }finally{await page.close();}
},20000);
it('offers cancellation while a plan request is pending',async()=>{
 const page=await open();try{await page.getByRole('button',{name:'실행 계획 읽기',exact:true}).click();await page.getByRole('button',{name:'계획 읽기 취소',exact:true}).click();expect(await calls(page,'cancel')).toHaveLength(1);}finally{await page.close();}
},15000);
it('dispatches memo save without a selection and disables it for an empty query',async()=>{
 const page=await open(true);try{
  await page.waitForFunction(()=>!!(window as any).__editor);
  await page.evaluate(()=>{(window as any).__savedMemo=null;window.addEventListener('ax:snippet-save',event=>{(window as any).__savedMemo=(event as CustomEvent).detail;},{once:true});});
  await page.evaluate(()=>(window as any).__editor.replace('SELECT 2'));
  await page.getByRole('button',{name:'메모로 저장'}).click();
  await expect.poll(()=>page.evaluate(()=>(window as any).__savedMemo)).toEqual({text:undefined});
  await page.evaluate(()=>(window as any).__editor.replace(''));
  expect(await page.getByRole('button',{name:'메모로 저장'}).isDisabled()).toBe(true);
 }finally{await page.close();}
},15000);
it('retains diagnostics and editor geometry during typing, debounces lint, and rejects old results',async()=>{
 const page=await open(true);try{
  await page.waitForFunction(()=>!!(window as any).__editor);await expect.poll(async()=>calls(page,'lint_query')).toHaveLength(1);
  await page.evaluate(()=>(window as any).__pending['SELECT * FROM people']([{rule_id:'select_star',start:0,end:6,severity:'warning',message:'Previous diagnostic',quick_fix:null}]));
  await page.getByText('Previous diagnostic',{exact:false}).waitFor();const before=await page.locator('.editor-pane').boundingBox();
  await page.evaluate(()=>(window as any).__editor.replace('SELECT 2'));
  expect(await page.getByText('Previous diagnostic',{exact:false}).count()).toBe(1);expect(await page.locator('.editor-pane').boundingBox()).toEqual(before);
  await page.waitForTimeout(250);expect(await calls(page,'lint_query')).toHaveLength(1);
  await expect.poll(async()=>calls(page,'lint_query')).toHaveLength(2);
  await page.evaluate(()=>(window as any).__editor.replace('SELECT 3'));
  await expect.poll(async()=>calls(page,'lint_query')).toHaveLength(3);
  await page.evaluate(()=>(window as any).__pending['SELECT 3']([{rule_id:'new',start:0,end:6,severity:'warning',message:'Latest diagnostic',quick_fix:null}]));
  await page.getByText('Latest diagnostic',{exact:false}).waitFor();
  await page.evaluate(()=>(window as any).__pending['SELECT 2']([]));await page.waitForTimeout(100);
  expect(await page.getByText('Latest diagnostic',{exact:false}).count()).toBe(1);expect(await page.locator('.editor-pane').boundingBox()).toEqual(before);
  expect(await calls(page,'explain_query')).toHaveLength(0);
 }finally{await page.close();}
},20000);
