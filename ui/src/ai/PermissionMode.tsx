import { Check, ShieldCheck, Shield } from 'lucide-preact';
import { useEffect, useRef } from 'preact/hooks';
import { Button } from '../ui/Button';
export function PermissionMode({readOnly,locked,connection,onChange}:{readOnly:boolean;locked:boolean;connection:string;onChange:(value:boolean)=>void}) {
 const root=useRef<HTMLDetailsElement>(null);
 const selected=readOnly||locked;
 function close(){if(root.current){root.current.open=false;root.current.querySelector('summary')?.focus();}}
 function focusOption(last=false){const options=root.current?.querySelectorAll<HTMLButtonElement>('[role=menuitemradio]:not(:disabled)');if(options?.length)options[last?options.length-1:0].focus();}
 useEffect(()=>{const outside=(event:PointerEvent)=>{if(root.current&&!root.current.contains(event.target as Node))root.current.open=false;};document.addEventListener('pointerdown',outside);return()=>document.removeEventListener('pointerdown',outside);},[]);
 return <details ref={root} class="ai-permission" onKeyDown={event=>{
  if(event.key==='Escape'){event.preventDefault();event.stopPropagation();close();}
  if(event.key==='ArrowDown'||event.key==='ArrowUp'){
   event.preventDefault();const details=root.current!;
   if(!details.open){details.open=true;focusOption(event.key==='ArrowUp');return;}
   const options=[...details.querySelectorAll<HTMLButtonElement>('[role=menuitemradio]:not(:disabled)')];
   const index=options.indexOf(document.activeElement as HTMLButtonElement);
   options[(index+(event.key==='ArrowDown'?1:options.length-1)+options.length)%options.length]?.focus();
  }
 }} onToggle={()=>{if(root.current?.open)root.current.querySelector<HTMLButtonElement>('[aria-checked=true]')?.focus();}}>
  <summary aria-label="AI 권한 모드" aria-haspopup="menu" title={`${selected?'읽기 전용':'승인 후 쓰기'} · ${connection}`} data-read-only={selected}><ShieldCheck/><span class="ai-permission-dot"/><span>{selected?'읽기 전용':'승인 후 쓰기'}</span></summary>
  <div class="ai-permission-menu" role="menu" aria-label={`${connection} · AI가 제안할 작업 범위`}>
   <header><strong>{connection}</strong><p>AI가 제안할 작업 범위</p></header>
   <Button bare class="ai-permission-option" role="menuitemradio" aria-label="읽기 전용" aria-checked={selected} onClick={()=>{onChange(true);close();}}><ShieldCheck aria-hidden="true"/><span><strong>읽기 전용</strong><small>조회 쿼리와 분석만 제안합니다.</small></span>{selected&&<Check aria-hidden="true"/>}</Button>
   <Button bare class="ai-permission-option" role="menuitemradio" aria-label="승인 후 쓰기" aria-checked={!selected} disabled={locked} onClick={()=>{if(!locked){onChange(false);close();}}}><Shield aria-hidden="true"/><span><strong>승인 후 쓰기</strong><small>{locked?'이 연결은 읽기 전용이어서 쓰기를 허용할 수 없습니다.':'쓰기 제안도 미리보기와 사용자의 실행 승인이 필요합니다.'}</small></span>{!selected&&<Check aria-hidden="true"/>}</Button>
   <footer>연결 권한을 확대하거나 쿼리를 자동 실행하지 않습니다.</footer>
  </div>
 </details>;
}
