// Opt-in native regression: AXDB_BENCH=1 AXDB_BENCH_TARGET=qa.
// Uses only the benchmark's guarded localhost PostgreSQL fixture and synthetic data.
import { invoke } from '@tauri-apps/api/core';
import { EditorView } from '@codemirror/view';
import { startCompletion, closeCompletion } from '@codemirror/autocomplete';
import { api } from '../api';
import type { BenchConfig } from './benchConfig';
import type { ExecRequest } from '../contract';
const pause = () => new Promise(resolve => setTimeout(resolve, 30));
async function wait(test: () => boolean, description: string) { const until=Date.now()+30000; while(!test()){if(Date.now()>until)throw new Error(`Timeout: ${description}`);await pause();} }
function button(label:string,root:ParentNode=document){return [...root.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.getClientRects().length>0&&(b.getAttribute('aria-label')??b.textContent?.trim())===label);}
function click(label:string,root:ParentNode=document){const b=button(label,root);if(!b||b.disabled)throw new Error(`Unavailable button: ${label} (${b?.title??'missing'})`);b.click();}
async function input(label:string,value:string){const el=document.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`);if(!el)throw new Error(`Missing input: ${label}`);el.value=value;el.dispatchEvent(new Event('input',{bubbles:true}));await pause();}
function visible<T extends HTMLElement>(selector:string){return [...document.querySelectorAll<T>(selector)].find(el=>el.getClientRects().length>0)!;}
function assert(value:unknown,message:string):asserts value {if(!value)throw new Error(message);}
function menuReachable(selector:string){const el=document.querySelector<HTMLElement>(selector)!;assert(el,'Menu missing');const rect=el.getBoundingClientRect();assert(rect.top>=0&&rect.left>=0&&rect.right<=innerWidth&&rect.bottom<=innerHeight,'Menu outside viewport');for(const button of el.querySelectorAll('button')){const r=button.getBoundingClientRect();assert(button.contains(document.elementFromPoint(r.left+r.width/2,r.top+r.height/2)),'Menu item covered');}}

export async function nativeQa(){
  const report:{checks:string[];complete:boolean;stage:string;error?:string;environment:object}={checks:[],complete:false,stage:'startup',environment:{userAgent:navigator.userAgent,viewport:{width:innerWidth,height:innerHeight},mock:false,at:new Date().toISOString()}};
  const record=async(label:string)=>{report.checks.push(label);report.stage=label;await invoke('viewer_bench_write',{report});};
  let setupSession:string|undefined;
  try{
    const config=await invoke<BenchConfig>('viewer_bench_config');
    assert(config.table.fields.host==='127.0.0.1'&&config.table.fields.port==='55432','Only isolated fixture allowed');
    const driver=(await api('drivers_list',{})).find(d=>d.id==='postgres');assert(driver,'PostgreSQL driver missing');
    const schema=`qa_ui_${Date.now()}`;
    const fixtureNode=(label:string)=>[...document.querySelectorAll<HTMLElement>('.tree-label')].find(el=>el.getAttribute('aria-label')===label&&el.dataset.nodeId?.includes(schema));
    const setup=await api('profile_save',{profile:{id:crypto.randomUUID(),name:'QA setup',driver_id:driver.id,fields:config.table.fields,save_secret:false},secret:null});
    setupSession=await api('session_open',{profileId:setup.id,secret:config.table.secret});
    const query=async(text:string)=>api('execute',{sessionId:setupSession!,runId:crypto.randomUUID(),req:{text,params:{},row_limit:100,graph_node_limit:100,timeout_ms:10000} as ExecRequest});
    await query(`CREATE SCHEMA ${schema}; CREATE TABLE ${schema}.qa_people(id integer PRIMARY KEY, name text NOT NULL); INSERT INTO ${schema}.qa_people VALUES(1,'before'),(2,'second'); CREATE VIEW ${schema}.qa_names AS SELECT name FROM ${schema}.qa_people`);
    await record('actual PostgreSQL host/driver connection and isolated table/view fixture');
    await wait(()=>!!button('Add connection'),'application startup');click('Add connection');
    await wait(()=>!!document.querySelector('dialog[open] input[aria-label="Name"]'),'new connection form');
    const select=document.querySelector<HTMLButtonElement>('[role=combobox][aria-label=Driver]')!;select.click();
    await wait(()=>!!document.querySelector('[role=option]'),'driver options');
    [...document.querySelectorAll<HTMLElement>('[role=option]')].find(el=>el.textContent?.includes('PostgreSQL'))!.click();await pause();
    await input('Name','QA PostgreSQL');await input('Host',config.table.fields.host);await input('Port',config.table.fields.port);await input('Database',config.table.fields.database);
    click('3. 인증');await input('User',config.table.fields.user);await input('Password',config.table.secret);
    click('Test connection');await wait(()=>!!document.querySelector('.ax-test-status[data-status=success]'),'native connection test');
    assert(!document.querySelector('dialog[open] .db-error'),'Connection form contains error');
    await record('native add connection → real connection test passes with DNS/auth/permission stages');
    click('저장',document.querySelector('dialog[open]')!);
    await wait(()=>!!button('Connection actions for QA PostgreSQL')&&!document.querySelector('.connection-dialog[open]'),'profile save');
    click('Connection actions for QA PostgreSQL');await wait(()=>!!document.querySelector('.connection-menu'),'connection menu');menuReachable('.connection-menu');
    click('편집',document.querySelector('.connection-menu')!);await wait(()=>!!document.querySelector('dialog[aria-label="Edit connection"][open]'),'edit profile');
    assert(document.querySelector<HTMLInputElement>('[aria-label=Host]')?.value==='127.0.0.1','Saved host lost');
    click('Close connection form');await record('profile save/edit/cancel and connection menu hit-testing');
    click('Connect or disconnect QA PostgreSQL');await wait(()=>!!document.querySelector('[name=credential]'),'credential prompt');
    const credential=document.querySelector<HTMLInputElement>('[name=credential]')!;credential.value=config.table.secret;credential.dispatchEvent(new Event('input',{bubbles:true}));await pause();credential.form!.requestSubmit();
    await wait(()=>!!fixtureNode('qa_people'),'automatic table discovery');
    await wait(()=>!!fixtureNode('qa_names'),'automatic view discovery');
    await record('session connect → non-public schema table/view discovery without manual expansion');
    const editor=EditorView.findFromDOM(visible('.cm-editor'))!;assert(editor,'Editor missing');
    const setSql=async(text:string)=>{editor.dispatch({changes:{from:0,to:editor.state.doc.length,insert:text},selection:{anchor:text.length}});await pause();};
    await setSql('SELECT * FROM qa_pe');startCompletion(editor);
    await wait(()=>!!document.querySelector('.cm-tooltip-autocomplete li')?.textContent?.includes('qa_people'),'real catalog autocomplete');closeCompletion(editor);
    await record('SQL autocomplete uses real PostgreSQL catalog');
    const selectSql=`SELECT id,name FROM ${schema}.qa_people ORDER BY id`;
    await setSql(selectSql);click('Run query',visible('.query-tab'));
    await wait(()=>visible('.query-tab')?.textContent?.includes('before')===true&&!visible<HTMLButtonElement>('[aria-label="Run query"]').disabled,'SQL results');
    await record('native SQL execution renders actual result rows');
    const table=fixtureNode('qa_people')!;table.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,clientX:280,clientY:innerHeight-15}));
    await wait(()=>!!document.querySelector('.context-menu'),'explorer context menu');menuReachable('.context-menu');click('데이터 보기',document.querySelector('.context-menu')!);
    await wait(()=>!!visible('.browse-tab')&&button('Edit rows',visible('.browse-tab'))?.disabled===false,'browse edit action enabled');click('Edit rows',visible('.browse-tab'));
    await wait(()=>!!document.querySelector('.edit-table td[aria-label=name]'),'editable cell');
    document.querySelector('.edit-table td[aria-label=name]')!.dispatchEvent(new MouseEvent('dblclick',{bubbles:true}));
    await wait(()=>!!document.querySelector('[aria-label="Edit name"]'),'cell editor');await input('Edit name','after');click('Save cell');await pause();click('변경 미리보기');
    await wait(()=>!!visible('.change-preview')&&button('적용',visible('.browse-tab'))?.disabled===false,'edit preview');
    await record('edit preview rendered; application requires confirmation');
    assert(JSON.stringify(await query(`SELECT name FROM ${schema}.qa_people WHERE id=1`)).includes('before'),'Preview changed DB');
    click('적용',visible('.browse-tab'));await wait(()=>!!document.querySelector('dialog[aria-label="Apply row changes"][open]'),'apply confirmation');
    click('취소',document.querySelector('dialog[aria-label="Apply row changes"][open]')!);await pause();
    assert(JSON.stringify(await query(`SELECT name FROM ${schema}.qa_people WHERE id=1`)).includes('before'),'Cancel changed DB');
    click('적용',visible('.browse-tab'));await wait(()=>!!document.querySelector('dialog[aria-label="Apply row changes"][open]'),'apply confirmation again');click('적용',document.querySelector('dialog[aria-label="Apply row changes"][open]')!);
    await wait(()=>!document.querySelector('dialog[aria-label="Apply row changes"][open]'),'apply completed');
    assert(JSON.stringify(await query(`SELECT name FROM ${schema}.qa_people WHERE id=1`)).includes('after'),'Apply not persisted');
    await record('real table edit: preview rollback → confirmation cancel → apply persisted');
    click('AX 워크벤치');await wait(()=>!!button('사용자 · 권한'),'workbench navigation');click('사용자 · 권한');
    await wait(()=>!!button('postgres관리자'),'real role directory');click('postgres관리자');
    await wait(()=>!!document.querySelector('.roles-table tbody tr'),'real paged permissions');
    await input('권한 스키마 필터',schema);await input('권한 객체 필터','qa_people');click('조회');
    await wait(()=>document.querySelector('.roles-pagination')?.textContent?.includes('1–7 / 7')===true,'filtered permission page');
    assert(document.querySelectorAll('.roles-table tbody tr').length===7,'Expected seven PostgreSQL table privileges');
    assert(!document.querySelector('.roles-page .db-error'),'Role page contains error');
    await record('native user/role directory → selected-role pagination → schema/object filters');
    click('워크벤치 닫기');
    await api('session_close',{sessionId:setupSession!});setupSession=undefined;await pause();
    assert(!document.querySelector('.statusbar')?.textContent?.includes('네트워크 연결 끊김'),'Other profile disconnect polluted active status');
    await record('closing another connection preserves the active profile network status');
    click('Connection actions for QA PostgreSQL');await wait(()=>!!document.querySelector('.connection-menu'),'final connection menu');menuReachable('.connection-menu');
    report.complete=true;await record('native QA complete; synthetic fixture retained for screenshot');
  }catch(error){report.error=error instanceof Error?error.message:JSON.stringify(error);}
  finally{if(setupSession)await api('session_close',{sessionId:setupSession}).catch(()=>{});await invoke('viewer_bench_write',{report});}
}
