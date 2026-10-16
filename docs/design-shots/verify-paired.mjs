import { chromium } from '../../ui/node_modules/playwright/index.mjs';
import { execFileSync } from 'node:child_process';
import { writeFile } from 'node:fs/promises';
import os from 'node:os';
const browser = await chromium.launch({ headless: true, args: ['--enable-precise-memory-info'] });
const environment = { os: `${os.platform()} ${os.release()}`, cpu: os.cpus()[0].model, arch: os.arch(), browser: browser.version(), viewport: '1440x1000', mode: 'Vite dev / headless Chromium / alternating paired runs' };
const pages = {}, outputs = {};
try {
  for (const [name, port] of [['before',5175],['after',5177]]) {
    const page = pages[name] = await browser.newPage({ viewport: { width:1440,height:1000 } }); outputs[name] = { environment, complete:false, at:new Date().toISOString(), results:{}, errors:[] };
    page.on('pageerror', e => { outputs[name].errors.push(e.message); console.error(name,e.message); });
    await page.addInitScript(() => { window.__viewerShifts=[]; new PerformanceObserver(list => window.__viewerShifts.push(...list.getEntries().map(e => ({value:e.value,recent:e.hadRecentInput,at:e.startTime})))).observe({type:'layout-shift',buffered:true}); });
    await page.goto(`http://127.0.0.1:${port}/viewer-bench.html`); await page.waitForLoadState('networkidle'); await page.waitForFunction(() => window.axdb?.perf);
  }
  const scenarios = process.env.SCENARIOS?.split(',') ?? ['table-1000-10','table-10000-10','table-100000-10','table-10000-200','graph-100','graph-300','graph-1000','graph-5000','transitions','codec','techniques','graph-idle-300','graph-motion-300'];
  for (const scenario of scenarios) for (let pair=0;pair<Number(process.env.REPEATS??3);pair++) {
    for (const name of pair%2?['after','before']:['before','after']) {
      const page=pages[name]; await page.bringToFront();
      const uptime=execFileSync('uptime',{encoding:'utf8'}).trim(), load=os.loadavg();
      const result=await page.evaluate(async scenario => { const since=performance.now(), value=await window.axdb.perf.run(scenario); await new Promise(requestAnimationFrame); await new Promise(requestAnimationFrame); const shifts=window.__viewerShifts.filter(e=>e.at>=since); return {...value,cls:shifts.filter(e=>!e.recent).reduce((s,e)=>s+e.value,0),allLayoutShifts:shifts.reduce((s,e)=>s+e.value,0)}; },scenario);
      (outputs[name].results[scenario]??=[]).push({...result,pair,uptime,load});
      await page.evaluate(()=>window.axdb.perf.run('reset'));
      console.log(scenario,pair,name,'done');
    }
  }
  for(const output of Object.values(outputs)){if(output.errors.length)throw new Error(output.errors.join('\n'));output.complete=true;}
} catch(error) { for(const output of Object.values(outputs)) output.errors.push(String(error)); throw error; }
finally { for(const [name,output] of Object.entries(outputs))await writeFile(`/Users/iron/Project/psw/db-tool/docs/design-shots/results-paired-${name}-${process.env.OUTPUT_SUFFIX ?? "final"}.json`,JSON.stringify(output,null,2)); await browser.close(); }
