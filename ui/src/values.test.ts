import { describe, it, expect } from 'vitest';
import { formatJson, valueText } from './values';
import { mergeGraph, nodeCaption } from './graph';
import { visibleRange } from './virtual';
import type { Graph } from './contract';
describe('exact value rendering', () => {
  it('keeps integers and decimal text exact', () => {
    expect(valueText({ t: 'Int', v: '900719925474099312345' })).toBe('900719925474099312345');
    expect(valueText({ t: 'Decimal', v: '1234567890.1234000000' })).toBe('1234567890.1234000000');
    expect(valueText({ t: 'Null' })).toBe('NULL');
  });
  it('indents raw JSON without changing number or string tokens', () => {
    const raw = '{ "id": 900719925474099312345, "decimal": 1.23000000000000001, "s":"a \\" b", "a": [true, null, {}] }';
    const formatted = formatJson(raw);
    expect(formatted).toContain('900719925474099312345');
    expect(formatted).toContain('1.23000000000000001');
    expect(formatted).toContain('"a \\" b"');
    expect(formatted).toContain('\n  "id":');
    expect(formatJson(formatted, false)).toBe(formatJson(raw, false));
    expect(formatJson('{"a":[],"b":{}}')).toBe('{\n  "a": [],\n  "b": {}\n}');
  });
  it('renders bytes, maps, other and temporal values', () => {
    expect(valueText({ t: 'Bytes', v: { len: 120, hex: 'abcd' } })).toBe('<120 bytes> 0xabcd');
    expect(valueText({ t: 'Map', v: [['x', { t: 'Int', v: '12' }]] })).toBe('{x: 12}');
    expect(valueText({ t: 'DateTimeTz', v: '2026-01-01T00:00:00+09:00' })).toBe('2026-01-01T00:00:00+09:00');
  });
});
describe('graph merge', () => {
  it('deduplicates nodes and relationships by opaque id', () => {
    const a: Graph = { nodes: [{ id: 'a', labels: ['Person'], props: [] }], rels: [{ id: 'r', start: 'a', end: 'b', rel_type: 'KNOWS', props: [] }] };
    const b: Graph = { nodes: [{ id: 'a', labels: ['Person'], props: [['name', { t: 'Text', v: 'Alice' }]] }, { id: 'b', labels: [], props: [] }], rels: [...a.rels] };
    expect(mergeGraph(a, b).nodes).toHaveLength(2);
    expect(mergeGraph(a, b).rels).toHaveLength(1);
    expect(nodeCaption(mergeGraph(a, b).nodes[0])).toBe('Alice');
    expect(a.nodes[0].props).toHaveLength(0);
  });
});
describe('virtual table range', () => {
  it('bounds 100,000 rows to viewport plus overscan', () => {
    expect(visibleRange(100000, 0, 280)).toEqual({ start: 0, end: 16 });
    expect(visibleRange(100000, 28000, 280)).toEqual({ start: 994, end: 1016 });
    expect(visibleRange(100000, 2799900, 280).end).toBe(100000);
    expect(visibleRange(0, 0, 280)).toEqual({ start: 0, end: 0 });
  });
});
