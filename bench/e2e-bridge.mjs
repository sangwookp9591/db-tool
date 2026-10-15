// Coordinator E2E: drives the real desktop app through its localhost MCP bridge against the TEST databases only.
// Prereq: app running with bridge on; profiles "e2e-pg" (127.0.0.1:55432), "e2e-neo4j" (127.0.0.1:57687),
// "e2e-pg-badport" (127.0.0.1:1) saved in CONFIG_DIR with credentials in the OS keychain.
// Fixtures: schema e2e (PG) and labels DbtE_* (Neo4j) created beforehand.
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';

const dir = process.env.AXDB_CONFIG_DIR;
assert(dir, 'set AXDB_CONFIG_DIR');
const { url, token } = JSON.parse(await readFile(path.join(dir, 'agent-bridge.json'), 'utf8'));
const headers = { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json', Accept: 'application/json', 'MCP-Protocol-Version': '2025-11-25' };
let id = 0;
async function rpc(method, params = {}) {
  const r = await fetch(url, { method: 'POST', headers, body: JSON.stringify({ jsonrpc: '2.0', id: ++id, method, params }) });
  const d = await r.json(); if (d.error) throw new Error(JSON.stringify(d.error)); return d.result;
}
async function tool(name, args = {}) {
  const r = await rpc('tools/call', { name, arguments: args });
  return r.structuredContent ?? JSON.parse(r.content[0].text);
}
const results = [];
async function check(name, fn) {
  const t0 = performance.now();
  try { const note = await fn(); results.push({ name, ok: true, ms: Math.round(performance.now() - t0), note }); console.log('PASS', name, note ?? ''); }
  catch (e) { results.push({ name, ok: false, error: String(e.message ?? e) }); console.log('FAIL', name, e.message); }
}
const cell = (res, r, c) => res.table.rows[r][res.table.columns.findIndex(x => x.name === c)];

await rpc('initialize', { protocolVersion: '2025-11-25', clientInfo: { name: 'axdb-e2e', version: '1' }, capabilities: {} });
const profiles = JSON.parse(await readFile(path.join(dir, 'profiles.json'), 'utf8'));
const byName = n => profiles.find(p => p.name === n)?.id;

await check('drivers declared via manifests', async () => {
  const d = await tool('list_drivers');
  const list = Array.isArray(d) ? d : (d.drivers ?? d.items ?? []);
  assert(list.length >= 2, 'list_drivers: ' + JSON.stringify(d).slice(0, 300));
  return list.map(x => `${x.id}:${x.paradigm}`).join(',');
});

// ---------------- PostgreSQL ----------------
let pg;
await check('pg open (credential from keychain)', async () => { const o = await tool('open_connection', { profile_id: byName('e2e-pg') }); assert(o.session_id, JSON.stringify(o)); pg = o.session_id; });
await check('pg explore schema>table>columns/indexes (lazy)', async () => {
  const roots = await tool('explore', { session_id: pg }); const arr = roots.nodes ?? roots;
  const e2e = arr.find(n => n.label === 'e2e'); assert(e2e, 'schema e2e missing');
  assert(!arr.some(n => n.label === 'pg_catalog'), 'system schema shown');
  let level = await tool('explore', { session_id: pg, parent: e2e.id }); level = level.nodes ?? level;
  let table = level.find(n => n.label === 'typed');
  for (const folder of level.filter(n => n.has_children)) { if (table) break; let k = await tool('explore', { session_id: pg, parent: folder.id }); k = k.nodes ?? k; table = k.find(n => n.label === 'typed'); }
  assert(table, 'table typed missing');
  let kids = await tool('explore', { session_id: pg, parent: table.id }); kids = kids.nodes ?? kids;
  const labels = []; for (const k of kids) { let g = await tool('explore', { session_id: pg, parent: k.id }); g = g.nodes ?? g; labels.push(...g.map(n => `${n.label}${n.detail ? ' ' + n.detail : ''}`)); }
  assert(labels.some(l => l.startsWith('id') && /PK/.test(l)), 'PK not marked: ' + labels.join('|'));
  assert(labels.some(l => l.includes('typed_created_idx')), 'index missing');
  return labels.slice(0, 4).join(' | ');
});
await check('pg exact types (int8 max/min, numeric, NULL, timestamptz, jsonb big, bytea, uuid, array, unicode)', async () => {
  const r = (await tool('run_query', { session_id: pg, text: 'SELECT * FROM e2e.typed ORDER BY id' })).results[0];
  assert.deepEqual(cell(r, 0, 'big'), { t: 'Int', v: '9223372036854775807' });
  assert.deepEqual(cell(r, 1, 'big'), { t: 'Int', v: '-9223372036854775808' });
  assert.deepEqual(cell(r, 0, 'amount'), { t: 'Decimal', v: '12345678901234567890.123456789' });
  assert.deepEqual(cell(r, 1, 'amount'), { t: 'Decimal', v: '-0.000000001' });
  assert.deepEqual(cell(r, 0, 'note'), { t: 'Null' });
  assert.equal(cell(r, 0, 'created').t, 'DateTimeTz');
  assert.equal(cell(r, 0, 'd').t, 'Date');
  assert(cell(r, 0, 'doc').t === 'Json' && cell(r, 0, 'doc').v.includes('9223372036854775807'));
  assert.equal(cell(r, 0, 'raw').t, 'Bytes');
  assert.equal(cell(r, 0, 'uid').t, 'Uuid');
  assert.deepEqual(cell(r, 1, 'note'), { t: 'Text', v: '한글 텍스트' });
  return `columns: ${r.table.columns.map(c => c.name + ':' + c.type_name).join(', ')} | created=${cell(r, 0, 'created').v}`;
});
await check('pg browse paging', async () => {
  let roots = await tool('explore', { session_id: pg }); roots = roots.nodes ?? roots;
  const e2e = roots.find(n => n.label === 'e2e');
  let level = await tool('explore', { session_id: pg, parent: e2e.id }); level = level.nodes ?? level;
  let big = level.find(n => n.label === 'big');
  for (const f of level.filter(n => n.has_children)) { if (big) break; let k = await tool('explore', { session_id: pg, parent: f.id }); k = k.nodes ?? k; big = k.find(n => n.label === 'big'); }
  const p1 = (await tool('browse', { session_id: pg, node_id: big.id, offset: 0, limit: 100 })).results[0];
  const p2 = (await tool('browse', { session_id: pg, node_id: big.id, offset: 100, limit: 100 })).results[0];
  assert.equal(p1.page.has_more, true); assert.deepEqual(p2.table.rows[0][0], { t: 'Int', v: '101' });
  return `page1 rows=${p1.row_count} page2 first id=${p2.table.rows[0][0].v}`;
});
await check('pg large result bounded (250k rows, row_limit 1000)', async () => {
  const r = (await tool('run_query', { session_id: pg, text: 'SELECT * FROM e2e.big', row_limit: 1000 })).results[0];
  assert.equal(r.truncated, true); assert(r.row_count <= 1000);
  return `row_count=${r.row_count} truncated=${r.truncated} elapsed=${r.elapsed_ms}ms`;
});
await check('pg query error with position', async () => {
  const r = await tool('run_query', { session_id: pg, text: 'SELECT * FROM e2e.nope' });
  assert.equal(r.error?.kind, 'Query'); return r.error.message;
});
await check('pg timeout cancels server-side, session survives', async () => {
  const t0 = Date.now();
  const r = await tool('run_query', { session_id: pg, text: 'SELECT pg_sleep(30)', timeout_ms: 700 });
  assert.equal(r.error?.kind, 'Timeout'); const ms = Date.now() - t0; assert(ms < 5000, `took ${ms}`);
  const ok = (await tool('run_query', { session_id: pg, text: 'SELECT 1 AS one' })).results[0];
  assert.deepEqual(ok.table.rows[0][0], { t: 'Int', v: '1' });
  return `timeout after ${ms}ms; follow-up ok`;
});
await check('pg multiple result sets', async () => {
  const r = await tool('run_query', { session_id: pg, text: "SELECT 1 AS a; SELECT 'x' AS b, 2 AS c" });
  assert.equal(r.results.length, 2); return r.results.map(x => x.title).join(' / ');
});
await check('pg agent read-only enforced at DB level', async () => {
  const r = await tool('run_query', { session_id: pg, text: 'CREATE TABLE e2e.should_not_exist(i int)' });
  assert.equal(r.error?.kind, 'Query'); return r.error.message;
});
await check('connection failure classified', async () => {
  const o = await tool('open_connection', { profile_id: byName('e2e-pg-badport') });
  assert.equal(o.error?.kind, 'Connection', JSON.stringify(o)); return o.error.message;
});

// ---------------- Neo4j ----------------
let neo;
await check('neo4j open', async () => { const o = await tool('open_connection', { profile_id: byName('e2e-neo4j') }); assert(o.session_id, JSON.stringify(o)); neo = o.session_id; });
await check('neo4j explore labels / rel types / property keys', async () => {
  let roots = await tool('explore', { session_id: neo }); roots = roots.nodes ?? roots;
  const out = [];
  for (const r of roots) { let k = await tool('explore', { session_id: neo, parent: r.id }); k = k.nodes ?? k; out.push(`${r.label}(${k.length})`); if (r.label.startsWith('Label')) assert(k.some(n => n.label === 'DbtE_Person')); if (r.label.startsWith('Rel')) assert(k.some(n => n.label === 'KNOWS')); }
  return out.join(', ');
});
await check('neo4j params + mixed path/node/rel/list/map result -> graph + table', async () => {
  const r = (await tool('run_query', { session_id: neo, text: "MATCH p=(a:DbtE_Person {name:$n})-[r:KNOWS]->(b) RETURN a, r, b, p, [a,b] AS l, {who: b} AS m, a.big AS big, a.seen AS seen, b.loc AS loc", params: { n: 'Alice' } })).results[0];
  assert(r.graph && r.table, 'need both views');
  const ids = new Set(r.graph.nodes.map(n => n.id)); assert.equal(ids.size, r.graph.nodes.length, 'dup nodes');
  assert.equal(r.graph.nodes.length, 2); assert.equal(r.graph.rels.length, 1);
  assert.equal(cell(r, 0, 'p').t, 'Path');
  assert.deepEqual(cell(r, 0, 'big'), { t: 'Int', v: '9007199254740993' });
  assert.equal(cell(r, 0, 'seen').t, 'DateTimeTz');
  assert.equal(cell(r, 0, 'loc').t, 'Point');
  return `nodes=${r.graph.nodes.length} rels=${r.graph.rels.length} cols=${r.table.columns.map(c => c.name + ':' + c.type_name).join(',')}`;
});
await check('neo4j expand neighbours', async () => {
  const r = (await tool('run_query', { session_id: neo, text: "MATCH (a:DbtE_Person {name:'Alice'}) RETURN a" })).results[0];
  const e = (await tool('expand_node', { session_id: neo, element_id: r.graph.nodes[0].id })).results[0];
  assert(e.graph.nodes.length >= 3, 'Alice has KNOWS Bob and ACTED_IN Matrix');
  return `expanded nodes=${e.graph.nodes.length} rels=${e.graph.rels.length}`;
});
await check('neo4j graph limits (1200+ nodes requested)', async () => {
  const r = (await tool('run_query', { session_id: neo, text: 'MATCH (p:DbtE_Person)-[r:ACTED_IN]->(m:DbtE_Movie) RETURN p, r, m', row_limit: 1000 })).results[0];
  assert.equal(r.truncated, true); return `row_count=${r.row_count} graph nodes=${r.graph?.nodes.length} truncated=${r.truncated}`;
});
await check('neo4j syntax error', async () => {
  const r = await tool('run_query', { session_id: neo, text: 'MATCH (n RETURN n' }); assert.equal(r.error?.kind, 'Query'); return `${r.error.message.slice(0, 80)}`;
});
await check('neo4j timeout, session survives', async () => {
  const t0 = Date.now();
  const r = await tool('run_query', { session_id: neo, text: 'UNWIND range(1, 500000000) AS x WITH x WHERE x < 0 RETURN x', timeout_ms: 700 });
  assert.equal(r.error?.kind, 'Timeout', JSON.stringify(r).slice(0, 200)); const ms = Date.now() - t0;
  const ok = (await tool('run_query', { session_id: neo, text: 'RETURN 1 AS one' })).results[0];
  assert.deepEqual(ok.table.rows[0][0], { t: 'Int', v: '1' }); return `timeout after ${ms}ms; follow-up ok`;
});
await check('neo4j agent read-only enforced', async () => {
  const r = await tool('run_query', { session_id: neo, text: "CREATE (:DbtE_ShouldNot)" }); assert.equal(r.error?.kind, 'Query'); return r.error.message.slice(0, 80);
});
await check('history records agent source', async () => {
  const h = await tool('get_history', { limit: 50 }); const list = Array.isArray(h) ? h : (h.entries ?? h.items ?? []);
  assert(Array.isArray(list) && list.length, 'get_history: ' + JSON.stringify(h).slice(0, 300));
  assert(list.some(e => e.source === 'agent')); return `${list.length} entries`;
});
await check('close connections', async () => { await tool('close_connection', { session_id: pg }); await tool('close_connection', { session_id: neo }); });

const failed = results.filter(r => !r.ok).length;
console.log(`\n${results.length - failed}/${results.length} passed`);
process.stdout.write(JSON.stringify(results, null, 2) + '\n');
process.exit(failed ? 1 : 0);
