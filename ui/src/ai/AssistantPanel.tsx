import { QueryDiff } from './FileReview';
import { fileChanges, toolProposals, type FileReviewActions } from './review';
import type { ToolState } from './ToolApproval';
import { PermissionMode } from './PermissionMode';
import { ContextMeter } from './ContextMeter';
import { useCallback, useEffect, useRef, useState } from 'preact/hooks';
import { Channel } from '@tauri-apps/api/core';
import { X, Sparkles, Settings2, ArrowUp, Square } from 'lucide-preact';
import { api, isMock } from '../api';
import { Button } from '../ui/Button';
import { SpringCheck } from '../ui/SpringCheck';
import { Dialog } from '../Dialog';
import { ModelPicker } from './ModelPicker';
import { ChatMessages, type QueryAction } from './ChatMessages';
import { conversationContext, type ChatMessage } from './chat';
import { textBatch } from './textBatch';
import { selectRows, type AiSettings, type WorkInput } from './types';
import type { Table } from '../contract';
import './AssistantPanel.css';
interface Props { fileReview:FileReviewActions; askSequence:number; createFile:(session:string,text:string)=>void; open:boolean; work:()=>WorkInput|null; table:()=>Table|undefined; insert:(session:string,text:string)=>string|undefined; edit:(session:string,text:string,before?:string)=>boolean; close:()=>void; notify:(message:string,tone?:'success'|'error')=>void }
export function AssistantPanel({fileReview,open,work,table,insert,edit,close,notify,createFile,askSequence}:Props) {
  const [settings,setSettings]=useState<AiSettings>({enabled:false,provider:'anthropic',model:'claude-opus-5-5',effort:'medium',auth:'api_key'});
  const [ready,setReady]=useState(false),[hasKey,setHasKey]=useState(false),[loggedIn,setLoggedIn]=useState(false),[key,setKey]=useState('');
  const [saving,setSaving]=useState(false),[saved,setSaved]=useState(false),[error,setError]=useState(''),[settingsOpen,setSettingsOpen]=useState(false);
  const [input,setInput]=useState<WorkInput|null>(()=>work()),[context,setContext]=useState<Record<string,unknown>|null>(null);
  const [question,setQuestion]=useState(''),[includeRows,setIncludeRows]=useState(false),[selection,setSelection]=useState(''),[masked,setMasked]=useState('');
  const [histories,setHistories]=useState<Record<string,ChatMessage[]>>({});
  const [request,setRequest]=useState<{id:string;session:string}|null>(null);
  const [diff,setDiff]=useState<{session:string;tabId?:string;before:string;after:string}|null>(null);
  const [sessionModel,setSessionModel]=useState<AiSettings|null>(null);const effectiveSettings=sessionModel??settings;
  const [readOnly,setReadOnly]=useState(false);
  const remembered=useRef(new Set<string>());
  const modeRef=useRef(readOnly);modeRef.current=readOnly;
  useEffect(()=>{setReadOnly(false);setSessionModel(null);},[input?.session_id]);
  const [replaceAll,setReplaceAll]=useState(false),[detached,setDetached]=useState(false),[mention,setMention]=useState(false);
  const [usage,setUsage]=useState<{input_tokens:number;output_tokens:number}>();
  const [threads,setThreads]=useState<Record<string,string>>({});
  const [width,setWidth]=useState(440),[limit,setLimit]=useState(()=>Math.max(320,innerWidth-480));
  const requestRef=useRef(request),latest=useRef(work),historyRef=useRef(histories),composer=useRef<HTMLTextAreaElement>(null);
  requestRef.current=request;latest.current=work;historyRef.current=histories;
  const session=input?.session_id??'';
  const historyKey=threads[session]??session;
  const messages=histories[historyKey]??[];
  const mentions=[input?.tab,input?.connection,...String(context?.schema??'').matchAll(/\b[A-Za-z_][A-Za-z0-9_.]{2,60}\b/g)].map(v=>typeof v==='string'?v:v?.[0]).filter((v):v is string=>!!v);
  function newChat(){if(session){const id=`${session}:${crypto.randomUUID()}`;setThreads(t=>({...t,[session]:id}));setHistories(h=>({...h,[id]:[]}));}}
  async function refresh() {
    const data=await api('ai_settings',{});setSettings(data.settings);setHasKey(data.has_key);
    const login=data.settings.auth==='claude_login'?await api('ai_login_status',{}):null;
    setLoggedIn(login?.logged_in??!!data.accounts?.some(a=>a.active&&a.logged_in));setReady(true);
  }
  useEffect(()=>{if(!open)return;let alive=true;void refresh().catch(e=>{if(alive)setError(String(e.message??e));});const changed=()=>{void refresh().catch(e=>setError(String(e.message??e)));};window.addEventListener('ax:ai-changed',changed);return()=>{alive=false;window.removeEventListener('ax:ai-changed',changed);};},[open]);
  useEffect(()=>{if(!open)return;const sync=()=>setInput(latest.current());sync();window.addEventListener('ax:work-context',sync);return()=>window.removeEventListener('ax:work-context',sync);},[open,work]);
  useEffect(()=>{setIncludeRows(false);setSelection('');setContext(null);if(!session||!open)return;let alive=true;void api('context_get',{sessionId:session,scope:{run:input?.scope.run}}).then(value=>{if(alive)setContext(value);}).catch(()=>{});return()=>{alive=false;};},[session,input?.scope.run,open]);
  useEffect(()=>{if(!askSequence)return;setQuestion('이 선택 영역 또는 쿼리를 설명하고 개선안을 제안해 주세요.');setInput(latest.current());requestAnimationFrame(()=>composer.current?.focus());},[askSequence]);
  useEffect(()=>{const ask=(event:Event)=>{setQuestion((event as CustomEvent<string>).detail?'이 선택 영역을 설명하고 개선안을 제안해 주세요.':'이 쿼리의 개선안을 제안해 주세요.');setInput(latest.current());requestAnimationFrame(()=>composer.current?.focus());};const clear=()=>{if(requestRef.current)void api('cancel',{runId:requestRef.current.id}).catch(()=>{});setHistories({});remembered.current.clear();};window.addEventListener('ax:ask-selection',ask);window.addEventListener('ax:clear-chat',clear);return()=>{window.removeEventListener('ax:ask-selection',ask);window.removeEventListener('ax:clear-chat',clear);};},[]);
  useEffect(()=>{const resize=()=>setLimit(Math.max(320,innerWidth-480));window.addEventListener('resize',resize);return()=>window.removeEventListener('resize',resize);},[]);
  useEffect(()=>()=>{if(requestRef.current)void api('cancel',{runId:requestRef.current.id}).catch(()=>{});},[]);
  useEffect(()=>{const sync=()=>setHistories(all=>{
    let changed=false;const next=Object.fromEntries(Object.entries(all).map(([key,messages])=>[key,messages.map(message=>{
      if(!message.files||!message.session)return message;
      const files=message.files.map(file=>{if(file.decision!=='accepted')return file;try{const current=fileReview.snapshot(message.session!,file.targetPath??file.path);if(current.tabId===file.tabId&&current.before===file.before){changed=true;return {...file,decision:'pending' as const,revision:undefined};}}catch{}return file;});
      return changed?{...message,files}:message;
    })]));return changed?next:all;
  });window.addEventListener('ax:work-context',sync);return()=>window.removeEventListener('ax:work-context',sync);},[fileReview]);
  async function save(next=settings,remove=false) {
    if(saving)return;
    if(remove&&!confirm('저장된 API 키를 삭제할까요?'))return;
    setSaving(true);setSaved(false);setError('');
    try { await api('ai_configure',{settings:next,apiKey:remove?'':key||null});await refresh();setKey('');setSaved(true);notify(next.enabled?'AI 설정을 저장하고 적용했습니다.':'설정을 저장했습니다. AI 사용은 꺼져 있습니다.');window.dispatchEvent(new Event('ax:ai-changed'));return true; }
    catch(e){const message=String((e as Error).message??e);setError(message);notify(`AI 설정 저장 실패: ${message}`,'error');return false;}
    finally{setSaving(false);}
  }
  function change(next:AiSettings){setSettings(next);setSaved(false);}
  function update(sessionId:string,id:string,patch:Partial<ChatMessage>){setHistories(all=>({...all,[sessionId]:(all[sessionId]??[]).map(m=>m.id===id?{...m,...patch}:m)}));}
  async function ask(regenerate?:string) {
    const source=latest.current();if(!source||requestRef.current)return;
    const runId=crypto.randomUUID(),assistantId=regenerate??crypto.randomUUID();
    const allPrevious=historyRef.current[historyKey]??[];
    const oldIndex=regenerate?allPrevious.findIndex(m=>m.id===regenerate):-1;
    if(regenerate&&oldIndex<1)return;
    const old=oldIndex>=0?allPrevious[oldIndex]:undefined;
    const versions=old?.versions??(old?[{text:old.text,error:old.error}]:[]);
    const startedAt=Date.now();
    let selected:Table|undefined;
    try {selected=includeRows?selectRows(table(),selection):undefined;if(includeRows&&!selected?.rows.length)throw new Error('전송할 행 번호를 선택하세요.');}catch(e){setError(String((e as Error).message));return;}
    const prompt=regenerate?allPrevious[oldIndex-1].text:question.trim();if(!prompt)return;
    const previous=regenerate?allPrevious.slice(0,oldIndex-1):allPrevious;
    const active={id:runId,session:source.session_id};requestRef.current=active;setRequest(active);setError('');setQuestion('');
    const response:ChatMessage={id:assistantId,role:'assistant',files:undefined,tools:undefined,text:'',complete:false,sensitive:includeRows,phase:0,startedAt,versions};
    if(regenerate)update(historyKey,assistantId,response);
    else setHistories(all=>({...all,[historyKey]:[...(all[historyKey]??[]),{id:crypto.randomUUID(),role:'user',text:prompt,complete:true,sensitive:includeRows,startedAt,contextLabels:[source.connection??'연결',source.tab??'쿼리',`선택 ${source.scope.selection?.length??0}자`]},response]}));
    const batch=textBatch(text=>update(historyKey,assistantId,{text}));
    const channel=isMock?{onmessage:batch.push} as Channel<string>:new Channel<string>(batch.push);
    type Progress={phase:number;schema?:string;context_elapsed_ms?:number};
    const onEvent=isMock?{onmessage:(_event:Progress)=>{}} as Channel<Progress>:new Channel<Progress>();
    onEvent.onmessage=event=>update(historyKey,assistantId,{phase:event.phase,...(event.schema!==undefined?{context:{schema:event.schema,elapsedMs:event.context_elapsed_ms??0}}:{})});
    try {
      const result=await api('ai_request',{sessionId:source.session_id,scope:{...source.scope,read_only:readOnly||!!source.readOnly,include_rows:includeRows,selected_rows:selected,masked_columns:masked.split(',').map(s=>s.trim()).filter(Boolean)},question:prompt,overrideSettings:effectiveSettings,runId,history:conversationContext(previous),channel,onEvent});
      setUsage(result.usage);window.dispatchEvent(new CustomEvent('ax:ai-model-used',{detail:{model:result.model,effort:result.effort}}));
      const error=result.refused?'공급자가 응답을 거부했습니다.':result.stop_reason!=='end_turn'&&result.stop_reason!=='tool_use'?`응답 미완료 · ${result.stop_reason}`:undefined;
      const candidates=!error?fileChanges(result.text):[];
      const files=(candidates.length>1||candidates.some(f=>!/^proposal-\d+\.sql$/.test(f.path)))?candidates.map(file=>({...file,...fileReview.snapshot(source.session_id,file.path)})):undefined;
      const proposals=!error?[...toolProposals(result.text),...(result.tools??[]).map(tool=>({request:{name:tool.name,arguments:tool.input},reason:'AI가 이 도구 호출을 제안했습니다.'}))]:[];
      const tools:ToolState[]=proposals.map(proposal=>({...proposal,request:{...proposal.request,arguments:{...proposal.request.arguments,...(!('session_id' in proposal.request.arguments)&&['run_query','preview_changes','lint_query','explain_query','browse','explore','get_schema_context','expand_node','get_result_page','backup_status','schema_diff','plan_import','plan_migration','plan_grants'].includes(proposal.request.name)?{session_id:source.session_id}:{})}},id:crypto.randomUUID(),session:source.session_id,readOnly:modeRef.current||!!source.readOnly,status:'pending'}));
      update(historyKey,assistantId,{session:source.session_id,files,tools,text:result.text,complete:true,error,elapsedMs:Date.now()-startedAt,versions:[...versions,{text:result.text,error}],version:versions.length});
    } catch(e){batch.flush();update(historyKey,assistantId,{complete:true,error:String((e as Error).message??e),elapsedMs:Date.now()-startedAt});}
    finally{batch.dispose();channel.onmessage=()=>{};onEvent.onmessage=()=>{};requestRef.current=null;setRequest(null);}
  }
  async function stop(){if(!requestRef.current)return;try{await api('cancel',{runId:requestRef.current.id});}catch(e){setError(`중지 실패: ${String((e as Error).message??e)}`);}}
  const action:QueryAction=useCallback((query,mode)=>{if(!session)return;if(mode==='tool'){const id=crypto.randomUUID();setHistories(all=>({...all,[historyKey]:[...(all[historyKey]??[]),{id,role:'assistant',text:'',complete:true,session,tools:[{id:crypto.randomUUID(),session,readOnly:readOnly||!!input?.readOnly,status:'pending',request:{name:'run_query',arguments:{session_id:session,text:query}},reason:'제안한 쿼리를 실행하기 전에 권한과 영향을 확인합니다.'}]}]}));}else if(mode==='file'){createFile(session,query);}else if(mode==='new'){if(!insert(session,query))setError('원래 연결이 닫혔습니다.');}else if(mode==='diff'){const source=latest.current();if(source?.session_id!==session){setError('같은 연결의 쿼리 탭을 선택하세요.');return;}setDiff({session,tabId:source.tab_id,before:source.scope.text??'',after:query});}else if(!edit(session,query,replaceAll?latest.current()?.scope.text:undefined))setError('같은 연결의 쿼리 편집기를 선택하세요.');},[session,historyKey,readOnly,input?.readOnly,replaceAll,insert,edit,createFile]);
  function resize(event:PointerEvent){const element=event.currentTarget as HTMLElement;const start=event.clientX,initial=Math.min(width,limit);element.setPointerCapture(event.pointerId);const move=(e:PointerEvent)=>setWidth(Math.max(320,Math.min(limit,initial+start-e.clientX)));const end=()=>{element.removeEventListener('pointermove',move);element.removeEventListener('pointerup',end);element.removeEventListener('pointercancel',end);};element.addEventListener('pointermove',move);element.addEventListener('pointerup',end);element.addEventListener('pointercancel',end);}
  const commands:Record<string,string>={'/explain':'현재 쿼리를 설명해 주세요.','/optimize':'현재 쿼리의 성능 개선안을 제안해 주세요.','/fix-error':'최근 오류를 설명하고 수정 쿼리를 제안해 주세요.','/schema':'현재 스키마 구조를 요약해 주세요.','/new-sql':'새 SQL 파일에 넣을 쿼리를 제안해 주세요: '};
  useEffect(()=>{const el=composer.current;if(el){el.style.height='auto';el.style.height=`${Math.min(200,Math.max(64,el.scrollHeight))}px`;}},[question]);
  useEffect(()=>{const form=composer.current?.closest('form');const panel=form?.closest<HTMLElement>('.ai-dock');if(!form||!panel)return;const observer=new ResizeObserver(()=>panel.style.setProperty('--composer-height',`${form.clientHeight}px`));observer.observe(form);return()=>observer.disconnect();},[open]);
  const authenticated=!!sessionModel||(settings.auth==='api_key'?hasKey:loggedIn);
  return <aside class={`ai-dock ${detached?'ai-dock--floating':''}`} hidden={!open} aria-label="AI 도우미" style={{width:Math.min(width,limit)}}>
    <div class="ai-dock-resize" role="separator" tabIndex={0} aria-label="AI 패널 너비" aria-orientation="vertical" aria-valuemin={320} aria-valuemax={limit} aria-valuenow={Math.min(width,limit)} onPointerDown={resize} onKeyDown={e=>{if(['ArrowLeft','ArrowRight','Home','End'].includes(e.key)){e.preventDefault();setWidth(e.key==='Home'?320:e.key==='End'?limit:Math.max(320,Math.min(limit,width+(e.key==='ArrowLeft'?16:-16))));}}}/>
    <header class="ai-dock-header"><div class="ai-actions"><Sparkles/><select aria-label="대화 목록" value={historyKey} onChange={e=>setThreads(t=>({...t,[session]:e.currentTarget.value}))}>{[...new Set([session,...Object.keys(histories).filter(k=>k===session||k.startsWith(`${session}:`))])].map(id=><option value={id}>{histories[id]?.find(m=>m.role==='user')?.text.slice(0,24)??'새 대화'}</option>)}</select><span class="toolbar-spacer"/><Button variant="ghost" aria-label="새 대화" disabled={!session||!!request} onClick={newChat}>＋</Button><Button variant="ghost" aria-label="패널 분리" aria-pressed={detached} onClick={()=>setDetached(v=>!v)}>↗</Button><Button variant="ghost" aria-label="AI 설정" onClick={()=>setSettingsOpen(v=>!v)}><Settings2/></Button><Button variant="ghost" aria-label="AI 패널 접기" onClick={()=>{setKey('');close();}}><X/></Button></div>
      <div class="ai-actions"><Button variant="ghost" class="ai-model-status" disabled={!ready||saving||!!request} aria-label="연결 상태에서 모델 선택" aria-haspopup="dialog" onClick={()=>window.dispatchEvent(new Event('ax:open-model-picker'))}>{effectiveSettings.provider==='openai'?'OpenAI':'Claude'} · {authenticated?'연결됨':'로그인 필요'}⌄</Button><Button variant="ghost" onClick={()=>window.dispatchEvent(new CustomEvent('ax:accounts'))}>계정 연결</Button></div>
    </header>
    {settingsOpen&&<section class="ai-settings profile-form"><fieldset disabled={saving||!ready}><legend>AI 설정</legend><SpringCheck checked={settings.enabled} onChange={enabled=>change({...settings,enabled})} label="AI 도우미 사용 (기본 꺼짐)"/><label>인증 방식<select value={settings.auth} onChange={e=>change({...settings,auth:e.currentTarget.value as AiSettings['auth']})}><option value="api_key">API 키 (OS 키체인)</option><option value={settings.provider==='openai'?'chatgpt_login':'claude_login'}>{settings.provider==='openai'?'ChatGPT':'Claude'} 로그인</option></select></label>{settings.auth==='api_key'&&<label>API 키<input type="password" autoComplete="off" value={key} placeholder={hasKey?'저장됨 · 변경할 때 입력':'API 키 입력'} onInput={e=>{setKey(e.currentTarget.value);setSaved(false);}}/></label>}<div class="ai-actions"><Button loading={saving} disabled={!ready} onClick={()=>void save()}>{saving?'저장 중…':saved?'저장됨 ✓':'설정 저장'}</Button>{hasKey&&<Button variant="ghost" onClick={()=>void save(settings,true)}>키 삭제</Button>}</div></fieldset><SpringCheck checked={replaceAll} onChange={setReplaceAll} label="적용 시 쿼리 전체 교체"/><Button variant="ghost" disabled={!!request} onClick={()=>{setHistories({});notify('이 앱 세션의 AI 대화를 지웠습니다.');}}>대화 기록 지우기</Button><small>대화는 연결 세션별 메모리에만 보존되며 앱 종료 시 사라집니다.</small></section>}
    {!settings.enabled&&<div class="ai-consent"><p>켜면 질문을 보낼 때 스키마·쿼리·선택 영역·최근 오류를 선택한 공급자에 전송합니다. 쿼리에 직접 쓴 값도 포함될 수 있습니다. 행 데이터는 별도 동의해야 전송합니다.</p><Button disabled={!ready||saving} loading={saving} onClick={()=>void save({...settings,enabled:true})}>동의하고 AI 켜기 · 저장</Button></div>}
    <div class="ai-context" aria-label="자동 포함 맥락"><Button variant="ghost" aria-pressed={includeRows} title="선택한 행만 최대 20개 전송 · 행 번호 지정 필요" onClick={()=>setIncludeRows(v=>!v)}>행 20개 포함</Button><div class="ai-context-scroll"><span>탭 · {input?.tab??'없음'}</span><span title={input?.connection}>연결 · {input?.connection??'없음'}</span>{input?.scope.text&&<span title={input.scope.text}>쿼리 · {input.scope.text.length}자</span>}<span title={String(context?.schema??'')}>스키마 · {context?.schema?'준비됨':'조회 대기'}</span>{!!input?.scope.selection&&<span title={input.scope.selection}>선택 · {input.scope.selection.length}자</span>}{!!context?.error&&<span>최근 오류</span>}</div></div>
    {includeRows&&<div class="ai-rows"><label>전송할 행 번호<input value={selection} placeholder="1,3,5" onInput={e=>setSelection(e.currentTarget.value)}/></label><label>추가 제외 컬럼<input value={masked} onInput={e=>setMasked(e.currentTarget.value)}/></label><small>직접 선택한 행만 보냅니다. 민감 컬럼과 중첩 데이터는 제외합니다.</small></div>}
    <ChatMessages fileReview={fileReview} readOnly={readOnly||!!input?.readOnly} remembered={remembered.current} toolChange={(message,id,patch)=>setHistories(all=>({...all,[historyKey]:(all[historyKey]??[]).map(m=>m.id===message?{...m,tools:m.tools?.map(t=>t.id===id?{...t,...patch}:t)}:m)}))} messages={messages} action={action} session={historyKey} visible={open} busy={!!request} regenerate={id=>void ask(id)} change={(id,patch)=>update(historyKey,id,patch)}/>
    {error&&<p role="alert" class="ai-error">{error}</p>}
    <form class="ai-composer" onSubmit={e=>{e.preventDefault();void ask();}}><textarea ref={composer} aria-label="AI question" placeholder="답장하기 · @로 맥락 추가" value={question} onInput={e=>{setQuestion(e.currentTarget.value);setMention(/@[^ ]*$/.test(e.currentTarget.value));}}/>{question.startsWith('/')&&<div class="ai-mentions" aria-label="질문 명령">{Object.entries(commands).filter(([name])=>name.startsWith(question.split(' ')[0])).map(([name,prompt])=><Button variant="ghost" onClick={()=>{setQuestion(prompt);composer.current?.focus();}}>{name}</Button>)}</div>}{mention&&<div class="ai-mentions" aria-label="맥락 멘션">{[...new Set(mentions)].filter(value=>value.toLowerCase().includes(question.match(/@([^ ]*)$/)?.[1]?.toLowerCase()??'')).slice(0,12).map(value=><Button variant="ghost" onClick={()=>{setQuestion(q=>/@[^ ]*$/.test(q)?q.replace(/@[^ ]*$/,`@${value} `):`${q} @${value} `);setMention(false);composer.current?.focus();}}>{value}</Button>)}</div>}<div class="ai-composer-options"><Button variant="ghost" class="ai-round" aria-label="맥락 첨부" onClick={()=>setMention(v=>!v)}>＋</Button><PermissionMode readOnly={readOnly||!!input?.readOnly} locked={!!input?.readOnly} connection={input?.connection??'연결 없음'} onChange={setReadOnly}/><ModelPicker compact onPreferences={()=>setSettingsOpen(true)} settings={effectiveSettings} disabled={!ready||saving||!!request} onSettings={()=>window.dispatchEvent(new CustomEvent('ax:accounts'))} onChange={async(next,persist)=>{if(persist){if(!await save(next))throw new Error('모델 설정을 저장하지 못했습니다.');setSessionModel(null);}else setSessionModel(next);}}/><ContextMeter messages={messages} query={input?.scope.text??''} schema={String(context?.schema??'')} busy={!!request} usage={usage} onCompact={()=>{const safe=conversationContext(messages.slice(0,-4));const summary=safe.map(m=>`${m.role}: ${m.content.slice(0,160)}`).join('\n').slice(0,4000);setHistories(h=>({...h,[historyKey]:[{id:crypto.randomUUID(),role:'user',text:`이전 대화 발췌 요약 (일부 생략):\n${summary}`,complete:true},...messages.slice(-4)]}));notify('이전 대화를 로컬 발췌로 요약했습니다.');}}/>{request?<Button class="ai-round" variant="destructive" aria-label="중지" title="중지" onClick={()=>void stop()}><Square/></Button>:<Button class="ai-round" variant="primary" type="submit" aria-label="질문 보내기" title="질문 보내기" disabled={!question.trim()||!session||!settings.enabled||!authenticated||saving}><ArrowUp/></Button>}</div>{isMock&&<small>모의 공급자 · 실제 AI 호출 없음</small>}</form>
    {diff&&<Dialog label="AI 제안 비교" wide onClose={()=>setDiff(null)} footer={<><Button onClick={()=>setDiff(null)}>취소</Button><Button variant="primary" onClick={()=>{if(latest.current()?.tab_id===diff.tabId&&edit(diff.session,diff.after,diff.before)){setDiff(null);notify('제안을 편집기에 적용했습니다. 실행하지 않았습니다.');}else setError('비교 중 편집기나 탭이 바뀌었습니다. 다시 비교하세요.');}}>편집기에 적용</Button></>}><QueryDiff before={diff.before} after={diff.after}/><p>쿼리 전체를 교체합니다. 실행은 별도로 승인하세요.</p></Dialog>}
  </aside>;
}
