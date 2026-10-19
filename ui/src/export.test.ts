import { describe, expect, it } from 'vitest';
import type { Table, Value } from './contract';
import { exportGraph, exportValue, serializeTable } from './export';
import { schemaSvg } from './exportImage';
import { tableIndex } from './perf/tableIndex';

const table = (rows: Value[][]): Table => ({ columns: [{ name: '값', type_name: 'text' }], rows });
describe('파일 내보내기', () => {
  it('CSV uses UTF-8 BOM, CRLF, RFC 4180 quotes, and distinct NULL/empty cells', () => {
    const result = serializeTable(table([[{ t: 'Null' }], [{ t: 'Text', v: '' }], [{ t: 'Text', v: '한글,"값"\r\n줄' }]]), 'CSV');
    expect(new TextEncoder().encode(result).slice(0, 3)).toEqual(new Uint8Array([239, 187, 191]));
    expect(result).toBe('\ufeff값\r\n\r\n""\r\n"한글,""값""\r\n줄"\r\n');
    expect(serializeTable(table([[{ t: 'Null' }]]), 'CSV', true)).toBe('\ufeff값\r\nNULL\r\n');
  });
  it('preserves integers, decimals, and nested raw JSON numeric lexemes as strings', () => {
    expect(JSON.parse(serializeTable(table([[{ t: 'Int', v: '9223372036854775807' }], [{ t: 'Decimal', v: '1234567890.00000000000000001' }]]), 'JSON'))).toEqual([{ 값: '9223372036854775807' }, { 값: '1234567890.00000000000000001' }]);
    expect(exportValue({ t: 'Json', v: '{"n":9223372036854775807,"d":1.2300,"array":[1e100,true,null],"s":"123\\\"4"}' })).toEqual({ n: '9223372036854775807', d: '1.2300', array: ['1e100', true, null], s: '123"4' });
    expect(exportValue({ t: 'Map', v: [['nested', { t: 'List', v: [{ t: 'Int', v: '9007199254740993' }] }]] })).toEqual({ nested: ['9007199254740993'] });
  });
  it('writes NDJSON records without a spurious record for an empty result', () => {
    expect(serializeTable(table([]), 'NDJSON')).toBe('');
    expect(serializeTable(table([[{t:'Int',v:'9007199254740993'}]]), 'NDJSON')).toBe('{"값":"9007199254740993"}\n');
  });
  it('escapes Markdown pipes, newlines, backslashes, code delimiters, and HTML', () => {
    expect(serializeTable(table([[{ t: 'Text', v: 'a|b\\c\n<script>`' }]]), 'Markdown')).toBe('| 값 |\n| --- |\n| a\\|b\\\\c<br>&lt;script&gt;&#96; |\n');
  });
  it('uses the same filtered sorted row index as the table, including no matches', () => {
    const source = table([[{ t: 'Text', v: '한국 B' }], [{ t: 'Text', v: '기타' }], [{ t: 'Text', v: '한국 A' }]]);
    const order = tableIndex(source, '한국', 0, false);
    expect(JSON.parse(serializeTable({ ...source, rows: Array.from(order, i => source.rows[i]) }, 'JSON'))).toEqual([{ 값: '한국 A' }, { 값: '한국 B' }]);
    expect(JSON.parse(serializeTable({ ...source, rows: Array.from(tableIndex(source, '없음'), i => source.rows[i]) }, 'JSON'))).toEqual([]);
  });
  it('emits graph nodes/edges and rejects duplicate JSON columns instead of losing values', () => {
    expect(exportGraph({ nodes: [{ id: '1', labels: ['사람'], props: [['id', { t: 'Int', v: '9007199254740993' }]] }], rels: [] })).toEqual({ nodes: [{ id: '1', labels: ['사람'], props: { id: '9007199254740993' } }], edges: [] });
    expect(() => serializeTable({ columns: [{ name: 'n', type_name: 'text' }, { name: 'n', type_name: 'text' }], rows: [] }, 'JSON')).toThrow('중복');
  });
  it('exports a vector ERD with escaped labels and optional themed background', () => {
    const schema = { entities: [{ id: '1', name: '<people & "team">', kind: 'table', fields: [] }], relations: [] };
    const nodes = [{ id: '1', x: 0, y: 0, color: 'purple', visible: true }];
    const palette = { surface: 'white', text: 'black', edge: 'gray', selected: 'gold', font: 'sans-serif' };
    const svg = schemaSvg(schema, nodes, [], { x: 0, y: 0, scale: 1 }, 800, 600, palette);
    expect(svg).toContain('&lt;people &amp; &quot;team&quot;&gt;');
    expect(svg).not.toContain('<people');
    expect(svg).not.toContain('width="100%"');
    expect(schemaSvg(schema, nodes, [], { x: 0, y: 0, scale: 1 }, 800, 600, palette, 'white')).toContain('width="100%" height="100%" fill="white"');
  });
});
