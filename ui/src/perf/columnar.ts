import type { Table, Value } from '../contract';
export interface PackedColumn { tag: Value['t'] | null; values: unknown[] }
export interface PackedTable { columns: Table['columns']; count: number; data: PackedColumn[] }
export function packTable(table: Table): PackedTable {
  return { columns: table.columns, count: table.rows.length, data: table.columns.map((_, c) => {
    const tag = table.rows[0]?.[c]?.t ?? null;
    const uniform = tag && table.rows.every(row => row[c]?.t === tag || row[c]?.t === 'Null');
    return uniform ? { tag, values: table.rows.map(row => row[c].t === 'Null' ? null : 'v' in row[c] ? row[c].v : null) } : { tag: null, values: table.rows.map(row => row[c]) };
  }) };
}
export function unpackTable(packed: PackedTable): Table {
  if (!Number.isInteger(packed.count) || packed.count < 0 || packed.count > 100000 || packed.data.length !== packed.columns.length || packed.data.some(c => c.values.length !== packed.count)) throw new Error('Invalid columnar table dimensions');
  return { columns: packed.columns, rows: Array.from({ length: packed.count }, (_, r) => packed.data.map(column => {
    const v = column.values[r];
    return (column.tag === null ? v : v === null || column.tag === 'Null' ? { t: 'Null' } : { t: column.tag, v }) as Value;
  })) };
}
