export type FieldKind = 'Text' | 'Number' | 'Password' | 'FilePath' | 'Bool' | { Select: { options: string[] } };
export interface FieldSpec { key: string; label: string; kind: FieldKind; required: boolean; default: string | null; secret: boolean; placeholder: string | null }
export type PreviewSupport = 'None' | 'Transactional' | 'DryRun';
export interface Capabilities { explain?: boolean; lint?: boolean; schema_graph?: boolean; filtered_browse?: boolean; preview_changes?: PreviewSupport; read_only_exec?: boolean; cancel: 'None' | 'Abort' | 'Server'; timeout: boolean; params: boolean; transactions: boolean; paged_browse: boolean; graph_expand: boolean; multi_statement: boolean; edit_rows: boolean }
export interface QueryLanguage { id: string; display: string; editor_mode: string; supports_params: boolean }
export interface DriverManifest { id: string; icon_key?: string; display_name: string; paradigm: 'Relational' | 'Graph' | 'Document' | 'KeyValue'; query_language: QueryLanguage; connection_fields: FieldSpec[]; capabilities: Capabilities; default_query: string }
export interface ConnectConfig { fields: Record<string, string>; secret: string | null; connect_timeout_ms: number }
export type Icon = 'Folder' | 'Table' | 'Column' | 'Key' | 'Index' | 'Label' | 'Relationship' | 'Property' | 'Collection' | 'Database' | 'View' | 'Function';
export type ActionKind = { OpenSchemaGraph: { scope: string | null } } | 'Browse' | { OpenQuery: { text: string } } | { Insert: { text: string } };
export interface NodeAction { id: string; label: string; kind: ActionKind }
export interface ExplorerNode { id: string; label: string; detail: string | null; kind: string; icon: Icon; has_children: boolean; actions: NodeAction[] }
export interface PageRequest { filter?: [string, Value][]; offset: number; limit: number }
export interface PageInfo extends PageRequest { has_more: boolean }
export interface ExecRequest { preview_sample_limit?: number; read_only?: boolean; text: string; params: Record<string, JsonValue>; row_limit: number; graph_node_limit: number; timeout_ms: number }
export interface Column { name: string; type_name: string }
export interface Table { columns: Column[]; rows: Value[][] }
export interface GNode { id: string; labels: string[]; props: [string, Value][] }
export interface GRel { id: string; rel_type: string; start: string; end: string; props: [string, Value][] }
export interface Graph { nodes: GNode[]; rels: GRel[] }
export interface ResultSet { title: string; table: Table | null; graph: Graph | null; documents: Value[] | null; message: string | null; affected: number | null; row_count: number; truncated: boolean; page: PageInfo | null; elapsed_ms: number; edit_target?: { node_id: string; key_columns: string[] } | null }
export type Value = { t: 'Null' } | { t: 'Bool'; v: boolean } | { t: 'Int' | 'Decimal' | 'Text' | 'Date' | 'Time' | 'DateTime' | 'DateTimeTz' | 'Duration' | 'Uuid' | 'Json'; v: string } | { t: 'Float'; v: number | 'NaN' | 'Infinity' | '-Infinity' } | { t: 'Bytes'; v: { len: number; hex: string } } | { t: 'List'; v: Value[] } | { t: 'Map'; v: [string, Value][] } | { t: 'Node'; v: GNode } | { t: 'Rel'; v: GRel } | { t: 'Path'; v: Graph } | { t: 'Point'; v: { srid: number; x: number; y: number; z: number | null } } | { t: 'Other'; v: { type_name: string; text: string } };
export type ErrorKind = 'Connection' | 'Auth' | 'Query' | 'Timeout' | 'Cancelled' | 'Unsupported' | 'Conflict' | 'Internal';
export interface DbError { kind: ErrorKind; message: string; detail: string | null; position: number | null }
// Host persistence types are kept separate from the driver contract.
export interface Profile { network?: import('./ax/connectionContract').ConnectionNetwork | null; auth?: import('./ax/connectionContract').ConnectionAuth; id: string; name: string; driver_id: string; fields: Record<string, string>; save_secret: boolean; access_mode?: 'read_only' | 'read_write' }
export interface HistoryEntry { kind?: 'execute' | 'preview'; source?: "ui" | "agent"; timestamp: string | number; profile_id?: string | null; profile_name: string; text: string; elapsed_ms: number; row_count: number; error: string | null }

export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };

export interface SchemaDiff { added: string[]; removed: string[]; changed: string[] }
export interface StatementPreview { text: string; kind: 'Insert' | 'Update' | 'Delete' | 'Ddl' | 'GraphWrite' | 'Other'; affected: number | null; before: Table | null; after: Table | null; schema_diff: SchemaDiff | null; counters: [string, number][]; warnings: string[] }
export interface ChangePreview { statements: StatementPreview[]; committed: false; notes: string[] }
export type RowEdit = { op: 'Update'; key: [string, Value][]; set: [string, Value][] } | { op: 'Insert'; values: [string, Value][] } | { op: 'Delete'; key: [string, Value][] };
export interface EditRequest { node_id: string; edits: RowEdit[]; timeout_ms?: number; preview_sample_limit?: number }
export interface EditResult { affected: number[]; statements: string[] }
export interface EditTarget { node_id: string; key_columns: string[] }

export interface SchemaField { name: string; type_name: string; pk: boolean; nullable: boolean }
export interface SchemaEntity { id: string; name: string; kind: string; fields: SchemaField[] }
export interface SchemaRelation { id: string; name: string; from: string; from_fields: string[]; to: string; to_fields: string[]; kind: string }
export interface SchemaGraph { entities: SchemaEntity[]; relations: SchemaRelation[] }
export interface BrowseTarget { nodeId: string; title: string; filter?: [string, Value][] }

export type AgentPermission = 'none' | 'read' | 'preview' | 'write_with_approval';
export interface AgentClient { id: string; name: string; connections: Record<string, AgentPermission>; allowed_tools: string[]; max_rows: number; expires_at: string | null; revoked: boolean }
export interface AgentAudit { timestamp: string; client_id: string; tool: string; connection_id: string | null; decision: string; statement_hash: string | null }

export interface AgentCall { id: string; name: string; client_id: string; connection_id: string | null; status: 'running' | 'success' | 'error'; decision?: 'allowed' | 'denied' | 'approved' | null; duration_ms?: number; row_count?: number | null; startedAt?: number }
