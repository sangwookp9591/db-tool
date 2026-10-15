import { promisify } from 'node:util';
const execFile = promisify(execFileCallback);
import { execFile as execFileCallback, spawn } from 'node:child_process';
import { mkdtemp, readFile, writeFile, open, unlink } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
const lockPath = path.join(os.tmpdir(), 'dbtool-viewer-native.lock');
const lock = await open(lockPath, 'wx').catch(() => { throw new Error('A native viewer benchmark is already running; serialize runs to keep WKWebView visible'); });
await lock.writeFile(String(process.pid));
// DBTOOL_BENCH uses fixed loopback test ports; never accepts user database endpoints.
const config = await mkdtemp(path.join(os.tmpdir(), 'dbtool-viewer-bench-'));
console.log('Native benchmark directory:', config);
const webkit = async () => (await execFile('ps', ['-axo', 'pid=,command='])).stdout.split('\n').filter(line => line.includes('/com.apple.WebKit.WebContent.xpc/')).map(line => Number(line.trim().split(/\s+/)[0]));
const priorWebkit = new Set(await webkit());
const uptime = (await execFile('uptime')).stdout.trim();
let idleCpu;
const app = spawn(process.env.DBTOOL_BINARY ?? 'target/debug/db-tool', [], { env: { ...process.env, DBTOOL_BENCH: '1', DBTOOL_AGENT_BRIDGE: '0', AXDB_AGENT_BRIDGE: '0', AXDB_CONFIG_DIR: config, DBTOOL_BENCH_TARGET: ['graph', 'workspace', 'idle', 'small', 'matrix'].includes(process.argv[2]) ? process.argv[2] : 'all', DBTOOL_CONFIG_DIR: config }, stdio: 'inherit' });
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
      for (let i = 0; i < 10; i++) { await new Promise(resolve => setTimeout(resolve, 1000)); const raw = (await execFile('ps', ['-o', 'pid=,%cpu=', '-p', pids.join(',')])).stdout; samples.push(raw.trim().split('\n').reduce((sum,line) => sum + Number(line.trim().split(/\s+/)[1] ?? 0), 0)); }
      idleCpu = { pids, webkitCandidates: candidates, samples, meanPercent: candidates.length ? samples.reduce((a,b) => a+b,0) / samples.length : null, ownership: 'app plus new WebKit processes since spawn; conservative if another app launched WebKit concurrently' };
    }
    if ('complete' in report) { report.uptime = uptime; report.idleCpu = idleCpu; await writeFile(new URL(['graph', 'workspace', 'idle', 'small', 'matrix'].includes(process.argv[2]) ? `./results-viewer-app-${process.argv[2]}.json` : './results-viewer-app.json', import.meta.url), JSON.stringify(report, null, 2)); assert.equal(report.complete, true, report.error); console.log('Native benchmark complete'); break; }
  }
  assert.ok(Date.now() < deadline, 'Native benchmark exceeded 15 minutes');
} finally { process.removeListener('SIGINT', stop); process.removeListener('SIGTERM', stop); app.kill('SIGTERM'); await lock.close(); await unlink(lockPath); }
