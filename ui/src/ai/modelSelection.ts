import type { AiSettings } from './types';
import type { Provider } from '../ax/aiContract';
import type { Commands } from '../api';
type Model = Commands['ai_models']['result'][number];
export function modelSettings(settings:AiSettings,provider:Provider,model:Model):AiSettings {
  return {...settings,provider,model:model.id,auth:model.auth??(provider===settings.provider?settings.auth:'api_key'),effort:model.efforts.includes(settings.effort)?settings.effort:model.efforts.includes('medium')?'medium':model.efforts[0]??''};
}
