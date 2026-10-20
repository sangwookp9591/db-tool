import { isCompleteValue } from './values';
import type { EditRequest, EditTarget, RowEdit, Table, Value } from './contract';

export interface PendingEdits { updates: Record<number, Record<string, Value>>; deleted: number[]; added: { id: string; values: Value[] }[] }
export const emptyEdits = (): PendingEdits => ({ updates: {}, deleted: [], added: [] });
export function pendingCount(edits: PendingEdits): number { return Object.keys(edits.updates).length + edits.deleted.length + edits.added.length; }
export function buildEditRequest(target: EditTarget, table: Table, edits: PendingEdits): EditRequest {
  const keys = new Set(target.key_columns);
  const origins = table.columns.map((column,index) => target.columns?.length ? target.columns[index] : column.name);
  const original = (row: Value[]): [string,Value][] => origins.flatMap((name,index) => name && isCompleteValue(row[index]) ? [[name,row[index]] as [string,Value]] : []);
  const pairs = (row: Value[], names: string[]) => names.map(name => { const index = origins.findIndex(origin => origin === name); if (index < 0) throw new Error(`키 컬럼 ${name}이(가) 결과에 없습니다.`); return [name, row[index]] as [string, Value]; });
  const operations: RowEdit[] = [];
  const originals: [string,Value][][] = [];
  for (const [indexText, values] of Object.entries(edits.updates)) {
    const index = Number(indexText), row = table.rows[index];
    if (!row) throw new Error(`편집한 ${index + 1}번째 행이 더 이상 결과에 없습니다.`);
    const set: [string,Value][] = Object.entries(values).flatMap(([name,value]) => { const index=table.columns.findIndex(column=>column.name===name); const origin=origins[index]; return origin && !keys.has(origin) ? [[origin,value] as [string,Value]] : []; });
    if (set.length) { operations.push({ op: 'Update', key: pairs(row, target.key_columns), set }); originals.push(original(row)); }
  }
  for (const index of edits.deleted) {
    const row = table.rows[index];
    if (!row) throw new Error(`삭제한 ${index + 1}번째 행이 더 이상 결과에 없습니다.`);
    operations.push({ op: 'Delete', key: pairs(row, target.key_columns) }); originals.push(original(row));
  }
  for (const added of edits.added) { operations.push({ op: 'Insert', values: table.columns.map((column, index) => [column.name, added.values[index] ?? { t: 'Null' }] as [string, Value]) }); originals.push([]); }
  return { node_id: target.node_id, edits: operations, originals, timeout_ms: 30000, preview_sample_limit: 50 };
}
