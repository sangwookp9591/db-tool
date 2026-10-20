import type { AiSettings } from './types';
import type { Provider } from '../ax/aiContract';
import type { Commands } from '../api';
type Model = Commands['ai_models']['result'][number];
export function modelSettings(settings:AiSettings,provider:Provider,model:Model):AiSettings {
  return {...settings,provider,model:model.id,auth:model.auth??(provider===settings.provider?settings.auth:'api_key'),effort:model.efforts.includes(settings.effort)?settings.effort:model.efforts.includes('medium')?'medium':model.efforts[0]??''};
}

export function modelLabel(name:string,id:string):string {
 if(name!==id)return name.replace(/^Claude\s+/i,'');
 return id.replace(/^claude-/,'').replace(/-(\d+)-(\d+)(?=-|$)/g,'-$1.$2').split('-').map(part=>part.toLowerCase()==='gpt'?'GPT':part?part[0].toUpperCase()+part.slice(1):part).join(' ').replace(/^GPT /,'GPT-');
}
