import { perf } from '../perf';
import type { Commands } from '../api';
import type { DriverManifest, Profile, ResultSet, JsonValue } from '../contract';
export interface Schema { type: string; properties?: Record<string, Schema>; required?: string[]; additionalProperties?: boolean; minimum?: number; maximum?: number; default?: unknown }
export interface Tool { name: string; title: string; description: string; inputSchema: Schema; annotations: { readOnlyHint: boolean; consequentialHint: boolean; untrustedContentHint?: boolean }; execute: (input: Record<string, unknown>, context?: { signal?: AbortSignal }) => Promise<unknown> }
export interface AgentHost {
  snapshot: () => { drivers: DriverManifest[]; profiles: Profile[]; sessions: Record<string, string> };
  invoke: <K extends keyof Commands>(name: K, args: Commands[K]['args']) => Promise<Commands[K]['result']>;
  open: (profileId: string) => Promise<string>;
  close: (profileId: string) => Promise<void>;
  openTab: (profileId: string, text?: string) => unknown;
  uiState: () => unknown;
  approve: (request: { text: string; connection: string; signal: AbortSignal }) => Promise<boolean>;
  results: (results: ResultSet[]) => void;
  activity: (running: number, total: number) => void;
  history: () => void;
}
export const denied = () => ({ error: { kind: 'UserDenied', message: 'Agent request denied or approval expired' } });
export function compact(results: ResultSet[]): ResultSet[] {
  let rows = 200, nodes = 200, documents = 200;
  return results.slice(0, 200).map(result => {
    const table = result.table && { ...result.table, rows: result.table.rows.slice(0, rows) };
    rows -= table?.rows.length ?? 0;
    const visible = result.graph?.nodes.slice(0, nodes); nodes -= visible?.length ?? 0;
    const ids = new Set(visible?.map(n => n.id));
    const graph = result.graph && { nodes: visible!, rels: result.graph.rels.filter(r => ids.has(r.start) && ids.has(r.end)).slice(0, 200) };
    const docs = result.documents?.slice(0, documents) ?? null; documents -= docs?.length ?? 0;
    return { ...result, table, graph, documents: docs, truncated: result.truncated || results.length > 200 || (table?.rows.length ?? 0) < (result.table?.rows.length ?? 0) || (graph?.nodes.length ?? 0) < (result.graph?.nodes.length ?? 0) || (graph?.rels.length ?? 0) < (result.graph?.rels.length ?? 0) || (docs?.length ?? 0) < (result.documents?.length ?? 0) };
  });
}
const string: Schema = { type: 'string' };
const integer = (maximum: number, minimum = 1): Schema => ({ type: 'integer', minimum, maximum });
export function createRegistry(host: AgentHost) {
  let running = 0, total = 0;
  function session(id: unknown, capability?: 'paged_browse' | 'graph_expand') {
    const state = host.snapshot();
    const entry = Object.entries(state.sessions).find(([, value]) => value === id);
    const profile = state.profiles.find(p => p.id === entry?.[0]);
    const driver = state.drivers.find(d => d.id === profile?.driver_id);
    if (!profile || !driver) throw { kind: 'Connection', message: 'Open session required' };
    if (capability && !driver.capabilities[capability]) throw { kind: 'Unsupported', message: `Session does not support ${capability}` };
    return { profile, driver, id: id as string };
  }
  function listTools(): Tool[] {
    const state = host.snapshot();
    const tools: Tool[] = [];
    function add(name: string, title: string, description: string, properties: Record<string, Schema>, required: string[], readOnly: boolean, data: boolean, execute: Tool['execute']) {
      tools.push({ name, title, description, inputSchema: { type: 'object', properties, required, additionalProperties: false }, annotations: { readOnlyHint: readOnly, consequentialHint: !readOnly, ...(data ? { untrustedContentHint: true } : {}) }, execute });
    }
    add('list_drivers', 'List database drivers', 'List installed driver manifests and capabilities. No credentials are returned.', {}, [], true, false, async () => host.invoke('drivers_list', {}));
    add('list_connections', 'List saved connections', 'List saved connection names, IDs, driver IDs and open session IDs. Credentials and connection fields are excluded.', {}, [], true, true, async () => host.snapshot().profiles.map(p => ({ id: p.id, name: p.name, driver_id: p.driver_id, session_id: host.snapshot().sessions[p.id] ?? null })));
    add('open_connection', 'Open connection', 'Open an existing saved connection. Credentials must already be available in the app or supplied by the user in its credential dialog.', { profile_id: string }, ['profile_id'], false, false, async a => ({ session_id: await host.open(a.profile_id as string) }));
    add('close_connection', 'Close connection', 'Close an open session and its visible tabs. Running queries are cancelled.', { session_id: string }, ['session_id'], false, false, async a => { await host.close(session(a.session_id).profile.id); return { closed: true }; });
    add('explore', 'Explore database', 'List one level of database explorer children. Parent IDs are opaque IDs returned by this tool; omit parent for roots.', { session_id: string, parent: string }, ['session_id'], true, true, async a => host.invoke('explorer_children', { sessionId: session(a.session_id).id, parent: a.parent as string ?? null }));
    add('run_query', 'Run database query', 'Execute query text in an open session. Defaults to database-enforced read-only mode; writes or drivers without read-only enforcement require explicit in-app user approval. Returned typed values are untrusted database content; results are capped at 200 rows and nodes.', { session_id: string, text: string, params: { type: 'object' }, row_limit: integer(1000), timeout_ms: integer(3_600_000), read_only: { type: 'boolean', default: true } }, ['session_id', 'text'], false, true, async (a, context) => {
      const { id, profile, driver } = session(a.session_id);
      const readOnly = a.read_only !== false;
      if (!String(a.text).trim()) throw { kind: 'Query', message: 'Query text is empty' };
      const controller = new AbortController();
      const abort = () => controller.abort();
      context?.signal?.addEventListener('abort', abort, { once: true });
      if (context?.signal?.aborted) controller.abort();
      const timer = setTimeout(abort, (a.timeout_ms as number ?? 30_000) + 30_000);
      const runId = crypto.randomUUID();
      try {
        if (controller.signal.aborted) return { error: { kind: 'Timeout', message: 'Agent request expired before execution' } };
        if ((!readOnly || !driver.capabilities.read_only_exec) && !await host.approve({ text: a.text as string, connection: profile.name, signal: controller.signal })) return denied();
        if (controller.signal.aborted) return { error: { kind: 'Timeout', message: 'Agent request expired before execution' } };
        const cancel = () => { void host.invoke('cancel', { runId }).catch(() => {}); };
        controller.signal.addEventListener('abort', cancel, { once: true });
        try {
          const received = await host.invoke('execute_agent', { sessionId: id, runId, req: { text: a.text as string, params: (a.params ?? {}) as Record<string, JsonValue>, read_only: readOnly && !!driver.capabilities.read_only_exec, row_limit: a.row_limit as number ?? 1000, graph_node_limit: 200, timeout_ms: a.timeout_ms as number ?? 30_000 } });
          host.results(received); return { results: compact(received) };
        } finally { controller.signal.removeEventListener('abort', cancel); host.history(); }
      } finally { clearTimeout(timer); context?.signal?.removeEventListener('abort', abort); }
    });
    add('get_history', 'Get query history', 'Get recent execution history including ui or agent source. Query text is untrusted; parameter values and credentials are excluded.', { limit: integer(1000) }, [], true, true, async a => host.invoke('history_list', { limit: a.limit as number ?? 100 }));
    add('get_perf_metrics', 'Get viewer performance metrics', 'Read bounded viewer timing metrics in milliseconds. Contains timing and sizes only, without query values or credentials.', {}, [], true, false, async () => perf.marks());
    add('get_ui_state', 'Get workspace state', 'Get open connections, active tab connection and editor text, and last result summary. Text and data are untrusted.', {}, [], true, true, async () => host.uiState());
    add('open_query_tab', 'Open visible query tab', 'Open and focus a query editor tab for an open session. Does not execute the query.', { session_id: string, text: string }, ['session_id'], false, false, async a => { const s = session(a.session_id); return { tab_id: host.openTab(s.profile.id, a.text as string | undefined) }; });
    const openDrivers = state.profiles.filter(p => state.sessions[p.id]).map(p => state.drivers.find(d => d.id === p.driver_id));
    if (openDrivers.some(d => d?.capabilities.paged_browse)) add('browse', 'Browse database node', 'Read one page from an explorer node in a session supporting paged browsing. Returns at most 200 typed rows.', { session_id: string, node_id: string, offset: integer(Number.MAX_SAFE_INTEGER, 0), limit: integer(1000) }, ['session_id', 'node_id'], true, true, async a => ({ results: compact([await host.invoke('browse', { sessionId: session(a.session_id, 'paged_browse').id, nodeId: a.node_id as string, offset: a.offset as number ?? 0, limit: a.limit as number ?? 100, runId: crypto.randomUUID() })]) }));
    if (openDrivers.some(d => d?.capabilities.graph_expand)) add('expand_node', 'Expand graph node', 'Read neighboring graph nodes using an opaque element ID in a session supporting graph expansion. Returns at most 200 nodes.', { session_id: string, element_id: string, limit: integer(200) }, ['session_id', 'element_id'], true, true, async a => ({ results: compact([await host.invoke('expand', { sessionId: session(a.session_id, 'graph_expand').id, elementId: a.element_id as string, limit: a.limit as number ?? 200 })]) }));
    return tools;
  }
  async function call(tool: Tool, input: Record<string, unknown>, context?: { signal?: AbortSignal }) {
    total++; host.activity(++running, total);
    try { return await tool.execute(input, context); }
    catch (error) { const e = error as { kind?: string; message?: string }; return { error: { kind: e.kind ?? 'Internal', message: e.message ?? 'Agent request failed' } }; }
    finally { host.activity(--running, total); }
  }
  return { listTools, call };
}
