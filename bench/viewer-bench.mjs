import { chromium } from '../ui/node_modules/playwright/index.mjs';
import { writeFile } from 'node:fs/promises';
import os from 'node:os';
const label = process.argv[2] ?? 'after';
const browser = await chromium.launch({ headless: true, args: ['--enable-precise-memory-info'] });
const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
const errors = []; page.on('pageerror', e => errors.push(e.message));
await page.goto(`${process.env.BENCH_URL ?? 'http://127.0.0.1:1424'}/viewer-bench.html`);
await page.waitForLoadState('networkidle');
await page.waitForFunction(() => window.dbtool?.perf);
const scenarios = process.env.SCENARIOS?.split(',') ?? ['table-1000-10', 'table-10000-10', 'table-100000-10', 'table-10000-200', 'graph-100', 'graph-300', 'graph-1000', 'graph-5000', 'transitions', 'codec', 'techniques'];
const results = {};
for (const scenario of scenarios) { results[scenario] = []; for (let i = 0; i < Number(process.env.REPEATS ?? 3); i++) { const value = await page.evaluate(s => window.dbtool.perf.run(s), scenario); results[scenario].push(value); } console.log(scenario, JSON.stringify(results[scenario])); }
const output = { at: new Date().toISOString(), label, environment: { os: `${os.platform()} ${os.release()}`, cpu: os.cpus()[0].model, arch: os.arch(), browser: browser.version(), mode: 'Vite dev / headless Chromium', viewport: '1440x1000' }, results, errors };
await writeFile(new URL(`./results-viewer-${label}.json`, import.meta.url), JSON.stringify(output, null, 2));
await browser.close();
if (errors.length) throw new Error(errors.join('\n'));
