// 사용자 제공 Message Actions (Clean) 기반. 피드백은 호출자의 세션 메모리에만 남긴다.
import type { ComponentChildren } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import { Check, Copy, RotateCw, ThumbsUp, ThumbsDown, ChevronLeft, ChevronRight } from 'lucide-preact';
import { Button } from './Button';
import './MessageActions.css';
export interface Feedback { kind:'up'|'down';reason?:string }
export function MessageText({version,children}:{version:number;children:ComponentChildren}) {
  const last=useRef({version,children}),[out,setOut]=useState<ComponentChildren>(null);
  useEffect(()=>{if(last.current.version!==version){setOut(last.current.children);const timer=setTimeout(()=>setOut(null),240);last.current={version,children};return()=>clearTimeout(timer);}last.current={version,children};},[version,children]);
  return <div class="message-text">{out&&<div class="message-text-out" inert aria-hidden="true">{out}</div>}<div key={version} class={out?'message-text-in':''}>{children}</div></div>;
}
export function MessageActions({text,versions,version,onVersion,onRegenerate,disabled,feedback,onFeedback}:{text:string;versions:number;version:number;onVersion:(v:number)=>void;onRegenerate:()=>void;disabled:boolean;feedback?:Feedback;onFeedback:(value:Feedback)=>void}) {
 const [copied,setCopied]=useState(false),[error,setError]=useState(''),[reason,setReason]=useState('');
 useEffect(()=>{if(!copied)return;const timer=setTimeout(()=>setCopied(false),1600);return()=>clearTimeout(timer);},[copied]);
 return <div class="message-actions" aria-label="답변 액션"><div class="ai-actions"><Button variant="ghost" aria-label="답변 복사" title={copied?'복사됨':'복사'} onClick={()=>void navigator.clipboard.writeText(text).then(()=>setCopied(true)).catch(()=>setError('복사하지 못했습니다.'))}>{copied?<Check/>:<Copy/>}</Button><Button variant="ghost" aria-label="다시 생성" title="다시 생성 · 이전 답변 보존" disabled={disabled} onClick={onRegenerate}><RotateCw/></Button>{versions>1&&<><Button variant="ghost" aria-label="이전 답변" disabled={disabled||version===0} onClick={()=>onVersion(version-1)}><ChevronLeft/></Button><small>{version+1}/{versions}</small><Button variant="ghost" aria-label="다음 답변" disabled={disabled||version===versions-1} onClick={()=>onVersion(version+1)}><ChevronRight/></Button></>}<Button variant="ghost" aria-label="좋아요" aria-pressed={feedback?.kind==='up'} onClick={()=>onFeedback({kind:'up'})}><ThumbsUp/></Button><Button variant="ghost" aria-label="싫어요" aria-pressed={feedback?.kind==='down'} onClick={()=>onFeedback({kind:'down'})}><ThumbsDown/></Button></div>{feedback?.kind==='down'&&<div class="ai-actions">{['부정확함','너무 김','지시 미준수','기타'].map(value=><Button variant="ghost" aria-pressed={reason===value} onClick={()=>{setReason(value);onFeedback({kind:'down',reason:value});}}>{value}</Button>)}<small>이 세션에만 기록됨</small></div>}{error&&<small role="alert">{error}</small>}</div>;
}
