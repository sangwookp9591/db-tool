import { describe, expect, it, vi } from 'vitest';
import { createRegistry, compact, type AgentHost } from './registry';
import { expose, register, type AgentAPI } from './webmcp';
import type { DriverManifest, ResultSet } from '../contract';
function setup(readOnly = true) {
  const driver = { id: 'fixture', capabilities: { read_only_exec: readOnly, paged_browse: true, graph_expand: false } } as DriverManifest;
  const state = { drivers: [driver], profiles: [{ id: 'p', name: 'Test', driver_id: 'fixture', fields: { password: 'must-not-return' }, save_secret: false }], sessions: {} as Record<string, string> };
  const invoke = vi.fn().mockResolvedValue([]), approve = vi.fn().mockResolvedValue(false);
  const host: AgentHost = { snapshot: () => state, invoke, approve, open: vi.fn().mockResolvedValue('s'), close: vi.fn(), openTab: vi.fn().mockReturnValue('tab'), uiState: () => ({}), results: vi.fn(), activity: vi.fn(), history: vi.fn() };
  const registry = createRegistry(host); const target: { axdb?: { agent: AgentAPI } } = {}; const api = expose(registry, target);
  return { state, host, invoke, approve, registry, api };
}
describe('agent registry', () => {
  it('keeps explain and lint read-only and rejects agent ANALYZE arguments', async () => {
    const {state,api,invoke,approve}=setup(); state.sessions.p='s';
    expect(api.listTools().some(t=>t.name==='explain_query')).toBe(false);
    state.drivers[0].capabilities.explain=true; state.drivers[0].capabilities.lint=true;
    expect(api.listTools().find(t=>t.name==='explain_query')?.annotations.readOnlyHint).toBe(true);
    expect(api.listTools().find(t=>t.name==='lint_query')?.annotations.readOnlyHint).toBe(true);
    await api.callTool('explain_query',{session_id:'s',query:'SELECT 1'});
    expect(invoke).toHaveBeenCalledWith('explain_query',expect.objectContaining({query:'SELECT 1',analyze:false}));
    invoke.mockClear();
    const result=await api.callTool('explain_query',{session_id:'s',query:'UPDATE people SET id=2',analyze:true});
    expect(result).toHaveProperty('error'); expect(invoke).not.toHaveBeenCalled(); expect(approve).not.toHaveBeenCalled();
  });
  it('loads manifests from the host before the UI snapshot is ready', async () => {
    const { state, invoke, api } = setup(); const drivers = state.drivers; state.drivers = [];
    let ready!: (value: DriverManifest[]) => void;
    invoke.mockReturnValue(new Promise(resolve => { ready = resolve; }));
    const request = api.callTool('list_drivers', {});
    ready(drivers);
    expect(await request).toEqual(drivers);
    expect(invoke).toHaveBeenCalledWith('drivers_list', {});
  });
  it('derives optional tools from open session capabilities and rechecks target capability', async () => {
    const { state, api } = setup(); expect(api.listTools().map(t => t.name)).not.toContain('browse');
    state.sessions.p = 's'; expect(api.listTools().map(t => t.name)).toContain('browse'); expect(api.listTools().map(t => t.name)).not.toContain('expand_node');
    state.drivers[0].capabilities.graph_expand = true; expect(api.listTools().map(t => t.name)).toContain('expand_node');
    delete state.sessions.p; expect(api.listTools().map(t => t.name)).not.toContain('expand_node');
  });
  it('registers preview only for capable open sessions and rolls through the host without approval', async () => {
    const { state, api, invoke, approve, host } = setup(); state.sessions.p = 's';
    expect(api.listTools().some(t => t.name === 'preview_changes')).toBe(false);
    state.drivers[0].capabilities.preview_changes = 'Transactional';
    expect(api.listTools().find(t => t.name === 'preview_changes')?.annotations.readOnlyHint).toBe(true);
    invoke.mockResolvedValue({ committed: false, statements: [], notes: ['rolled back'] });
    expect(await api.callTool('preview_changes', { session_id: 's', text: 'DELETE FROM items' })).toMatchObject({ committed: false });
    expect(approve).not.toHaveBeenCalled(); expect(host.history).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledWith('preview', expect.objectContaining({ source: 'agent', req: expect.objectContaining({ preview_sample_limit: 50 }) }));
    const controller = new AbortController(); controller.abort(); invoke.mockClear();
    expect(await api.callTool('preview_changes', { session_id: 's', text: 'DELETE FROM items' }, { signal: controller.signal })).toMatchObject({ error: { kind: 'Cancelled' } });
    expect(invoke).not.toHaveBeenCalled();
  });
  it('defaults read_only true, caps driver limits and preserves typed values', async () => {
    const { state, api, invoke, approve } = setup(); state.sessions.p = 's';
    invoke.mockResolvedValue([result(201)]);
    const response = await api.callTool('run_query', { session_id: 's', text: 'SELECT 1' }) as { results: ResultSet[] };
    expect(approve).not.toHaveBeenCalled(); expect(invoke).toHaveBeenCalledWith('execute_agent', expect.objectContaining({ req: expect.objectContaining({ read_only: true, row_limit: 1000, graph_node_limit: 200 }) }));
    expect(response.results[0].table!.rows).toHaveLength(200); expect(response.results[0].table!.rows[0][0]).toEqual({ t: 'Int', v: '9223372036854775807' }); expect(response.results[0].truncated).toBe(true);
  });
  it.each([true, false])('requires approval for writes or missing enforcement, denial never executes (%s)', async readOnly => {
    const { state, api, invoke, approve } = setup(readOnly); state.sessions.p = 's'; state.drivers[0].capabilities.preview_changes = 'Transactional';
    const response = await api.callTool('run_query', { session_id: 's', text: 'DELETE FROM items', ...(readOnly ? { read_only: false } : {}) });
    expect(response).toMatchObject({ error: { kind: 'UserDenied' } }); expect(approve).toHaveBeenCalledOnce(); expect(invoke).toHaveBeenCalledWith('preview', expect.anything()); expect(invoke.mock.calls.some(([name]) => name === 'execute_agent')).toBe(false);
  });
  it('executes only after explicit approval and reports agent history', async () => {
    const { state, api, invoke, approve, host } = setup(); state.sessions.p = 's'; state.drivers[0].capabilities.preview_changes = 'Transactional'; approve.mockResolvedValue(true);
    await api.callTool('run_query', { session_id: 's', text: 'DELETE FROM items', read_only: false });
    expect(invoke).toHaveBeenCalledWith('execute_agent', expect.objectContaining({ req: expect.objectContaining({ read_only: false }) })); expect(host.history).toHaveBeenCalledOnce(); expect(invoke).toHaveBeenCalledWith('agent_approve_write', expect.anything());
  });
  it('refuses writes without preview support before asking for approval', async () => {
    const { state, api, invoke, approve } = setup(); state.sessions.p = 's';
    expect(await api.callTool('run_query', { session_id: 's', text: 'DELETE FROM items', read_only: false })).toMatchObject({ error: { kind: 'Unsupported' } });
    expect(invoke).not.toHaveBeenCalled(); expect(approve).not.toHaveBeenCalled();
  });
  it.each([true, false])('classifies transport cancellation before execution as Timeout (read_only=%s)', async readOnly => {
    const { state, api, invoke } = setup(); state.sessions.p = 's'; const controller = new AbortController(); controller.abort();
    expect(await api.callTool('run_query', { session_id: 's', text: 'SELECT 1', read_only: readOnly }, { signal: controller.signal })).toMatchObject({ error: { kind: 'Timeout' } }); expect(invoke).not.toHaveBeenCalled();
  });
  it('validates required fields, types, ranges, unknown fields and JSON values', async () => {
    const { state, api, invoke } = setup(); state.sessions.p = 's';
    for (const input of [{ text: 'SELECT 1' }, { session_id: 's', text: 1 }, { session_id: 's', text: 'x', row_limit: 1001 }, { session_id: 's', text: 'x', read_only: 'false' }, { session_id: 's', text: 'x', extra: true }, { session_id: 's', text: 'x', params: { x: NaN } }]) expect(await api.callTool('run_query', input)).toMatchObject({ error: { kind: 'InvalidInput' } });
    expect(invoke).not.toHaveBeenCalled();
  });
  it('never exposes connection fields or secrets', async () => {
    const { api } = setup(); expect(JSON.stringify(await api.callTool('list_connections', {}))).not.toContain('must-not-return');
  });
  it('registers WebMCP tools with a lifetime signal and validates execute calls', async () => {
    const { api } = setup(); const registerTool = vi.fn(); vi.stubGlobal('document', { modelContext: { registerTool } }); vi.stubGlobal('navigator', {}); const stop = register(api);
    expect(registerTool.mock.calls.map(([tool]) => tool.name).sort()).toEqual(api.listTools().map(tool => tool.name).sort()); const [tool, options] = registerTool.mock.calls[0]; expect(options.signal.aborted).toBe(false);
    expect(await tool.execute({ unknown: 1 })).toMatchObject({ error: { kind: 'InvalidInput' } }); stop(); expect(options.signal.aborted).toBe(true); vi.unstubAllGlobals();
  });
  it('limits aggregate rows, graph nodes and documents without dangling relations', () => {
    const source = result(201); source.graph = { nodes: Array.from({ length: 201 }, (_, i) => ({ id: String(i), labels: [], props: [] })), rels: [{ id: 'r', rel_type: 'X', start: '0', end: '200', props: [] }] }; source.documents = Array(201).fill({ t: 'Null' });
    const results = compact([source, source]); expect(results.flatMap(r => r.table!.rows)).toHaveLength(200); expect(results.flatMap(r => r.graph!.nodes)).toHaveLength(200); expect(results[0].graph!.rels).toHaveLength(0); expect(results.flatMap(r => r.documents!)).toHaveLength(200); expect(source.table!.rows).toHaveLength(201);
  });
});
function result(count: number): ResultSet { return { title: 'select', table: { columns: [{ name: 'id', type_name: 'int8' }], rows: Array.from({ length: count }, () => [{ t: 'Int', v: '9223372036854775807' }]) }, graph: null, documents: null, message: null, affected: null, row_count: count, truncated: false, page: null, elapsed_ms: 1 }; }

describe('AI context tools', () => {
  it('shares the Rust builder and excludes row data from work snapshots', async () => {
    const { state, host, invoke, api } = setup(); state.sessions.p = 's';
    host.workContext = () => ({session_id:'s',scope:{text:'SELECT 1',run:'r',include_rows:true,selected_rows:{columns:[],rows:[]}}});
    await api.callTool('get_schema_context', {session_id:'s',focus:'users'});
    expect(invoke).toHaveBeenLastCalledWith('context_get',{sessionId:'s',scope:{focus:'users'}});
    await api.callTool('get_work_context', {});
    expect(invoke).toHaveBeenLastCalledWith('context_get',{sessionId:'s',scope:{text:'SELECT 1',run:'r',focus:undefined}});
    await api.callTool('explain_error',{run:'r'});
    expect(invoke).toHaveBeenLastCalledWith('context_error',{runId:'r'});
    expect(invoke.mock.calls.every(([name]) => !['execute','execute_agent','preview'].includes(name))).toBe(true);
  });
  it('rejects extra context parameters and closed sessions', async () => {
    const { api, invoke } = setup();
    expect(await api.callTool('get_schema_context',{session_id:'missing'})).toHaveProperty('error');
    expect(await api.callTool('get_work_context',{include_rows:true})).toHaveProperty('error');
    expect(invoke).not.toHaveBeenCalled();
  });
});
