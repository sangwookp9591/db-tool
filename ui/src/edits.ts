import type { EditRequest, EditTarget, RowEdit, Table, Value } from './contract';

export interface PendingEdits { updates: Record<number, Record<string, Value>>; deleted: number[]; added: { id: string; values: Value[] }[] }
export const emptyEdits = (): PendingEdits => ({ updates: {}, deleted: [], added: [] });
export function pendingCount(edits: PendingEdits): number { return Object.keys(edits.updates).length + edits.deleted.length + edits.added.length; }
export function buildEditRequest(target: EditTarget, table: Table, edits: PendingEdits): EditRequest {
  const keys = new Set(target.key_columns);
  const pairs = (row: Value[], names: string[]) => names.map(name => { const index = table.columns.findIndex(column => column.name === name); if (index < 0) throw new Error(`키 컬럼 ${name}이(가) 결과에 없습니다.`); return [name, row[index]] as [string, Value]; });
  const operations: RowEdit[] = [];
  for (const [indexText, values] of Object.entries(edits.updates)) {
    const index = Number(indexText), row = table.rows[index];
    if (!row) throw new Error(`편집한 ${index + 1}번째 행이 더 이상 결과에 없습니다.`);
    const set = Object.entries(values).filter(([name]) => !keys.has(name));
    if (set.length) operations.push({ op: 'Update', key: pairs(row, target.key_columns), set });
  }
  for (const index of edits.deleted) {
    const row = table.rows[index];
    if (!row) throw new Error(`삭제한 ${index + 1}번째 행이 더 이상 결과에 없습니다.`);
    operations.push({ op: 'Delete', key: pairs(row, target.key_columns) });
  }
  for (const added of edits.added) operations.push({ op: 'Insert', values: table.columns.map((column, index) => [column.name, added.values[index] ?? { t: 'Null' }] as [string, Value]) });
  return { node_id: target.node_id, edits: operations, timeout_ms: 30000, preview_sample_limit: 50 };
}
