import { useEffect, useState } from 'preact/hooks';
import { Channel } from '@tauri-apps/api/core';
import { api } from '../api';
import { Dialog } from '../Dialog';
import type { Table } from '../contract';
import { selectRows, suggestedQuery, type AiReply, type AiSettings, type WorkInput } from './types';
export function AssistantPanel({ work, table, insert, close }: { work: () => WorkInput | null; table: () => Table | undefined; insert: (session: string, text: string) => void; close: () => void }) {
  const [settings, setSettings] = useState<AiSettings>({ enabled: false, model: 'claude-opus-5-5', effort: 'medium', auth: 'api_key' });
  const [hasAnt, setHasAnt] = useState(false), [loggedIn, setLoggedIn] = useState(false), [loginUrl, setLoginUrl] = useState<string | null>(null), [code, setCode] = useState(''), [account, setAccount] = useState('');
  const [hasKey, setHasKey] = useState(false), [key, setKey] = useState(''), [ready, setReady] = useState(false);
  const [question, setQuestion] = useState('이 스키마에서 필요한 쿼리를 작성해 주세요.');
  const [includeRows, setIncludeRows] = useState(false), [selection, setSelection] = useState(''), [masked, setMasked] = useState('');
  const [text, setText] = useState(''), [reply, setReply] = useState<AiReply | null>(null), [session, setSession] = useState('');
  const [busy, setBusy] = useState(false), [error, setError] = useState('');
  useEffect(() => { let alive = true; void api('ai_settings', {}).then(s => { if (alive) { setSettings(s.settings); setHasKey(s.has_key); setHasAnt(s.has_ant_cli); setReady(true); } }).catch(e => { if (alive) setError(String(e.message ?? e)); }); return () => { alive = false; }; }, []);
  async function save(remove = false) {
    setBusy(true); setError('');
    try { await api('ai_configure', { settings, apiKey: remove ? '' : key || null }); const s = await api('ai_settings', {}); setSettings(s.settings); setHasKey(s.has_key); setHasAnt(s.has_ant_cli); setKey(''); }
    catch (e) { setError(String((e as Error).message ?? e)); }
    finally { setBusy(false); }
  }
  async function refreshLogin() { try { const s = await api('ai_login_status', {}); setHasAnt(s.installed); setLoggedIn(s.logged_in); } catch (e) { setError(String((e as Error).message ?? e)); } }
  useEffect(() => { if (settings.auth === 'claude_login') void refreshLogin(); }, [settings.auth]);
  async function login(step: 'start' | 'submit' | 'cancel' | 'logout') {
    setBusy(true); setError('');
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
    setBusy(true); setSession(input.session_id);
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
  const query = reply && !reply.refused && reply.stop_reason === 'end_turn' ? suggestedQuery(reply.text) : null;
  return <Dialog label="AI 도우미" onClose={() => { if (!busy) close(); }}><section class="profile-form" style={{ width: 'min(820px, 90vw)', maxHeight: '85vh', overflow: 'auto' }}>
    <div class="section-title"><h2>AI 도우미</h2><button disabled={busy} aria-label="Close AI assistant" onClick={close}>×</button></div>
    <p>스키마·편집기 쿼리·최근 오류·컬럼과 행 수를 Anthropic에 보냅니다. 쿼리에 직접 쓴 값도 전송될 수 있으므로 확인하세요.</p>
    <fieldset disabled={busy || !ready}><legend>설정 · 저장 후 적용</legend>
      <label><input type="checkbox" checked={settings.enabled} onChange={e => setSettings({ ...settings, enabled: e.currentTarget.checked })} />AI 도우미 사용 (기본 꺼짐)</label>
      <label>모델<input value={settings.model} onInput={e => setSettings({ ...settings, model: e.currentTarget.value })} /></label>
      <label>추론 수준<select value={settings.effort} onChange={e => setSettings({ ...settings, effort: e.currentTarget.value })}>{['low', 'medium', 'high', 'max'].map(v => <option key={v}>{v}</option>)}</select></label>
      <label>인증 방식<select value={settings.auth} onChange={e => setSettings({ ...settings, auth: e.currentTarget.value as AiSettings['auth'] })}><option value="api_key">API 키 (OS 키체인)</option><option value="claude_login">Claude 로그인 (ant CLI)</option></select></label>
      {settings.auth === 'api_key' ? <label>Anthropic API 키<input type="password" autoComplete="off" value={key} placeholder={hasKey ? 'OS 키체인에 저장됨 · 변경 시 입력' : 'API 키 입력'} onInput={e => setKey(e.currentTarget.value)} /></label>
        : !hasAnt ? <small>ant CLI를 찾지 못했습니다. 설치 후 다시 여세요 (brew install anthropic/tap/ant).</small>
        : <div class="toolbar" aria-label="Claude login">
            <span role="status">{loggedIn ? `Claude 로그인됨${account ? ` · ${account}` : ''}` : 'Claude 로그인 필요'}</span>
            {loginUrl === null && <button onClick={() => void login('start')}>{loggedIn ? '다시 로그인' : 'Claude 로그인'}</button>}
            {loggedIn && loginUrl === null && <button onClick={() => void login('logout')}>로그아웃</button>}
          </div>}
      {settings.auth === 'claude_login' && loginUrl !== null && <div>
        <small>브라우저에서 승인한 뒤 표시된 코드를 붙여넣으세요.{loginUrl && <> 브라우저가 열리지 않으면 이 주소를 여세요: <input readOnly aria-label="Login URL" value={loginUrl} onFocus={e => e.currentTarget.select()} /></>}</small>
        <label>로그인 코드<input type="password" autoComplete="off" aria-label="Login code" value={code} onInput={e => setCode(e.currentTarget.value)} /></label>
        <div class="toolbar"><button disabled={!code.trim()} onClick={() => void login('submit')}>로그인 완료</button><button onClick={() => void login('cancel')}>취소</button></div>
      </div>}
      <div class="toolbar"><button onClick={() => void save()}>설정 저장</button>{hasKey && <button onClick={() => void save(true)}>저장된 키 삭제</button>}</div>
    </fieldset>
    {(settings.auth === 'api_key' ? !hasKey : !hasAnt || !loggedIn) ? <p>{settings.auth === 'api_key' ? '본인의 Anthropic API 키를 입력해 저장하세요. 키는 OS 키체인에 저장되며 요청 이력에는 남지 않습니다.' : 'Claude 로그인 방식을 쓰려면 위의 Claude 로그인 버튼으로 로그인하세요. 설정을 저장해야 적용됩니다.'}</p> : <fieldset disabled={busy || !settings.enabled}><legend>질문</legend>
      <div class="toolbar"><button onClick={() => setQuestion('이 스키마에서 다음 조건의 쿼리를 작성해 주세요: ')}>쿼리 작성</button><button onClick={() => setQuestion('최근 오류를 설명하고 수정 쿼리를 제안해 주세요.')}>오류 설명</button><button onClick={() => setQuestion('마지막 결과를 요약해 주세요. 행이 없으면 컬럼과 행 수에 근거해 설명하세요.')}>결과 요약</button></div>
      <textarea aria-label="AI question" value={question} onInput={e => setQuestion(e.currentTarget.value)} />
      <label><input type="checkbox" checked={includeRows} onChange={e => setIncludeRows(e.currentTarget.checked)} />선택한 행 포함 · 최대 20행</label>
      {includeRows && <><label>첫 번째 표의 행 번호 (1부터, 쉼표 구분)<input value={selection} placeholder="1,3,5" onInput={e => setSelection(e.currentTarget.value)} /></label><label>추가 제외 컬럼 (쉼표 구분)<input value={masked} onInput={e => setMasked(e.currentTarget.value)} /></label><small>비밀번호·토큰 등 민감 컬럼과 중첩 JSON·그래프 값은 제외합니다.</small></>}
      <button onClick={() => void ask()} disabled={!question.trim()}>질문 보내기</button>
    </fieldset>}
    {busy && <p role="status">처리 중…</p>}{error && <p role="alert">{error}</p>}
    {text && <pre style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>{text}</pre>}
    {query && <button disabled={busy} onClick={() => { insert(session, query); close(); }}>새 쿼리 탭에 삽입</button>}
    <small>제안은 자동 실행되지 않습니다. 쓰기는 기존 미리보기·승인 절차를 따릅니다.</small>
  </section></Dialog>;
}
