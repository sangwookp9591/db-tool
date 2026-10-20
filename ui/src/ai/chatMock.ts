import type { Commands } from '../api';
const cancelled=new Set<string>();
export function cancelChat(id:string){cancelled.add(id);}
export async function chatMock(args:Commands['ai_request']['args']):Promise<Commands['ai_request']['result']>{
 const text='## 쿼리 제안\n\n현재 스키마를 참고한 예시입니다. 실행 전에 테이블과 조건을 확인하세요.\n\n```sql\nSELECT id, name\nFROM people\nORDER BY id\nLIMIT 20;\n```\n\n쿼리는 실행하지 않았습니다.';
 args.onEvent?.onmessage({phase:1});
 await new Promise(resolve=>setTimeout(resolve,100));
 args.onEvent?.onmessage({phase:0,schema:'people(id, name)',context_elapsed_ms:100});
 try{for(let i=0;i<text.length;i+=6){if(args.runId&&cancelled.has(args.runId))throw new Error('AI 응답을 중지했습니다');await new Promise(resolve=>setTimeout(resolve,30));if(i===0)args.onEvent?.onmessage({phase:2});args.channel.onmessage(text.slice(i,i+6));}args.onEvent?.onmessage({phase:3});return {text,refused:false,stop_reason:'end_turn'};}
 finally{if(args.runId)cancelled.delete(args.runId);}
}
