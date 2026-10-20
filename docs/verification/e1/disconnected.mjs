import { spawn, execFile as callback } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdtemp, readFile, writeFile, copyFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
const exec=promisify(callback), root=path.resolve(new URL('../../..',import.meta.url).pathname), output=path.dirname(new URL(import.meta.url).pathname);
const config=await mkdtemp(path.join(tmpdir(),'axdb-e1-reconnect-'));
const fixtures=JSON.parse(await readFile(path.join(root,'bench/fixtures.json'),'utf8'));
assert.equal(fixtures.table.fields.host,'127.0.0.1');assert.equal(fixtures.table.fields.port,'55432');assert.equal(fixtures.table.fields.database,'axdb_test');
const profile={id:'e1-reconnect-pg',name:'e1-reconnect-pg',driver_id:'postgres',fields:fixtures.table.fields,save_secret:true,access_mode:'read_write'};
await writeFile(path.join(config,'profiles.json'),JSON.stringify([profile]),{mode:0o600});
const secretStore=path.join(config,'secrets.json');await writeFile(secretStore,JSON.stringify({[profile.id]:fixtures.table.secret}),{mode:0o600});
const env={...process.env,AXDB_CONFIG_DIR:config,AXDB_DRIVER_DIR:path.join(root,'target-d7/debug'),AXDB_DEV_SECRET_STORE:secretStore,AXDB_AGENT_BRIDGE:'1'};delete env.AXDB_BENCH;delete env.AXDB_BENCH_TARGET;
const app=spawn(path.join(root,'target-d7/debug/axdb'),[],{cwd:root,env,stdio:'ignore'});
const delay=ms=>new Promise(r=>setTimeout(r,ms));
const checks=[];let id=0,bridge;
async function gui(command,...args){const {stdout}=await exec('orca',['computer',command,...args,'--app',`pid:${app.pid}`,'--restore-window','--no-screenshot','--json'],{maxBuffer:16*1024*1024});const r=JSON.parse(stdout);assert(r.ok,JSON.stringify(r.error));return r.result.snapshot?.treeText??'';}
async function waitFor(predicate){const end=Date.now()+15000;while(Date.now()<end){const tree=await gui('get-app-state');if(predicate(tree))return tree;await delay(100);}throw Error('Native state timed out');}
async function click(pattern){const tree=await waitFor(t=>t.split('\n').some(l=>pattern.test(l)));const match=tree.split('\n').map(l=>l.match(/^\s*(\d+) (.*)$/)).find(m=>m&&pattern.test(m[2]));assert(match,`Control missing ${pattern}`);return gui('click','--element-index',match[1]);}
async function capture(name){const {stdout}=await exec('orca',['computer','get-app-state','--app',`pid:${app.pid}`,'--restore-window','--json'],{maxBuffer:16*1024*1024});const r=JSON.parse(stdout);assert(r.ok,JSON.stringify(r.error));const s=r.result.screenshot;assert(s);if(s.path)await copyFile(s.path,path.join(output,name+'.png'));else await writeFile(path.join(output,name+'.png'),Buffer.from(s.data,'base64'));await writeFile(path.join(output,name+'.txt'),r.result.snapshot.treeText);return r.result.snapshot.treeText;}
async function tool(name,args={}){const r=await fetch(bridge.url,{method:'POST',headers:{Authorization:`Bearer ${bridge.token}`,'Content-Type':'application/json'},body:JSON.stringify({jsonrpc:'2.0',id:++id,method:'tools/call',params:{name,arguments:args}})});const d=await r.json();assert(!d.error,JSON.stringify(d.error));const v=d.result.structuredContent??JSON.parse(d.result.content[0].text);assert(!v.error,JSON.stringify(v.error));return v;}
try{
 console.log(`Candidate PID ${app.pid}`);
 const end=Date.now()+60000;while(!bridge){assert.equal(app.exitCode,null);try{bridge=JSON.parse(await readFile(path.join(config,'agent-bridge.json'),'utf8'));}catch{}assert(Date.now()<end);if(!bridge)await delay(200);}
 const session=await tool('open_connection',{profile_id:profile.id});assert(session.session_id);
 const query='SELECT 42 AS reconnect_check';const tab=await tool('open_query_tab',{session_id:session.session_id,text:query});assert(tab.tab_id);
 await waitFor(t=>t.includes('Run query'));
 await click(/button 연결 해제$/);
 await waitFor(t=>t.includes('연결이 끊겼습니다')&&t.includes('button 다시 연결'));
 checks.push('status bar disconnect preserves a query tab and shows disconnected state');
 for(const [theme,label] of [['midnight','미드나이트'],['daylight','데이라이트']]){
  await click(/button Workspace settings/);await click(/Appearance/);
  await click(new RegExp(`text.*${label}$`));await click(/button Close settings/);
  await delay(300);const tree=await capture(`disconnected-${theme}`);assert(tree.includes('연결이 끊겼습니다')&&tree.includes(profile.name));checks.push(`${theme} disconnected screenshot`);
 }
 const progress=await click(/button 다시 연결$/);await writeFile(path.join(output,'disconnected-progress.txt'),progress);assert(progress.includes('실제 연결 진행')||progress.includes('연결됨 ✓')||progress.includes('연결 중…'),'Connection progress popup not observed');checks.push('reconnect button opens connection progress popup');
 await waitFor(t=>t.includes('Run query')&&!t.includes('연결이 끊겼습니다'));
 await capture('disconnected-restored');await click(/Run query/);
 let history;const until=Date.now()+10000;while(Date.now()<until){history=(await tool('get_history',{limit:100})).find(h=>h.source==='ui'&&h.text===query);if(history)break;await delay(100);}assert(history&&!history.error&&history.row_count===1,JSON.stringify(history));checks.push('original SELECT restored and runs against test PostgreSQL');
 await writeFile(path.join(output,'disconnected.json'),JSON.stringify({ok:true,checks,appPid:app.pid,config,history},null,2));console.log(`PASS ${checks.length}/${checks.length} disconnected checks`);
}catch(e){try{await capture('disconnected-failure');}catch{}throw e;}
finally{const exited=new Promise(r=>app.once('exit',r));if(app.exitCode===null&&app.signalCode===null){app.kill('SIGTERM');await exited;}await rm(config,{recursive:true,force:true});console.log('Candidate stopped; isolated config and debug credentials removed; no keychain entries created.');}
