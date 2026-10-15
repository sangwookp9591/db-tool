// Minimal MCP client for the AxDB localhost bridge: node bench/mcp-call.mjs <tool> '<json args>'
// Session ids persist between calls in $AXDB_CONFIG_DIR/.mcp-call-state.json (test use only).
import { readFile } from 'node:fs/promises';
const dir = process.env.AXDB_CONFIG_DIR;
if (!dir) throw new Error('set AXDB_CONFIG_DIR');
const { url, token } = JSON.parse(await readFile(`${dir}/agent-bridge.json`, 'utf8'));
const headers = { Authorization: `Bearer ${process.env.AXDB_MCP_TOKEN ?? token}`, 'Content-Type': 'application/json', Accept: 'application/json', 'MCP-Protocol-Version': '2025-11-25' };
let id = 0;
const rpc = async (method, params = {}) => (await (await fetch(url, { method: 'POST', headers, body: JSON.stringify({ jsonrpc: '2.0', id: ++id, method, params }) })).json());
await rpc('initialize', { protocolVersion: '2025-11-25', clientInfo: { name: 'mcp-call', version: '1' }, capabilities: {} });
const [tool, args = '{}'] = process.argv.slice(2);
const res = tool === 'tools/list' ? await rpc('tools/list') : await rpc('tools/call', { name: tool, arguments: JSON.parse(args) });
const out = res.result?.structuredContent ?? res.result ?? res;
console.log(JSON.stringify(out, null, 1).slice(0, Number(process.env.MAX ?? 6000)));
