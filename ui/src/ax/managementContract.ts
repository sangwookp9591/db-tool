import type { ChangePreview, Column, EditRequest, EditResult, Value } from '../contract';
import type { Risk } from './PlanCard';
export interface ImportMapping { source: string; target: string; type_name: string }
export interface ImportPlan { format: string; columns: ImportMapping[]; sample: Value[][]; row_count: number; errors: {row: number; column?: string; message: string}[]; warnings: string[]; request: EditRequest }
export interface SnapshotObject { namespace: string; name: string; kind: string; definition: string; columns: {name: string; type_name: string; nullable: boolean; default: string | null}[] }
export interface Snapshot { version: number; dialect: string; objects: SnapshotObject[] }
export interface SnapshotDiff { dialect: string; changes: {action: string; before: SnapshotObject | null; after: SnapshotObject | null; risk: Risk}[] }
export interface Migration { id: string; name: string; text: string; applied: boolean; format: string; checksum: string }
export interface MigrationList { migrations: Migration[]; history_tables: string[]; warnings: string[] }
declare module '../contract' { interface EditRequest { create_table?: Column[] } interface Capabilities { import_rows?: boolean; schema_snapshot?: boolean; ddl_render?: boolean; roles?: boolean; backup?: boolean; lint?: boolean } }
declare module '../api' { interface Commands {
 import_target: {args: {sessionId: string; namespace?: string; name: string}; result: string};
 snapshot_encode: {args: {snapshot: Snapshot}; result: string};
 snapshot_decode: {args: {bytes: number[]}; result: Snapshot};
 import_plan: {args: {filename: string; bytes: number[]; nodeId: string; mapping?: ImportMapping[]; createTable?: boolean}; result: ImportPlan};
 import_preview: {args: {sessionId: string; runId: string; req: EditRequest}; result: ChangePreview};
 import_apply: {args: {sessionId: string; runId: string; req: EditRequest}; result: EditResult};
 schema_snapshot: {args: {sessionId: string; scope?: string | null}; result: Snapshot};
 schema_diff: {args: {before: Snapshot; after: Snapshot}; result: SnapshotDiff};
 ddl_render: {args: {sessionId: string; diff: SnapshotDiff}; result: string[]};
 migration_scan: {args: {sessionId: string; path: string}; result: MigrationList};
 migration_preview: {args: {sessionId: string; runId: string; text: string}; result: ChangePreview};
 migration_apply: {args: {sessionId: string; runId: string; text: string; confirmation?: string; migrationId?: string}; result: unknown};
} }
