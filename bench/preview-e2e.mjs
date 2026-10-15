// Coordinator check: change previews through the MCP bridge never commit (test DBs only).
import { readFile } from 'node:fs/promises';
import assert from 'node:assert/strict';
const dir = process.env.AXDB_CONFIG_DIR; assert(dir, 'set AXDB_CONFIG_DIR');
const { url, token } = JSON.parse(await readFile(dir + '/agent-bridge.json', 'utf8'));
const headers = { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json', Accept: 'application/json', 'MCP-Protocol-Version': '2025-11-25' };
let id = 0;
const rpc = async (m, p = {}) => { const d = await (await fetch(url, { method: 'POST', headers, body: JSON.stringify({ jsonrpc: '2.0', id: ++id, method: m, params: p }) })).json(); if (d.error) throw new Error(JSON.stringify(d.error)); return d.result; };
const tool = async (n, a = {}) => { const r = await rpc('tools/call', { name: n, arguments: a }); return r.structuredContent ?? JSON.parse(r.content[0].text); };
await rpc('initialize', { protocolVersion: '2025-11-25', clientInfo: { name: 'preview-e2e', version: '1' }, capabilities: {} });
const out = [];
const check = async (name, fn) => { try { out.push(`PASS ${name} ${await fn() ?? ''}`); } catch (e) { out.push(`FAIL ${name} ${e.message}`); } };
const scalar = r => r.results[0].table.rows[0][0].v;

const pg = (await tool('open_connection', { profile_id: 'e2e-pg-0001' })).session_id;
await check('pg UPDATE preview shows before/after, does not commit', async () => {
  const before = scalar(await tool('run_query', { session_id: pg, text: "SELECT coalesce(note,'<null>') FROM e2e.typed WHERE id=1" }));
  const p = await tool('preview_changes', { session_id: pg, text: "UPDATE e2e.typed SET note='previewed' WHERE id=1" });
  const s = (p.preview ?? p).statements[0];
  assert.equal(s.affected, 1, JSON.stringify(s).slice(0, 300));
  assert(JSON.stringify(s.after).includes('previewed'));
  const after = scalar(await tool('run_query', { session_id: pg, text: "SELECT coalesce(note,'<null>') FROM e2e.typed WHERE id=1" }));
  assert.equal(after, before); return `affected=1 value unchanged (${after})`;
});
await check('pg DDL preview shows schema diff, does not commit', async () => {
  const p = await tool('preview_changes', { session_id: pg, text: 'ALTER TABLE e2e.typed ADD COLUMN preview_col int' });
  const s = (p.preview ?? p).statements[0];
  assert(JSON.stringify(s.schema_diff ?? {}).includes('preview_col'), JSON.stringify(s).slice(0, 300));
  const n = scalar(await tool('run_query', { session_id: pg, text: "SELECT count(*) FROM information_schema.columns WHERE table_schema='e2e' AND column_name='preview_col'" }));
  assert.equal(n, '0'); return 'diff has preview_col, column absent afterwards';
});
await check('pg non-rollbackable statement refused', async () => {
  const p = await tool('preview_changes', { session_id: pg, text: 'VACUUM e2e.typed' });
  assert(p.error || JSON.stringify(p).match(/refus|reject|not.*roll/i), JSON.stringify(p).slice(0, 300)); return (p.error?.message ?? 'refused in notes').slice(0, 80);
});
const neo = (await tool('open_connection', { profile_id: 'e2e-neo-0001' })).session_id;
await check('neo4j CREATE preview shows counters, does not commit', async () => {
  const p = await tool('preview_changes', { session_id: neo, text: "CREATE (:DbtE_Preview {k: 1})" });
  const s = (p.preview ?? p).statements[0];
  assert(JSON.stringify(s.counters).match(/nodes_created|nodesCreated/), JSON.stringify(s).slice(0, 300));
  const n = scalar(await tool('run_query', { session_id: neo, text: 'MATCH (n:DbtE_Preview) RETURN count(n)' }));
  assert.equal(n, '0'); return `counters=${JSON.stringify(s.counters)} committed nodes=0`;
});
const sq = (await tool('open_connection', { profile_id: 'e2e-sqlite-0001' })).session_id;
await check('sqlite DELETE preview, does not commit', async () => {
  const before = scalar(await tool('run_query', { session_id: sq, text: 'SELECT count(*) FROM items' }));
  const p = await tool('preview_changes', { session_id: sq, text: "DELETE FROM items WHERE id <= 10" });
  const s = (p.preview ?? p).statements[0]; assert.equal(s.affected, 10, JSON.stringify(s).slice(0, 300));
  const after = scalar(await tool('run_query', { session_id: sq, text: 'SELECT count(*) FROM items' }));
  assert.equal(after, before); return `affected=10 count unchanged (${after})`;
});
for (const s of [pg, neo, sq]) await tool('close_connection', { session_id: s });
console.log(out.join('\n')); const fails = out.filter(l => l.startsWith('FAIL')).length; console.log(`${out.length - fails}/${out.length} passed`); process.exit(fails ? 1 : 0);
