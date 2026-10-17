import { SuggestionCard, type Suggestion } from '../ui/SuggestionCard';
import { ToolCallGroup } from '../ui/ToolCall';
import { CallChip } from '../ui/CallChip';
import { suggestedQueries, parsedQuery, knownSimpleReferences, explainText } from './suggestions';
import { Button } from '../ui/Button';
import { SpringCheck } from '../ui/SpringCheck';
import { JellyRadio } from '../ui/JellyRadio';
import { LatticeLoader } from '../ui/LatticeLoader';
import { X, Sparkles } from 'lucide-preact';
import { useEffect, useState } from 'preact/hooks';
import { Channel } from '@tauri-apps/api/core';
import { api, isMock } from '../api';
import { Dialog } from '../Dialog';
import type { Table, DriverManifest, SchemaGraph } from '../contract';
import { selectRows, type AiReply, type AiSettings, type WorkInput } from './types';
export function AssistantPanel({ work, table, insert, undo, driver, close, open = true }: { open?:boolean; work: () => WorkInput | null; table: () => Table | undefined; insert: (session: string, text: string) => string | undefined; undo:(tabId:string)=>Promise<boolean>; driver:(session:string)=>DriverManifest|undefined; close: () => void }) {
  const [settings, setSettings] = useState<AiSettings>({ enabled: false, model: 'claude-opus-5-5', effort: 'medium', auth: 'api_key' });
  const [hasAnt, setHasAnt] = useState(false), [loggedIn, setLoggedIn] = useState(false), [loginUrl, setLoginUrl] = useState<string | null>(null), [code, setCode] = useState(''), [account, setAccount] = useState('');
  const [hasKey, setHasKey] = useState(false), [key, setKey] = useState(''), [ready, setReady] = useState(false);
  const [question, setQuestion] = useState('이 스키마에서 필요한 쿼리를 작성해 주세요.');
  const [includeRows, setIncludeRows] = useState(false), [selection, setSelection] = useState(''), [masked, setMasked] = useState('');
  const [text, setText] = useState(''), [reply, setReply] = useState<AiReply | null>(null), [session, setSession] = useState('');
  const [busy, setBusy] = useState(false), [error, setError] = useState('');
  const [validating, setValidating] = useState(false);
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [inserted, setInserted] = useState<{suggestionId:string;tabId:string}|null>(null);
  const [busyAction, setBusyAction] = useState('');
  useEffect(() => { if(!open)return; let alive = true; setReady(false); void api('ai_settings', {}).then(s => { if (alive) { setSettings(s.settings); setHasKey(s.has_key); setHasAnt(s.has_ant_cli); setReady(true); } }).catch(e => { if (alive) setError(String(e.message ?? e)); }); return () => { alive = false; }; }, [open]);
  async function save(remove = false) {
    setBusyAction('save'); setBusy(true); setError('');
    try { await api('ai_configure', { settings, apiKey: remove ? '' : key || null }); const s = await api('ai_settings', {}); setSettings(s.settings); setHasKey(s.has_key); setHasAnt(s.has_ant_cli); setKey(''); }
    catch (e) { setError(String((e as Error).message ?? e)); }
    finally { setBusy(false); }
  }
  async function refreshLogin() { try { const s = await api('ai_login_status', {}); setHasAnt(s.installed); setLoggedIn(s.logged_in); } catch (e) { setError(String((e as Error).message ?? e)); } }
  useEffect(() => { if (settings.auth === 'claude_login') void refreshLogin(); }, [settings.auth]);
  useEffect(() => {
    if (loginUrl === null || !open) return;
    const timer = setInterval(() => { void api('ai_login_poll', {}).then(async r => {
      if (r.state === 'done') { setLoginUrl(null); setCode(''); setAccount(r.account ?? ''); await refreshLogin(); }
      else if (r.state === 'failed') { setLoginUrl(null); setCode(''); setError('Claude 로그인이 시간 초과(15분)되었거나 실패했습니다. 다시 시도하세요.'); }
    }).catch(() => undefined); }, 2000);
    return () => clearInterval(timer);
  }, [loginUrl,open]);
  async function login(step: 'start' | 'submit' | 'cancel' | 'logout') {
    setBusyAction('login'); setBusy(true); setError('');
    try {
      if (step === 'start') { const r = await api('ai_login_start', {}); setLoginUrl(r.url ?? ''); setCode(''); }
      else if (step === 'submit') { const r = await api('ai_login_submit', { code }); setAccount(r.account ?? ''); setLoginUrl(null); setCode(''); await refreshLogin(); }
      else if (step === 'cancel') { await api('ai_login_cancel', {}); setLoginUrl(null); setCode(''); }
      else { await api('ai_logout', {}); setAccount(''); await refreshLogin(); }
    } catch (e) { setError(String((e as Error).message ?? e)); if (step === 'submit') setLoginUrl(null); }
    finally { setBusy(false); }
  }
  async function ask() {
    setError(''); setReply(null); setText('');
    const input = work(); if (!input) { setError('연결된 쿼리 탭을 먼저 여세요.'); return; }
    setBusyAction('request'); setBusy(true); setSession(input.session_id);
    try {
      const selected = includeRows ? selectRows(table(), selection) : undefined;
      if (includeRows && !selected?.rows.length) throw new Error('전송할 행 번호를 선택하세요.');
      const channel = new Channel<string>(); channel.onmessage = delta => setText(previous => previous + delta);
      const result = await api('ai_request', { sessionId: input.session_id, scope: { ...input.scope, include_rows: includeRows, selected_rows: selected, masked_columns: masked.split(',').map(s => s.trim()).filter(Boolean) }, question, channel });
      setReply(result); setText(result.text);
      if (result.refused) setError('공급자가 요청을 거부했습니다. 질문을 수정해 주세요.');
      else if (result.stop_reason !== 'end_turn') setError(`응답이 완료되지 않았습니다 (${result.stop_reason}). 다시 요청하세요.`);
    } catch (e) { setError(String((e as Error).message ?? e)); }
    finally { setBusy(false); }
  }
  useEffect(() => {
    let alive = true; setInserted(null);
    const list: Suggestion[] = reply && !reply.refused && reply.stop_reason === 'end_turn' ? suggestedQueries(reply.text).map((query,index)=>({id:`suggestion-${index}`,title:index===0?'추천 쿼리':`대안 ${index+1}`,query,reason:'AI 제안입니다. 검사 근거를 확인하고 새 탭에 삽입하세요.',signal:0,evidence:'아직 검사하지 않았습니다.'})) : [];
    setSuggestions(list);
    if (!list.length) return;
    void (async () => {
      const manifest = driver(session), mode = manifest?.query_language.editor_mode ?? '';
      let schema: SchemaGraph | undefined;
      if (mode === 'sql' && manifest?.capabilities.schema_graph) { try { schema = await api('schema_graph',{sessionId:session,scope:null}); } catch { /* A missing metadata response cannot increase the evidence level. */ } }
      const checked = await Promise.all(list.map(async item => {
        const parsed = await parsedQuery(item.query,mode);
        const known = parsed && schema && knownSimpleReferences(item.query,schema);
        return {...item,signal:(known?2:parsed?1:0) as Suggestion['signal'],evidence:known?(isMock?'모의 스키마의 테이블·컬럼 참조 확인 · 실제 DB 검증 아님':'편집기 파싱과 실제 조회한 스키마의 테이블·컬럼 참조 확인'):parsed?'편집기 파싱 완료 · DB 서버의 문법·실행 검증은 아님':'이 언어의 완전한 구문 검사는 아직 실행하지 않았습니다.'};
      }));
      if(alive)setSuggestions(checked);
    })().catch(() => { /* Keep unverified suggestions at level zero if the parser cannot load. */ });
    return () => { alive=false; };
  }, [reply,session]);
  async function validate(suggestion: Suggestion) {
    const manifest=driver(session); if(!manifest?.capabilities.read_only_exec)return;
    setValidating(true); setError('');
    try {
      if(isMock)throw new Error('EXPLAIN은 실제 앱의 연결에서 검증하세요. mock 결과로 검사 수준을 올리지 않습니다.');
      const text=explainText(suggestion.query,manifest.query_language.editor_mode);
      await api('execute_agent',{sessionId:session,runId:crypto.randomUUID(),req:{text,params:{},row_limit:200,graph_node_limit:200,timeout_ms:5000,read_only:true}});
      setSuggestions(previous=>previous.map(item=>item.id===suggestion.id?{...item,signal:3,evidence:'사용자가 요청한 EXPLAIN이 읽기 전용으로 통과했습니다. ANALYZE와 원문 실행은 하지 않았습니다.'}:item));
    } catch(e) { setError(String((e as Error).message??e)); }
    finally { setValidating(false); }
  }
  function closePanel() { setKey(''); setCode(''); if(loginUrl!==null){setLoginUrl(null);void api('ai_login_cancel',{}).catch(e=>setError(`로그인 취소 확인 실패: ${String((e as Error).message??e)}`));} close(); }
  return <Dialog open={open} label="AI 도우미" closeLabel="Close AI assistant" wide closeDisabled={busy||validating} onClose={closePanel} footer={<><small>제안은 자동 실행되지 않습니다. 쓰기는 미리보기·승인을 거칩니다.</small><span class="toolbar-spacer" /></>}><section class="profile-form ai-assistant">

    <p>스키마·편집기 쿼리·최근 오류·컬럼과 행 수를 Anthropic에 보냅니다. 쿼리에 직접 쓴 값도 전송될 수 있으므로 확인하세요.</p>
    <fieldset disabled={busy || validating || !ready}><legend>설정 · 저장 후 적용</legend>
      <SpringCheck checked={settings.enabled} onChange={enabled => setSettings({ ...settings, enabled })} label="AI 도우미 사용 (기본 꺼짐)" />
      <label>모델<input value={settings.model} onInput={e => setSettings({ ...settings, model: e.currentTarget.value })} /></label>
      <div class="setting-row"><span>추론 수준</span><JellyRadio aria-label="추론 수준" items={[{value:"low",label:"낮음"},{value:"medium",label:"보통"},{value:"high",label:"높음"},{value:"max",label:"최대"}]} value={settings.effort} onChange={effort => setSettings({ ...settings, effort })} disabled={busy || validating || !ready} /></div>
      <div class="setting-row"><span>인증 방식</span><JellyRadio aria-label="인증 방식" items={[{value:"api_key",label:"API 키 (OS 키체인)"},{value:"claude_login",label:"Claude 로그인 (ant CLI)"}]} value={settings.auth} onChange={auth => setSettings({ ...settings, auth: auth as AiSettings["auth"] })} disabled={busy || validating || !ready} /></div>
      {settings.auth === 'api_key' ? <label>Anthropic API 키<input type="password" autoComplete="off" value={key} placeholder={hasKey ? 'OS 키체인에 저장됨 · 변경 시 입력' : 'API 키 입력'} onInput={e => setKey(e.currentTarget.value)} /></label>
        : !hasAnt ? <small>ant CLI를 찾지 못했습니다. 설치 후 다시 여세요 (brew install anthropic/tap/ant).</small>
        : <div class="toolbar" aria-label="Claude login">
            <span role="status">{loggedIn ? `Claude 로그인됨${account ? ` · ${account}` : ''}` : 'Claude 로그인 필요'}</span>
            {loginUrl === null && <Button morph={false} onClick={() => void login('start')}>{loggedIn ? '다시 로그인' : 'Claude 로그인'}</Button>}
            {loggedIn && loginUrl === null && <Button morph={false} onClick={() => void login('logout')}>로그아웃</Button>}
          </div>}
      {settings.auth === 'claude_login' && loginUrl !== null && <div>
        <small>브라우저에서 승인한 뒤 표시된 코드를 붙여넣으세요.{loginUrl && <> 브라우저가 열리지 않으면 이 주소를 여세요: <input readOnly aria-label="Login URL" value={loginUrl} onFocus={e => e.currentTarget.select()} /></>}</small>
        <label>로그인 코드<input type="password" autoComplete="off" aria-label="Login code" value={code} onInput={e => setCode(e.currentTarget.value)} /></label>
        <div class="toolbar"><Button morph={false} disabled={!code.trim()} onClick={() => void login('submit')}>로그인 완료</Button><Button morph={false} onClick={() => void login('cancel')}>취소</Button></div>
      </div>}
      <div class="toolbar"><Button morph loading={busy && busyAction === "save"} onClick={() => void save()}>설정 저장</Button>{hasKey && <Button morph={false} onClick={() => void save(true)}>저장된 키 삭제</Button>}</div>
    </fieldset>
    {(settings.auth === 'api_key' ? !hasKey : !hasAnt || !loggedIn) ? <p>{settings.auth === 'api_key' ? '본인의 Anthropic API 키를 입력해 저장하세요. 키는 OS 키체인에 저장되며 요청 이력에는 남지 않습니다.' : 'Claude 로그인 방식을 쓰려면 위의 Claude 로그인 버튼으로 로그인하세요. 설정을 저장해야 적용됩니다.'}</p> : <fieldset disabled={busy || validating || !settings.enabled}><legend>질문</legend>
      <div class="toolbar"><Button morph={false} onClick={() => setQuestion('이 스키마에서 다음 조건의 쿼리를 작성해 주세요: ')}>쿼리 작성</Button><Button morph={false} onClick={() => setQuestion('최근 오류를 설명하고 수정 쿼리를 제안해 주세요.')}>오류 설명</Button><Button morph={false} onClick={() => setQuestion('마지막 결과를 요약해 주세요. 행이 없으면 컬럼과 행 수에 근거해 설명하세요.')}>결과 요약</Button></div>
      <textarea aria-label="AI question" value={question} onInput={e => setQuestion(e.currentTarget.value)} />
      <SpringCheck checked={includeRows} onChange={setIncludeRows} label="선택한 행 포함 · 최대 20행" />
      {includeRows && <><label>첫 번째 표의 행 번호 (1부터, 쉼표 구분)<input value={selection} placeholder="1,3,5" onInput={e => setSelection(e.currentTarget.value)} /></label><label>추가 제외 컬럼 (쉼표 구분)<input value={masked} onInput={e => setMasked(e.currentTarget.value)} /></label><small>비밀번호·토큰 등 민감 컬럼과 중첩 JSON·그래프 값은 제외합니다.</small></>}
      <Button morph loading={busy && busyAction === "request"} onClick={() => void ask()} disabled={!question.trim() || validating}>질문 보내기</Button>
    </fieldset>}
    <LatticeLoader running={busy} label="처리 중…" />{error && <p role="alert">{error}</p>}
    {reply?.context && <><CallChip icon="search" name="get_work_context" argument="스키마 · 쿼리 맥락" status="done" durationMs={reply.context_elapsed_ms} showTimer={reply.context_elapsed_ms !== undefined} expectedMs={reply.context_elapsed_ms ?? 2400} /><ToolCallGroup calls={[{id:'ai-context',name:'get_work_context',summary:'응답 요청에 실제 사용한 스키마·작업 맥락',status:'success',durationMs:reply.context_elapsed_ms,result:reply.context}]} /></>}
    {suggestions.length > 0 && <SuggestionCard suggestions={suggestions} acceptedId={inserted?.suggestionId} disabled={busy||validating} validating={validating} onValidate={driver(session)?.capabilities.read_only_exec && ['sql','cypher'].includes(driver(session)?.query_language.editor_mode??'') ? suggestion=>void validate(suggestion) : undefined} onAccept={suggestion=>{const tabId=insert(session,suggestion.query);if(tabId)setInserted({suggestionId:suggestion.id,tabId});else setError('원래 연결이 닫혔습니다. 연결 후 다시 삽입하세요.');}} onUndo={()=>{if(inserted)void undo(inserted.tabId).then(closed=>{if(closed)setInserted(null);}).catch(e=>setError(String(e)));}} />}
    {text && <pre style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>{text}</pre>}
  </section></Dialog>;
}
