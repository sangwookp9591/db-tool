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
import { fitGraph, relationOffsets, edgeCurve, curvePoint, curveDistance } from '../graph';
it('fits all nodes with padding and separates parallel, reverse and self-loop relations', () => {
  const nodes = [{ id: 'a', x: -2000, y: -1000 }, { id: 'b', x: 3000, y: 900 }], fit = fitGraph(nodes, 800, 300);
  for (const n of nodes) { expect(Math.abs(n.x * fit.scale + fit.x)).toBeLessThan(400 - 40); expect(Math.abs(n.y * fit.scale + fit.y)).toBeLessThan(150 - 40); }
  const rels = [{ id: 'r1', start: 'a', end: 'b', rel_type: 'X', props: [] }, { id: 'r2', start: 'b', end: 'a', rel_type: 'Y', props: [] }];
  const offsets = relationOffsets(rels), first = edgeCurve(nodes[0], nodes[1], offsets.get('r1')!), second = edgeCurve(nodes[1], nodes[0], offsets.get('r2')!);
  expect(Math.hypot(curvePoint(first, .5).x - curvePoint(second, .5).x, curvePoint(first, .5).y - curvePoint(second, .5).y)).toBeGreaterThan(25);
  expect(curveDistance(first, curvePoint(first, .5))).toBeLessThan(.001);
  expect(curvePoint(edgeCurve(nodes[0], nodes[0], 24, true), .5).y).toBeLessThan(nodes[0].y - 40);
});

import { bitmapTransform } from '../graph';
it('maps the full-scene bitmap to the exact camera through pan and zoom', () => {
  const base = { x: -150, y: 30, scale: .25 }, width = 1400, height = 900;
  for (const camera of [base, { x: 280, y: -140, scale: 2 }, { x: -800, y: 600, scale: .001 }]) {
    const t = bitmapTransform(base, camera, width, height);
    for (const p of [{ x: -1000, y: 2000 }, { x: 0, y: 0 }]) {
      expect((width / 2 + base.x + p.x * base.scale) * t.scale + t.x).toBeCloseTo(width / 2 + camera.x + p.x * camera.scale);
      expect((height / 2 + base.y + p.y * base.scale) * t.scale + t.y).toBeCloseTo(height / 2 + camera.y + p.y * camera.scale);
    }
  }
});

it('fits dense edge bounds without exceeding the JavaScript argument limit', () => {
  const points = Array.from({ length: 200000 }, (_, i) => ({ x: i, y: -i }));
  const fit = fitGraph(points, 1440, 1000);
  expect(Number.isFinite(fit.x) && Number.isFinite(fit.y) && Number.isFinite(fit.scale)).toBe(true);
});

import { appendPacked, tableCell } from './columnar';
import { serializeTable } from '../export';
import { selectRows } from '../ai/types';
it('keeps column chunks lazy while preserving exact cells, sorting, filtering, exports and AI selection', () => {
  const source = fixtureTable(100, 2), target = { columns: source.columns, rows: [] as typeof source.rows };
  source.rows[22][0] = { t: 'Null' };
  source.rows[40][1] = { t: 'Json', v: '{"nested":[1,true,null]}' };
  appendPacked(target, packTable({ ...source, rows: source.rows.slice(0, 25) }));
  appendPacked(target, packTable({ ...source, rows: source.rows.slice(25) }));
  expect(target.rows.length).toBe(100); expect(target.rows[100]).toBeUndefined();
  expect(target.rows.slice()).toEqual(source.rows);
  expect(tableCell(target, 40, 1)).toEqual(source.rows[40][1]);
  expect([...tableIndex(target, '', 0, true)]).toEqual([...tableIndex(source, '', 0, true)]);
  expect([...tableIndex(target, 'row 3')]).toEqual([...tableIndex(source, 'row 3')]);
  for (const format of ['CSV', 'JSON', 'Markdown'] as const) expect(serializeTable(target, format, false)).toEqual(serializeTable(source, format, false));
  expect(selectRows(target, '40,2')).toEqual(selectRows(source, '40,2'));
  expect(() => target.rows.push(source.rows[0])).toThrow();
  expect(target.rows).toHaveLength(100);
});
