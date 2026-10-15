import { spawn } from 'node:child_process';
import { mkdtemp, readFile, copyFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
// DBTOOL_BENCH uses fixed loopback test ports; never accepts user database endpoints.
const config = await mkdtemp(path.join(os.tmpdir(), 'dbtool-viewer-bench-'));
console.log('Native benchmark directory:', config);
const app = spawn(process.env.DBTOOL_BINARY ?? 'target/debug/db-tool', [], { env: { ...process.env, DBTOOL_BENCH: '1', DBTOOL_CONFIG_DIR: config }, stdio: 'inherit' });
const deadline = Date.now() + 15 * 60_000;
try {
  while (Date.now() < deadline) {
    await new Promise(resolve => setTimeout(resolve, 1000));
    assert.ok(app.exitCode === null && app.signalCode === null, 'Native app exited before benchmark completion');
    let report;
    try { report = JSON.parse(await readFile(path.join(config, 'perf.json'), 'utf8')); } catch { continue; }
    if ('complete' in report) { await copyFile(path.join(config, 'perf.json'), new URL('./results-viewer-app.json', import.meta.url)); assert.equal(report.complete, true, report.error); console.log('Native benchmark complete'); break; }
  }
  assert.ok(Date.now() < deadline, 'Native benchmark exceeded 15 minutes');
} finally { app.kill('SIGTERM'); }
