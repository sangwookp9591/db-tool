import { readFile, stat } from 'node:fs/promises';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import assert from 'node:assert/strict';
const directory = process.env.DBTOOL_CONFIG_DIR;
assert(directory, 'Set DBTOOL_CONFIG_DIR to the running desktop app config directory');
const discovery = path.join(directory, 'agent-bridge.json');
const { url, token } = JSON.parse(await readFile(discovery, 'utf8'));
assert.equal(new URL(url).hostname, '127.0.0.1');
if (process.platform !== 'win32') assert.equal((await stat(discovery)).mode & 0o777, 0o600);
const headers = { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json', Accept: 'application/json, text/event-stream', 'MCP-Protocol-Version': '2025-11-25' };
const probe = `agent_smoke_${randomUUID().replaceAll('-', '')}`;
let next = 0;
async function rpc(method, params = {}) {
  const response = await fetch(url, { method: 'POST', headers, body: JSON.stringify({ jsonrpc: '2.0', id: ++next, method, params }), signal: AbortSignal.timeout(100_000) });
  assert.equal(response.status, 200, `HTTP failure for ${method}`);
  const data = await response.json(); assert(!data.error, `JSON-RPC failure for ${method}`); return data.result;
}
async function tool(name, args = {}) {
  const response = await rpc('tools/call', { name, arguments: args });
  return response.structuredContent ?? JSON.parse(response.content[0].text);
}
assert.equal((await fetch(url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' })).status, 401);
assert.equal((await fetch(url, { method: 'POST', headers: { ...headers, Origin: 'https://example.invalid' }, body: '{}' })).status, 403);
assert.equal((await rpc('initialize', { protocolVersion: '2025-11-25', clientInfo: { name: 'dbtool-smoke', version: '1' }, capabilities: {} })).protocolVersion, '2025-11-25');
assert.equal((await fetch(url, { method: 'POST', headers, body: JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) })).status, 202);
const listed = await rpc('tools/list'); assert(listed.tools.some(t => t.name === 'run_query'));
const profiles = JSON.parse(await readFile(path.join(directory, 'profiles.json'), 'utf8'));
const profile = profiles.find(p => (!process.env.DBTOOL_TEST_PROFILE || p.id === process.env.DBTOOL_TEST_PROFILE) && p.fields.host === '127.0.0.1' && p.fields.port === '55432');
assert(profile, 'Prepare a saved test PG profile on 127.0.0.1:55432 with its credential stored or enter the credential in the app when prompted');
const opened = await tool('open_connection', { profile_id: profile.id }); assert(opened.session_id, JSON.stringify(opened)); const session_id = opened.session_id;
try {
  const explored = await tool('explore', { session_id }); assert(Array.isArray(explored));
  const selected = await tool('run_query', { session_id, text: 'SELECT 9223372036854775807::int8 AS exact', read_only: true });
  assert.equal(selected.results[0].table.rows[0][0].v, '9223372036854775807');
  const blocked = await tool('run_query', { session_id, text: `CREATE TABLE ${probe}(id integer)`, read_only: true }); assert.equal(blocked.error?.kind, 'Query');
  console.log('Waiting for write approval to expire; do not approve the query in the app.');
  const started = Date.now();
  const denied = await tool('run_query', { session_id, text: `CREATE TABLE ${probe}(id integer)`, read_only: false, timeout_ms: 1 });
  assert.equal(denied.error?.kind, 'UserDenied'); assert(Date.now() - started >= 29_000, 'Approval must remain pending until its deadline');
  const check = await tool('run_query', { session_id, text: `SELECT to_regclass('public.${probe}') IS NULL AS absent` }); assert.equal(check.results[0].table.rows[0][0].v, true);
  const history = await tool('get_history', { limit: 10 }); assert(history.some(entry => entry.source === 'agent'));
  console.log(JSON.stringify({ passed: true, checks: ['owner-only discovery', 'authentication', 'origin rejection', 'initialize', 'tools/list', 'open_connection', 'explore', 'read-only exact value', 'read-only write rejection', 'approval timeout denial', 'no write persisted', 'agent history'] }));
} finally { await tool('close_connection', { session_id }); }
