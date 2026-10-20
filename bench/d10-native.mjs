// Real debug app, real QueryTab and isolated loopback DBs. Captures only this child app's window.
import { spawn, execFile as execCallback } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdtemp, readFile, writeFile, copyFile, mkdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
const exec=promisify(execCallback);
const root=path.resolve(new URL('..',import.meta.url).pathname);
const config=await mkdtemp(path.join(tmpdir(),'axdb-d10-native-'));
const output=path.join(root,'docs/design-shots');await mkdir(output,{recursive:true});
const app=spawn(path.join(root,'target/debug/axdb'),[],{cwd:root,env:{...process.env,AXDB_DRIVER_DIR:path.join(root,'target/debug'),AXDB_CONFIG_DIR:config,AXDB_BENCH:'1',AXDB_BENCH_TARGET:'d10',AXDB_AGENT_BRIDGE:'0'},stdio:'ignore'});
console.log(`D10 native PID ${app.pid}, config ${config}`);
let checkpoint=0,report;
const until=Date.now()+15*60000;
try {
  while(Date.now()<until) {
    assert.equal(app.exitCode,null,'Debug app exited early');
    try {report=JSON.parse(await readFile(path.join(config,'perf.json'),'utf8'));}catch{}
    if(report?.complete!==undefined)break;
    if(report?.checkpoint>checkpoint) {
      const {stdout}=await exec('orca',['computer','get-app-state','--app',`pid:${app.pid}`,'--restore-window','--json'],{maxBuffer:16*1024*1024});
      const capture=JSON.parse(stdout);assert(capture.ok,JSON.stringify(capture.error));
      const screenshot=capture.result.screenshot;assert(screenshot,'No native screenshot');
      const destination=path.join(output,`d10-${report.stage}.png`);
      if(screenshot.path)await copyFile(screenshot.path,destination);else {assert(screenshot.data,'No screenshot data');await writeFile(destination,Buffer.from(screenshot.data,'base64'));}
      checkpoint=report.checkpoint;
      console.log(`Captured ${report.stage}`);
      await writeFile(path.join(config,'resource-ack'),String(checkpoint));
    }
    await new Promise(resolve=>setTimeout(resolve,300));
  }
  assert(report,'No native report');
  await writeFile(path.join(output,'d10-native.json'),JSON.stringify({...report,config,appPid:app.pid,transport:'real debug Tauri/WKWebView + real drivers; screenshots via Orca computer'},null,2));
  assert.equal(report.complete,true,report.error??'Native run timed out');
  console.log(`PASS ${report.checks.length} native checks`);
} finally {
  app.kill('SIGTERM');
  if(report?.sqlitePath?.startsWith('/tmp/axdb-d10-'))await rm(report.sqlitePath,{force:true});
}
