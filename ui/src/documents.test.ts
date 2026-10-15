import { describe,it,expect } from 'vitest';
import { parseDocument,documentRows,fieldPath } from './documents';
describe('document tree',()=>{
 it('preserves exact integers and decimals and nested paths',()=>{const value=parseDocument('{"a":{"b":[0,1,{"c":9223372036854775807}]},"d":1.000000000000001}');const rows=documentRows([value],new Set(['0','0/0','0/0/0','0/0/0/2']));expect(rows.find(r=>r.path==='a.b[2].c')?.value).toEqual({t:'Int',v:'9223372036854775807'});expect(rows.find(r=>r.path==='d')?.value).toEqual({t:'Decimal',v:'1.000000000000001'});expect(fieldPath('a','b.c')).toBe('a["b.c"]');expect(fieldPath('a','[2]')).toBe('a["[2]"]');});
 it('collapses thousands of documents without expanding their descendants',()=>{const docs=Array.from({length:5000},()=>parseDocument('{"x":[1,2,3]}'));expect(documentRows(docs,new Set())).toHaveLength(5000);expect(documentRows(docs,new Set(['0']))).toHaveLength(5001);});
 it('rejects malformed JSON and excessive nesting',()=>{for(const raw of ['[1,]','{"x":}','1 2','[01]', '['.repeat(130)+'0'+']'.repeat(130)])expect(()=>parseDocument(raw)).toThrow();});
});
