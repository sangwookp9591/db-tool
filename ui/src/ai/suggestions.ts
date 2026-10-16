import type { SchemaGraph } from '../contract';
export function suggestedQueries(text: string) {
  return [...text.matchAll(/```[^\n]*\n([\s\S]*?)```/g)].map(match => match[1].trim()).filter(Boolean).slice(0, 8);
}
export async function parsedQuery(query: string, mode: string) {
  if (!query.trim() || query.length > 8000) return false;
  if (mode === 'json') { try { JSON.parse(query); return true; } catch { return false; } }
  // The SQL editor parser checks tokens, not database semantics or execution safety.
  if (mode !== 'sql') return false;
  const tree = (await import('@codemirror/lang-sql')).sql().language.parser.parse(query);
  let valid = true;
  tree.iterate({ enter: node => { if (node.type.isError) valid = false; } });
  return valid;
}
export function knownSimpleReferences(query: string, schema: SchemaGraph) {
  const identifier = '(?:"(?:[^"]|"")+"|[A-Za-z_][\\w$]*)';
  const match = new RegExp(`^\\s*SELECT\\s+(.+?)\\s+FROM\\s+(${identifier})(?:\\s+LIMIT\\s+\\d+)?\\s*;?\\s*$`, 'i').exec(query);
  if (!match) return false;
  const name = (value: string) => value.startsWith('"') ? value.slice(1,-1).replace(/""/g,'"') : value.toLowerCase();
  const entity = schema.entities.find(item => item.name === name(match[2]));
  if (!entity) return false;
  return match[1].split(',').every(column => { const value = column.trim(); return value === '*' || new RegExp(`^${identifier}$`).test(value) && entity.fields.some(field => field.name === name(value)); });
}
export function explainText(query: string, mode: string) {
  if (!['sql', 'cypher'].includes(mode)) throw new Error('이 쿼리 언어의 EXPLAIN 검증은 지원하지 않습니다.');
  const statement = query.trim().replace(/;\s*$/, '');
  // Conservative single-statement boundary: quoted semicolons require manual verification too.
  if (!statement || statement.includes(';') || statement.length > 8000) throw new Error('검증은 세미콜론을 포함하지 않는 단일 쿼리만 지원합니다.');
  return `EXPLAIN ${statement}`;
}
