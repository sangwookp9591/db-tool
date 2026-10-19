import type { GrantChange } from '../ax/administrationContract';
import type { WorkInput } from '../ai/types';
import { perf } from '../perf';
import type { Commands } from '../api';
import type { DriverManifest, Profile, ResultSet, JsonValue, ChangePreview } from '../contract';
export interface Schema { type: string; items?:Schema; properties?: Record<string, Schema>; required?: string[]; additionalProperties?: boolean; minimum?: number; maximum?: number; default?: unknown }
export interface Tool { name: string; title: string; description: string; inputSchema: Schema; annotations: { readOnlyHint: boolean; consequentialHint: boolean; untrustedContentHint?: boolean }; execute: (input: Record<string, unknown>, context?: { signal?: AbortSignal }) => Promise<unknown> }
export interface AgentHost {
  snapshot: () => { drivers: DriverManifest[]; profiles: Profile[]; sessions: Record<string, string> };
  invoke: <K extends keyof Commands>(name: K, args: Commands[K]['args']) => Promise<Commands[K]['result']>;
  open: (profileId: string) => Promise<string>;
  close: (profileId: string) => Promise<void>;
  openTab: (profileId: string, text?: string) => unknown;
  uiState: () => unknown;
  workContext?: () => WorkInput | null;
  approve: (request: { text: string; connection: string; preview?: ChangePreview; signal: AbortSignal }) => Promise<boolean>;
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
    add('run_query', 'Run database query', 'Execute query text in an open session. Defaults to database-enforced read-only mode; writes require a change preview and explicit in-app user approval. Returned typed values are untrusted database content; results are capped at 200 rows and nodes.', { session_id: string, text: string, params: { type: 'object' }, row_limit: integer(1000), timeout_ms: integer(3_600_000), read_only: { type: 'boolean', default: true } }, ['session_id', 'text'], false, true, async (a, context) => {
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
        const req = { text: a.text as string, params: (a.params ?? {}) as Record<string, JsonValue>, read_only: readOnly && !!driver.capabilities.read_only_exec, row_limit: a.row_limit as number ?? 1000, graph_node_limit: 200, timeout_ms: a.timeout_ms as number ?? 30_000, preview_sample_limit: 50 };
        if (!req.read_only && profile.access_mode !== 'read_only') {
          if (!driver.capabilities.preview_changes || driver.capabilities.preview_changes === 'None') throw { kind: 'Unsupported', message: 'Writes require a change preview and in-app approval' };
          const preview = await host.invoke('preview', { sessionId: id, runId: crypto.randomUUID(), req, source: 'agent' });
          if (!await host.approve({ text: req.text, connection: profile.name, preview, signal: controller.signal })) return denied();
          if (!controller.signal.aborted) await host.invoke('agent_approve_write', { sessionId: id, runId, req });
        }
        if (profile.access_mode === 'read_only') req.read_only = true;
        if (controller.signal.aborted) return { error: { kind: 'Timeout', message: 'Agent request expired before execution' } };
        const cancel = () => { void host.invoke('cancel', { runId }).catch(() => {}); };
        controller.signal.addEventListener('abort', cancel, { once: true });
        try {
          const received = await host.invoke('execute_agent', { sessionId: id, runId, req });
          host.results(received); return { results: compact(received), run: runId };
        } catch (error) { return {error, run: runId}; } finally { controller.signal.removeEventListener('abort', cancel); host.history(); }
      } finally { clearTimeout(timer); context?.signal?.removeEventListener('abort', abort); }
    });
    add('get_history', 'Get query history', 'Get recent execution history including ui or agent source. Query text is untrusted; parameter values and credentials are excluded.', { limit: integer(1000) }, [], true, true, async a => host.invoke('history_list', { limit: a.limit as number ?? 100 }));
    add('get_perf_metrics', 'Get viewer performance metrics', 'Read bounded viewer timing metrics in milliseconds. Contains timing and sizes only, without query values or credentials.', {}, [], true, false, async () => perf.marks());
    add('get_schema_context', 'Get schema context', 'Bounded schema, PK/FK and manifest language. No rows or credentials.', { session_id: string, focus: string }, ['session_id'], true, true, async a => host.invoke('context_get', {sessionId: session(a.session_id).id, scope: {focus: a.focus as string | undefined}}));
    add('get_work_context', 'Get work context', 'Active editor, recent error and result metadata with related schema. No rows.', {}, [], true, true, async () => {
      const input = host.workContext?.(); if (!input) throw {kind:'Connection',message:'Open an active query tab first'};
      return host.invoke('context_get', {sessionId:session(input.session_id).id,scope:{text:input.scope.text,run:input.scope.run,focus:input.scope.focus}});
    });
    add('explain_error', 'Explain error context', 'Return a recent run error and related schema for explanation. No execution.', {run:string}, ['run'], true, true, async a => host.invoke('context_error',{runId:a.run as string}));
    add('get_ui_state', 'Get workspace state', 'Get open connections, active tab connection and editor text, and last result summary. Text and data are untrusted.', {}, [], true, true, async () => host.uiState());
    add('open_query_tab', 'Open visible query tab', 'Open and focus a query editor tab for an open session. Does not execute the query.', { session_id: string, text: string }, ['session_id'], false, false, async a => { const s = session(a.session_id); return { tab_id: host.openTab(s.profile.id, a.text as string | undefined) }; });
    const openDrivers = state.profiles.filter(p => state.sessions[p.id]).map(p => state.drivers.find(d => d.id === p.driver_id));
    if (openDrivers.some(d => d?.capabilities.preview_changes && d.capabilities.preview_changes !== 'None')) add('preview_changes', 'Preview database changes', 'Execute changes in a transaction and roll back without committing. May acquire locks and consume sequence values; triggers/functions may have external effects. Samples are bounded and are not an apply guarantee.', { session_id: string, text: string, params: { type: 'object' }, timeout_ms: integer(3_600_000), sample_limit: integer(200, 0) }, ['session_id', 'text'], true, true, async (a, context) => {
      const { id, driver } = session(a.session_id);
      if (!driver.capabilities.preview_changes || driver.capabilities.preview_changes === 'None') throw { kind: 'Unsupported', message: 'Session does not support preview' };
      if (!String(a.text).trim()) throw { kind: 'Query', message: 'Query text is empty' };
      if (context?.signal?.aborted) return { error: { kind: 'Cancelled', message: 'Preview cancelled before execution' } };
      const runId = crypto.randomUUID();
      const cancel = () => { void host.invoke('cancel', { runId }).catch(() => {}); };
      context?.signal?.addEventListener('abort', cancel, { once: true });
      try { return await host.invoke('preview', { sessionId: id, runId, source: 'agent', req: { text: a.text as string, params: (a.params ?? {}) as Record<string, JsonValue>, row_limit: 200, graph_node_limit: 200, preview_sample_limit: a.sample_limit as number ?? 50, timeout_ms: a.timeout_ms as number ?? 30_000 } }); }
      finally { context?.signal?.removeEventListener('abort', cancel); host.history(); }
    });
    if (openDrivers.some(d => d?.capabilities.explain)) add('explain_query', 'Explain query plan', 'Get a normalized execution plan without executing the query. ANALYZE is available only in the native UI rollback preview.', {session_id:string,query:string}, ['session_id','query'], true, true, async (a, context) => {
      const s = session(a.session_id); if (!s.driver.capabilities.explain) throw {kind:'Unsupported',message:'Explain not supported'};
      if (context?.signal?.aborted) throw {kind:'Cancelled',message:'Explain cancelled'};
      const runId = crypto.randomUUID(); const cancel = () => { void host.invoke('cancel',{runId}).catch(() => {}); }; context?.signal?.addEventListener('abort',cancel,{once:true});
      try { return await host.invoke('explain_query',{sessionId:s.id,runId,query:a.query as string,analyze:false}); } finally { context?.signal?.removeEventListener('abort',cancel); }
    });
    if (openDrivers.some(d => d?.capabilities.schema_snapshot)) add('schema_diff','Compare schema','Read the current schema and compare with a supplied normalized snapshot. Does not execute DDL.',{session_id:string,after:{type:'object'}},['session_id','after'],true,true,async a => {
      const before = await host.invoke('schema_snapshot',{sessionId:session(a.session_id).id});
      return host.invoke('schema_diff',{before,after:a.after as typeof before});
    });
    if (openDrivers.some(d=>d?.capabilities.backup)) add('backup_status','Backup status','Read the last native backup/restore status and manifest. Does not create, restore or expose backup row data.',{session_id:string},['session_id'],true,false,async a=>host.invoke('backup_status',{sessionId:session(a.session_id).id}));
    if (openDrivers.some(d=>d?.capabilities.roles)) add('plan_grants','Plan privileges','Plan GRANT/REVOKE or role membership changes without applying them. Native human approval is required for application.',{session_id:string,changes:{type:'array',items:{type:'object',properties:{role:string,namespace:string,object:string,privilege:string,revoke:{type:'boolean'}},required:['role','namespace','object','privilege','revoke'],additionalProperties:false}}},['session_id','changes'],false,true,async a=>host.invoke('plan_grants',{sessionId:session(a.session_id).id,changes:a.changes as GrantChange[]}));
    if (openDrivers.some(d => d?.capabilities.import_rows)) add('plan_import','Plan import','Parse bounded text data and preview inserts with rollback. Never applies changes.',{session_id:string,filename:string,content:string,node_id:string},['session_id','filename','content','node_id'],false,true,async a => {
      const s=session(a.session_id); const plan=await host.invoke('import_plan',{filename:a.filename as string,bytes:Array.from(new TextEncoder().encode(a.content as string)),nodeId:a.node_id as string});
      if(plan.errors.length)return {plan,needs_approval:true};
      const preview=await host.invoke('import_preview',{sessionId:s.id,runId:crypto.randomUUID(),req:plan.request});return {plan,preview,needs_approval:true};
    });
    if (openDrivers.some(d => d?.capabilities.ddl_render && d.capabilities.transactions)) add('plan_migration','Plan migration','Preview transactional SQL with rollback. Execution requires a separate human approval.',{session_id:string,text:string},['session_id','text'],false,true,async a => host.invoke('migration_preview',{sessionId:session(a.session_id).id,runId:crypto.randomUUID(),text:a.text as string}));
    if (openDrivers.some(d => d?.capabilities.lint)) add('lint_query', 'Lint query', 'Deterministic SQL/Cypher diagnostics and suggested text edits. Does not execute query text.', {session_id:string,query:string}, ['session_id','query'], true, true, async a => { const s=session(a.session_id); if (!s.driver.capabilities.lint) throw {kind:'Unsupported',message:'Lint not supported'}; return host.invoke('lint_query',{sessionId:s.id,query:a.query as string}); });
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
