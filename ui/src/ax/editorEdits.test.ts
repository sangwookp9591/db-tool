import { describe,it,expect } from 'vitest';
import { applyTextEdits } from './editorEdits';
describe('editor change boundaries',()=>{it('uses original UTF-16 offsets and preserves unrelated text',()=>{expect(applyTextEdits('😀 select a, b',[{start:10,end:11,text:'name'},{start:13,end:14,text:'age'}])).toBe('😀 select name, age');});it('rejects overlapping and out-of-range fixes',()=>{expect(()=>applyTextEdits('abc',[{start:0,end:2,text:''},{start:1,end:3,text:''}])).toThrow();expect(()=>applyTextEdits('abc',[{start:0,end:4,text:''}])).toThrow();});});
