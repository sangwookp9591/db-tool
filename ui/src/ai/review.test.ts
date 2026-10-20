import {describe,it,expect} from 'vitest';
import {fileChanges,toolProposals,lineCounts,diffLines} from './review';
describe('AI review input',()=>{
 it('recognizes filename fences, headings and unnamed SQL blocks without interpreting prose as calls',()=>{
  const files=fileChanges('```sql migrations/001.sql\nSELECT 1;\n```\n\n### `migrations/002.sql`\n\n```sql\nSELECT 2;\n```\n\n```sql\nSELECT 3;\n```');
  expect(files.map(f=>f.path)).toEqual(['migrations/001.sql','migrations/002.sql','proposal-3.sql']);
  expect(files.every(f=>f.decision==='pending')).toBe(true);
  expect(lineCounts('a\nb\nc','a\nx\nc')).toEqual({added:1,removed:1});
  expect(diffLines('keep\nold\nend','keep\nnew\nend').map(line=>[line.kind,line.text,line.number])).toEqual([['context','keep',1],['remove','old',2],['add','new',2],['context','end',3]]);
  expect(toolProposals('run_query({text: "DELETE"})')).toEqual([]);
  expect(toolProposals('```tool\n{"name":"run_query","arguments":{"text":"SELECT 1"},"reason":"check"}\n```')[0].reason).toBe('check');
  expect(toolProposals('```tool\n{"name":"run_query","arguments":[]}\n```')).toEqual([]);
 });
});
