import type { Column, Value } from './contract';
import { nodeCaption } from './graph';

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
export function valueClass(v: Value): string { return v.t === 'Null' ? 'value-null' : ['Int', 'Decimal', 'Float'].includes(v.t) ? 'value-number' : v.t === 'Bool' ? 'value-boolean' : ['Date', 'Time', 'DateTime', 'DateTimeTz', 'Duration'].includes(v.t) ? 'value-date' : v.t === 'Bytes' ? 'value-binary' : ['Json', 'List', 'Map', 'Node', 'Rel', 'Path', 'Point'].includes(v.t) ? 'value-json' : 'value-string'; }
export function valueTitle(v: Value): string { return v.t === 'Other' ? v.v.type_name : v.t; }
export function isCompleteValue(value: Value): boolean { return value.t!=='Bytes'||value.v.hex.length===value.v.len*2; }
export function editValueText(value: Value): string {
  const nested = (value: Value): unknown => {
    switch(value.t) {
      case 'Null': return null;
      case 'Int': return {$numberLong:value.v};
      case 'Decimal': return {$numberDecimal:value.v};
      case 'DateTimeTz': return {$date:value.v};
      case 'Other': return {$oid:value.v.text};
      case 'Map': return Object.fromEntries(value.v.map(([name,value])=>[name,nested(value)]));
      case 'List': return value.v.map(nested);
      default: return value.v;
    }
  };
  return value.t==='Map'||value.t==='List' ? JSON.stringify(nested(value)) : value.t==='Null' ? '' : value.t==='Bytes' ? `0x${value.v.hex}` : valueText(value);
}
export function inputValue(input: string, column: Column, original: Value): Value {
  const type = column.type_name.toLowerCase();
  if (original.t === 'Null' && input.trim().toUpperCase() === 'NULL') return { t: 'Null' };
  if (original.t === 'Bytes' || /^(bytea|blob|binary|varbinary)(?:\b|\s|\(|$)/.test(type)) {
    const raw = input.trim();
    if (raw.toUpperCase() === 'NULL') return { t: 'Null' };
    const hex = raw.startsWith('0x') || raw.startsWith('0X') ? raw.slice(2)
      : raw.startsWith('\\x') || raw.startsWith('\\X') ? raw.slice(2) : null;
    if (hex === null || hex.length % 2 !== 0 || !/^[0-9a-f]*$/i.test(hex)) {
      throw new Error(`${column.name}: 16진수 바이트(0x… 또는 \\x…)나 NULL을 입력하세요.`);
    }
    return { t: 'Bytes', v: { len: hex.length / 2, hex: hex.toLowerCase() } };
  }
  if (original.t === 'Bool' || /^(bool|boolean)$/.test(type)) {
    if (/^(true|t|1|yes)$/i.test(input.trim())) return { t: 'Bool', v: true };
    if (/^(false|f|0|no)$/i.test(input.trim())) return { t: 'Bool', v: false };
    throw new Error(`${column.name}: true 또는 false를 입력하세요.`);
  }
  if (original.t === 'Int' || /\b(int|int2|int4|int8|integer|serial|bigint|smallint|smallserial|bigserial)\b/.test(type)) {
    if (!/^[+-]?\d+$/.test(input.trim())) throw new Error(`${column.name}: 정수를 입력하세요.`);
    return { t: 'Int', v: input.trim() };
  }
  if (original.t === 'Decimal' || /numeric|decimal|money/.test(type)) {
    if (!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/i.test(input.trim())) throw new Error(`${column.name}: 숫자를 입력하세요.`);
    return { t: 'Decimal', v: input.trim() };
  }
  if (original.t === 'Float' || /float|double|real/.test(type)) {
    const parsed = Number(input.trim());
    if (!Number.isFinite(parsed)) throw new Error(`${column.name}: 유한한 숫자를 입력하세요.`);
    return { t: 'Float', v: parsed };
  }
  if (original.t === 'Other' && original.v.type_name === 'ObjectId') {
    if(!/^[a-f0-9]{24}$/i.test(input.trim()))throw new Error(`${column.name}: 올바른 ObjectId를 입력하세요.`);
    return {t:'Other',v:{type_name:'ObjectId',text:input.trim()}};
  }
  if (original.t === 'List' || original.t === 'Map') {
    try { JSON.parse(input); } catch { throw new Error(`${column.name}: 올바른 JSON을 입력하세요.`); }
    return {t:'Json',v:input};
  }
  if (original.t === 'Json' || /json/.test(type)) {
    try { JSON.parse(input); } catch { throw new Error(`${column.name}: 올바른 JSON을 입력하세요.`); }
    return { t: 'Json', v: input };
  }
  if (original.t === 'Date' || /^date$/.test(type)) return { t: 'Date', v: input };
  if (original.t === 'Time' || /^time/.test(type)) return { t: 'Time', v: input };
  if (original.t === 'DateTimeTz' || /timestamptz|timestamp with time zone/.test(type)) return { t: 'DateTimeTz', v: input };
  if (original.t === 'DateTime' || /timestamp|datetime/.test(type)) return { t: 'DateTime', v: input };
  if (original.t === 'Uuid' || /uuid/.test(type)) return { t: 'Uuid', v: input };
  return { t: 'Text', v: input };
}
export function normalizeError(error: unknown): import('./contract').DbError {
  if (error && typeof error === 'object' && 'kind' in error && 'message' in error) return error as import('./contract').DbError;
  return { kind: 'Internal', message: error instanceof Error ? error.message : String(error), detail: null, position: null };
}
