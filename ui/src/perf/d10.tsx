// Native integration harness: real QueryTab, approval dialog and test databases only.
import { render } from 'preact';
import { invoke } from '@tauri-apps/api/core';
import { api } from '../api';
import { QueryTab } from '../QueryTab';
import type { EditorHandle } from '../Editor';
import type { DriverManifest, ExecRequest, ResultSet } from '../contract';
const wait=async(test:()=>boolean)=>{const end=Date.now()+60000;while(!test()){if(Date.now()>end)throw new Error('D10 native UI timeout');await new Promise(r=>setTimeout(r,30));}};
export async function d10Bench() {
  const root=document.createElement('div');root.style.cssText='position:fixed;inset:0;z-index:10000;display:flex;background:var(--bg-base)';document.body.append(root);
  const report:{checks:string[];complete?:boolean;error?:string;stage?:string;checkpoint:number;sqlitePath?:string}={checks:[],checkpoint:0};
  const checkpoint=async(stage:string)=>{report.stage=stage;report.checkpoint++;await invoke('viewer_bench_write',{report});await invoke('viewer_bench_checkpoint',{checkpoint:report.checkpoint});};
  const assert=(value:unknown,message:string)=>{if(!value)throw new Error(message);};
  const button=(text:string,parent:ParentNode=root)=>[...parent.querySelectorAll<HTMLButtonElement>('button')].find(b=>(b.getAttribute('aria-label')??b.textContent?.trim())===text);
  const req=(text:string):ExecRequest=>({text,params:{},row_limit:100,graph_node_limit:0,timeout_ms:30000});
  const manifests=await api('drivers_list',{});
  const suffix=Date.now().toString();
  const definitions: {driver:string;fields:Record<string,string>;secret:string|null}[]=[
    {driver:'postgres',fields:{host:'127.0.0.1',port:'55432',database:'axdb_test',user:'postgres',sslmode:'prefer'},secret:'testpw'},
    {driver:'mysql',fields:{host:'127.0.0.1',port:'53306',database:'axdb_test',user:'root',sslmode:'disable'},secret:'testmysql'},
    {driver:'sqlite',fields:{path:`/tmp/axdb-d10-${suffix}.sqlite`},secret:null},
    {driver:'mongodb',fields:{uri:'mongodb://127.0.0.1:57017',database:'axdb_test',user:'axdb',authSource:'admin'},secret:'testmongo'},
  ];
  report.sqlitePath=definitions.find(f=>f.driver==='sqlite')!.fields.path;
  try {
    for (const fixture of definitions) {
      const manifest=manifests.find(m=>m.id===fixture.driver) as DriverManifest|undefined;assert(manifest,`Missing ${fixture.driver}`);
      const profile=await api('profile_save',{profile:{id:crypto.randomUUID(),name:`D10 ${fixture.driver}`,driver_id:fixture.driver,fields:fixture.fields,save_secret:false},secret:null});
      let sessionId:string|undefined;
      const name=`d10_native_${suffix}`;
      const mongo=fixture.driver==='mongodb';
      const query=mongo?JSON.stringify({find:name,filter:{_id:1}}):`SELECT id,name,id+1 AS calculated FROM ${name} WHERE id=1`;
      try {
        sessionId=await api('session_open',{profileId:profile.id,secret:fixture.secret});
        const execute=(text:string)=>api('execute',{sessionId:sessionId!,runId:crypto.randomUUID(),req:req(text)});
        await execute(mongo?JSON.stringify({insert:name,documents:[{_id:1,name:'old'}]}):`CREATE TABLE ${name}(id INTEGER PRIMARY KEY,name TEXT); INSERT INTO ${name} VALUES(1,'old'),(2,'old')`);
        let handle:EditorHandle|null=null;let results:ResultSet[]=[];let running:string|null=null;let error:unknown;
        const mount=(readOnly=false,agent=false)=>{
          render(<QueryTab key={fixture.driver} active initialText={query} sessionId={sessionId!} manifest={manifest!} graphLimit={0} readOnly={readOnly} aiSuggested={agent} onRun={id=>{running=id;}} onReady={value=>{handle=value;}} onHistory={()=>{}} onResults={value=>{results=value;}} onError={e=>{error=e;}}/>,root);
        };
        mount();await wait(()=>!!handle);button('Run query')!.click();await wait(()=>!running&&results.length>0);
        assert(!error,JSON.stringify(error));assert(results[0].edit_target,`${fixture.driver}: query target missing`);
        await wait(()=>button('Edit rows')?.disabled===false);button('Edit rows')!.click();await wait(()=>!!root.querySelector('td[aria-label="name"]'));
        const cell=root.querySelector<HTMLTableCellElement>('td[aria-label="name"]')!;cell.dispatchEvent(new MouseEvent('dblclick',{bubbles:true}));
        await wait(()=>!!root.querySelector('[aria-label="Edit name"]'));
        const input=root.querySelector<HTMLInputElement>('[aria-label="Edit name"]')!;input.value='D10 edited';input.dispatchEvent(new Event('input',{bubbles:true}));
        await new Promise(r=>setTimeout(r,40));input.form!.requestSubmit();await wait(()=>!root.querySelector('[aria-label="Edit name"]'));
        if(!mongo)assert(root.querySelector('td[aria-label="calculated, 계산 컬럼"]')?.getAttribute('aria-disabled')==='true','Computed column is editable');
        await checkpoint(`${fixture.driver}-edit`);
        button('변경 미리보기')!.click();await wait(()=>!!button('적용')&&!running);
        const old=await execute(mongo?JSON.stringify({find:name,filter:{_id:1}}):`SELECT name FROM ${name} WHERE id=1`);
        assert(old[0].table?.rows.some(row=>row.some(v=>v.t==='Text'&&v.v==='old')),'Preview changed data');
        await checkpoint(`${fixture.driver}-preview`);
        button('적용')!.click();await wait(()=>!!document.querySelector('dialog[open][aria-label="Apply row changes"]'));
        await checkpoint(`${fixture.driver}-approval`);
        button('적용',document.querySelector('dialog[open][aria-label="Apply row changes"]')!)!.click();
        await wait(()=>!root.querySelector('dialog[open]')&&!running&&results[0].table?.rows.some(row=>row.some(v=>v.t==='Text'&&v.v==='D10 edited'))===true);
        assert(!error,JSON.stringify(error));await checkpoint(`${fixture.driver}-requery`);
        report.checks.push(`${fixture.driver}: SELECT/find -> field edit -> preview leaves DB unchanged -> confirmation -> apply -> original query refresh`);
        if(!mongo) {
          handle!.replace(`SELECT a.id,a.name FROM ${name} a JOIN ${name} b ON a.id=b.id`);results=[];button('Run query')!.click();await wait(()=>!running&&results.length>0);
          assert(button('Edit rows')?.disabled,'JOIN editing enabled');assert(button('Edit rows')?.title.includes('여러 테이블'),'JOIN reason missing');await checkpoint(`${fixture.driver}-join-disabled`);
          report.checks.push(`${fixture.driver}: JOIN disabled with reason`);
        }
        handle!.replace(mongo?'not valid JSON':'SELECT * FROM d10_table_does_not_exist');button('Run query')!.click();
        await wait(()=>!running&&!!error);assert(!button('Edit rows'),'Failed query retained an editable stale result');error=undefined;
        report.checks.push(`${fixture.driver}: failed query clears stale result and cannot change the refresh request`);
        results=[];handle!.replace(query);mount(true);button('Run query')!.click();await wait(()=>!running&&results.length>0);assert(button('Edit rows')?.disabled,'Read-only editing enabled');
        results=[];handle!.replace(query);mount(false,true);button('Run query')!.click();await wait(()=>!running&&results.length>0);assert(button('Edit rows')?.disabled,'Agent editing enabled');
        report.checks.push(`${fixture.driver}: read-only and agent tabs disable editing`);
        render(null,root);
      } finally {
        if(sessionId){
          await api('execute',{sessionId,runId:crypto.randomUUID(),req:req(mongo?JSON.stringify({drop:name}):`DROP TABLE IF EXISTS ${name}`)});
          await api('session_close',{sessionId});
        }
        await api('profile_delete',{profileId:profile.id});
      }
    }
    report.complete=true;
  } catch(error){report.complete=false;report.error=JSON.stringify(error,Object.getOwnPropertyNames(error??{}));}
  await invoke('viewer_bench_write',{report});
}
