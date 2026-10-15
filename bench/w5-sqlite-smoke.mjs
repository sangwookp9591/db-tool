// Run against the W5 debug app, with its isolated SQLite profile already saved.
import { readFile, writeFile } from 'node:fs/promises';
import assert from 'node:assert/strict';
import path from 'node:path';
const directory = process.env.DBTOOL_CONFIG_DIR;
assert(directory, 'Set DBTOOL_CONFIG_DIR for the running W5 debug app');
const { url, token } = JSON.parse(await readFile(path.join(directory, 'agent-bridge.json'), 'utf8'));
assert.equal(new URL(url).hostname, '127.0.0.1');
let id = 0;
async function rpc(method, params = {}) {
  const response = await fetch(url, {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json', 'MCP-Protocol-Version': '2025-11-25' },
    body: JSON.stringify({ jsonrpc: '2.0', id: ++id, method, params }),
    signal: AbortSignal.timeout(60_000),
  });
  assert.equal(response.status, 200);
  const data = await response.json();
  assert(!data.error, JSON.stringify(data.error));
  return data.result;
}
async function tool(name, args = {}) {
  const response = await rpc('tools/call', { name, arguments: args });
  return response.structuredContent ?? JSON.parse(response.content[0].text);
}
await rpc('initialize', { protocolVersion: '2025-11-25', clientInfo: { name: 'w5-sqlite-smoke', version: '1' }, capabilities: {} });
const drivers = await tool('list_drivers');
const manifest = drivers.find(d => d.id === 'sqlite');
assert(manifest?.capabilities.read_only_exec);
assert(manifest.connection_fields.some(f => f.key === 'path' && f.kind === 'FilePath'));
assert(!drivers.some(d => d.id === 'example-kv'), 'Example must not be registered');
const connections = await tool('list_connections');
const profile = connections.find(p => p.id === 'w5-sqlite');
assert(profile);
const opened = await tool('open_connection', { profile_id: profile.id });
assert(opened.session_id, JSON.stringify(opened));
const session_id = opened.session_id;
const roots = await tool('explore', { session_id });
assert.deepEqual(roots.map(n => n.label), ['Tables', 'Views']);
const tables = await tool('explore', { session_id, parent: roots[0].id });
const table = tables.find(n => n.label === 'w5_samples'); assert(table);
const folders = await tool('explore', { session_id, parent: table.id });
const columns = await tool('explore', { session_id, parent: folders[0].id });
assert.equal(columns[0].icon, 'Key');
const indexes = await tool('explore', { session_id, parent: folders[1].id });
assert(indexes.some(n => n.detail?.includes('CREATE INDEX')));
const text = 'SELECT 9223372036854775807 AS exact, payload FROM w5_samples ORDER BY id';
await tool('open_query_tab', { session_id, text });
const selected = await tool('run_query', { session_id, text, read_only: true });
assert.equal(selected.results[0].table.rows[0][0].v, '9223372036854775807');
assert.equal(selected.results[0].table.rows[0][1].t, 'Json');
assert.equal(selected.results[0].table.rows[0][1].v, '{"big":9223372036854775807}');
const ui = await tool('get_ui_state');
assert.equal(ui.active_tab?.text, text);
assert.equal(ui.active_tab?.connection, 'W5 SQLite smoke');
assert.equal(ui.last_result[0].row_count, 3);
const blocked = await tool('run_query', { session_id, text: 'PRAGMA query_only=OFF; DELETE FROM w5_samples', read_only: true });
assert.equal(blocked.error?.kind, 'Query');
const timedOut = await tool('run_query', { session_id, text: 'WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n) SELECT sum(x) FROM n', timeout_ms: 300 });
assert.equal(timedOut.error?.kind, 'Timeout');
const recovered = await tool('run_query', { session_id, text: 'SELECT count(*) AS n FROM w5_samples' });
assert.equal(recovered.results[0].table.rows[0][0].v, '3');
const page1 = await tool('browse', { session_id, node_id: table.id, offset: 0, limit: 2 });
assert.equal(page1.results[0].row_count, 2); assert(page1.results[0].page.has_more);
const page2 = await tool('browse', { session_id, node_id: table.id, offset: 2, limit: 2 });
assert.equal(page2.results[0].row_count, 1); assert(!page2.results[0].page.has_more);
await tool('run_query', { session_id, text });
const evidence = {
  passed: true, timestamp: new Date().toISOString(), transport: 'running Tauri debug app localhost MCP bridge',
  checks: ['manifest registration', 'KV not registered', 'open SQLite file connection', 'explorer Tables/Views', 'columns and PK', 'index definition', 'visible query tab', 'exact integer and JSON', 'read-only bypass rejected', '300ms timeout', 'same session recovery', 'browse first/last page'],
  manifest, roots, tables, columns, indexes, selected, ui, blocked, timedOut, recovered, page1, page2,
};
await writeFile('bench/results-w5-sqlite-app.json', JSON.stringify(evidence, null, 2) + '\n');
console.log(JSON.stringify({ passed: true, checks: evidence.checks }));
// Leave the isolated app and tab open for visual inspection.
