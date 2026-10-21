import { afterAll, beforeAll, expect, it } from 'vitest';
import { chromium, webkit, type Browser, type Page } from 'playwright';
import { createServer, type ViteDevServer } from 'vite';
declare const process:{env:Record<string,string|undefined>};
let server:ViteDevServer,browser:Browser,base:string;
beforeAll(async()=>{
  server=await createServer({configFile:false,plugins:[{
    name:'roles-matrix-probe',
    enforce:'pre',
    configureServer(server){server.middlewares.use('/roles-probe',(_req,res)=>{res.setHeader('Content-Type','text/html');res.end('<html><body><main id="root"></main></body></html>');});},
    resolveId(id,importer){if(id==='virtual:roles-probe'||id==='virtual:roles-api')return id;if(importer?.includes('RolesMatrix.tsx?roles-probe=1')&&id.includes('api'))return 'virtual:roles-api';},
    load(id){
      if(id==='virtual:roles-api')return `
        export const isMock=true;
        const roles=['analyst','editor'].map(name=>({name,login:true,superuser:false,member_of:[],summary:[]}));
        const matrix=roles.flatMap(({name})=>Array.from({length:120},(_,index)=>({role:name,namespace:'public',object:'table_'+String(index+1).padStart(3,'0'),privilege:'SELECT',allowed:true})));
        export async function api(command,args){if(command!=='roles_page')throw Error('Unexpected command '+command);const {query}=args;(window.__rolesCalls??=[]).push(query);if(query.role==='analyst')await new Promise(resolve=>setTimeout(resolve,220));if(query.object==='retry'&&!window.__retryOnce){window.__retryOnce=true;throw Error('temporary roles failure');}const matches=matrix.filter(cell=>cell.role===query.role&&cell.namespace.includes(query.namespace||'')&&cell.object.includes(query.object||''));return {supported:true,reason:null,roles,matrix:matches.slice(query.offset,query.offset+query.limit),page:{...query,total:matches.length,has_more:query.offset+query.limit<matches.length,role_required:!query.role}};}
      `;
      if(id==='virtual:roles-probe')return `import {h,render} from 'preact';import {RolesMatrix} from '/src/ax/RolesMatrix.tsx?roles-probe=1';window.__rolesHarness=true;render(h(RolesMatrix,{sessionId:'roles-fixture',readOnly:window.__rolesReadOnly===true,onPendingChange:()=>{}}),document.querySelector('#root'));`;
    }
  }],optimizeDeps:{rolldownOptions:{transform:{jsx:{runtime:'automatic',importSource:'preact'}}}},cacheDir:`node_modules/.vite-roles-${crypto.randomUUID()}`,server:{host:'127.0.0.1',port:0,hmr:false},logLevel:'error'});await server.listen();base=server.resolvedUrls!.local[0];
  browser=await (process.env.AXDB_QA_BROWSER==='webkit'?webkit:chromium).launch({headless:true,...(process.env.AXDB_QA_WEBKIT_PATH?{executablePath:process.env.AXDB_QA_WEBKIT_PATH}:{})});
},30000);
afterAll(async()=>{await browser?.close();await server?.close();});
async function start(page:Page){
  page.setDefaultTimeout(6000);await page.goto(base);
  await page.getByRole('button',{name:'드라이버',exact:true}).click();await page.getByRole('tab',{name:'사용 가능',exact:true}).click();
  await page.getByRole('button',{name:'PostgreSQL · 설치 가능',exact:true}).click();await page.getByRole('button',{name:'PostgreSQL · 설치됨',exact:true}).waitFor();
  await page.getByRole('button',{name:'드라이버 화면 닫기'}).click();
  await page.getByLabel('Connect or disconnect Analytics sandbox',{exact:true}).click();
  await page.getByRole('button',{name:'people',exact:true}).waitFor();await page.getByRole('button',{name:'AX 워크벤치',exact:true}).click();
  await page.getByRole('tab',{name:'사용자 · 권한',exact:true}).click();
  await page.locator('.roles-list-item').first().waitFor();
}
for(const theme of ['daylight','midnight'])for(const width of [960,1280])it(`role workflow, pending changes and wrapped navigation ${theme} ${width}`,async()=>{
  const page=await browser.newPage({viewport:{width,height:800},reducedMotion:'reduce'});
  try{
    await page.addInitScript(theme=>localStorage.setItem('axdb-theme',theme),theme);await start(page);
    expect(await page.getByText('조회할 사용자 또는 역할을 선택하세요',{exact:true}).isVisible()).toBe(true);
    await page.getByRole('button',{name:'analyst 로그인 가능',exact:true}).click();
    const revoke=page.getByRole('button',{name:'analyst public.people SELECT 회수 계획',exact:true});await revoke.waitFor();await revoke.click();
    await page.getByRole('button',{name:'editor 로그인 가능',exact:true}).click();
    await page.getByRole('button',{name:'editor public.people UPDATE 회수 계획',exact:true}).click();
    expect(await page.locator('.roles-review li').count()).toBe(2);
    await page.getByRole('tab',{name:'AI 계정',exact:true}).click();await page.getByRole('dialog',{name:'권한 변경 계획 유지'}).waitFor();
    await page.getByRole('button',{name:'계속 검토',exact:true}).click();expect(await page.locator('.roles-review li').count()).toBe(2);
    await page.getByRole('button',{name:'권한 변경 미리보기',exact:true}).click();await page.locator('.roles-plan').waitFor();
    expect(await page.getByRole('button',{name:'승인한 권한 변경 적용',exact:true}).isDisabled()).toBe(true);
    await page.getByRole('checkbox',{name:'위 2개 권한 변경을 확인했습니다.',exact:true}).check();
    await page.getByRole('button',{name:'editor UPDATE 계획 제거',exact:true}).click();
    expect(await page.locator('.roles-plan').count()).toBe(0);
    await page.getByRole('button',{name:'권한 변경 미리보기',exact:true}).click();await page.locator('.roles-plan').waitFor();
    await page.getByRole('checkbox',{name:'위 1개 권한 변경을 확인했습니다.',exact:true}).check();
    await page.getByRole('button',{name:'승인한 권한 변경 적용',exact:true}).click();await page.getByText('1개 권한 변경을 적용했습니다.',{exact:true}).waitFor();
    expect(await page.locator('.roles-review li').count()).toBe(0);
    await page.getByLabel('권한 객체 필터',{exact:true}).fill('missing');await page.getByRole('button',{name:'조회',exact:true}).click();
    await page.getByText('조건에 맞는 권한이 없습니다. 조회 범위를 넓혀 보세요.',{exact:true}).waitFor();
    await page.getByLabel('권한 객체 필터',{exact:true}).fill('');await page.getByRole('button',{name:'조회',exact:true}).click();await page.locator('.roles-table tbody tr').first().waitFor();
    await page.locator('.ax-content').evaluate(el=>el.scrollTop=0);
    const overflow=await page.locator('.ax-workbench').evaluate(root=>({page:root.scrollWidth>root.clientWidth,tabs:root.querySelector('.workbench-tabs')!.scrollWidth>root.querySelector('.workbench-tabs')!.clientWidth}));
    expect(overflow).toEqual({page:false,tabs:false});
    if(process.env.AXDB_QA_SCREENSHOTS)await page.screenshot({path:`${process.env.AXDB_QA_SCREENSHOTS}/roles-${theme}-${width}.png`});
  }finally{await page.close();}
},30000);

it('keeps permission edits unavailable in read-only mode',async()=>{
  const page=await openRolesProbe(true);
  try{await page.getByRole('button',{name:'analyst 로그인 가능',exact:true}).click();await page.locator('.roles-table tbody tr').first().waitFor();expect(await page.getByRole('button',{name:'analyst public.table_001 SELECT 회수 계획',exact:true}).isDisabled()).toBe(true);expect(await page.locator('.roles-manual').count()).toBe(0);expect(await page.getByRole('button',{name:'권한 변경 미리보기',exact:true}).count()).toBe(0);}finally{await page.close();}
},30000);

async function openRolesProbe(readOnly=false){const page=await browser.newPage({viewport:{width:1000,height:760},reducedMotion:'reduce'});page.setDefaultTimeout(5000);await page.goto(base+'roles-probe');await page.evaluate(value=>{(window as any).__rolesReadOnly=value;},readOnly);await page.addScriptTag({type:'module',url:'/@id/virtual:roles-probe'});await page.waitForFunction(()=>Boolean((window as any).__rolesHarness));await page.getByRole('button',{name:'analyst 로그인 가능',exact:true}).waitFor();return page;}

it('paginates 120 role permissions in 50-row pages and retains pending changes across pages',async()=>{
 const page=await openRolesProbe();try{await page.getByRole('button',{name:'analyst 로그인 가능',exact:true}).click();await page.locator('.roles-table tbody tr').first().waitFor();expect(await page.locator('.roles-table tbody tr').count()).toBe(50);expect(await page.locator('.roles-pagination').innerText()).toContain('1–50 / 120');await page.getByRole('button',{name:'analyst public.table_001 SELECT 회수 계획',exact:true}).click();await page.getByRole('button',{name:'다음',exact:true}).click();await expect.poll(()=>page.locator('.roles-table tbody tr').count()).toBe(50);expect(await page.locator('.roles-pagination').innerText()).toContain('51–100 / 120');expect(await page.locator('.roles-review li').count()).toBe(1);expect(await page.locator('.roles-review').innerText()).toContain('table_001');}finally{await page.close();}
},30000);

it('does not show stale role rows while the newly selected role request is pending',async()=>{
 const page=await openRolesProbe();try{await page.getByRole('button',{name:'analyst 로그인 가능',exact:true}).click();await page.getByRole('button',{name:'editor 로그인 가능',exact:true}).click();await page.locator('.roles-table tbody tr').first().waitFor();expect(await page.locator('.roles-table caption').innerText()).toContain('editor');expect(await page.getByRole('button',{name:/^editor public\.table_/}).count()).toBe(50);expect(await page.getByRole('button',{name:/^analyst public\.table_/}).count()).toBe(0);await page.waitForTimeout(240);expect(await page.getByRole('button',{name:/^editor public\.table_/}).count()).toBe(50);expect(await page.getByRole('button',{name:/^analyst public\.table_/}).count()).toBe(0);}finally{await page.close();}
},30000);

it('lets the user retry a role permission query after a retrieval error',async()=>{
 const page=await openRolesProbe();try{await page.getByRole('button',{name:'analyst 로그인 가능',exact:true}).click();await page.locator('.roles-table tbody tr').first().waitFor();await page.getByLabel('권한 객체 필터',{exact:true}).fill('retry');await page.getByRole('button',{name:'조회',exact:true}).click();await page.getByRole('button',{name:'권한 다시 조회',exact:true}).waitFor();expect(await page.locator('.roles-table tbody tr').count()).toBe(0);await page.getByRole('button',{name:'권한 다시 조회',exact:true}).click();await page.getByText('조건에 맞는 권한이 없습니다. 조회 범위를 넓혀 보세요.',{exact:true}).waitFor();expect(await page.locator('.db-error[role="alert"]').count()).toBe(0);}finally{await page.close();}
},30000);
