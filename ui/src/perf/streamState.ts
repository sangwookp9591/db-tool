import type { ResultSet } from '../contract';
import { unpackTable, type PackedTable } from './columnar';
export type StreamEvent = { kind: 'start'; index: number; result: ResultSet } | { kind: 'rows'; index: number; offset: number; table: PackedTable } | { kind: 'end'; index: number; result: Omit<ResultSet, 'table' | 'graph' | 'documents'> } | { kind: 'done' };
export class ResultStream {
  results: ResultSet[] = [];
  done = false;
  accept(event: StreamEvent) {
    if (this.done) throw new Error('Result stream already completed');
    if (event.kind === 'done') { this.done = true; return; }
    if (!Number.isInteger(event.index) || event.index < 0 || event.index > 10000) throw new Error('Invalid result index');
    if (event.kind === 'start') {
      if (event.index !== this.results.length) throw new Error('Out-of-order result start');
      this.results.push(event.result); return;
    }
    const result = this.results[event.index]; if (!result) throw new Error('Result stream has no metadata');
    if (event.kind === 'rows') {
      if (!result.table || result.table.rows.length !== event.offset) throw new Error('Out-of-order result rows');
      const table = unpackTable(event.table); if (table.columns.length !== result.table.columns.length || result.table.rows.length + table.rows.length > 100000) throw new Error('Result stream row limit exceeded');
      for (const row of table.rows) result.table.rows.push(row);
      result.row_count = result.table.rows.length;
    } else Object.assign(result, event.result);
  }
}
