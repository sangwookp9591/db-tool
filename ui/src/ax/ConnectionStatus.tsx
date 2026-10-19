import {StatusMark} from '../ui/StatusMark';
import { useEffect, useState } from 'preact/hooks';
import { listen } from '@tauri-apps/api/event';
import { api, isMock } from '../api';
import { Dialog } from '../Dialog';
import { Button } from '../ui/Button';
import { useAiReady } from './useAiReady';
import { networkStatus, type ConnectionEvent } from './connectionContract';
export type StageEvent=Extract<ConnectionEvent,{event:'stage'}>;
const stageNames={dns:'DNS',ssh:'SSH 인증',tunnel:'터널',tls:'TLS',db_auth:'DB 인증',permissions:'권한 확인'};
export function ConnectionTimeline({events}:{events:StageEvent[]}){
 const [diagnosis,setDiagnosis]=useState(''),[busy,setBusy]=useState(false);
 const ready=useAiReady();
 async function diagnose(){setBusy(true);try{const result=await api('ai_diagnose_connection',{steps:events.filter(e=>e.status!=='running').map(e=>({stage:e.stage==='ssh'?'ssh_auth':e.stage,status:e.status==='passed'?'ok':e.status==='failed'?'failed':'skipped',elapsed_ms:e.elapsed_ms,error_code:safeCode(e.code)}))});setDiagnosis(result.text);}catch(e){setDiagnosis(String((e as Error).message??e));}finally{setBusy(false);}}
 return <section aria-label="단계별 연결 테스트"><ol class="ax-timeline">{(Object.keys(stageNames) as StageEvent['stage'][]).map(stage=>{const item=events.find(e=>e.stage===stage);return <li key={stage} data-status={item?.status??'waiting'}><span class="ax-timeline-dot" aria-label={item?.status==='passed'?'통과':item?.status==='failed'?'실패':item?.status==='skipped'?'생략':'대기'}><StatusMark status={item?.status==='passed'?'done':item?.status==='failed'?'failed':item?.status==='running'?'running':item?.status==='skipped'?'cancelled':'pending'} strike={false} size={16}/></span><div><strong>{stageNames[stage]}</strong>{item?.status==='failed'&&<p>{help(item.code)}</p>}{item?.status==='skipped'&&<small> · 해당 없음 / 미검증</small>}</div><span class="ax-test-time">{item?`${item.elapsed_ms} ms`:'대기'}</span></li>;})}</ol>{events.some(e=>e.status==='failed')&&<><Button morph={false} type="button" disabled={busy} onClick={()=>ready?void diagnose():window.dispatchEvent(new CustomEvent('ax:accounts'))}>{ready?'AI로 진단':'로그인 필요'}</Button><small>단계와 안전한 오류 코드만 전송합니다.</small></>}{diagnosis&&<p class="ax-note">{diagnosis}</p>}</section>;
}
const codes=['dns_failed','connection_refused','timeout','host_key_unknown','host_key_mismatch','key_permissions','ssh_auth_failed','proxy_auth_failed','tls_failed','db_auth_failed','permission_denied','unsupported','unknown'];
function safeCode(code:string|null){if(!code)return null;const normalized=code.toLowerCase();return codes.includes(normalized)?normalized:'unknown';}
function help(code:string|null){const value=(code??'').toLowerCase();if(value.includes('host_key'))return '서버 관리자에게 지문을 확인하세요. 지문이 바뀌었다면 연결을 승인하지 마세요.';if(value.includes('key')||value.includes('permission'))return '키 파일 경로와 소유자 접근 권한, DB 역할 권한을 확인하세요.';if(value.includes('auth'))return '사용자 이름, 키체인 자격 증명, IAM 프로필과 DB 인증 정책을 확인하세요.';if(value.includes('tls'))return 'CA 인증서와 서버 호스트 이름을 확인하세요.';return '호스트·포트와 네트워크 경로를 확인한 뒤 다시 테스트하세요.';}
export function HostKeyDialog(){
 const [request,setRequest]=useState<Extract<ConnectionEvent,{event:'host_key'}>|null>(null),[error,setError]=useState(''),[busy,setBusy]=useState(false),[state,setState]=useState(''),[blocked,setBlocked]=useState(false);
 useEffect(()=>{
  let alive=true;
  const receive=(event:ConnectionEvent)=>{if(!alive)return;if(event.event==='host_key'){setError('');setRequest(event);}if(event.event==='network'){setState(networkStatus(event));setBlocked(event.code==='SSH_HOST_KEY_CHANGED');}};
  if(isMock){const mock=(event:Event)=>receive((event as CustomEvent<ConnectionEvent>).detail);window.addEventListener('ax:mock-network',mock);return()=>{alive=false;window.removeEventListener('ax:mock-network',mock);};}
  const sub=listen<ConnectionEvent>('connection-network',event=>receive(event.payload));
  sub.catch(()=>{if(alive)setState('네트워크 상태 알림을 연결하지 못했습니다.');});
  return()=>{alive=false;void sub.then(unlisten=>unlisten()).catch(()=>{});};
 },[]);
 async function answer(accept:boolean){if(!request||busy)return;setBusy(true);setError('');try{await api('connection_host_key_confirm',{requestId:request.request_id,accept});setRequest(null);}catch(e){setError(String((e as Error).message??e));}finally{setBusy(false);}}
 return <>{state&&<span role={blocked?"alert":"status"}>{state}</span>}{request&&<Dialog title="SSH 호스트 지문 확인" label="SSH 호스트 지문 확인" closeDisabled={busy} onClose={()=>void answer(false)} footer={<><Button morph={false} disabled={busy} onClick={()=>void answer(false)}>연결 거부</Button><Button morph={false} variant="brand" disabled={busy} onClick={()=>void answer(true)}>지문 확인 및 신뢰</Button></>}><p>{request.host}:{request.port}</p><pre class="ax-code">{request.fingerprint}</pre><p>서버 관리자가 알려 준 지문과 일치하는지 별도 경로로 확인하세요. 승인하면 앱의 known_hosts에 저장됩니다.</p>{error&&<p role="alert">{error}</p>}</Dialog>}</>;
}
