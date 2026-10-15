import { chromium } from '../ui/node_modules/playwright/index.mjs';
import { writeFile } from 'node:fs/promises';
import os from 'node:os';
const browser = await chromium.launch({headless:true});
try {
  const page = await browser.newPage({viewport:{width:1440,height:1000}});
  await page.goto(`http://127.0.0.1:${process.env.PORT??1426}/viewer-bench.html?graphProfile`);
  await page.waitForFunction(() => window.axdb?.perf);
  const output = {at:new Date().toISOString(),load:os.loadavg(),browser:browser.version(),results:{}};
  const client = await page.context().newCDPSession(page);
  if (process.env.TRACE === '1') await client.send('Tracing.start',{categories:'devtools.timeline,disabled-by-default-devtools.timeline,cc,gpu',transferMode:'ReturnAsStream'});
  for (const count of process.env.TRACE === '1' ? [1000] : [300,1000,5000]) {
    const value = await page.evaluate(count => window.axdb.perf.run(`graph-${count}`),count);
    const phases = {};
    for (const m of value.metrics.filter(m => m.name.startsWith('graph-phase-') || m.name === 'graph-render' || m.name === 'graph-bitmap')) (phases[m.name]??=[]).push(m.duration);
    for (const [name,samples] of Object.entries(phases)) { samples.sort((a,b)=>a-b); phases[name] = {p50:samples[Math.ceil(samples.length*.5)-1],p95:samples[Math.ceil(samples.length*.95)-1],samples:samples.length}; }
    output.results[count]={frames:value.frames,phases}; console.log(count,JSON.stringify(output.results[count]));
    await page.evaluate(() => window.axdb.perf.run('reset'));
  }
  if (process.env.TRACE === '1') {
    const done = new Promise(resolve=>client.once('Tracing.tracingComplete',resolve)); await client.send('Tracing.end'); const {stream} = await done;
    let raw = ''; for (;;) { const chunk = await client.send('IO.read',{handle:stream}); raw += chunk.data; if (chunk.eof) break; } await client.send('IO.close',{handle:stream});
    const { gzipSync } = await import('node:zlib'); await writeFile(new URL(`./viewer-w4b-trace-${process.env.PHASE??'before'}.json.gz`,import.meta.url),gzipSync(raw));
    output.trace = {}; for (const e of JSON.parse(raw).traceEvents) if (e.ph === 'X' && e.dur && ['LayerTreeHost::DoUpdateLayers','FireAnimationFrame','Commit'].includes(e.name)) { const s = output.trace[e.name]??={totalMs:0,maxMs:0,count:0}; s.totalMs += e.dur/1000; s.maxMs = Math.max(s.maxMs,e.dur/1000); s.count++; }
  }
  await writeFile(new URL(`./results-viewer-w4b-profile-${process.env.PHASE??'before'}.json`,import.meta.url),JSON.stringify(output,null,2));
} finally {await browser.close();}
