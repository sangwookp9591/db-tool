import {it,expect} from 'vitest';
import {loginDomain,loginCode,loginMark} from './loginFlow';
it('opens only the expected official HTTPS identity domain',()=>{
 expect(loginDomain('anthropic','https://claude.ai/oauth/authorize?state=opaque')).toBe('claude.ai');
 expect(loginDomain('openai','https://auth.openai.com/oauth/authorize')).toBe('auth.openai.com');
 for(const url of ['http://auth.openai.com/oauth','https://auth.openai.com.evil.test/oauth','https://x@auth.openai.com/oauth','https://claude.ai/oauth'])expect(()=>loginDomain('openai',url)).toThrow();
});
it('validates pasted codes without persisting or splitting them',()=>{
 expect(loginCode('  example#state  ')).toBe('example#state');
 for(const code of ['','a\nb','a b','x'.repeat(2049)])expect(()=>loginCode(code)).toThrow();
});

it('maps login lifecycle to status marks without conflating failure and completion',()=>{
 expect([0,1,2].map(i=>loginMark('waiting',i))).toEqual(['done','running','pending']);
 expect(loginMark('failed',1)).toBe('failed');expect(loginMark('cancelled',1)).toBe('cancelled');expect(loginMark('done',2)).toBe('done');
});
