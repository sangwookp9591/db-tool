// Real desktop bridge + native UI, isolated test profiles only. No tokens/credentials are printed.
// AXDB_CONFIG_DIR must contain running candidate app discovery and the existing e2e-pg/e2e-neo4j fixtures.
// AXDB_APP_PID selects that exact app for native Run/approval clicks; creates/revokes temporary client fixtures.
import assert from 'node:assert/strict';
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { readFile, writeFile, rename, stat } from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { setTimeout as delay } from 'node:timers/promises';
const exec = promisify(execFile);
const dir = process.env.AXDB_CONFIG_DIR, appPid = process.env.AXDB_APP_PID;
assert(dir && appPid, 'set AXDB_CONFIG_DIR and AXDB_APP_PID');
const profiles = JSON.parse(await readFile(`${dir}/profiles.json`, 'utf8'));
const pg = profiles.find(p => p.name === 'e2e-pg'), neo = profiles.find(p => p.name === 'e2e-neo4j');
assert(pg?.fields.host === '127.0.0.1' && pg.fields.port === '55432' && ['axdb_test', 'dbtool_test'].includes(pg.fields.database), 'test PostgreSQL only');
assert(neo?.fields.uri === 'bolt://127.0.0.1:57687', 'test Neo4j only');
const { url, token: local } = JSON.parse(await readFile(`${dir}/agent-bridge.json`, 'utf8'));
const put = async (name, value) => { const tmp = `${dir}/${name}.${randomUUID()}.tmp`; await writeFile(tmp, JSON.stringify(value), { mode: 0o600 }); await rename(tmp, `${dir}/${name}`); };
let rpcId = 0;
async function rpc(token, method, params = {}) {
  const r = await fetch(url, { method: 'POST', signal: AbortSignal.timeout(70000), headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: ++rpcId, method, params }) });
  if (r.status !== 200) return { http_status: r.status };
  const reply = await r.json(); assert(!reply.error, JSON.stringify(reply.error)); return reply.result;
}
async function tool(token, name, args = {}) { const r = await rpc(token, 'tools/call', { name, arguments: args }); return r.structuredContent ?? r; }
async function gui(...args) {
  const { stdout } = await exec('orca', ['computer', ...args, '--app', `pid:${appPid}`, '--restore-window', '--no-screenshot', '--json']);
  const r = JSON.parse(stdout); assert(r.ok, JSON.stringify(r.error)); return r.result.snapshot?.treeText ?? '';
}
async function click(pattern) {
  const until = Date.now() + 15000;
  while (Date.now() < until) {
    const tree = await gui('get-app-state');
    const match = tree.split('\n').map(l => l.match(/^\s*(\d+) (.*)$/)).find(m => m && pattern.test(m[2]));
    if (match) {
      try { await gui('click', '--element-index', match[1]); return; }
      catch (error) {
        let code;try { code=JSON.parse(error.stdout??'{}').error?.code; } catch {}
        if(code!=='element_not_found')throw error;
      }
    }
    await delay(100);
  }
  throw new Error(`native control missing: ${pattern}`);
}
const clients = JSON.parse(await readFile(`${dir}/agent_clients.json`, 'utf8'));
const knownTokens = [local], fixtureIds = [];
function make(name, permission, allowed, maxRows = 3) {
  const token = randomBytes(32).toString('hex'), salt = randomUUID(), id = randomUUID(); knownTokens.push(token); fixtureIds.push(id);
  const client = { id, name: `a1-e2e-${name}`, token_salt: salt, token_hash: createHash('sha256').update(`${salt}:${token}`).digest('hex'), connections: { [pg.id]: permission, [neo.id]: 'none' }, allowed_tools: allowed, max_rows: maxRows, expires_at: null, revoked: false };
  clients.push(client); return { client, token };
}
const common = ['list_connections', 'open_connection', 'close_connection', 'run_query'];
const A = make('A', 'read', common), B = make('B', 'preview', [...common, 'preview_changes']), C = make('C', 'write_with_approval', common);
await put('agent_clients.json', clients);
const runMarker = randomUUID();
const checks = [];
async function check(name, run) { await run(); checks.push(name); console.log(`PASS ${name}`); }
const scalar = r => r.results[0].table.rows[0][0].v;
let session;
try {
  await check('default client preserves all connections and tools', async () => { const list = await tool(local, 'list_connections'); assert(list.some(p => p.id === pg.id) && list.some(p => p.id === neo.id)); const tools = (await rpc(local, 'tools/list')).tools; assert(tools.some(t => t.name === 'get_history')); });
  await check('client A filters connections and tools', async () => { assert.deepEqual((await tool(A.token, 'list_connections')).map(p => p.id), [pg.id]); assert((await rpc(A.token, 'tools/list')).tools.every(t => common.includes(t.name))); });
  await check('forbidden tools and invisible connections fail closed', async () => { assert.equal((await tool(A.token, 'get_history')).error.kind, 'Forbidden'); assert.equal((await tool(A.token, A.token)).error.kind, 'Forbidden'); assert.equal((await tool(A.token, 'open_connection', { profile_id: A.token })).error.kind, 'NotFound'); for (const id of [neo.id, randomUUID()]) assert.equal((await tool(A.token, 'open_connection', { profile_id: id })).error.kind, 'NotFound'); });
  const opening = tool(A.token, 'open_connection', { profile_id: pg.id });
  if (!pg.save_secret) {
    const until = Date.now() + 15000;
    let field;
    while (!field && Date.now() < until) {
      field = (await gui('get-app-state')).split('\n').map(l => l.match(/^\s*(\d+) (.*)$/)).find(m => m && /text field.*Password/.test(m[2]));
      if (!field) await delay(100);
    }
    assert(field, 'credential field missing');
    await gui('set-value', '--element-index', field[1], '--value', 'testpw');
    await click(/button Connect$/);
  }
  const opened = await opening; session = opened.session_id; assert(session, JSON.stringify(opened));
  await check('A write is rejected by database read-only transaction', async () => { const r = await tool(A.token, 'run_query', { session_id: session, text: "UPDATE e2e.typed SET note='a1-must-not-persist' WHERE id=1" }); assert.equal(r.error?.kind, 'Query'); assert.match(r.error.message, /read.only/i); });
  await check('A cannot request write or preview privilege', async () => { assert.equal((await tool(A.token, 'run_query', { session_id: session, text: 'SELECT 1', read_only: false })).error.kind, 'Forbidden'); assert.equal((await tool(A.token, 'preview_changes', { session_id: session, text: 'SELECT 1' })).error.kind, 'Forbidden'); });
  await check('host enforces max_rows across statements', async () => { const r = await tool(A.token, 'run_query', { session_id: session, text: 'SELECT generate_series(1,7); SELECT generate_series(1,7)' }); assert(!r.error); assert.equal(r.results.reduce((n, r) => n + r.table.rows.length, 0), 3); assert(r.results.some(r => r.truncated)); });
  const before = scalar(await tool(A.token, 'run_query', { session_id: session, text: "SELECT coalesce(note,'<null>') FROM e2e.typed WHERE id=1" }));
  await check('B preview returns changes and never commits', async () => { const r = await tool(B.token, 'preview_changes', { session_id: session, text: "UPDATE e2e.typed SET note='a1-preview-only' WHERE id=1" }); assert.equal(r.committed, false); assert.equal(r.statements[0].affected, 1); assert.equal(scalar(await tool(A.token, 'run_query', { session_id: session, text: "SELECT coalesce(note,'<null>') FROM e2e.typed WHERE id=1" })), before); assert.equal((await tool(B.token, 'run_query', { session_id: session, text: 'SELECT 1', read_only: false })).error.kind, 'Forbidden'); });
  await check('write approval shows rollback preview and denial is audited', async () => { const pending = tool(C.token, 'run_query', { session_id: session, text: 'UPDATE e2e.typed SET note=note WHERE id=1', read_only: false }); await click(/Deny agent query/); assert.equal((await pending).error.kind, 'UserDenied'); });
  await check('write executes only after preview and in-app approval', async () => { const pending = tool(C.token, 'run_query', { session_id: session, text: 'UPDATE e2e.typed SET note=note WHERE id=1', read_only: false }); await click(/Approve agent query/); const r = await pending; assert(!r.error); assert.equal(r.results[0].affected, 1); });
  await check('revocation during approval prevents execution', async () => { const pending = tool(C.token, 'run_query', { session_id: session, text: "UPDATE e2e.typed SET note='a1-revoked-must-not-persist' WHERE id=1", read_only: false });
    const until = Date.now() + 15000; while (!(await gui('get-app-state')).includes('Approve agent query')) { assert(Date.now() < until); await delay(100); }
    C.client.revoked = true; await put('agent_clients.json', clients); await click(/Approve agent query/); assert.equal((await pending).error.kind, 'Forbidden');
    assert.equal(scalar(await tool(A.token, 'run_query', { session_id: session, text: "SELECT coalesce(note,'<null>') FROM e2e.typed WHERE id=1" })), before);
  });
  await check('read_only connection rejects UI Run and agent preview on an already open session', async () => {
    await put('profiles.json', profiles.map(p => p.id === pg.id ? { ...p, access_mode: 'read_only' } : p));
    const text = `UPDATE e2e.typed SET note='a1-ui-${runMarker}' WHERE id=1`;
    assert((await tool(local, 'open_query_tab', { session_id: session, text })).tab_id);
    await click(/Run query/);
    const until = Date.now() + 10000;
    let entry;
    while (!entry && Date.now() < until) { const history = await tool(local, 'get_history', { limit: 100 }); entry = history.find(h => h.source === 'ui' && h.text === text); if (!entry) await delay(100); }
    assert(entry?.error && /read.only/i.test(entry.error));
    assert.equal((await tool(B.token, 'preview_changes', { session_id: session, text })).error.kind, 'Forbidden');
    assert.equal(scalar(await tool(A.token, 'run_query', { session_id: session, text: "SELECT coalesce(note,'<null>') FROM e2e.typed WHERE id=1" })), before);
    await put('profiles.json', profiles);
  });
  await check('token rotation, expiration and revocation return 401', async () => {
    const old = A.token; A.token = randomBytes(32).toString('hex'); knownTokens.push(A.token); A.client.token_hash = createHash('sha256').update(`${A.client.token_salt}:${A.token}`).digest('hex'); await put('agent_clients.json', clients);
    assert.equal((await rpc(old, 'ping')).http_status, 401); assert.deepEqual(await rpc(A.token, 'ping'), {});
    B.client.expires_at = '2000-01-01T00:00:00Z'; A.client.revoked = true; await put('agent_clients.json', clients);
    assert.equal((await rpc(A.token, 'ping')).http_status, 401); assert.equal((await rpc(B.token, 'ping')).http_status, 401);
  });
  await check('private hash-only store and redacted audit/logs', async () => {
    const store = await readFile(`${dir}/agent_clients.json`, 'utf8'), audit = await readFile(`${dir}/agent-audit.jsonl`, 'utf8'), log = await readFile(`${dir}/axdb.log`, 'utf8');
    for (const name of ['agent_clients.json', 'agent-audit.jsonl']) assert.equal((await stat(`${dir}/${name}`)).mode & 0o777, 0o600);
    for (const secret of [...knownTokens, 'testpw', 'testpassword', 'a1-must-not-persist', 'a1-preview-only', 'a1-revoked-must-not-persist']) { assert(!audit.includes(secret)); assert(!log.includes(secret)); assert(!store.includes(secret)); }
    const entries = audit.trim().split('\n').map(JSON.parse); assert(entries.some(e => e.client_id === A.client.id && e.decision === 'denied')); assert(entries.some(e => e.client_id === C.client.id && e.decision === 'approved')); assert(entries.some(e => e.client_id === C.client.id && e.decision === 'rejected')); assert(entries.filter(e => e.statement_hash).every(e => /^[a-f0-9]{64}$/.test(e.statement_hash)));
  });
  console.log(`${checks.length}/${checks.length} permissions checks passed`);
} finally {
  await put('profiles.json', profiles);
  await put('agent_clients.json', clients.filter(c => !fixtureIds.includes(c.id)));
  if (session) await tool(local, 'close_connection', { session_id: session });
}
