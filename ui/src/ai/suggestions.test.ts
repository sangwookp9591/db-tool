import { describe, expect, it } from 'vitest';
import { explainText, knownSimpleReferences, suggestedQueries } from './suggestions';
const schema = { entities: [{id:'users',name:'users',kind:'table',fields:[{name:'id',type_name:'int',pk:true,nullable:false}]}],relations:[] };
describe('suggestion evidence and explicit planning', () => {
  it('extracts alternatives without treating prose as executable query text', () => { expect(suggestedQueries('text\n```sql\nSELECT 1\n```\n```sql\nSELECT 2\n```')).toEqual(['SELECT 1','SELECT 2']); expect(suggestedQueries('SELECT 1')).toEqual([]); });
  it('claims reference evidence only for supported, existing table and column names', () => { expect(knownSimpleReferences('SELECT id FROM users LIMIT 10;',schema)).toBe(true); for(const text of ['SELECT secret FROM users','SELECT * FROM missing','SELECT id FROM users WHERE invalid=1','SELECT u.id FROM users u'])expect(knownSimpleReferences(text,schema)).toBe(false); });
  it('never allows a second statement or unsupported command language into EXPLAIN', () => { expect(explainText('SELECT 1;', 'sql')).toBe('EXPLAIN SELECT 1'); for(const text of ['SELECT 1; DELETE FROM users','SELECT 1; SELECT 2;',''])expect(()=>explainText(text,'sql')).toThrow(); expect(()=>explainText('{"find":"users"}','json')).toThrow(); expect(explainText('MATCH (n) RETURN n','cypher')).toBe('EXPLAIN MATCH (n) RETURN n'); });
});
