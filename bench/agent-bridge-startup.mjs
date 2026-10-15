// Runs five fresh real desktop processes; no database profiles or connections are needed.
// DBTOOL_BINARY must point to a debug app containing the candidate UI assets.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const binary = path.resolve(process.env.DBTOOL_BINARY ?? 'target-w7b/debug/db-tool');
const results = [];
for (let run = 1; run <= 5; run++) {
  const directory = await mkdtemp(path.join(tmpdir(), 'dbtool-bridge-startup-'));
  const app = spawn(binary, [], { env: { ...process.env, DBTOOL_CONFIG_DIR: directory, DBTOOL_AGENT_BRIDGE: '1', DBTOOL_BENCH: '0' }, stdio: 'ignore' });
  let spawnError;
  app.on('error', error => { spawnError = error; });
  const exited = new Promise(resolve => app.once('exit', resolve));
  try {
    const deadline = performance.now() + 30_000;
    let discovery;
    while (!discovery) {
      if (spawnError) throw spawnError;
      assert(app.exitCode === null && app.signalCode === null, 'App exited before bridge readiness');
      try { discovery = JSON.parse(await readFile(path.join(directory, 'agent-bridge.json'), 'utf8')); }
      catch (error) { if (error.code !== 'ENOENT') throw error; }
      assert(performance.now() < deadline, 'Bridge discovery was not published within 30 seconds');
      if (!discovery) await delay(5);
    }
    const start = performance.now();
    const response = await fetch(discovery.url, {
      method: 'POST', signal: AbortSignal.timeout(2000),
      headers: { Authorization: `Bearer ${discovery.token}`, 'Content-Type': 'application/json', 'MCP-Protocol-Version': '2025-11-25' },
      body: JSON.stringify({ jsonrpc: '2.0', id: run, method: 'tools/call', params: { name: 'list_drivers', arguments: {} } })
    });
    assert(response.ok, `HTTP ${response.status}`);
    const reply = await response.json();
    const manifests = reply.result?.structuredContent;
    assert(Array.isArray(manifests) && manifests.length > 0 && manifests.every(driver => typeof driver.id === 'string' && driver.capabilities), JSON.stringify(reply));
    const ms = Math.round(performance.now() - start);
    assert(ms < 2000, `Startup request took ${ms}ms`);
    results.push({ run, ms, drivers: manifests.map(driver => driver.id) });
    console.log(JSON.stringify(results.at(-1)));
  } finally {
    if (app.pid && app.exitCode === null && app.signalCode === null) {
      app.kill('SIGTERM');
      let timer;
      await Promise.race([exited, new Promise(resolve => { timer = setTimeout(() => { app.kill('SIGKILL'); resolve(); }, 5000); })]);
      clearTimeout(timer);
      await exited;
    }
    await rm(directory, { recursive: true, force: true });
  }
}
assert.equal(results.length, 5);
console.log('5/5 startup requests returned manifests within 2 seconds');
