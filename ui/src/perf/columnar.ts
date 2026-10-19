import { arrayView } from './arrayView';
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
  validatePacked(packed);
  return { columns: packed.columns, rows: Array.from({ length: packed.count }, (_, r) => packed.data.map(column => packedValue(column, r))) };
}
function validatePacked(packed: PackedTable) {
  if (!Number.isInteger(packed.count) || packed.count < 0 || packed.count > 100000 || !Array.isArray(packed.columns) || !Array.isArray(packed.data) || packed.data.length !== packed.columns.length || packed.data.some(c => !Array.isArray(c.values) || c.values.length !== packed.count)) throw new Error('Invalid columnar table dimensions');
 }
function packedValue(column: PackedColumn, row: number): Value {
  const v = column.values[row];
  return (column.tag === null ? v : v === null || column.tag === 'Null' ? { t: 'Null' } : { t: column.tag, v }) as Value;
}
interface ColumnChunks { chunks: { offset: number; table: PackedTable }[]; length: number }
const columnar = new WeakMap<Value[][], ColumnChunks>();
function chunkAt(state: ColumnChunks, row: number) {
  let low = 0, high = state.chunks.length;
  while (low + 1 < high) { const middle = (low + high) >>> 1; if (state.chunks[middle].offset <= row) low = middle; else high = middle; }
  return state.chunks[low];
}
function columnarRow(state: ColumnChunks, row: number) {
  const chunk = chunkAt(state, row);
  return chunk.table.data.map(column => packedValue(column, row - chunk.offset));
}
// Preserve the row-array contract for exports, navigation and selected AI samples; only accessed rows allocate cells.
export function appendPacked(table: Table, packed: PackedTable) {
  validatePacked(packed);
  if (packed.columns.length !== table.columns.length || table.rows.length + packed.count > 100000) throw new Error('Result stream row limit exceeded');
  let state = columnar.get(table.rows);
  if (!state) {
    if (table.rows.length) throw new Error('Cannot append columnar rows to an existing row table');
    state = { chunks: [], length: 0 };
    const storage = state;
    table.rows = arrayView(0, row => columnarRow(storage, row));
    columnar.set(table.rows, state);
    // The sparse array carries length only; the column chunks own the actual data.
    state.chunks.push({ offset: 0, table: packed }); state.length = packed.count; Object.defineProperty(table.rows, 'length', { value: state.length });
    return;
  }
  state.chunks.push({ offset: state.length, table: packed }); state.length += packed.count;
  // Updating the target via its descriptor avoids exposing writable row storage to consumers.
  Object.defineProperty(table.rows, 'length', { value: state.length });
}
export function tableCell(table: Table, row: number, column: number): Value {
  const state = columnar.get(table.rows);
  if (!state) return table.rows[row][column];
  const chunk = chunkAt(state, row);
  return packedValue(chunk.table.data[column], row - chunk.offset);
}
