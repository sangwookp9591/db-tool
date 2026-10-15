import { promisify } from 'node:util';
const execFile = promisify(execFileCallback);
import { execFile as execFileCallback, spawn } from 'node:child_process';
import { mkdtemp, readFile, writeFile, open, unlink } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
const lockPath = path.join(os.tmpdir(), 'axdb-viewer-native.lock');
const lock = await open(lockPath, 'wx').catch(() => { throw new Error('A native viewer benchmark is already running; serialize runs to keep WKWebView visible'); });
await lock.writeFile(String(process.pid));
// AXDB_BENCH uses fixed loopback test ports; never accepts user database endpoints.
const config = await mkdtemp(path.join(os.tmpdir(), 'axdb-viewer-bench-'));
console.log('Native benchmark directory:', config);
const webkit = async () => (await execFile('ps', ['-axo', 'pid=,command='])).stdout.split('\n').filter(line => line.includes('/com.apple.WebKit.WebContent.xpc/')).map(line => Number(line.trim().split(/\s+/)[0]));
const priorWebkit = new Set(await webkit());
const uptime = (await execFile('uptime')).stdout.trim();
const cpuSeconds = value => value.split(':').reduce((total, part) => total * 60 + Number(part), 0);
assert.equal(cpuSeconds('1:02.34'), 62.34);
assert.equal(cpuSeconds('1:02:03.45'), 3723.45);
let idleCpu;
const app = spawn(process.env.AXDB_BINARY ?? 'target/debug/axdb', [], { env: { ...process.env, AXDB_BENCH: '1', AXDB_AGENT_BRIDGE: '0', AXDB_BENCH_TARGET: ['graph', 'workspace', 'idle', 'small', 'matrix'].includes(process.argv[2]) ? process.argv[2] : 'all', AXDB_CONFIG_DIR: config }, stdio: 'inherit' });
let spawnError; app.on('error', error => { spawnError = error; });
const stop = () => app.kill('SIGTERM'); process.once('SIGINT', stop); process.once('SIGTERM', stop);
const started = Date.now(), deadline = started + 15 * 60_000;
try {
  while (Date.now() < deadline) {
    await new Promise(resolve => setTimeout(resolve, 1000));
    assert.ok(!spawnError, spawnError?.message);
    assert.ok(app.exitCode === null && app.signalCode === null, 'Native app exited before benchmark completion');
    let report;
    try { report = JSON.parse(await readFile(path.join(config, 'perf.json'), 'utf8')); } catch { assert.ok(Date.now() - started < 60000, 'Native benchmark did not bootstrap; inspect embedded UI assets and commands'); continue; }
    if (report.idleReady && !idleCpu) {
      const candidates = (await webkit()).filter(pid => !priorWebkit.has(pid)); const pids = [app.pid, ...candidates], samples = [];
      const cpuTime = async () => (await execFile('ps', ['-o', 'time=', '-p', pids.join(',')])).stdout.trim().split(/\s+/).reduce((sum, value) => sum + cpuSeconds(value), 0);
      const cpuStart = await cpuTime(), idleStart = performance.now();
      for (let i = 0; i < 10; i++) { await new Promise(resolve => setTimeout(resolve, 1000)); const raw = (await execFile('ps', ['-o', 'pid=,%cpu=', '-p', pids.join(',')])).stdout; samples.push(raw.trim().split('\n').reduce((sum,line) => sum + Number(line.trim().split(/\s+/)[1] ?? 0), 0)); }
      const cpuElapsedMs = performance.now() - idleStart, cpuDeltaSeconds = await cpuTime() - cpuStart;
      assert.ok(Date.now() < report.idleReadyAt + report.idleWindowMs, 'CPU sampling exceeded graph-idle window; rebuild the UI or extend the idle window');
      idleCpu = { pids, webkitCandidates: candidates, samples, meanPercent: candidates.length ? samples.reduce((a,b) => a+b,0) / samples.length : null, cpuDeltaSeconds, cpuElapsedMs, intervalMeanPercent: candidates.length ? cpuDeltaSeconds / (cpuElapsedMs / 1000) * 100 : null, measurement: 'intervalMeanPercent: cumulative ps TIME delta inside the graph-idle window; meanPercent retains legacy rolling ps %cpu', ownership: 'app plus new WebKit processes since spawn; conservative if another app launched WebKit concurrently' };
    }
    if ('complete' in report) { report.uptime = uptime; report.idleCpu = idleCpu; await writeFile(new URL(['graph', 'workspace', 'idle', 'small', 'matrix'].includes(process.argv[2]) ? `./results-viewer-app-${process.argv[2]}${process.env.OUTPUT_SUFFIX ? `-${process.env.OUTPUT_SUFFIX}` : ''}.json` : './results-viewer-app.json', import.meta.url), JSON.stringify(report, null, 2)); assert.equal(report.complete, true, report.error); console.log('Native benchmark complete'); break; }
  }
  assert.ok(Date.now() < deadline, 'Native benchmark exceeded 15 minutes');
} finally { process.removeListener('SIGINT', stop); process.removeListener('SIGTERM', stop); app.kill('SIGTERM'); await lock.close(); await unlink(lockPath); }
