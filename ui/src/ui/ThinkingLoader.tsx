// 사용자 제공 Thinking Loader (clean style) 기반. 실제 작업 이벤트만 phase를 바꾼다.
import { useEffect, useState } from 'preact/hooks';
import './ThinkingLoader.css';
export function ThinkingLoader({phase=0,done=false,doneLabel,seconds=0,variant='plain',size='sm',visible=true}:{phase?:number;done?:boolean;doneLabel?:string;seconds?:number;variant?:'pill'|'plain';size?:'sm'|'md';visible?:boolean}) {
  const [shown,setShown]=useState(()=>!document.hidden);
  useEffect(()=>{const change=()=>setShown(!document.hidden);document.addEventListener('visibilitychange',change);return()=>document.removeEventListener('visibilitychange',change);},[]);
  if(!visible||!shown)return null;
  const label=done?(doneLabel??(seconds<1?'1초 미만 생각함':`${seconds}초 동안 생각함`)):['생각 중','스키마 읽는 중','쿼리 작성 중','답변 정리 중'][phase]??'생각 중';
  return <span class="ui-tlc" data-size={size} data-variant={variant} data-done={done||undefined} role="status"><span class="ui-tlc__icon" aria-hidden="true">{done?<svg class="ui-tlc__check" viewBox="0 0 16 16"><path d="M3.5 8.4 6.6 11.3 12.5 4.9" pathLength={1}/></svg>:<svg class="ui-tlc__spinner" viewBox="0 0 16 16">{Array.from({length:8},(_,i)=><line key={i} x1="8" y1="1.75" x2="8" y2="4.75" transform={`rotate(${i*45} 8 8)`} style={{animationDelay:`${(i-8)*100}ms`}}/>)}</svg>}</span><span class={!done?'ui-tlc__shine':''} style={{position:'relative'}} data-text={label}>{label}</span></span>;
}
