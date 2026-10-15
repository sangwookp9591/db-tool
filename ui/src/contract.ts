export type FieldKind = 'Text' | 'Number' | 'Password' | 'FilePath' | 'Bool' | { Select: { options: string[] } };
export interface FieldSpec { key: string; label: string; kind: FieldKind; required: boolean; default: string | null; secret: boolean; placeholder: string | null }
export interface Capabilities { cancel: 'None' | 'Abort' | 'Server'; timeout: boolean; params: boolean; transactions: boolean; paged_browse: boolean; graph_expand: boolean; multi_statement: boolean; edit_rows: boolean }
export interface QueryLanguage { id: string; display: string; editor_mode: string; supports_params: boolean }
export interface DriverManifest { id: string; display_name: string; paradigm: 'Relational' | 'Graph' | 'Document' | 'KeyValue'; query_language: QueryLanguage; connection_fields: FieldSpec[]; capabilities: Capabilities; default_query: string }
export interface ConnectConfig { fields: Record<string, string>; secret: string | null; connect_timeout_ms: number }
export type Icon = 'Folder' | 'Table' | 'Column' | 'Key' | 'Index' | 'Label' | 'Relationship' | 'Property' | 'Collection' | 'Database' | 'View' | 'Function';
export type ActionKind = 'Browse' | { OpenQuery: { text: string } } | { Insert: { text: string } };
export interface NodeAction { id: string; label: string; kind: ActionKind }
export interface ExplorerNode { id: string; label: string; detail: string | null; kind: string; icon: Icon; has_children: boolean; actions: NodeAction[] }
export interface PageRequest { offset: number; limit: number }
export interface PageInfo extends PageRequest { has_more: boolean }
export interface ExecRequest { text: string; params: Record<string, JsonValue>; row_limit: number; graph_node_limit: number; timeout_ms: number }
export interface Column { name: string; type_name: string }
export interface Table { columns: Column[]; rows: Value[][] }
export interface GNode { id: string; labels: string[]; props: [string, Value][] }
export interface GRel { id: string; rel_type: string; start: string; end: string; props: [string, Value][] }
export interface Graph { nodes: GNode[]; rels: GRel[] }
export interface ResultSet { title: string; table: Table | null; graph: Graph | null; documents: Value[] | null; message: string | null; affected: number | null; row_count: number; truncated: boolean; page: PageInfo | null; elapsed_ms: number }
export type Value = { t: 'Null' } | { t: 'Bool'; v: boolean } | { t: 'Int' | 'Decimal' | 'Text' | 'Date' | 'Time' | 'DateTime' | 'DateTimeTz' | 'Duration' | 'Uuid' | 'Json'; v: string } | { t: 'Float'; v: number | 'NaN' | 'Infinity' | '-Infinity' } | { t: 'Bytes'; v: { len: number; hex: string } } | { t: 'List'; v: Value[] } | { t: 'Map'; v: [string, Value][] } | { t: 'Node'; v: GNode } | { t: 'Rel'; v: GRel } | { t: 'Path'; v: Graph } | { t: 'Point'; v: { srid: number; x: number; y: number; z: number | null } } | { t: 'Other'; v: { type_name: string; text: string } };
export type ErrorKind = 'Connection' | 'Auth' | 'Query' | 'Timeout' | 'Cancelled' | 'Unsupported' | 'Internal';
export interface DbError { kind: ErrorKind; message: string; detail: string | null; position: number | null }
// Host persistence types are kept separate from the driver contract.
export interface Profile { id: string; name: string; driver_id: string; fields: Record<string, string>; save_secret: boolean }
export interface HistoryEntry { timestamp: number; profile_name: string; text: string; elapsed_ms: number; row_count: number; error: string | null }

export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
