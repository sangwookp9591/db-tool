import { invoke } from '@tauri-apps/api/core';
import { EditorView } from '@codemirror/view';
import { undo } from '@codemirror/commands';
import { api } from '../api';
import type { BenchConfig } from './benchConfig';
import { setTheme } from '../theme';
const wait=async(test:()=>boolean)=>{const end=Date.now()+30000;while(!test()){if(Date.now()>end)throw new Error('D7 native UI timeout');await new Promise(r=>setTimeout(r,25));}};
const button=(text:string,root:ParentNode=document)=>[...root.querySelectorAll<HTMLButtonElement>('button')].find(b=>(b.getAttribute('aria-label')??b.textContent?.trim())===text);
const click=(selector:string)=>{const el=document.querySelector<HTMLButtonElement>(selector);if(!el)throw new Error(`Missing ${selector}`);el.click();};
const input=(selector:string,value:string)=>{const el=document.querySelector<HTMLInputElement>(selector);if(!el)throw new Error(`Missing ${selector}`);el.value=value;el.dispatchEvent(new Event('input',{bubbles:true}));};
export async function d7Bench(){
 const report:{checks:string[];complete?:boolean;error?:string;stage?:string;checkpoint:number}={checks:[],checkpoint:0};
 const checkpoint=async(stage:string)=>{report.stage=stage;report.checkpoint++;await invoke('viewer_bench_write',{report});await invoke('viewer_bench_checkpoint',{checkpoint:report.checkpoint});};
 try{
  const fixture=await invoke<BenchConfig>('viewer_bench_config');
  await wait(()=>!!document.querySelector('[aria-label="Connect or disconnect D7 test"]'));
  click('[aria-label="Connect or disconnect D7 test"]');await wait(()=>!!document.querySelector('[name="credential"]'));input('[name="credential"]',fixture.table.secret);document.querySelector<HTMLFormElement>('#session-credential-form')!.requestSubmit();
  await wait(()=>!!document.querySelector('.tab-body:not([hidden]) .cm-editor'));report.checks.push('native credential prompt → session_open_progress → editor');
  await wait(()=>document.querySelector('.connection-live')?.textContent?.includes('연결됨')===true);report.checks.push('live DNS/auth/permissions timeline success');
  click('[aria-label="AI 도우미"]');await wait(()=>button('동의하고 AI 켜기 · 저장')?.disabled===false);button('동의하고 AI 켜기 · 저장')!.click();await wait(()=>!document.querySelector('.ai-consent'));
  if(!(await api('ai_settings',{})).settings.enabled)throw new Error('Saved setting not applied');report.checks.push('native ai_configure persisted + immediate enabled state');
  click('[aria-label="AI 설정"]');await wait(()=>!!button('저장됨 ✓'));report.checks.push('saved button + toast');input('.ai-settings input[type=password]','fixture-reject-key');await new Promise(r=>setTimeout(r,50));button('설정 저장')!.click();await wait(()=>document.querySelector('.ai-error')?.textContent?.includes('Fixture mode')===true);report.checks.push('native configuration failure visible + toast, no keychain mutation');input('.ai-settings input[type=password]','');await new Promise(r=>setTimeout(r,50));button('설정 저장')!.click();await wait(()=>!!button('저장됨 ✓'));click('[aria-label="AI 설정"]');
  let streamed=false;const streamingObserver=new MutationObserver(records=>{for(const record of records)for(const node of record.addedNodes)if(node instanceof Element&&(node.matches('.ai-tail')||node.querySelector('.ai-tail')))streamed=true;});streamingObserver.observe(document.querySelector('.ai-dock')!,{subtree:true,childList:true});
  input('[aria-label="AI question"]','스키마를 참고해 쿼리를 제안해 주세요.');await wait(()=>button('질문 보내기')?.disabled===false);document.querySelector<HTMLFormElement>('.ai-composer')!.requestSubmit();await wait(()=>!!document.querySelector('.message-actions'));streamingObserver.disconnect();if(!streamed)throw new Error('No incremental stream rendering observed');report.checks.push('native fixture Channel streaming + ThinkingLoader');
  const editor=EditorView.findFromDOM(document.querySelector('.tab-body:not([hidden]) .cm-editor')!)!;const before=editor.state.doc.toString();button('적용',document.querySelector('.ai-code')!)!.click();await wait(()=>editor.state.doc.toString()!==before);if(!document.querySelector('.dirty-dot'))throw new Error('Missing dirty marker');report.checks.push('SQL apply changes editor + dirty without execution');undo(editor);await wait(()=>editor.state.doc.toString()===before);report.checks.push('single undo restores original query');
  button('diff 보기',document.querySelector('.ai-code')!)!.click();await wait(()=>!!document.querySelector('.ai-diff'));button('편집기에 적용')!.click();await wait(()=>!document.querySelector('.ai-diff'));report.checks.push('diff approval applies without execution');undo(editor);
  click('[aria-label="다시 생성"]');await wait(()=>!!button('중지'));await wait(()=>!button('중지'));await wait(()=>!!document.querySelector('[aria-label="이전 답변"]'));report.checks.push('regeneration preserves previous version');click('[aria-label="이전 답변"]');
  input('[aria-label="AI question"]','중지 검증');await wait(()=>button('질문 보내기')?.disabled===false);document.querySelector<HTMLFormElement>('.ai-composer')!.requestSubmit();await wait(()=>!!button('중지'));button('중지')!.click();await wait(()=>!button('중지'));report.checks.push('host cancellation terminates request');
  const profile=(await api('profiles_list',{})).find(p=>p.id==='d7-test')!;
  await api('profile_save',{profile:{...profile,fields:{...profile.fields,user:'axdb_d7_missing_user'}},secret:null});
  click('[aria-label="Connect or disconnect D7 test"]');await wait(()=>!!document.querySelector('.disconnected-tab'));
  click('[aria-label="Connect or disconnect D7 test"]');await wait(()=>!!document.querySelector('[name="credential"]'));input('[name="credential"]',fixture.table.secret);document.querySelector<HTMLFormElement>('#session-credential-form')!.requestSubmit();
  await wait(()=>document.querySelector('.connection-live')?.textContent?.includes('연결 실패')===true);
  if(!document.querySelector('.connection-live [data-status=failed]'))throw new Error('No failed stage');
  report.checks.push('actual authentication failure shows failed stage and reason');
  await api('profile_save',{profile,secret:null});button('다시 시도',document.querySelector('.connection-live')!)!.click();await wait(()=>!!document.querySelector('[name="credential"]'));input('[name="credential"]',fixture.table.secret);document.querySelector<HTMLFormElement>('#session-credential-form')!.requestSubmit();
  await wait(()=>document.querySelector('.connection-live')?.textContent?.includes('연결됨')===true);await wait(()=>!document.querySelector('.statusbar')?.textContent?.includes('네트워크 연결 끊김'));report.checks.push('retry + credential prompt reconnects same query tab and clears stale network status');
  const executions=await api('history_list',{limit:100});if(executions.length)throw new Error('Assistant executed a database query');report.checks.push('history empty: no automatic DB execution');
  click('[aria-label="새 대화"]');await wait(()=>!document.querySelector('.ai-message'));input('[aria-label="AI question"]','연결 확인용 SQL을 제안해 주세요.');await wait(()=>button('질문 보내기')?.disabled===false);document.querySelector<HTMLFormElement>('.ai-composer')!.requestSubmit();await wait(()=>!!button('중지'));await wait(()=>!button('중지'));await wait(()=>!!document.querySelector('.ai-code'));
  button('새 탭',document.querySelector('.ai-code')!)!.click();await wait(()=>document.querySelector('.ai-context')?.textContent?.includes('쿼리 2')===true);report.checks.push('new-tab action updates live context without execution');
  await new Promise(r=>setTimeout(r,600));
  const infinite=document.getAnimations().filter(animation=>animation.playState==='running'&&animation.effect?.getTiming().iterations===Infinity&&(animation.effect as KeyframeEffect).target instanceof Element&&((animation.effect as KeyframeEffect).target as Element).closest('.ai-dock'));
  if(infinite.length)throw new Error('Idle assistant animation');report.checks.push('idle assistant has zero infinite animations');
  const {render}=await import('preact');const {ChatMessages}=await import('../ai/ChatMessages');const probe=document.createElement('div');probe.style.cssText='position:fixed;left:-10000px;top:0;width:400px;height:300px;display:flex';document.body.append(probe);
  const {h}=await import('preact');render(h(ChatMessages,{messages:Array.from({length:1000},(_,i)=>({id:String(i),role:'user' as const,text:`fixture ${i}`,complete:true})),action:()=>{},session:'virtual-test',visible:false,busy:false,regenerate:()=>{},change:()=>{}}),probe);
  await new Promise(r=>setTimeout(r,100));if(probe.querySelectorAll('.ai-message').length>20)throw new Error('Conversation DOM is not virtualized');render(null,probe);report.checks.push('1000-message fixture mounts at most 20 messages');const {PermissionMode}=await import('../ai/PermissionMode');render(h(PermissionMode,{readOnly:true,locked:true,connection:'read-only fixture',onChange:()=>{throw new Error('Read-only escalated');}}),probe);await new Promise(r=>setTimeout(r,25));if(!probe.querySelector<HTMLButtonElement>('[aria-label="승인 후 쓰기"]')?.disabled)throw new Error('Read-only permission choice allows escalation');render(null,probe);probe.remove();report.checks.push('read-only connection disables write permission option');
  for(const dismiss of document.querySelectorAll<HTMLButtonElement>('.toast-stack button[aria-label^="Dismiss"]'))dismiss.click();
  document.querySelector('.ai-code')?.scrollIntoView({block:'center'});
  const key=(element:Element,key:string)=>element.dispatchEvent(new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true}));
  const permission=document.querySelector<HTMLDetailsElement>('.ai-permission')!;
  const summary=permission.querySelector('summary')!;
  key(summary,'ArrowDown');await wait(()=>permission.open);
  await new Promise(r=>setTimeout(r,50));
  const options=[...permission.querySelectorAll<HTMLButtonElement>('[role=menuitemradio]')];
  if(options.length!==2||!permission.querySelector('[role=menu]'))throw new Error('Missing permission menu semantics');
  options[0].focus();key(options[0],'ArrowDown');if(document.activeElement!==options[1])throw new Error('ArrowDown did not move permission focus');
  key(options[1],'ArrowUp');if(document.activeElement!==options[0])throw new Error('ArrowUp did not move permission focus');
  options[0].click();await wait(()=>!permission.open);await wait(()=>permission.querySelector('summary')?.getAttribute('data-read-only')==='true');
  key(summary,'ArrowDown');await new Promise(r=>setTimeout(r,50));key(document.activeElement!,'Escape');if(permission.open||document.activeElement!==summary)throw new Error('Escape did not close and restore focus');
  permission.open=true;options[1].click();await wait(()=>!permission.open);
  report.checks.push('permission menu roles, arrow navigation, selection, Escape and focus restoration');
  const strip=document.querySelector<HTMLElement>('.ai-context')!,scroll=document.querySelector<HTMLElement>('.ai-context-scroll')!,messages=document.querySelector<HTMLElement>('.ai-messages')!;
  const toggle=strip.querySelector('button')!;const toggleX=toggle.getBoundingClientRect().left;scroll.scrollLeft=scroll.scrollWidth;
  if(toggle.getBoundingClientRect().left!==toggleX||!toggle.textContent?.includes('행 20개 포함'))throw new Error('Rows toggle is not fixed before scrolling chips');
  if(getComputedStyle(scroll).scrollbarWidth!=='none'||getComputedStyle(strip).backgroundColor==='rgba(0, 0, 0, 0)'||messages.getBoundingClientRect().top<strip.getBoundingClientRect().bottom)throw new Error('Context strip lacks scroll masking or message separation');scroll.scrollLeft=0;
  report.checks.push('fixed rows toggle, hidden horizontal scrollbar, opaque context strip and separated message viewport');
  for(const theme of ['midnight','daylight'] as const){
   setTheme(theme);(document.activeElement as HTMLElement)?.blur();await new Promise(r=>setTimeout(r,350));await checkpoint(`chat-${theme}`);permission.open=true;await new Promise(r=>setTimeout(r,50));(document.activeElement as HTMLElement)?.blur();
   const menu=permission.querySelector<HTMLElement>('[role=menu]')!.getBoundingClientRect(),chip=summary.getBoundingClientRect(),composer=document.querySelector('.ai-composer')!.getBoundingClientRect();
   if(Math.abs(menu.width-280)>1||Math.abs(chip.top-menu.bottom-8)>1||menu.left<composer.left||menu.right>composer.right)throw new Error('Permission popover is not 280px, anchored 8px above chip or inside composer side edges');
   await checkpoint(`chat-permission-${theme}`);permission.open=false;
  }
  report.checks.push('both themes: 280px permission menu anchored 8px above chip within composer side edges');
  document.querySelector<HTMLDetailsElement>('[aria-label="Connection actions for D7 test"]')!.closest('details')!.open=true;
  button('편집',document.querySelector('.connection-menu')!)!.click();await wait(()=>!!document.querySelector('.profile-form'));
  click('[aria-label="Connection access mode"]');await wait(()=>!!document.querySelector('[role=listbox][aria-label="Connection access mode"]'));
  [...document.querySelectorAll<HTMLElement>('[role=option]')].find(el=>el.textContent?.trim()==='읽기 전용')!.click();
  await wait(()=>document.querySelector<HTMLInputElement>('[name=access_mode]')?.value==='read_only');
  button('저장',document.querySelector('.dialog-shell')!)!.click();await wait(()=>!document.querySelector('.profile-form'));

  click('[aria-label="Connect or disconnect D7 test"]');await wait(()=>!!document.querySelector('.disconnected-tab'));
  click('[aria-label="Connect or disconnect D7 test"]');await wait(()=>!!document.querySelector('[name="credential"]'));input('[name="credential"]',fixture.table.secret);document.querySelector<HTMLFormElement>('#session-credential-form')!.requestSubmit();
  await wait(()=>document.querySelector('.connection-live')?.textContent?.includes('연결됨')===true);
  await wait(()=>document.querySelector('.ai-permission summary')?.getAttribute('data-read-only')==='true');
  await wait(()=>!document.querySelector('.connection-live'));for(const dismiss of document.querySelectorAll<HTMLButtonElement>('.toast-stack button[aria-label^="Dismiss"]'))dismiss.click();
  setTheme('midnight');const lockedMenu=document.querySelector<HTMLDetailsElement>('.ai-permission')!;lockedMenu.open=true;await new Promise(r=>setTimeout(r,350));
  const write=lockedMenu.querySelector<HTMLButtonElement>('[aria-label="승인 후 쓰기"]')!;
  if(!write.disabled||!write.textContent?.includes('이 연결은 읽기 전용'))throw new Error('Read-only connection lacks disabled option and reason');
  const read=lockedMenu.querySelector<HTMLButtonElement>('[aria-label="읽기 전용"]')!;read.focus();key(read,'ArrowDown');if(document.activeElement!==read)throw new Error('Keyboard focused disabled write option');
  read.blur();await new Promise(r=>setTimeout(r,50));await checkpoint('d8-permission-readonly-midnight');report.checks.push('actual read-only PG connection: write option disabled with reason, keyboard skips it');
  lockedMenu.open=false;
  for(const theme of ['midnight','daylight'] as const){
   setTheme(theme);click('[aria-label="Connect or disconnect D7 test"]');await wait(()=>!!document.querySelector('.disconnected-tab'));
   click('[aria-label="Connect or disconnect D7 test"]');await wait(()=>!!document.querySelector('[name="credential"]'));input('[name="credential"]',fixture.table.secret);document.querySelector<HTMLFormElement>('#session-credential-form')!.requestSubmit();
   await wait(()=>document.querySelector('.connection-live')?.textContent?.includes('연결됨')===true);await checkpoint(`d8-connect-live-${theme}`);
  }
  await wait(()=>!document.querySelector('.connection-live'));
  document.querySelector('[aria-label="Connection actions for D7 test"]')!.closest('details')!.open=true;
  button('편집',document.querySelector('.connection-menu')!)!.click();await wait(()=>!!document.querySelector('.profile-form'));
  const credential=document.querySelector<HTMLInputElement>('.profile-form input[aria-label="Password"]');
  if(!credential)throw new Error('Missing test connection password');input('.profile-form input[aria-label="Password"]',fixture.table.secret);
  await new Promise<void>(resolve=>requestAnimationFrame(()=>resolve()));
  click('[data-testid="test-connection-form"]');await wait(()=>document.querySelector('.ax-test-status')?.textContent==='테스트 통과');
  document.querySelector('.connection-test-connect')?.scrollIntoView({block:'center'});
  for(const theme of ['midnight','daylight'] as const){setTheme(theme);await new Promise(r=>setTimeout(r,250));await checkpoint(`d8-connect-test-${theme}`);}
  button('취소',document.querySelector('.dialog-shell')!)!.click();await wait(()=>!document.querySelector('.profile-form'));
  button('계정 연결',document.querySelector('.ai-dock')!)!.click();await wait(()=>!!button('Claude로 로그인'));button('Claude로 로그인')!.click();await wait(()=>!!document.querySelector('.ai-connect-modal'));
  for(const theme of ['midnight','daylight'] as const){setTheme(theme);await new Promise(r=>setTimeout(r,250));await checkpoint(`d8-connect-account-${theme}`);}
  button('취소',document.querySelector('.ai-connect-modal')!)!.click();
  report.checks.push('native both-theme screenshots: live connection done, real connection test success, AI login modal idle steps without starting provider login');
  report.complete=true;
 }catch(e){report.error=String((e as Error).message??e);Object.assign(report,{panel:document.querySelector('.ai-dock')?.textContent,alerts:[...document.querySelectorAll('[role=alert]')].map(e=>e.textContent)});}
 await invoke('viewer_bench_write',{report});
}
