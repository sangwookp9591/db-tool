import type { Commands } from './api';
import type { AgentClient, DriverManifest, Profile, ExplorerNode, ResultSet, Table, Value, Graph, DbError, HistoryEntry, ExecRequest, EditRequest } from './contract';
const fields: DriverManifest['connection_fields'] = [
  { key: 'host', label: 'Host', kind: 'Text', required: true, default: 'localhost', secret: false, placeholder: 'localhost' },
  { key: 'port', label: 'Port', kind: 'Number', required: true, default: '5432', secret: false, placeholder: '' },
  { key: 'user', label: '사용자', kind: 'Text', required: true, default: 'demo', secret: false, placeholder: '' },
  { key: 'password', label: '비밀번호', kind: 'Password', required: false, default: null, secret: true, placeholder: '' },
  { key: 'tls', label: 'TLS 모드', kind: {Select:{options:['disable','prefer','require','verify-full']}}, required:false, default:'prefer', secret:false, placeholder:null },
  { key: 'tls_ca_file', label:'TLS CA 파일', kind:'FilePath',required:false,default:null,secret:false,placeholder:null }
];
const capabilities: DriverManifest['capabilities'] = { schema_graph: true, preview_changes: 'DryRun', read_only_exec: true, cancel: 'Server', timeout: true, params: true, transactions: true, paged_browse: true, graph_expand: false, multi_statement: true, edit_rows: true };
export const drivers: DriverManifest[] = [
  { id: 'postgres', display_name: 'PostgreSQL · 데모', paradigm: 'Relational', query_language: { id: 'sql', display: 'SQL', editor_mode: 'sql', supports_params: true }, connection_fields: fields, capabilities, default_query: 'SELECT * FROM people;' },
  { id: 'neo4j', display_name: '그래프 · 데모', paradigm: 'Graph', query_language: { id: 'cypher', display: 'Cypher', editor_mode: 'cypher', supports_params: true }, connection_fields: fields.map(f => f.key === 'port' ? { ...f, default: '7687' } : f), capabilities: { ...capabilities, edit_rows: false, cancel: 'Abort', graph_expand: true, multi_statement: false }, default_query: 'MATCH (n)-[r]->(m) RETURN n, r, m LIMIT 100;' },
  { id: 'mongodb', display_name: 'MongoDB · 데모', paradigm: 'Document', query_language: { id: 'json', display: 'JSON', editor_mode: 'json', supports_params: false }, connection_fields: [{key:'uri',label:'URI',kind:'Text',required:true,default:'mongodb://127.0.0.1:57017',secret:false,placeholder:null},{key:'database',label:'데이터베이스',kind:'Text',required:true,default:'axdb_test',secret:false,placeholder:null},...fields.filter(f=>['user','password','tls','tls_ca_file'].includes(f.key)),{key:'auth_source',label:'인증 데이터베이스',kind:'Text',required:false,default:'admin',secret:false,placeholder:null}], capabilities: { ...capabilities, edit_rows: false, params: false, preview_changes: 'None' }, default_query: '{"find":"people","filter":{}}' }
];
let profiles: Profile[] = drivers.map((d, i) => ({ id: `demo-${i}`, name: ['Analytics sandbox', 'Graph playground', 'Document studio'][i], driver_id: d.id, fields: Object.fromEntries(d.connection_fields.filter(f => !f.secret).map(f => [f.key, f.default ?? ''])), save_secret: false }));
const sessions = new Map<string, Profile>();
const runs = new Map<string, () => void>();
const history: HistoryEntry[] = [];
let mockAgentClients: AgentClient[] = [{ id: 'local', name: '로컬 MCP', connections: Object.fromEntries(profiles.map(profile => [profile.id, 'read'])), allowed_tools: ['list_connections', 'run_query'], max_rows: 200, expires_at: null, revoked: false }];
const mockRowChanges = new Map<string, Record<string, Value>>();
const mockDeletedRows = new Set<string>();
const mockInsertedRows: Record<string, Value>[] = [];
const text = (v: string): Value => ({ t: 'Text', v });
const int = (v: string): Value => ({ t: 'Int', v });
function failure(kind: DbError['kind'], message: string, detail: string | null = null, position: number | null = null): DbError { return { kind, message, detail, position }; }
function session(id: string): Profile { const profile = sessions.get(id); if (!profile) throw failure('Connection', '세션이 닫혔습니다. 다시 연결하세요.'); return profile; }
function base(title: string): ResultSet { return { title, table: null, graph: null, documents: null, message: null, affected: null, row_count: 0, truncated: false, page: null, elapsed_ms: 12 }; }
function tableRows(count: number, offset = 0): Table {
  const columns = [{ name: 'id', type_name: 'int8' }, { name: 'name', type_name: 'text' }, { name: 'balance', type_name: 'numeric' }, { name: 'metadata', type_name: 'jsonb' }, { name: 'created_at', type_name: 'timestamptz' }, { name: 'optional', type_name: 'text' }];
  const rows: Value[][] = [];
  let source = offset;
  while (rows.length < count && source < 100000) {
    const id = String(9007199254740993n + BigInt(source));
    if (!mockDeletedRows.has(id)) {
      const values: Value[] = [int(id), text(`Person ${source + 1}`), { t: 'Decimal', v: '12345678901234567890.123400' }, { t: 'Json', v: `{"sequence":${id},"active":true,"tags":["demo","exact"]}` }, { t: 'DateTimeTz', v: '2026-10-15T09:30:00+09:00' }, { t: 'Null' }];
      for (const [name, value] of Object.entries(mockRowChanges.get(id) ?? {})) { const index = columns.findIndex(column => column.name === name); if (index >= 0) values[index] = value; }
      rows.push(values);
    }
    source++;
  }
  for (const inserted of mockInsertedRows.slice(offset, offset + count)) rows.push(columns.map(column => inserted[column.name] ?? { t: 'Null' }));
  return { columns, rows };
}
function graphFor(seed = ''): Graph {
  const ids = seed ? [seed, `${seed}-a`, `${seed}-b`] : ['alice', 'bob', 'project'];
  return { nodes: ids.map((id, i) => ({ id, labels: [i === 2 ? 'Project' : 'Person'], props: [['name', text(seed ? `Neighbor ${id}` : ['Alice', 'Bob', 'Database tool'][i])], ['score', int(String(i + 1))]] })), rels: ids.slice(1).map((id, i) => ({ id: `${ids[0]}-${id}`, rel_type: i ? 'WORKS_ON' : 'KNOWS', start: ids[0], end: id, props: [['since', int('2024')]] })) };
}
// Mock-only: `LIMIT n` (n > 3) returns an n-node graph so large-graph rendering can be checked in the browser.
function syntheticGraph(count: number): Graph {
  const kinds = ['Person', 'Project', 'Team'];
  const nodes = Array.from({ length: count }, (_, i) => ({ id: `n${i}`, labels: [kinds[i % kinds.length]], props: [['name', text(`${kinds[i % kinds.length]} ${i}`)]] as [string, Value][] }));
  const rels = nodes.slice(1).map((n, i) => ({ id: `r${i}`, rel_type: ['KNOWS', 'WORKS_ON', 'MEMBER_OF'][i % 3], start: nodes[Math.floor(i / 2)].id, end: n.id, props: [] as [string, Value][] }));
  return { nodes, rels };
}
async function execute(sessionId: string, runId: string, req: ExecRequest, source: "ui" | "agent" = "ui"): Promise<ResultSet[]> {
  const profile = session(sessionId);
  const start = performance.now();
  let result: ResultSet[] = [];
  let error: DbError | null = null;
  try {
    if (req.read_only && /^\s*(update|create|delete|insert)\b/i.test(req.text)) throw failure('Query', 'Read-only transaction rejects writes.');
    if (/^\s*SELECT\s+(?:slow|(?:pg_)?sleep)\b/i.test(req.text)) await new Promise<void>((resolve, reject) => {
      const delay = req.timeout_ms > 0 ? Math.min(15000, req.timeout_ms) : 15000;
      const timer = setTimeout(() => req.timeout_ms > 0 && req.timeout_ms < 15000 ? reject(failure('Timeout', '실행 시간이 초과되었습니다.')) : resolve(), delay);
      runs.set(runId, () => { clearTimeout(timer); reject(failure('Cancelled', '실행이 취소되었습니다.')); });
    });
    if (/^\s*SELECT\s+disconnect\b/i.test(req.text)) { sessions.delete(sessionId); throw failure('Connection', '모의 연결이 끊겼습니다.'); }
    if (/^\s*SELECT\s+error\b/i.test(req.text)) throw failure('Query', 'Syntax error near "error"', '모의 오류입니다. 다른 쿼리로 계속하세요.', 8);
    if (drivers.find(d => d.id === profile.driver_id)?.paradigm === 'Document' || /^\s*SELECT\s+documents\b/i.test(req.text)) result = [{ ...base('문서'), documents: [{t:'Map',v:[['_id',{t:'Other',v:{type_name:'ObjectId',text:'507f1f77bcf86cd799439011'}}],['name',text('Avery Chen')],['team',text('Platform')],['active',{t:'Bool',v:true}],['profile',{t:'Map',v:[['location',text('Seoul')],['skills',{t:'List',v:[text('SQL'),text('TypeScript')]}]]}],['sequence',int('900719925474099312345')]]}], row_count: 1 }];
    else if (/^\s*(update|create|delete|insert)\b/i.test(req.text)) result = [{ ...base('문장 실행 완료'), message: '3건 영향', affected: 3 }];
    else if (drivers.find(d => d.id === profile.driver_id)?.capabilities.graph_expand) {
      const synthetic = Number(/LIMIT\s+(\d+)/i.exec(req.text)?.[1] ?? 0);
      const graph = synthetic > 3 ? syntheticGraph(synthetic) : graphFor();
      const visible = graph.nodes.slice(0, req.graph_node_limit);
      result = [{ ...base('그래프 결과'), graph: { nodes: visible, rels: graph.rels.filter(r => visible.some(n => n.id === r.start) && visible.some(n => n.id === r.end)) }, table: { columns: [{ name: 'n', type_name: 'Node' }], rows: visible.map(v => [{ t: 'Node', v }]) }, row_count: visible.length, truncated: visible.length < graph.nodes.length }];
    } else {
      const total = /^\s*SELECT\s+large\b/i.test(req.text) ? 100000 : 24;
      const count = Math.min(total, req.row_limit);
      result = [{ ...base(/^\s*SELECT\s+large\b/i.test(req.text) ? '100,000행 샘플' : 'People'), table: tableRows(count), row_count: count, truncated: total > count }];
      if (/;\s*select/i.test(req.text)) result.push({ ...base('두 번째 문장'), message: '두 번째 결과', affected: 0 });
    }
    return result;
  } catch (e) { error = e as DbError; throw e; }
  finally { runs.delete(runId); history.unshift({ source, timestamp: Date.now(), profile_name: profile.name, text: req.text, elapsed_ms: Math.round(performance.now() - start), row_count: result.reduce((sum, r) => sum + r.row_count, 0), error: error?.message ?? null }); }
}
export async function mockInvoke<K extends keyof Commands>(command: K, args: Commands[K]['args']): Promise<Commands[K]['result']> {
  const a = args as Record<string, unknown>;
  let result: unknown;
  switch (command) {
    case 'agent_clients_list': result = mockAgentClients.map(client => ({ ...client, connections: { ...client.connections }, allowed_tools: [...client.allowed_tools] })); break;
    case 'agent_audit_list': result = []; break;
    case 'agent_client_save': {
      const input = a.input as Omit<AgentClient, 'id' | 'revoked'> & { id?: string };
      const id = input.id ?? crypto.randomUUID();
      const client: AgentClient = { ...input, id, revoked: mockAgentClients.find(existing => existing.id === id)?.revoked ?? false, connections: { ...input.connections }, allowed_tools: [...input.allowed_tools] };
      mockAgentClients = [...mockAgentClients.filter(existing => existing.id !== id), client];
      result = { client, token: null }; break;
    }
    case 'agent_client_rotate': case 'agent_client_revoke': throw failure('Unsupported','클라이언트 자격 증명은 데스크톱 앱에서 관리합니다.');
    case 'agent_bridge_status': result = false; break;
    case 'agent_bridge_set': if (a.enabled) throw failure('Unsupported', 'MCP 브리지는 데스크톱 앱에서만 사용할 수 있습니다. (모의 모드)'); break;
    case 'agent_bridge_reply': break;
    case 'ai_settings': result = { settings: { enabled: false, model: 'claude-opus-5-5', effort: 'medium', auth: 'api_key' }, has_key: false, has_ant_cli: false }; break;
    case 'ai_configure': throw failure('Unsupported', 'AI 설정은 데스크톱 앱에서 저장할 수 있습니다. (모의 모드)');
    case 'drivers_list': result = drivers; break;
    case 'profiles_list': result = profiles; break;
    case 'profile_save': {
      const profile = a.profile as Profile;
      const driver = drivers.find(d => d.id === profile.driver_id)!;
      const clean = { ...profile, fields: Object.fromEntries(Object.entries(profile.fields).filter(([key]) => !driver.connection_fields.some(f => f.key === key && f.secret))) };
      profiles = [...profiles.filter(p => p.id !== clean.id), clean]; result = clean; break;
    }
    case 'profile_delete': profiles = profiles.filter(p => p.id !== a.profileId); break;
    case 'connection_test': if ((a.profile as Profile).fields.host === 'bad') throw failure('Connection', 'Cannot reach host'); result = '모의 서버 · 연결됨'; break;
    case 'session_info': result = drivers.find(driver => driver.id === session(a.sessionId as string).driver_id)?.display_name ?? '모의 서버'; break;
    case 'session_open': { const p = profiles.find(p => p.id === a.profileId); if (!p) throw failure('Connection', '프로필을 찾을 수 없습니다.'); const id = crypto.randomUUID(); sessions.set(id, p); result = id; break; }
    case 'session_close': sessions.delete(a.sessionId as string); break;
    case 'explorer_children': {
      const profile = session(a.sessionId as string);
      const graph = drivers.find(d => d.id === profile.driver_id)!.capabilities.graph_expand;
      const query = graph ? 'MATCH (n)-[r]->(m) RETURN n, r, m' : 'SELECT * FROM people';
      const parent = a.parent as string | null;
      // Test hook: widen the lazy-load window so the loading row can be inspected.
      const slow = (globalThis as { __mockExplorerDelay?: number }).__mockExplorerDelay; if (slow) await new Promise(resolve => setTimeout(resolve, slow));
      const table = (id: string, label: string, detail: string, has_children = false): ExplorerNode => ({ id, label, detail, kind: 'table', icon: 'Table', has_children, actions: [{ id: 'browse', label: '데이터 보기', kind: 'Browse' }, { id: 'query', label: '쿼리 열기', kind: { OpenQuery: { text: id === 'people' ? query : `SELECT * FROM ${label}` } } }, { id: 'insert', label: '이름 삽입', kind: { Insert: { text: graph ? ':Person' : label } } }] });
      const group = (id: string, label: string, detail: string | null): ExplorerNode => ({ id, label, detail, kind: 'folder', icon: 'Folder', has_children: true, actions: [] });
      const column = (owner: string, name: string, type: string): ExplorerNode => ({ id: `${owner}.${name}`, label: name, detail: type, kind: 'column', icon: 'Column', has_children: false, actions: [] });
      if (!parent) { result = [{ id: 'public', label: graph ? '레이블' : 'public', detail: null, kind: 'schema', icon: 'Folder', has_children: true, actions: [{ id: 'diagram', label: '다이어그램 보기', kind: { OpenSchemaGraph: { scope: null } } }] } satisfies ExplorerNode]; break; }
      if (graph) { result = [table('people', 'Person', '100,000건')]; break; }
      // Demo fixture: a schema with tables, a view group, an index group and a long archive group (450 tables) exercises depth and list limits.
      if (parent === 'public') result = [table('people', 'people', '100,000건', true), table('orders', 'orders', '48,210건', true), group('public.views', '뷰', '2'), group('public.indexes', '인덱스', '3'), group('public.archive', 'Archive', '450')];
      else if (parent === 'people') result = [column('people', 'id', 'int8'), column('people', 'name', 'text'), column('people', 'balance', 'numeric'), column('people', 'metadata', 'jsonb'), column('people', 'created_at', 'timestamptz')];
      else if (parent === 'orders') result = [column('orders', 'id', 'int8'), column('orders', 'person_id', 'int8'), column('orders', 'total', 'numeric'), column('orders', 'placed_at', 'timestamptz')];
      else if (parent === 'public.views') result = [{ ...table('active_people', 'active_people', 'view'), icon: 'View', kind: 'view' }, { ...table('order_totals', 'order_totals', 'view'), icon: 'View', kind: 'view' }];
      else if (parent === 'public.indexes') result = ['people_pkey', 'orders_pkey', 'orders_person_id_idx'].map(name => ({ id: `idx.${name}`, label: name, detail: 'btree', kind: 'index', icon: 'Index', has_children: false, actions: [] }) satisfies ExplorerNode);
      else if (parent === 'public.archive') result = Array.from({ length: 450 }, (_, i) => table(`archive_${i + 1}`, `archive_${String(i + 1).padStart(3, '0')}`, `${(i + 1) * 12}건`));
      else result = [];
      break;
    }
    case 'browse': { session(a.sessionId as string); const count = Math.max(0, Math.min(a.limit as number, 100000 - (a.offset as number))); result = { ...base('Browse people'), table: tableRows(count, a.offset as number), edit_target: { node_id: a.nodeId as string, key_columns: ['id'] }, row_count: count, page: { offset: a.offset, limit: a.limit, has_more: (a.offset as number) + count < 100000 } }; break; }
    case 'preview': {
      const profile = session(a.sessionId as string), req = a.req as ExecRequest;
      const sample = tableRows(Math.min(req.preview_sample_limit ?? 50, 3));
      result = { committed: false, notes: ['데모 미리보기: 예시 샘플일 뿐이며 데이터베이스에 접속하지 않았습니다.'], statements: [{ text: req.text, kind: 'Other', affected: null, before: sample, after: sample, schema_diff: null, counters: [], warnings: [] }] };
      history.unshift({ kind: 'preview', source: (a.source as 'ui' | 'agent') ?? 'ui', timestamp: Date.now(), profile_name: profile.name, text: req.text, elapsed_ms: 0, row_count: 0, error: null });
      break;
    }
    case 'edit_preview': {
      session(a.sessionId as string);
      const req = a.req as EditRequest;
      result = { committed: false, notes: ['모의 미리보기: 예시일 뿐이며 데이터베이스에 접속하지 않았습니다.'], statements: req.edits.map((edit, i) => ({ text: `MOCK ${edit.op} ${req.node_id} · 변경 ${i + 1}`, kind: edit.op === 'Insert' ? 'Insert' : edit.op, affected: 1, before: null, after: null, schema_diff: null, counters: [], warnings: ['모의 전용입니다. 실제 데이터베이스 미리보기가 아닙니다.'] })) };
      break;
    }
    case 'edit_apply': {
      session(a.sessionId as string);
      const req = a.req as EditRequest;
      for (const edit of req.edits) {
        if (edit.op === 'Insert') mockInsertedRows.push(Object.fromEntries(edit.values));
        else {
          const id = edit.key.find(([name]) => name === 'id')?.[1];
          const key = id?.t === 'Int' ? id.v : '';
          if (edit.op === 'Delete') mockDeletedRows.add(key);
          else mockRowChanges.set(key, { ...(mockRowChanges.get(key) ?? {}), ...Object.fromEntries(edit.set) });
        }
      }
      result = { affected: req.edits.map(() => 1), statements: req.edits.map(edit => `MOCK ${edit.op} ${req.node_id}`) };
      break;
    }
    case 'execute_agent': result = await execute(a.sessionId as string, a.runId as string, a.req as ExecRequest, 'agent'); break;
    case 'execute': result = await execute(a.sessionId as string, a.runId as string, a.req as ExecRequest, a.source as "ui" | "agent"); break;
    case 'cancel': runs.get(a.runId as string)?.(); break;
    case 'expand': { session(a.sessionId as string); const id = a.elementId as string;
      const graph: Graph = id.startsWith('bench:') ? { nodes: Array.from({ length: 50 }, (_, i) => ({ id: `bench:extra${i}`, labels: ['Neighbor'], props: [['name', text(`Neighbor ${i}`)]] })), rels: Array.from({ length: 50 }, (_, i) => ({ id: `bench:edge${i}`, start: id, end: `bench:extra${i}`, rel_type: 'NEIGHBOR', props: [] })) } : graphFor(id); const nodes = graph.nodes.slice(0, a.limit as number); result = { ...base('Neighbors'), graph: { nodes, rels: graph.rels.filter(r => nodes.some(n => n.id === r.end)) }, row_count: nodes.length }; break; }
    case 'schema_graph': { session(a.sessionId as string); result = { entities: [{id:'people',name:'people',kind:'table',fields:[{name:'id',type_name:'int8',pk:true,nullable:false},{name:'name',type_name:'text',pk:false,nullable:false},{name:'team_id',type_name:'int8',pk:false,nullable:true}]},{id:'teams',name:'teams',kind:'table',fields:[{name:'id',type_name:'int8',pk:true,nullable:false},{name:'name',type_name:'text',pk:false,nullable:false}]}],relations:[{id:'people_team',name:'people_team_id_fkey',from:'people',from_fields:['team_id'],to:'teams',to_fields:['id'],kind:'ForeignKey'}] }; break; }
    case 'history_delete': { const index = history.findIndex(entry => String(entry.timestamp) === a.timestamp && entry.profile_name === a.profileName && entry.text === a.text); if (index >= 0) history.splice(index, 1); break; }
    case 'history_list': result = history.slice(0, a.limit as number); break;
  }
  return result as Commands[K]['result'];
}

/** 모의 전용: 브라우저 모의 모드에서 에이전트 MCP 호출 이벤트를 흉내 낸다. 네이티브 경로에서는 쓰이지 않는다. */
export function simulateAgentCalls(count: number, emit: (call: import('./contract').AgentCall) => void) {
  const names = ['run_query', 'get_schema_context', 'explore', 'list_connections', 'browse', 'preview_changes'];
  for (let i = 0; i < count; i++) {
    const id = `mock-call-${Date.now()}-${i}`, base = { id, name: names[i % names.length], client_id: 'local', connection_id: profiles[i % profiles.length].id };
    setTimeout(() => emit({ ...base, status: 'running' }), i * 40);
    setTimeout(() => emit({ ...base, status: i % 7 === 6 ? 'error' : 'success', duration_ms: 120 + (i * 37) % 900, row_count: i % 2 ? 40 + i : null }), i * 40 + 300);
  }
}
