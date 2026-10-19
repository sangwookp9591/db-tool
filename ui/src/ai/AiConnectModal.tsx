import {ConnectSteps,ConnectButton} from '../ui/ConnectSteps';
import {useEffect,useRef,useState} from 'preact/hooks';
import {openUrl} from '@tauri-apps/plugin-opener';
import {ExternalLink,Sparkles,Bot,Clipboard} from 'lucide-preact';
import {api,isMock} from '../api';
import type {Provider} from '../ax/aiContract';
import '../ax/aiContract';
import type {AiSettings} from './types';
import {Dialog} from '../Dialog';
import {Button} from '../ui/Button';
import {loginDomain,loginCode,loginMark,type LoginPhase} from './loginFlow';
import '../w7.css';
export function AiConnectModal({provider,settings,newAccount,onConnected,onClose}:{provider:Provider;settings:AiSettings;newAccount?:boolean;onConnected:()=>Promise<void>;onClose:()=>void}) {
  const [phase,setPhase]=useState<LoginPhase>('idle'),[error,setError]=useState(''),[code,setCode]=useState(''),[domain,setDomain]=useState(''),[needsCode,setNeedsCode]=useState(false);
  const attempt=useRef(0),pending=useRef(false),submitting=useRef(false);
  const name=provider==='anthropic'?'Claude':'ChatGPT';
  const running=['starting','waiting','submitting'].includes(phase);
  useEffect(()=>()=>{attempt.current++;if(pending.current)void api('ai_provider_login_cancel',{provider}).catch(()=>{});},[]);
  async function done(){pending.current=false;setCode('');setPhase('done');await onConnected();}
  useEffect(()=>{if(phase!=='waiting')return;let alive=true,polling=false;const timer=setInterval(()=>{
    if(polling||submitting.current)return;polling=true;
    void api('ai_provider_login_poll',{provider}).then(async result=>{if(!alive)return;if(result.state==='done')await done();else if(result.state==='failed'){pending.current=false;setPhase('failed');setError('로그인에 실패했습니다. 다시 연결하세요.');}}).catch(e=>{if(alive){setPhase('failed');setError(String((e as Error).message??e));}}).finally(()=>{polling=false;});
  },1500);return()=>{alive=false;clearInterval(timer);};},[phase]);
  async function start(){const current=++attempt.current;setError('');setCode('');setPhase('starting');pending.current=true;
    try{
      const next:AiSettings={...settings,provider,enabled:true,auth:provider==='anthropic'?'claude_login':'chatgpt_login',model:provider===settings.provider?settings.model:provider==='anthropic'?'claude-opus-5-5':'gpt-6.1-sol',effort:provider===settings.provider?settings.effort:'medium'};
      await api('ai_configure',{settings:next,apiKey:null});if(current!==attempt.current)return;
      const result=await api('ai_provider_login_start',{provider,...(newAccount?{accountId:''}:{})});
      if(current!==attempt.current){await api('ai_provider_login_cancel',{provider});return;}
      setNeedsCode(provider==='anthropic'&&!!result.waiting_for_code);
      if(result.url){setDomain(loginDomain(provider,result.url));if(!isMock)await openUrl(result.url);}
      setPhase('waiting');
    }catch(e){if(current===attempt.current){setPhase('failed');setError(String((e as Error).message??e));}}
  }
  async function cancel(){attempt.current++;setCode('');setPhase('cancelled');try{if(pending.current)await api('ai_provider_login_cancel',{provider});pending.current=false;onClose();}catch(e){setError(String((e as Error).message??e));setPhase('failed');}}
  async function submit(value:string){if(submitting.current)return;try{const checked=loginCode(value);submitting.current=true;setPhase('submitting');setCode('');await api('ai_login_submit',{code:checked});await done();}catch(e){setError(String((e as Error).message??e));setPhase('failed');}finally{submitting.current=false;}}
  return <Dialog label={`${name} 연결`} className="ai-connect-modal" onClose={()=>void cancel()} footer={<><Button morph={false} onClick={()=>void cancel()}>{phase==='done'?'닫기':'취소'}</Button><ConnectButton variant="brand" state={running?'running':phase==='done'?'done':phase==='failed'?'failed':'idle'} disabled={running||phase==='done'} onClick={()=>void start()}/></>}><div class="connect-heading ai-connect-heading">{provider==='anthropic'?<Sparkles/>:<Bot/>}<h3>Connect with {name}</h3></div><p>{name} 계정으로 로그인해 계정과 구독에서 허용하는 모델을 사용합니다.</p><ConnectSteps label="로그인 진행 단계" steps={['로그인 대기 중','승인 대기','연결됨'].map((label,index)=>({id:label,label,status:loginMark(phase,index)}))}/>{phase==='idle'&&<p class="ax-note">연결을 누르면 공식 로그인 흐름을 시작합니다. 기본 브라우저에서 로그인과 승인을 완료하세요.</p>}{domain&&<p class="ai-connect-domain"><ExternalLink/> 인증 도메인 <strong>{domain}</strong></p>}{phase==='waiting'&&<p role="status">브라우저에서 승인을 기다리고 있습니다. {provider==='openai'?'승인 결과는 앱에 자동으로 반영됩니다.':'공식 ant CLI에서 요청한 코드를 아래에 붙여넣으세요.'}</p>}{needsCode&&phase==='waiting'&&<div class="ax-stack"><label class="ax-field">코드 붙여넣기<input type="password" autoComplete="off" aria-label="Claude 인증 코드" value={code} onInput={e=>setCode(e.currentTarget.value)} onPaste={e=>{const value=e.clipboardData?.getData('text')??'';e.preventDefault();void submit(value);}}/></label><div class="ax-inline"><Button morph={false} onClick={()=>void navigator.clipboard.readText().then(submit).catch(()=>setError('클립보드 접근을 허용하거나 코드 칸에 직접 붙여넣으세요.'))}><Clipboard/>클립보드에서 감지</Button><Button morph={false} disabled={!code.trim()} onClick={()=>void submit(code)}>코드 확인</Button></div><small>붙여넣은 코드는 자동 제출하며 저장하지 않습니다.</small></div>}{phase==='done'&&<p role="status">{name} 계정이 연결되었습니다. 모델 선택기에서 사용할 모델을 고르세요.</p>}{error&&<p role="alert" class="ax-error">{error}</p>}{isMock&&<p class="ax-note">모의 화면 · 실제 공급자 로그인 없음</p>}</Dialog>;
}
