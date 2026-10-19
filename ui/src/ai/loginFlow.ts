import type { Provider } from '../ax/aiContract';
export type LoginPhase='idle'|'starting'|'waiting'|'submitting'|'done'|'failed'|'cancelled';
export function loginDomain(provider:Provider,value:string):string {
  const url=new URL(value);
  const allowed=provider==='openai'?['auth.openai.com']:['claude.ai','platform.claude.com','console.anthropic.com'];
  if(url.protocol!=='https:'||url.username||url.password||!allowed.includes(url.hostname))throw new Error('공식 공급자 인증 주소를 확인할 수 없습니다.');
  return url.hostname;
}
export function loginCode(value:string):string {
  const code=value.trim();
  if(!code||code.length>2048||/[\s\x00-\x1f\x7f]/.test(code))throw new Error('유효한 로그인 코드를 붙여넣으세요.');
  return code;
}

export function loginMark(phase:LoginPhase,index:number):import('../ui/statusMath').MarkStatus {
 if(phase==='done')return 'done';
 const current=phase==='starting'||phase==='idle'?0:1;
 if(index<current)return 'done';
 if(index>current||phase==='idle')return 'pending';
 return phase==='failed'?'failed':phase==='cancelled'?'cancelled':'running';
}
