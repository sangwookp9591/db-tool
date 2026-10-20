import { FuseButton } from '../ui/FuseButton';
import { SquareTerminal } from 'lucide-preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import { api } from '../api';
import { Button } from '../ui/Button';
import { SpringCheck } from '../ui/SpringCheck';
import { ToolCallGroup } from '../ui/ToolCall';
import { ChangePreviewView } from '../ChangePreviewView';
import type { ChangePreview } from '../contract';
import type { ToolPolicy, ToolProposal } from './review';
export interface ToolState extends ToolProposal { status:'pending'|'running'|'preview'|'success'|'denied'|'error'; policy?:ToolPolicy; preview?:ChangePreview; row_count?:number; elapsed_ms?:number; error?:string }
export function ToolApproval({tool,readOnly,remembered,onChange}:{tool:ToolState;readOnly:boolean;remembered:Set<string>;onChange:(patch:Partial<ToolState>)=>void}){
 const [remember,setRemember]=useState(false);
 const busy=useRef(false),latest=useRef({tool,readOnly,onChange});latest.current={tool,readOnly,onChange};
 const cacheKey=`${tool.session}:${tool.request.client_id??'assistant'}:${tool.request.name}`;
 const locked=readOnly||tool.readOnly;
 const policy=tool.policy;
 useEffect(()=>{let alive=true;
  if(tool.status!=='pending'&&tool.status!=='preview')return;
  onChange({policy:undefined});
  void api('ai_tool_policy',{sessionId:tool.session,request:tool.request,readOnly:locked}).then(policy=>{
   if(!alive)return;onChange({policy});
   if(policy.allowed&&policy.read_only&&tool.status==='pending'&&remembered.has(cacheKey))void decide('allow');
  }).catch(()=>{if(alive)onChange({policy:undefined,error:'도구 권한을 확인하지 못했습니다.'});});
  return()=>{alive=false;};
 },[tool.id,locked]);
 async function decide(decision:'allow'|'deny'|'apply'){
  if(busy.current)return;busy.current=true;
  const {tool,currentMode}= {tool:latest.current.tool,currentMode:latest.current.readOnly};
  latest.current.onChange({status:'running',error:undefined});
  try{
   const result=await api('ai_tool_decide',{sessionId:tool.session,request:tool.request,readOnly:currentMode||tool.readOnly,decision});
   if(decision==='deny'){latest.current.onChange({status:'denied',preview:undefined});return;}
   if(remember&&tool.policy?.read_only)remembered.add(cacheKey);
   latest.current.onChange({...result,status:result.preview?'preview':'success'});
  }catch{latest.current.onChange({status:'error',preview:undefined,error:'도구 호출이 거부되었거나 실패했습니다. 연결·권한과 감사 로그를 확인하세요.'});}
  finally{busy.current=false;}
 }
 if(['success','denied','error','running'].includes(tool.status))return <div class="ai-tool-trace" data-status={tool.status}><ToolCallGroup calls={[{id:tool.id,name:tool.request.name,summary:tool.status==='denied'?'사용자가 거절했습니다.':tool.row_count===undefined?'호출 처리':`${tool.row_count.toLocaleString()}행`,status:tool.status==='running'?'running':tool.status==='success'?'success':'error',decision:tool.status==='denied'?'denied':tool.status==='success'?'approved':undefined,durationMs:tool.elapsed_ms,error:tool.error,result:tool.status==='success'?{row_count:tool.row_count,elapsed_ms:tool.elapsed_ms}:undefined}]}/>{tool.status==='success'&&<small>{tool.row_count===undefined?'완료':`${tool.row_count.toLocaleString()}행`} · {tool.elapsed_ms??0} ms</small>}</div>;
 return <section class="ai-tool-approval" aria-label="도구 실행 승인"><header><SquareTerminal size={16}/><strong>{tool.status==='preview'?'변경 미리보기 · 실행 대기':'도구 실행 승인'}</strong><code>{tool.request.name}</code></header>
  <small>{policy?.connection??'대상 확인 중'} · {policy?.client??tool.request.client_id??'앱 AI 도우미'} · {policy?.mode==='read_only'?'읽기 전용':policy?.mode==='preview'?'미리보기만':policy?.mode==='none'?'권한 없음':'승인 후 쓰기'}</small>
  <pre class="ai-tool-command">{String(tool.request.arguments.text??JSON.stringify(tool.request.arguments,null,2))}</pre><p class="ai-tool-reason">{tool.reason}</p>
  {tool.status==='preview'&&policy?.allowed&&!policy.can_apply&&<p role="status" class="invalid">이 에이전트는 쓰기 실행 권한이 없습니다.</p>}
  {policy?.reason&&<p role="status" class="invalid">{policy.reason}</p>}{tool.error&&<p role="alert">{tool.error}</p>}
  {tool.status==='preview'&&tool.preview&&<ChangePreviewView preview={tool.preview}/>}
  <div class="ai-tool-footer">{policy?.read_only&&tool.status==='pending'&&<SpringCheck checked={remember} onChange={setRemember} label="이 세션에서 이 도구 다시 묻지 않기"/>}<div class="ai-actions"><Button morph={false} variant="ghost" onClick={()=>void decide('deny')}>거부</Button>
   {tool.status==='preview'?<Button morph={false} variant="destructive" disabled={!policy?.allowed||!policy.can_apply} onClick={()=>void decide('apply')}>미리보기 확인 · 실행</Button>:policy?.destructive?<FuseButton label="허용" undoLabel="취소" doneLabel="실행됨" commitOn="fuseEnd" undoWindow={4000} fuse="outline" pauseOnHover settle="stay" size="sm" disabled={!policy.allowed} onCommit={()=>void decide('allow')}/>:<Button morph={false} class="ai-review-primary" disabled={!policy?.allowed} onClick={()=>void decide('allow')}>허용</Button>}
  </div></div>
 </section>;
}
