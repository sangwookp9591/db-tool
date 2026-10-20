import { describe,it,expect } from 'vitest';
import { modelSettings, modelLabel } from './modelSelection';
import type { AiSettings } from './types';
describe('model selection',()=>{it('preserves supported effort and resets unsupported effort',()=>{
 expect(modelLabel('claude-opus-5-5','claude-opus-5-5')).toBe('Opus 5.5');expect(modelLabel('gpt-6.1-sol','gpt-6.1-sol')).toBe('GPT-6.1 Sol');
 const settings:AiSettings={enabled:true,provider:'anthropic',auth:'claude_login',model:'claude-opus-5-5',effort:'max'};
 const model={id:'gpt-6.1-sol',name:'GPT',efforts:['low','medium','high'],status:'connected' as const,cached:false};
 expect(modelSettings(settings,'openai',model)).toMatchObject({provider:'openai',auth:'api_key',effort:'medium'});
 expect(modelSettings(settings,'anthropic',{...model,efforts:[]})).toMatchObject({auth:'claude_login',effort:''});
 expect(modelSettings({...settings,effort:'high'},'openai',model).effort).toBe('high');
});});
