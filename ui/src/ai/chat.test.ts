import {describe,it,expect} from 'vitest';
import {blockStream,conversationContext,type ChatMessage} from './chat';
describe('bounded assistant conversation',()=>{
 it('commits only closed fences and keeps committed block references stable',()=>{
  const parse=blockStream();const first=parse('설명\n\n```sql\nSELECT ',false);
  expect(first.blocks).toHaveLength(1);expect(first.tail).toContain('SELECT');
  const next=parse('설명\n\n```sql\nSELECT 1;\n```\n\n완료',false);
  expect(next.blocks[0]).toBe(first.blocks[0]);expect(next.blocks[1]).toEqual({kind:'code',language:'sql',text:'SELECT 1;'});
  expect(parse('설명\n\n```sql\nSELECT 1;\n```\n\n완료',true).tail).toBe('');
 });
 it('never resends opted-in rows or failed/incomplete responses and uses a UTF-8 budget',()=>{
  const base:ChatMessage={id:'1',role:'assistant',text:'safe',complete:true};
  expect(conversationContext([base,{...base,sensitive:true,text:'rows'},{...base,error:'cancelled'},{...base,complete:false}])).toEqual([{role:'assistant',content:'safe'}]);
  expect(conversationContext([{...base,text:'한'.repeat(11000)}])).toEqual([]);
  expect(conversationContext(Array.from({length:30},()=>base))).toHaveLength(20);
 });
});
