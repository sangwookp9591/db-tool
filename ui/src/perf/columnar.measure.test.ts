import { expect, it } from 'vitest';
import { appendPacked, unpackTable, type PackedTable } from './columnar';
import type { Table } from '../contract';
const runtime = globalThis as unknown as { gc?: () => void; process: { memoryUsage(): { heapUsed: number } } };
it.skipIf(!runtime.gc)('measures retained V8 heap for 100k x 10 columns (run with node --expose-gc)', () => {
  const packed: PackedTable = { count: 100000, columns: Array.from({ length: 10 }, (_, c) => ({ name: `c${c}`, type_name: c % 2 ? 'text' : 'int8' })), data: Array.from({ length: 10 }, (_, c) => ({ tag: c % 2 ? 'Text' : 'Int', values: Array.from({ length: 100000 }, (_, r) => c % 2 ? `row ${r}` : String(9007199254740993n + BigInt(r))) })) };
  const heap = () => { runtime.gc!(); return runtime.process.memoryUsage().heapUsed; };
  const before = heap(), startRows = performance.now();
  let rows: Table | null = unpackTable(packed); const rowMs = performance.now() - startRows, rowHeap = heap() - before;
  expect(rows.rows[99999][0]).toEqual({ t: 'Int', v: '9007199254840992' });
  rows = null; const baseline = heap(), startColumns = performance.now();
  const columns: Table = { columns: packed.columns, rows: [] }; appendPacked(columns, packed);
  const columnMs = performance.now() - startColumns, columnHeap = heap() - baseline;
  expect(columns.rows[99999][0]).toEqual({ t: 'Int', v: '9007199254840992' });
  expect(rowHeap - columnHeap).toBeGreaterThan(10 * 1024 * 1024);
  console.log(JSON.stringify({ benchmark: 'V8-column-ownership', rows: 100000, columns: 10, rowHeap, columnHeap, rowMs, columnMs }));
});
