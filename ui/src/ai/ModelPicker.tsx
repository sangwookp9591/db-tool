import { useEffect, useRef, useState } from 'preact/hooks';
import { Sparkles, Bot, ChevronDown } from 'lucide-preact';
import { api, type Commands } from '../api';
import type { Provider } from '../ax/aiContract';
import '../ax/aiContract';
import { BranchedMenu } from '../ui/BranchedMenu';
import { GlideSelect } from '../ui/GlideSelect';
import { Button } from '../ui/Button';
import type { AiSettings } from './types';
import '../w7.css';
import { modelSettings } from './modelSelection';
type Model = Commands['ai_models']['result'][number];
export function ModelPicker({settings,onChange,disabled,onSettings,compact=false}:{compact?:boolean;settings:AiSettings;onChange:(settings:AiSettings)=>Promise<void>;disabled?:boolean;onSettings:()=>void}) {
  const [catalog,setCatalog]=useState<Partial<Record<Provider,Model[]>>>({}),[search,setSearch]=useState(''),[revision,setRevision]=useState(0),[saving,setSaving]=useState(false),[error,setError]=useState('');
  const savingRef=useRef(false);
  const effort=useRef<HTMLDivElement>(null),menu=useRef<HTMLDetailsElement>(null);
  useEffect(()=>{const changed=()=>setRevision(value=>value+1);window.addEventListener('ax:ai-changed',changed);return()=>window.removeEventListener('ax:ai-changed',changed);},[]);
  useEffect(()=>{let alive=true;for(const provider of ['anthropic','openai'] as const)void api('ai_models',{provider}).then(models=>{if(alive)setCatalog(previous=>({...previous,[provider]:models}));}).catch(e=>{if(alive)setError(String(e));});return()=>{alive=false;};},[settings.provider,settings.auth,revision]);
  useEffect(()=>{const key=(event:KeyboardEvent)=>{
    if(event.defaultPrevented||!event.shiftKey||!(event.metaKey||event.ctrlKey)||event.code!=='Period')return;
    const dialog=[...document.querySelectorAll('dialog[open]')].at(-1);
    const visible=[...document.querySelectorAll<HTMLElement>('.model-picker')].filter(node=>!node.closest('[hidden]')&&node.getClientRects().length&&(!dialog||node.closest('dialog')===dialog));
    const owner=visible.find(node=>node.contains(document.activeElement))??visible.find(node=>node.closest('.ai-dock'))??visible[0];
    if(owner!==effort.current?.parentElement)return;
    event.preventDefault();effort.current?.querySelector<HTMLButtonElement>('button')?.click();
  };window.addEventListener('keydown',key);return()=>window.removeEventListener('keydown',key);},[]);
  const model=catalog[settings.provider??'anthropic']?.find(model=>model.id===settings.model),efforts=model?.efforts??[];
  async function choose(next:AiSettings){if(savingRef.current)return;savingRef.current=true;setSaving(true);try{await onChange(next);menu.current?.removeAttribute('open');setError('');}catch(e){setError(String(e));}finally{savingRef.current=false;setSaving(false);}}
  return <div class="model-picker"><details ref={menu}><summary title={settings.model} aria-label="AI 모델 선택" aria-disabled={disabled||saving} onClick={e=>{if(disabled||saving)e.preventDefault();}}>{settings.provider==='openai'?<Bot/>:<Sparkles/>} {compact?settings.model.replace(/^(claude-|gpt-)/,''):settings.model} <ChevronDown/></summary><div class="model-picker__menu"><input aria-label="모델 검색" placeholder="모델 검색…" value={search} onInput={e=>setSearch(e.currentTarget.value)}/>{(['anthropic','openai'] as const).map(provider=>{
    const models=catalog[provider]??[],status=models[0]?.status,connected=status==='connected';
    return <div class={connected?'':'model-picker__offline'} key={provider}><BranchedMenu label={`${provider} 모델`} items={[{value:provider,label:`${provider==='anthropic'?'Claude':'ChatGPT / OpenAI'} · ${connected?'로그인됨':status==='expired'?'Expired':'미연결'}`,icon:provider==='anthropic'?<Sparkles/>:<Bot/>,children:connected?models.filter(model=>`${model.name} ${model.id}`.toLowerCase().includes(search.toLowerCase())).map(model=>({value:model.id,label:`${model.name}${model.cached?' · 캐시':''}`})):undefined}]} defaultOpen={[provider]} defaultActive={settings.model} onSelect={value=>{if(!connected){onSettings();return;}const model=models.find(model=>model.id===value);if(model)void choose(modelSettings(settings,provider,model));}}/></div>;
  })}<Button morph={false} onClick={onSettings}>Settings ↗</Button>{error&&<p role="alert">{error}</p>}</div></details><div ref={effort}><GlideSelect size="sm" ariaLabel="Effort ⇧⌘." value={settings.effort} options={efforts.map(value=>({value,label:compact?value:`Effort · ${value}`}))} placeholder="Effort · 미지원" disabled={disabled||saving||!efforts.length||model?.status!=='connected'} onChange={effort=>void choose({...settings,effort})}/></div></div>;
}
