import { FileReview } from './FileReview';
import { ToolApproval, type ToolState } from './ToolApproval';
import type { FileReviewActions } from './review';
import { QueryCode } from './QueryCode';
import { memo } from 'preact/compat';
import { useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';
import type { ComponentChildren } from 'preact';
import { ThinkingLoader } from '../ui/ThinkingLoader';
import { MessageActions, MessageText } from '../ui/MessageActions';
import { ToolCallGroup } from '../ui/ToolCall';
import { Button } from '../ui/Button';
import { AlertCircle, RotateCcw } from 'lucide-preact';
import { friendlyError } from './errorCopy';
import { blockStream, type ChatMessage, type MarkdownBlock } from './chat';
export type QueryAction = (query: string, mode: 'insert' | 'diff' | 'new' | 'file' | 'tool') => void;
function inline(text: string): ComponentChildren {
  return text.split(/(\*\*[^*]+\*\*|`[^`]+`)/g).map(part => part.startsWith('**') ? <strong>{part.slice(2,-2)}</strong> : part.startsWith('`') ? <code>{part.slice(1,-1)}</code> : part);
}
const Block = memo(({block, action, enabled}: {block:MarkdownBlock;action:QueryAction;enabled:boolean}) => block.kind === 'code'
  ? <div class="ai-code"><small>{block.language || '쿼리'} · 실행 전 검토</small><QueryCode text={block.text} language={block.language}/><div class="ai-actions"><Button morph={false} disabled={!enabled} onClick={()=>action(block.text,'insert')}>적용</Button><Button morph={false} disabled={!enabled} onClick={()=>action(block.text,'diff')}>diff 보기</Button><Button morph={false} disabled={!enabled} onClick={()=>action(block.text,'new')}>새 탭</Button><Button morph={false} disabled={!enabled} onClick={()=>action(block.text,'tool')}>실행 승인 요청</Button><Button morph={false} onClick={()=>void navigator.clipboard.writeText(block.text).catch(()=>window.alert('복사하지 못했습니다. 코드를 선택해 복사하세요.'))}>복사</Button>{block.language?.toLowerCase()==='sql'&&<Button morph={false} disabled={!enabled} onClick={()=>action(block.text,'file')}>.sql 파일 만들기</Button>}</div></div>
  : <div class="ai-prose">{block.text.trim().split('\n').map(line => /^#{1,6} /.test(line) ? <strong class="ai-heading">{inline(line.replace(/^#+ /,''))}</strong> : <p>{inline(line)}</p>)}</div>);
interface Actions { fileReview:FileReviewActions;readOnly:boolean;remembered:Set<string>; toolChange:(message:string,id:string,patch:Partial<ToolState>)=>void; visible:boolean;busy:boolean;regenerate:(id:string)=>void;change:(id:string,patch:Partial<ChatMessage>)=>void }
function Message({message,action,measure,visible,busy,regenerate,change,fileReview,readOnly,remembered,toolChange}: Actions & {message:ChatMessage;action:QueryAction;measure:(id:string,height:number)=>void}) {
  const parse=useRef(blockStream()), root=useRef<HTMLElement>(null),parsedVersion=useRef(message.version);
  if(parsedVersion.current!==message.version){parse.current=blockStream();parsedVersion.current=message.version;}
  const parsed=message.role==='assistant'?parse.current(message.text,message.complete):null;
  useLayoutEffect(()=>{if(!root.current)return;const observer=new ResizeObserver(entries=>measure(message.id,entries[0].borderBoxSize[0]?.blockSize??root.current!.offsetHeight));observer.observe(root.current);return()=>observer.disconnect();},[message.id]);
  return <article ref={root} class={`ai-message ai-message--${message.role}`}>
    {message.role==='assistant'&&<>{!message.error&&<ThinkingLoader phase={message.phase} done={message.complete} seconds={Math.round((message.elapsedMs??0)/1000)} visible={visible}/>}{message.context&&<details class="ai-work"><summary>작업 내역 · {(message.elapsedMs??message.context.elapsedMs)<1000?'1초 미만':`${Math.round((message.elapsedMs??message.context.elapsedMs)/1000)}초`}</summary>{visible&&<ToolCallGroup calls={[{id:'context',name:'get_work_context',summary:'스키마·테이블·작업 맥락 조회',status:'success',durationMs:message.context.elapsedMs,result:{schema:message.context.schema}}]}/>}</details>}</>}
    {message.contextLabels&&<div class="ai-message-context">{message.contextLabels.map(label=><span>{label}</span>)}</div>}
    <MessageText version={message.version??0}>{parsed?<>{parsed.blocks.filter(block=>!message.files?.length||block.kind!=='code'||!/^sql(?:\s|$)|\.sql$/i.test(block.language??'')).filter(block=>!message.tools?.length||block.kind!=='code'||!/^(tool|mcp)$/i.test(block.language??'')).map((block,i)=><Block key={`${message.version??0}-${i}`} block={block} action={action} enabled={message.complete&&!message.error}/>)}{parsed.tail&&<pre class="ai-tail">{parsed.tail}</pre>}</>:<p>{message.text}</p>}</MessageText>
    {message.files?.length&&message.session?<FileReview files={message.files} session={message.session} actions={fileReview} onChange={files=>change(message.id,{files})}/>:null}
    {visible&&message.tools?.map(tool=><ToolApproval key={tool.id} tool={tool} readOnly={readOnly} remembered={remembered} onChange={patch=>toolChange(message.id,tool.id,patch)}/>)}
    {message.error&&(()=>{const e=friendlyError(message.error);return <div class="ai-error-row" role="alert"><AlertCircle aria-hidden="true"/><div class="ai-error-body"><strong>응답 중지 · 오류</strong><p>{e.summary}</p>{e.detail&&<details><summary>자세히 보기</summary><pre>{e.detail}</pre></details>}</div>{message.complete&&<Button variant="ghost" class="ai-error-retry" disabled={busy} aria-label="다시 시도" onClick={()=>regenerate(message.id)}><RotateCcw aria-hidden="true"/>다시 시도</Button>}</div>;})()}
    <div class="ai-message-footer">{message.role==='assistant'&&message.complete&&!!message.text.trim()&&<MessageActions text={message.text} versions={message.files?.length||message.tools?.length?1:message.versions?.length??1} version={message.version??0} disabled={busy||!!message.files?.some(f=>f.decision==='accepted')||!!message.tools?.some(t=>t.status!=='pending'&&t.status!=='denied'&&t.status!=='error')} onVersion={version=>{const chosen=message.versions?.[version];if(chosen)change(message.id,{...chosen,version});}} onRegenerate={()=>regenerate(message.id)} feedback={message.feedback} onFeedback={feedback=>change(message.id,{feedback})}/>}
    {message.startedAt&&<time>{new Date(message.startedAt).toLocaleTimeString([],{hour:'2-digit',minute:'2-digit'})}</time>}</div>
  </article>;
}
export function ChatMessages({messages,action,session,...actions}: Actions & {messages:ChatMessage[];action:QueryAction;session:string}) {
  const root=useRef<HTMLDivElement>(null), heights=useRef(new Map<string,number>()), follow=useRef(true);
  const [viewport,setViewport]=useState({top:0,height:500}),[,redraw]=useState(0);
  useEffect(()=>{follow.current=true;if(root.current)root.current.scrollTop=root.current.scrollHeight;},[session]);
  useEffect(()=>{if(!root.current)return;const observer=new ResizeObserver(()=>setViewport(v=>({...v,height:root.current!.clientHeight})));observer.observe(root.current);return()=>observer.disconnect();},[]);
  useLayoutEffect(()=>{if(follow.current&&root.current){root.current.scrollTop=root.current.scrollHeight;setViewport(v=>v.top===root.current!.scrollTop?v:{...v,top:root.current!.scrollTop});}},[messages,viewport.height]);
  function measure(id:string,height:number){if(heights.current.get(id)===height)return;heights.current.set(id,height);redraw(n=>n+1);if(follow.current&&root.current)root.current.scrollTop=root.current.scrollHeight;}
  let total=0;const rows=messages.map(message=>{const top=total;total+=(heights.current.get(message.id)??180)+16;return {message,top};});
  const visible=rows.filter((r,i)=>r.top+(heights.current.get(r.message.id)??180)>viewport.top-600&&r.top<viewport.top+viewport.height+600||i===rows.length-1);
  return <div class="ai-messages" ref={root} role="log" aria-label="AI 대화" aria-live="off" onScroll={e=>{const el=e.currentTarget;follow.current=el.scrollHeight-el.scrollTop-el.clientHeight<80;setViewport({top:el.scrollTop,height:el.clientHeight});}}>{!messages.length?<div class="ai-empty"><h3>쿼리를 함께 작성하세요</h3><p>활성 탭의 맥락을 사용합니다.<br/>제안한 쿼리는 직접 검토하고 실행하세요.</p><small>편집기에서 선택 후 ⌘L · 패널 열기 ⌘J</small></div>:<><div class="ai-date">{messages[0]?.startedAt?new Date(messages[0].startedAt).toLocaleDateString():'오늘'}</div><div style={{height:total,position:'relative'}}>{visible.map(({message,top})=><div key={message.id} style={{position:'absolute',top,left:0,right:0}}><Message message={message} action={action} measure={measure} {...actions}/></div>)}</div></>}</div>;
}
