import {useEffect,useState} from 'preact/hooks';
import {api,isMock} from '../api';
import {Button} from '../ui/Button';
import {AiConnectModal} from '../ai/AiConnectModal';
import {ModelPicker} from '../ai/ModelPicker';
import type {AiSettings} from '../ai/types';
import type {Provider,Usage} from './aiContract';
import './aiContract';
export function AiAccounts() {
  const [settings,setSettings]=useState<AiSettings|null>(null),[key,setKey]=useState(''),[hasKey,setHasKey]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState('');
  const [connect,setConnect]=useState<{provider:Provider;newAccount?:boolean}|null>(null);
  const [usage,setUsage]=useState<Partial<Record<Provider,Usage>>>({}),[accounts,setAccounts]=useState<{id:string;label:string;active:boolean;logged_in?:boolean;expired?:boolean}[]>([]);
  async function refresh(){const data=await api('ai_settings',{});setSettings(data.settings);setHasKey(data.has_key);setUsage(data.usage??{});setAccounts(data.accounts??[]);window.dispatchEvent(new Event('ax:ai-changed'));}
  useEffect(()=>{void refresh().catch(e=>setError(String(e.message??e)));},[]);
  async function task(fn:()=>Promise<void>){if(busy)return;setBusy(true);setError('');setNotice('');try{await fn();}catch(e){setError(String((e as Error).message??e));}finally{setBusy(false);}}
  if(!settings)return <p role="status">{error||'AI 계정 설정을 읽고 있습니다…'}</p>;
  const provider=settings.provider??'anthropic';
  return <div class="ax-stack ax-account">
    {isMock&&<p class="ax-note">모의 화면 · 로그인과 토큰 사용량은 데모이며 외부 공급자에 연결하지 않습니다.</p>}
    <p class="ax-note">앱 안 로그인은 내장 AI 도우미에 사용됩니다. MCP는 외부 에이전트의 별도 연결이며, 연결별 권한과 사람의 쓰기 승인을 유지합니다. 스키마와 통계만 전송하며 행 데이터는 기본 제외합니다.</p>
    {(['anthropic','openai'] as const).map(id=><section class="ax-provider" data-selected={provider===id} key={id}>
      <div><h3>{id==='anthropic'?'Claude · Anthropic':'ChatGPT · OpenAI'}</h3><p>{provider===id?'선택된 공급자':'다른 공급자'} · 입력 {(usage[id]?.input_tokens??0).toLocaleString()} / 출력 {(usage[id]?.output_tokens??0).toLocaleString()} 토큰</p></div>
      <div class="ax-inline"><Button morph={false} disabled={busy} onClick={()=>setConnect({provider:id})}>{id==='anthropic'?'Claude로 로그인':'ChatGPT로 로그인'}</Button>
      <Button morph={false} disabled={busy} onClick={()=>void task(async()=>{
        if(!window.confirm(`${id==='anthropic'?'Claude':'ChatGPT'} 로그인 토큰과 저장된 API 키를 폐기하고 연결을 해제할까요?`))return;
        const result=await api('ai_provider_logout',{provider:id});await refresh();
        if(result.remote_revoked===false)setNotice('로컬 로그아웃은 완료했습니다. 원격 연결 해제는 확인되지 않아 ChatGPT 설정에서 앱 연결을 해제하세요.');
      })}>로그아웃 / 연결 해제</Button>
      <Button morph={false} disabled={busy||provider===id} onClick={()=>void task(async()=>{await api('ai_configure',{settings:{...settings,provider:id,auth:'api_key',model:id==='anthropic'?'claude-opus-5-5':'gpt-6.1-sol',effort:'medium'},apiKey:null});setKey('');await refresh();})}>선택</Button></div>
    </section>)}
    {accounts.map(account=><div class="ax-inline" key={account.id}><span>{account.label} {account.active&&'· 사용 중'} · {!account.logged_in?'해제됨':account.expired?'Expired':'연결됨'}</span>
      <Button morph={false} disabled={busy||!account.logged_in} onClick={()=>void task(async()=>{if(!window.confirm(`${account.label} 계정의 토큰을 폐기하고 로그아웃할까요?`))return;const result=await api('ai_provider_logout',{provider:'openai',accountId:account.id});await refresh();if(result.remote_revoked===false)setNotice('로컬 토큰은 폐기했습니다. 원격 폐기는 확인되지 않아 ChatGPT 설정에서 연결을 해제하세요.');})}>연결 해제 (토큰 폐기)</Button>
      {!account.active&&<Button morph={false} disabled={busy} onClick={()=>void task(async()=>{await api('ai_account_select',{accountId:account.id});await refresh();})}>이 계정 사용</Button>}
    </div>)}
    <Button morph={false} disabled={busy} onClick={()=>setConnect({provider:'openai',newAccount:true})}>다른 ChatGPT 계정 추가</Button>
    <ModelPicker settings={settings} disabled={busy} onSettings={()=>setConnect({provider})} onChange={async next=>{await api('ai_configure',{settings:next,apiKey:null});await refresh();}}/>
    <label class="ax-field">API 키로 연결<input aria-label="AI API 키" type="password" autoComplete="off" value={key} placeholder={hasKey?'키체인에 저장됨 · 변경할 때 입력':'키는 OS 키체인에 저장됩니다'} onInput={e=>setKey(e.currentTarget.value)}/></label>
    <div class="ax-inline"><Button morph={false} disabled={busy||!settings.model.trim()} onClick={()=>void task(async()=>{await api('ai_configure',{settings:{...settings,enabled:true,...(key?{auth:'api_key' as const}:{})},apiKey:key||null});setKey('');await refresh();setNotice('AI 설정을 저장했습니다.');})}>설정 저장</Button>
    {hasKey&&<Button morph={false} disabled={busy} onClick={()=>void task(async()=>{if(!window.confirm('저장된 API 키를 키체인에서 삭제할까요?'))return;await api('ai_configure',{settings,apiKey:''});await refresh();})}>API 키 연결 해제</Button>}</div>
    {error&&<p role="alert" class="ax-error">{error}</p>}{notice&&<p role="status">{notice}</p>}
    {connect&&<AiConnectModal {...connect} settings={settings} onConnected={refresh} onClose={()=>setConnect(null)}/>}
  </div>;
}
