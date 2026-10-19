import {it,expect} from 'vitest';
import {axMock} from './mock';
it('disconnecting one mock provider preserves the other provider account',async()=>{
 await axMock('ai_provider_login_start',{provider:'openai'});await axMock('ai_login_submit',{code:'mock'});
 await axMock('ai_provider_logout',{provider:'anthropic'});
 const openai=await axMock('ai_models',{provider:'openai'}) as {status:string}[];
 const anthropic=await axMock('ai_models',{provider:'anthropic'}) as {status:string}[];
 expect(openai[0].status).toBe('connected');expect(anthropic[0].status).toBe('disconnected');
 await axMock('ai_provider_logout',{provider:'openai'});
});
