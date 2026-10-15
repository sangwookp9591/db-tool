import type { Value, ResultSet } from './contract';
import { nodeCaption } from './graph';

export function resultSummary(results: ResultSet[]) {
  return results.map(r => ({ title: r.title, row_count: r.row_count, truncated: r.truncated, views: ['table', 'graph', 'documents', 'message'].filter(k => r[k as keyof ResultSet] !== null) }));
}

// Tokenize strings before whitespace: never decode numbers or strings from JSON.
export function formatJson(raw: string, pretty = true): string {
  const tokens = raw.match(/"(?:\\.|[^"\\])*"|[^\s]/g) ?? [];
  let depth = 0;
  let result = '';
  const indent = () => '\n' + '  '.repeat(Math.max(0, depth));
  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];
    if (!pretty) { result += token; continue; }
    if (token === '{' || token === '[') {
      result += token;
      depth++;
      if (tokens[i + 1] !== '}' && tokens[i + 1] !== ']') result += indent();
    } else if (token === '}' || token === ']') {
      depth--;
      if (tokens[i - 1] !== '{' && tokens[i - 1] !== '[') result += indent();
      result += token;
    } else if (token === ',') result += ',' + indent();
    else if (token === ':') result += ': ';
    else result += token;
  }
  return result;
}
const propsText = (props: [string, Value][]) => props.map(([k, v]) => `${k}: ${valueText(v)}`).join(', ');
export function valueText(value: Value, detail = false): string {
  switch (value.t) {
    case 'Null': return 'NULL';
    case 'Json': return formatJson(value.v, detail);
    case 'Bytes': return `<${value.v.len} bytes> 0x${value.v.hex}`;
    case 'List': return `[${value.v.map(v => valueText(v, detail)).join(', ')}]`;
    case 'Map': return `{${propsText(value.v)}}`;
    case 'Node': return `(:${value.v.labels.join(':')} {${propsText(value.v.props)}})`;
    case 'Rel': return `[:${value.v.rel_type}]`;
    case 'Path': return value.v.nodes.map((n, i) => `(${nodeCaption(n)})${value.v.rels[i] ? `-[:${value.v.rels[i].rel_type}]->` : ''}`).join('');
    case 'Point': return `Point(${value.v.x}, ${value.v.y}${value.v.z === null ? '' : `, ${value.v.z}`}; srid=${value.v.srid})`;
    case 'Other': return value.v.text;
    default: return String(value.v);
  }
}
export function valueClass(v: Value): string { return v.t === 'Null' ? 'value-null' : ['Int', 'Decimal', 'Float'].includes(v.t) ? 'value-number' : ''; }
export function valueTitle(v: Value): string { return v.t === 'Other' ? v.v.type_name : v.t; }
export function normalizeError(error: unknown): import('./contract').DbError {
  if (error && typeof error === 'object' && 'kind' in error && 'message' in error) return error as import('./contract').DbError;
  return { kind: 'Internal', message: error instanceof Error ? error.message : String(error), detail: null, position: null };
}
