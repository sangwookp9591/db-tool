import { describe, expect, it } from 'vitest';
import { packTable, unpackTable } from './columnar';
import { tableIndex } from './tableIndex';
import { fixtureTable } from './fixtures';
describe('columnar exact values', () => {
  it('roundtrips homogeneous, null, mixed and nested cells without number coercion', () => {
    const table = fixtureTable(3, 10); table.rows[1][0] = { t: 'Null' }; table.rows[2][1] = { t: 'Json', v: '{"id":9007199254740993123}' };
    expect(unpackTable(packTable(table))).toEqual(table);
    expect(unpackTable(packTable({ columns: table.columns, rows: [] }))).toEqual({ columns: table.columns, rows: [] });
    expect(() => unpackTable({ ...packTable(table), count: 100001 })).toThrow();
  });
  it('returns row indices, filters loaded data and sorts large integers exactly without mutation', () => {
    const table = fixtureTable(3, 2); const before = JSON.stringify(table);
    expect([...tableIndex(table, '', 0, true)]).toEqual([2, 1, 0]);
    expect([...tableIndex(table, 'row 1')]).toEqual([1]);
    expect(JSON.stringify(table)).toBe(before);
  });
});
import { hitIndex } from './spatial';
describe('quadtree hit test', () => {
  it('finds nearest, handles coincident nodes and returns no distant hit', () => {
    const points = Array.from({ length: 5000 }, (_, i) => ({ id: String(i), x: i % 100 * 10, y: Math.floor(i / 100) * 10 }));
    const hit = hitIndex(points);
    expect(hit(21, 19, 4)?.id).toBe('202'); expect(hit(-200, 0, 21)).toBeUndefined();
    expect(hitIndex([{ id: 'a', x: 0, y: 0 }, { id: 'b', x: 0, y: 0 }])(0, 0, 1)).toBeDefined();
    expect(hitIndex([])(0, 0, 1)).toBeUndefined();
  });
});
import { columnRange, visibleRange } from '../virtual';
it('virtualizes unequal columns and clamps empty/end ranges', () => {
  expect(columnRange([100, 200, 50, 80], 110, 60, 0)).toEqual({ start: 1, end: 2, offset: 100, total: 430 });
  expect(columnRange([], 0, 200)).toEqual({ start: 0, end: 0, offset: 0, total: 0 });
  expect(columnRange(Array(200).fill(190), 37000, 1000).end).toBe(200);
  expect(visibleRange(0, -20, 0)).toEqual({ start: 0, end: 0 });
});
import { seedPositions } from '../graph';
import { fixtureGraph } from './fixtures';
it('seeds additions near connected parents and pins existing positions', () => {
  const graph = fixtureGraph(1, 1, 'n0'); const positions = seedPositions({ nodes: [{ id: 'n0', labels: [], props: [] }, ...graph.nodes], rels: graph.rels }, [{ id: 'n0', x: 100, y: 200 }]);
  expect(positions[0]).toEqual({ id: 'n0', x: 100, y: 200, fixed: true });
  expect(Math.hypot(positions[1].x - 100, positions[1].y - 200)).toBeCloseTo(45);
});
import { ResultStream } from './streamState';
import { fixtureResult } from './fixtures';
it('rejects out-of-order, duplicate and oversized streamed rows', () => {
  const stream = new ResultStream(), table = fixtureTable(2, 2);
  stream.accept({ kind: 'start', index: 0, result: fixtureResult({ ...table, rows: [] }) });
  expect(() => stream.accept({ kind: 'rows', index: 0, offset: 1, table: packTable(table) })).toThrow();
  stream.accept({ kind: 'rows', index: 0, offset: 0, table: packTable(table) });
  expect(stream.results[0].table?.rows).toEqual(table.rows);
  stream.accept({ kind: 'done' }); expect(() => stream.accept({ kind: 'done' })).toThrow();
});
import { perf, start, finish, stats } from './index';
it('keeps metric history bounded and leaves unrelated browser timing intact', () => {
  performance.mark('unrelated'); for (let i = 0; i < 2005; i++) finish('test', start('test'));
  expect(perf.marks()).toHaveLength(2000); perf.clear(); expect(perf.marks()).toHaveLength(0);
  expect(performance.getEntriesByName('unrelated')).toHaveLength(1); performance.clearMarks('unrelated');
  expect(stats([3, 1, 2])).toEqual({ p50: 2, p95: 3, max: 3, samples: 3 });
});
it('sorts decimals exactly, including sign, fractional scale, exponent and zero', () => {
  const values = ['10.01', '9.9', '-9.99', '-10.01', '1.2300', '1.23', '1e2', '-0', '0.000'];
  const table = { columns: [{ name: 'money', type_name: 'numeric' }], rows: values.map(v => [{ t: 'Decimal' as const, v }]) };
  expect([...tableIndex(table, '', 0)].map(i => values[i])).toEqual(['-10.01', '-9.99', '-0', '0.000', '1.2300', '1.23', '9.9', '10.01', '1e2']);
});
