// Runs the repository's existing three real desktop E2E suites using a private config and dev secret file.
import { spawn, execFile as execCallback } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdtemp, readFile, writeFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { tmpdir } from 'node:os';
import assert from 'node:assert/strict';
const exec=promisify(execCallback),root=path.resolve(new URL('..',import.meta.url).pathname);
const config=await mkdtemp(path.join(tmpdir(),'axdb-d10-e2e-'));
const fixtures=JSON.parse(await readFile(path.join(root,'bench/fixtures.json'),'utf8'));
assert.equal(fixtures.table.fields.port,'55432');assert.equal(fixtures.graph.fields.uri,'bolt://127.0.0.1:57687');
const sqlite=path.join(config,'test.sqlite');
await exec('python3',['-c','import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute("CREATE TABLE items(id INTEGER PRIMARY KEY,value TEXT)"); c.executemany("INSERT INTO items VALUES(?,?)",[(i,str(i)) for i in range(1,101)]); c.commit()',sqlite]);
const profile=(id,name,driver_id,fields)=>({id,name,driver_id,fields,save_secret:true,access_mode:'read_write'});
const profiles=[profile('e2e-pg-0001','e2e-pg','postgres',fixtures.table.fields),profile('e2e-neo-0001','e2e-neo4j','neo4j',fixtures.graph.fields),profile('e2e-sqlite-0001','e2e-sqlite','sqlite',{path:sqlite}),profile('e2e-pg-badport-0001','e2e-pg-badport','postgres',{...fixtures.table.fields,port:'1'})];
await writeFile(path.join(config,'profiles.json'),JSON.stringify(profiles),{mode:0o600});
const secretStore=path.join(config,'test-secrets.json');await writeFile(secretStore,JSON.stringify({'e2e-pg-0001':fixtures.table.secret,'e2e-pg-badport-0001':fixtures.table.secret,'e2e-neo-0001':fixtures.graph.secret}),{mode:0o600});
const env={...process.env,AXDB_DRIVER_DIR:path.join(root,'target/debug'),AXDB_CONFIG_DIR:config,AXDB_AGENT_BRIDGE:'1',AXDB_DEV_SECRET_STORE:secretStore};delete env.AXDB_BENCH;delete env.AXDB_BENCH_TARGET;
const app=spawn(path.join(root,'target/debug/axdb'),[],{cwd:root,env,stdio:'ignore'});env.AXDB_APP_PID=String(app.pid);
const checks=[];let failed=false;
console.log(`D10 regression PID ${app.pid}, config ${config}`);
try {
 const until=Date.now()+60000;
 while(true){assert.equal(app.exitCode,null);try{await readFile(path.join(config,'agent-bridge.json'));break;}catch{}assert(Date.now()<until,'Bridge startup timed out');await new Promise(r=>setTimeout(r,200));}
 await mkdir(path.join(root,'docs/verification/d10'),{recursive:true});
 for(const suite of ['e2e-bridge','preview-e2e','permissions-e2e']) {
  let log,ok=true;try{const r=await exec('node',[path.join(root,`bench/${suite}.mjs`)],{env,timeout:300000,maxBuffer:4*1024*1024});log=r.stdout+'\n'+r.stderr;}catch(error){ok=false;log=(error.stdout??'')+'\n'+(error.stderr??'')+'\n'+error.message;}
  for(const fixture of Object.values(fixtures))log=log.replaceAll(fixture.secret,'[redacted]');
  await writeFile(path.join(root,`docs/verification/d10/${suite}.txt`),log);console.log(`${ok?'PASS':'FAIL'} ${suite}`);checks.push({suite,ok});failed ||= !ok;
 }
 await writeFile(path.join(root,'docs/verification/d10/regressions.json'),JSON.stringify({checks,config,pid:app.pid,credentials:'private debug fixture file; OS keychain not exercised'},null,2));
 assert(!failed,'Existing E2E regression failed; inspect docs/verification/d10');
} finally {app.kill('SIGTERM');}
