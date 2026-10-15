import { chromium } from '../ui/node_modules/playwright/index.mjs';
import { writeFile } from 'node:fs/promises';
import os from 'node:os';
const label = process.argv[2] ?? 'after';
const browser = await chromium.launch({ headless: true, args: ['--enable-precise-memory-info'] });
const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
await page.addInitScript(() => { window.__viewerShifts = []; if (PerformanceObserver.supportedEntryTypes.includes('layout-shift')) new PerformanceObserver(list => window.__viewerShifts.push(...list.getEntries().map(e => ({ value: e.value, recent: e.hadRecentInput })))).observe({ type: 'layout-shift', buffered: true }); });
const output = { at: new Date().toISOString(), label, complete: false, environment: { os: `${os.platform()} ${os.release()}`, cpu: os.cpus()[0].model, arch: os.arch(), browser: browser.version(), mode: 'Vite dev / headless Chromium', viewport: '1440x1000' }, results: {}, errors: [] };
page.on('pageerror', e => { output.errors.push(e.message); console.error('Page error:', e.message); });
try {
  await page.goto(`${process.env.BENCH_URL ?? 'http://127.0.0.1:1424'}/viewer-bench.html`); await page.waitForLoadState('networkidle'); await page.waitForFunction(() => window.dbtool?.perf);
  const scenarios = process.env.SCENARIOS?.split(',') ?? ['table-1000-10','table-10000-10','table-100000-10','table-10000-200','graph-100','graph-300','graph-1000','graph-5000','transitions','codec','techniques','graph-idle-300'];
  for (const scenario of scenarios) {
    output.results[scenario] = [];
    for (let i = 0; i < Number(process.env.REPEATS ?? 3); i++) output.results[scenario].push(await page.evaluate(async s => {
      const start = window.__viewerShifts.length, value = await window.dbtool.perf.run(s); await new Promise(requestAnimationFrame); await new Promise(requestAnimationFrame);
      const entries = window.__viewerShifts.slice(start); return { ...value, cls: entries.filter(e => !e.recent).reduce((sum,e) => sum+e.value,0), allLayoutShifts: entries.reduce((sum,e) => sum+e.value,0) };
    }, scenario));
    console.log(scenario, JSON.stringify(output.results[scenario].map(({metrics, ...summary}) => summary)));
  }
  if (output.errors.length) throw new Error(output.errors.join('\n'));
  output.complete = true;
} catch (error) { output.errors.push(String(error)); throw error; }
finally { await writeFile(new URL(`./results-viewer-${label}.json`, import.meta.url), JSON.stringify(output,null,2)); await browser.close(); }
