import { NumberSlots } from '../ui/CodeSlots';
import { CallChip } from '../ui/CallChip';
import { callChipProps } from './callChip';
import { ToolCallGroup } from '../ui/ToolCall';
import { Button } from '../ui/Button';
import { IntegrationBeam, type IntegrationBeamHandle, type BeamNode } from '../ui/IntegrationBeam';
import { SpringCheck } from '../ui/SpringCheck';
import { AGENT_TOOLS, ToolAccessList, toolLabel } from './ToolAccessList';
import { ClientConnectGuide } from './ClientConnectGuide';
import { FuseButton } from '../ui/FuseButton';
import { GlideSelect } from '../ui/GlideSelect';
import { Bot, Database, Plus, RefreshCw, ShieldCheck, ChevronRight } from 'lucide-preact';
import { useEffect, useId, useRef, useState } from 'preact/hooks';
import { lazy, Suspense } from 'preact/compat';
import { invoke } from '@tauri-apps/api/core';
import { api, isMock } from '../api';
import type { AgentClient, AgentAudit, AgentCall, AgentPermission, DriverManifest, Profile } from '../contract';
import { Dialog } from '../Dialog';
import { decisionLabel } from '../previewCopy';
import './clients.css';

const TeamInvite = lazy(() => import('../ui/TeamInvite').then(module => ({ default: module.TeamInvite })));
const AgentTeam = lazy(() => import('../ui/AgentTeam').then(module => ({ default: module.AgentTeam })));

export { AGENT_TOOLS } from './ToolAccessList';
const permissions: AgentPermission[] = ['none', 'read', 'preview', 'write_with_approval'];
const labels = { none: '접근 없음', read: '읽기', preview: '읽기 + 미리보기', write_with_approval: '쓰기 (앱 승인)' };
const clientActive = (client: AgentClient) => !client.revoked && (!client.expires_at || Date.parse(client.expires_at) > Date.now());

export function ClientsPanel({ profiles, drivers, onClose, approvalConnection, sessions, onProfileSelect, calls }: { calls: AgentCall[]; bridgeReply: number; activity: { running: number; total: number }; queryPulse: { profileId: string; running: boolean; sequence: number } | null; approvalConnection?: string; sessions: Record<string, string>; onProfileSelect: (id: string) => void; profiles: Profile[]; drivers: DriverManifest[]; onClose: () => void }) {
  const formId = useId();
  const beam = useRef<IntegrationBeamHandle>(null);
  const editor = useRef<HTMLFormElement>(null);
  const guide = useRef<HTMLDivElement>(null);
  const [pendingRevoke, setPendingRevoke] = useState<string | null>(null);
  const [clients, setClients] = useState<AgentClient[]>([]), [audit, setAudit] = useState<AgentAudit[]>([]);
  const [editing, setEditing] = useState<AgentClient | null>(null), [token, setToken] = useState(''), [error, setError] = useState(''), [busy, setBusy] = useState(false);
  const [enabled, setEnabled] = useState<boolean | null>(null), [address, setAddress] = useState(''), [loaded, setLoaded] = useState(false);
  const [copied, setCopied] = useState('');
  const [createdClient, setCreatedClient] = useState('');
  const [guideClient, setGuideClient] = useState('');
  const [tokenClient, setTokenClient] = useState('');
  const previousCalls = useRef(new Map(calls.map(call => [call.id, call.status])));
  useEffect(() => {
    for (const call of calls) {
      if (previousCalls.current.get(call.id) === call.status || !clients.some(client => client.id === call.client_id)) continue;
      const client = `client:${call.client_id}`;
      if (call.status === 'running') beam.current?.pulse(client, 'axdb');
      else beam.current?.pulse('axdb', client, call.status === 'error' ? 'warning' : 'success');
    }
    previousCalls.current = new Map(calls.map(call => [call.id, call.status]));
  }, [calls]);
  async function load() {
    const [c, a, status] = await Promise.all([api('agent_clients_list', {}), api('agent_audit_list', {}), api('agent_bridge_status', {})]);
    setClients(c); setAudit(a); setEnabled(status); setLoaded(true);
    setAddress('');
    if (status && !isMock) {
      // Keep only the endpoint; never reuse the default client's credential for a scoped client.
      const config = await invoke<{ mcpServers: { axdb: { url: string } } }>('onboarding_mcp_config');
      setAddress(config.mcpServers.axdb.url);
    }
  }
  useEffect(() => {
    const refresh = () => { void load().catch(e => setError(String(e.message ?? e))); };
    refresh();
    window.addEventListener('axdb-bridge-change', refresh);
    return () => window.removeEventListener('axdb-bridge-change', refresh);
  }, []);
  async function task(run: () => Promise<void>) { setBusy(true); setError(''); try { await run(); await load(); } catch (e) { setError(String((e as Error).message ?? e)); } finally { setBusy(false); } }
  useEffect(() => { if (calls.length && calls.at(-1)?.status !== 'running') void load().catch(e => setError(String(e))); }, [calls]);
  useEffect(() => { if (editing) { editor.current?.scrollIntoView({ block: 'start' }); editor.current?.querySelector('input')?.focus({ preventScroll: true }); } }, [editing?.id]);
  useEffect(() => { if (guideClient) guide.current?.scrollIntoView({ block: 'nearest' }); }, [guideClient]);
  const draft = (name = ''): AgentClient => ({ id: '', name, connections: Object.fromEntries(profiles.map(p => [p.id, 'none'])), allowed_tools: ['list_connections', 'open_connection', 'close_connection', 'explore', 'run_query', 'get_result_page'], max_rows: 200, max_result_rows: 200, max_result_bytes: 262144, expires_at: null, revoked: false });
  function edit(client: AgentClient) { setCreatedClient(''); setEditing({ ...client }); setToken(''); setTokenClient(''); setCopied(''); }
  async function copy(text: string, message: string) { try { await navigator.clipboard.writeText(text); setCopied(message); } catch (e) { setError(String(e)); } }
  const empty = clients.length === 0;
  const nodes: BeamNode[] = empty ? ['Claude Code', 'Codex', 'Cursor'].map((name, i) => ({ id: `example:${name}`, label: name, side: i === 0 ? 'source' : 'target', state: 'disconnected', icon: <Bot /> })) : [
    ...profiles.filter(profile => sessions[profile.id]).map(profile => ({ id: `db:${profile.id}`, label: profile.name, side: 'source' as const, state: 'connected' as const, icon: <Database /> })),
    ...clients.map(client => ({ id: `client:${client.id}`, label: client.name, side: 'target' as const, state: !clientActive(client) ? 'denied' as const : !enabled ? 'disconnected' as const : calls.some(call => call.client_id === client.id && call.status === 'running') ? 'pending' as const : 'disconnected' as const, icon: <Bot /> }))
  ];
  const connectionMap = <IntegrationBeam nodes={nodes} controllerRef={beam} status={empty ? 'MCP · 로컬 연결' : approvalConnection ? '승인 대기 중' : enabled ? '브리지 대기 중 · 실제 요청은 감사 로그에서 확인' : 'MCP 브리지 꺼짐'} onSelect={id => {
    if (id.startsWith('example:')) edit(draft(id.slice(8)));
    else if (id.startsWith('db:')) onProfileSelect(id.slice(3));
    else { const client = clients.find(item => `client:${item.id}` === id); if (client) edit(client); }
  }} />;

  return <Dialog className="agent-clients-dialog" label="에이전트 클라이언트" closeLabel="Close agent clients" wide onClose={onClose} footer={<Button morph={false} variant="secondary" onClick={onClose}>닫기</Button>}>
    <section class="profile-form agent-clients">
      <header class="agent-clients__header">
        <div class="agent-clients__bridge">
          <SpringCheck aria-label="Client dialog MCP bridge" checked={enabled === true} disabled={busy || enabled === null} label={<>MCP 브리지 <span class="agent-clients__status" data-on={enabled === true}>{enabled === null ? '확인 중' : enabled ? '켜짐' : '꺼짐'}</span></>} onChange={value => void task(async () => { await api('agent_bridge_set', { enabled: value }); setEnabled(value); window.dispatchEvent(new CustomEvent('axdb-bridge-change', { detail: value })); })} />
          <code title={address || undefined}>{address || (enabled ? (error ? '로컬 주소 확인 필요 · 새로고침' : '로컬 주소 확인 중') : '127.0.0.1 · 켜면 주소가 표시됩니다')}</code>
        </div>
        <span class="agent-clients__count">클라이언트 <strong>{clients.length}</strong><small>유효 토큰 {clients.filter(clientActive).length}</small></span>
        <Button morph={false} variant="brand" icon={<Plus />} disabled={busy || !loaded} onClick={() => edit(draft())}>클라이언트 연결</Button>
      </header>
      <p class="agent-clients__bridge-note">브리지가 켜지면 요청을 받을 준비가 됩니다. 클라이언트가 실제로 연결했는지는 감사 로그로 확인하세요. 개별 연결 해제는 해당 클라이언트 토큰만 폐기합니다.</p>
      {error && <p role="alert">{error}</p>}
      {!loaded && !error && <p role="status">클라이언트를 불러오는 중입니다.</p>}
      {loaded && empty && !editing && <section class="agent-clients__empty">
        <div class="agent-clients__empty-map"><Suspense fallback={<span>연결 장면을 불러오는 중…</span>}><TeamInvite clientName="새 AI 에이전트" /></Suspense></div>
        <h3>MCP로 AI 에이전트를 연결하세요</h3>
        <p>Claude Code, Codex, Cursor에서 허용한 데이터베이스만 안전하게 사용합니다.</p>
        <ol class="agent-clients__steps"><li><span>1</span>브리지 켜기</li><li><span>2</span>클라이언트 생성</li><li><span>3</span>설정 복사</li></ol>
        <Button morph={false} variant="brand" icon={<Plus />} disabled={busy} onClick={() => edit(draft())}>클라이언트 생성</Button>
        <small>연결 과정 예시 · 클라이언트 생성 후 설정을 복사하세요.</small>
      </section>}
      {loaded && !empty && !editing && <section class="agent-clients__list" aria-label="등록된 클라이언트">
        <div class="agent-clients__section-heading"><div><h3>클라이언트</h3><p>카드를 선택해 데이터베이스별 접근 권한을 편집하세요.</p></div><Button morph={false} icon={<RefreshCw />} disabled={busy} onClick={() => void task(async () => {})}>새로고침</Button></div>
        <div class="agent-clients__grid">{clients.map(client => {
          const grants = [...new Set(profiles.map(profile => client.connections[profile.id] ?? (client.id === 'local' ? 'write_with_approval' : 'none')))];
          const last = audit.filter(entry => entry.client_id === client.id).sort((a, b) => b.timestamp.localeCompare(a.timestamp))[0];
          return <article class="agent-client-card" key={client.id}>
            <Button bare class="agent-client-card__edit" disabled={busy} aria-label={`권한 편집 ${client.name}`} onClick={() => edit(client)}>
              <span class="agent-client-card__title"><Bot /><strong title={client.name}>{client.name}</strong><ChevronRight /></span>
              <span class="agent-client-card__badges">{(grants.length ? grants : ['none'] as AgentPermission[]).map(grant => <span class="agent-clients__badge" data-level={grant}>{labels[grant]}</span>)}{!clientActive(client) && <span class="agent-clients__badge" data-level="denied">{client.revoked ? '폐기됨' : '만료됨'}</span>}</span>
              <span class="agent-client-card__meta">최대 {client.max_rows.toLocaleString()}행 · {client.allowed_tools.length}개 도구</span>
              <span class="agent-client-card__meta">최근 활동 · {last ? new Date(last.timestamp).toLocaleString() : '아직 요청 없음'}</span>
              <span class="agent-client-card__meta">{client.expires_at ? `만료 ${client.expires_at}` : '만료 없음'}<span class="agent-client-card__edit-label">권한 편집</span></span>
            </Button>
            <div class="agent-client-card__actions"><Button morph={false} disabled={busy} onClick={() => setGuideClient(client.id)}>연결 방법</Button><Button morph={false} disabled={busy} onClick={() => void task(async () => { const issued = (await api('agent_client_rotate', { clientId: client.id })).token; setToken(issued); setTokenClient(client.id); setGuideClient(client.id); setCopied(''); setEditing(null); })}>토큰 회전</Button><FuseButton pending={pendingRevoke === client.id} onPendingChange={pending => setPendingRevoke(pending ? client.id : null)} onUndo={() => {}} unmountAction="cancel" label="연결 해제 (토큰 폐기)" doneLabel="폐기 예약" undoLabel="실행 취소" disabled={busy || client.revoked} onCommit={() => void task(async () => { if (!window.confirm(`${client.name} 연결을 해제하고 토큰을 폐기할까요? MCP 브리지 전체는 유지됩니다.`)) return; await api('agent_client_revoke', { clientId: client.id }); setToken(''); if (tokenClient === client.id) setTokenClient(''); })} /></div>
          </article>;
        })}</div>
        <details class="agent-clients__section" open><summary>연결 맵 <small>등록 상태 · 실제 요청에만 반응</small></summary><div class="agent-clients__map">{connectionMap}</div></details>
      </section>}
      {createdClient && !editing && <section class="agent-clients__created" role="status"><div class="agent-clients__scene"><Suspense fallback={null}><TeamInvite key={createdClient} clientName={createdClient} connectedName={clients.find(client => client.name !== createdClient)?.name} joined /></Suspense></div><p><strong>{createdClient}</strong> 클라이언트가 생성되었습니다. 아래 연결 방법에서 Codex 또는 Claude Code에 설정을 등록하세요.</p></section>}
      {token && <section class="agent-clients__token" role="status"><h3>{clients.find(client=>client.id===tokenClient)?.name || '클라이언트'} 설정</h3><p>토큰은 지금만 표시됩니다. 설정 복사는 아래 연결 방법에서 할 수 있습니다.</p><p>토큰 끝자리 <NumberSlots value={token.slice(-6)} ariaLabel="Token suffix" /></p><textarea aria-label="New agent token" readOnly value={token} /><div class="agent-clients__actions"><Button morph={false} onClick={() => void copy(token, '토큰을 복사했습니다.')}>토큰 복사</Button><Button morph={false} onClick={() => { setToken(''); setTokenClient(''); setCopied(''); }}>숨기기</Button></div></section>}
      {copied && <p role="status">{copied}</p>}
      {!editing && (guideClient || (loaded && empty)) && <div ref={guide}><ClientConnectGuide client={clients.find(client=>client.id===guideClient)?.name || '새 MCP 클라이언트'} url={address} token={tokenClient === guideClient ? token : undefined} onCopy={(value, message) => void copy(value, message)} /></div>}
      {editing && <form ref={editor} id={formId} class="agent-clients__editor" onSubmit={e => { e.preventDefault(); void task(async () => { const { id, revoked: _, ...input } = editing; const result = await api('agent_client_save', { input: { ...input, ...(id ? { id } : {}) } }); setToken(result.token ?? ''); setTokenClient(result.token ? result.client.id : ''); setGuideClient(result.client.id); if (!id) setCreatedClient(result.client.name); setEditing(null); }); }}>
        <div class="agent-clients__section-heading"><div><h3>{editing.id ? '클라이언트 권한 편집' : '클라이언트 생성'}</h3><p>연결별로 필요한 권한만 허용하세요. 쓰기는 매번 앱에서 승인합니다.</p></div><ShieldCheck /></div>
        <div class="agent-clients__scene"><Suspense fallback={null}>{editing.id ? <AgentTeam /> : <TeamInvite connectedName={clients[0]?.name} />}</Suspense></div><p class="agent-clients__scene-caption">{editing.id ? '권한 등급 예시입니다. 실제 MCP 권한은 아래에서 연결별로 지정합니다.' : '연결 과정 예시입니다. 저장하면 전용 토큰을 발급합니다.'}</p>
        <div class="agent-clients__fields"><label>이름<input aria-label="Agent client name" required value={editing.name} onInput={e => setEditing({ ...editing, name: e.currentTarget.value })} /></label><label>최대 반환 행<input aria-label="Agent max rows" type="number" min="1" max="1000" required value={editing.max_rows} onInput={e => setEditing({ ...editing, max_rows: Number(e.currentTarget.value) })} /></label><label>모델 결과 행<input aria-label="Agent result rows" type="number" min="1" max="1000" required value={editing.max_result_rows ?? 200} onInput={e => setEditing({ ...editing, max_result_rows: Number(e.currentTarget.value) })} /></label><label>모델 결과 KiB<input aria-label="Agent result KiB" type="number" min="8" max="1024" required value={(editing.max_result_bytes ?? 262144) / 1024} onInput={e => setEditing({ ...editing, max_result_bytes: Number(e.currentTarget.value) * 1024 })} /></label><label>만료 (RFC3339, 비우면 없음)<input aria-label="Agent expiry" placeholder="2026-12-01T00:00:00Z" value={editing.expires_at ?? ''} onInput={e => setEditing({ ...editing, expires_at: e.currentTarget.value || null })} /></label></div>
        <h4>데이터베이스별 접근 권한</h4>
        {profiles.length ? <table><thead><tr><th>연결</th><th>권한</th></tr></thead><tbody>{profiles.map(profile => { const canRead = drivers.find(d => d.id === profile.driver_id)?.capabilities.read_only_exec; return <tr key={profile.id}><td class="agent-client-name" title={profile.name}>{profile.name}{profile.access_mode === 'read_only' && ' · 읽기 전용'}</td><td><GlideSelect ariaLabel={`Permission ${profile.name}`} value={editing.connections[profile.id] ?? (editing.id === 'local' ? 'write_with_approval' : 'none')} options={permissions.filter(p => canRead || !['read', 'preview'].includes(p)).map(permission => ({ value: permission, label: labels[permission] }))} onChange={value => setEditing({ ...editing, connections: { ...editing.connections, [profile.id]: value as AgentPermission } })} /></td></tr>; })}</tbody></table> : <p class="agent-clients__quiet">저장된 데이터베이스 연결이 없습니다. 연결을 추가한 뒤 권한을 지정하세요.</p>}
        <details class="agent-clients__section"><summary>노출 도구 <small>{editing.allowed_tools.length}개 허용</small></summary><p class="agent-clients__quiet">허용한 도구만 이 클라이언트에서 사용할 수 있습니다. 연결별 권한도 함께 적용됩니다.</p><ToolAccessList allowed={editing.allowed_tools} onChange={allowed_tools => setEditing({ ...editing, allowed_tools })} /></details>
        <div class="agent-clients__actions"><Button morph={false} type="button" disabled={busy} onClick={() => setEditing(null)}>취소</Button><Button morph={false} variant="brand" type="submit" disabled={busy}>저장</Button></div>
      </form>}
      {!editing && <details class="agent-clients__section"><summary>노출 도구 <small>{AGENT_TOOLS.length}개 · 클라이언트별 허용</small></summary><p class="agent-clients__quiet">도구 허용 목록과 데이터베이스별 권한이 함께 적용됩니다.</p><ul class="agent-clients__tool-list">{AGENT_TOOLS.map(tool => <li key={tool}><strong>{toolLabel(tool)}</strong><code>{tool}</code></li>)}</ul></details>}
      <details class="agent-clients__section" open={!!approvalConnection}><summary>감사 로그 <small>최근 50건{approvalConnection ? ' · 승인 대기 중' : ''}</small></summary>
        <div class="agent-clients__section-heading"><p>실제 MCP 요청의 권한 결정을 확인합니다.</p><Button morph={false} icon={<RefreshCw />} disabled={busy} onClick={() => void task(async () => {})}>새로고침</Button></div>
        {calls.length > 0 && <><ToolCallGroup calls={calls.map(call => ({ id: call.id, name: call.name, status: call.status, decision: call.decision ?? undefined, startedAt: call.startedAt, durationMs: call.duration_ms, summary: [clients.find(client => client.id === call.client_id)?.name, profiles.find(profile => profile.id === call.connection_id)?.name].filter(Boolean).join(' · '), result: call.status === 'running' ? undefined : { decision: call.decision ?? 'error', row_count: call.row_count }, error: call.status === 'error' ? '요청이 완료되지 않았습니다. 감사 로그의 결정을 확인하세요.' : undefined }))} /><section class="call-history" aria-label="MCP call history"><h3>MCP 호출 이력 <small>{calls.length.toLocaleString()}</small></h3><div class="call-history__list">{calls.slice(-50).reverse().map(call => <CallChip key={call.id} size={26} radius={8} {...callChipProps(call, profiles)} />)}</div></section></>}
        {audit.length ? <table><thead><tr><th>시각</th><th>클라이언트 / 연결</th><th>도구</th><th>결정</th></tr></thead><tbody>{audit.map((entry, i) => <tr key={i}><td>{new Date(entry.timestamp).toLocaleString()}</td><td>{clients.find(c => c.id === entry.client_id)?.name ?? entry.client_id}<br />{profiles.find(p => p.id === entry.connection_id)?.name ?? entry.connection_id}</td><td title={entry.statement_hash ?? ''}>{entry.tool}</td><td><span class="agent-clients__badge" data-level={entry.decision}>{entry.decision === 'pending' ? '승인 대기' : decisionLabel(entry.decision)}</span></td></tr>)}</tbody></table> : <p class="agent-clients__quiet">{calls.length ? '저장된 감사 로그가 없습니다.' : '아직 MCP 요청이 없습니다'}</p>}
      </details>
    </section>
  </Dialog>;
}
