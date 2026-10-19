import type { Graph, Table, Value } from './contract';
import { valueText } from './values';

export type DataFormat = 'CSV' | 'JSON' | 'Markdown' | 'NDJSON';
export function exportValue(value: Value): unknown {
  switch (value.t) {
    case 'Null': return null;
    case 'Map': return Object.fromEntries(value.v.map(([key, item]) => [key, exportValue(item)]));
    case 'List': return value.v.map(exportValue);
    case 'Node': return { ...value.v, props: exportValue({ t: 'Map', v: value.v.props }) };
    case 'Rel': return { ...value.v, props: exportValue({ t: 'Map', v: value.v.props }) };
    case 'Path': return exportGraph(value.v);
    // Parse only after numeric tokens become strings, including nested JSON numeric values.
    case 'Json': return JSON.parse(value.v.replace(/"(?:\\.|[^"\\])*"|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/g, token => token.startsWith('"') ? token : JSON.stringify(token)));
    default: return value.v;
  }
}
export function exportGraph(graph: Graph) {
  return { nodes: graph.nodes.map(v => exportValue({ t: 'Node', v })), edges: graph.rels.map(v => exportValue({ t: 'Rel', v })) };
}
const csv = (text: string) => text === '' || /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
const markdown = (text: string) => text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('\\', '\\\\').replaceAll('|', '\\|').replaceAll('`', '&#96;').replace(/\r\n|\r|\n/g, '<br>');
export function serializeTable(table: Table, format: DataFormat, nullLabel = false): string {
  if (format === 'JSON' || format === 'NDJSON') {
    const names = table.columns.map(c => c.name);
    if (new Set(names).size !== names.length) throw new Error('중복된 컬럼 이름에 별칭을 지정한 뒤 JSON으로 저장하세요.');
    const rows = table.rows.map(row => Object.fromEntries(names.map((name, i) => [name, exportValue(row[i])])));
    return format === 'NDJSON' ? rows.map(row => JSON.stringify(row)).join('\n') + (rows.length ? '\n' : '') : JSON.stringify(rows, null, 2);
  }
  const text = (v: Value) => v.t === 'Null' ? (nullLabel ? 'NULL' : '') : valueText(v, true);
  if (format === 'CSV') return '\ufeff' + [table.columns.map(c => csv(c.name)).join(','), ...table.rows.map(row => row.map(v => v.t === 'Null' ? (nullLabel ? 'NULL' : '') : csv(text(v))).join(','))].join('\r\n') + '\r\n';
  const line = (cells: string[]) => `| ${cells.map(markdown).join(' | ')} |`;
  return [line(table.columns.map(c => c.name)), line(table.columns.map(() => '---')), ...table.rows.map(row => line(row.map(text)))].join('\n') + '\n';
}
