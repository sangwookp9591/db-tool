import { Button } from '../ui/Button';
import { CallChip } from '../ui/CallChip';
import { SpringCheck } from '../ui/SpringCheck';
import { Bot, X } from 'lucide-preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import { listen } from '@tauri-apps/api/event';
import { api, isMock } from '../api';
import { Dialog } from '../Dialog';
import { createRegistry, type AgentHost } from './registry';
import type { ChangePreview, ResultSet, AgentCall } from '../contract';
import { ClientsPanel } from './ClientsPanel';
import { expose, register } from './webmcp';
export function AgentPanel({ host, registrationKey, ready, queryPulse, openRequest, onProfileSelect }: { queryPulse: {profileId:string;running:boolean;sequence:number} | null; openRequest:number; onProfileSelect:(id:string)=>void; ready: boolean; host: Omit<AgentHost, 'approve' | 'activity'>; registrationKey: string }) {
  const current = useRef(host); current.current = host;
  const [activity, setActivity] = useState({ running: 0, total: 0 });
  const [calls, setCalls] = useState<AgentCall[]>([]);
  const [bridgeReply, setBridgeReply] = useState(0);
  const [clientsOpen, setClientsOpen] = useState(false);
  useEffect(() => { if (openRequest) setClientsOpen(true); }, [openRequest]);
  const [enabled, setEnabled] = useState(false);
  const [error, setError] = useState('');
  const [approval, setApproval] = useState<{ text: string; connection: string; preview?: ChangePreview; finish: (allow: boolean) => void } | null>(null);
  const pending = useRef<((allow: boolean) => void) | null>(null);
  const registry = useRef<ReturnType<typeof createRegistry> | null>(null);
  if (!registry.current) registry.current = createRegistry({
    snapshot: () => current.current.snapshot(), invoke: (name, args) => current.current.invoke(name, args),
    open: id => current.current.open(id), close: id => current.current.close(id), openTab: (id, text) => current.current.openTab(id, text), uiState: () => current.current.uiState(), results: r => current.current.results(r), history: () => current.current.history(),
    workContext: () => current.current.workContext?.() ?? null,
    activity: (running, total) => setActivity({ running, total }),
    approve: request => new Promise(resolve => {
      if (request.signal.aborted || pending.current) { resolve(false); return; }
      const finish = (allow: boolean) => { request.signal.removeEventListener('abort', abort); pending.current = null; setApproval(null); resolve(allow && !request.signal.aborted); };
      const abort = () => finish(false); pending.current = finish;
      request.signal.addEventListener('abort', abort, { once: true }); setApproval({ ...request, finish });
    })
  });
  const agent = useRef<ReturnType<typeof expose> | null>(null);
  useEffect(() => { agent.current = expose(registry.current!); return () => { pending.current?.(false); }; }, []);
  useEffect(() => register(agent.current!), [registrationKey]);
  useEffect(() => {
    if (isMock || !ready) return;
    let alive = true;
    void api('agent_bridge_status', {}).then(value => { if (alive) setEnabled(value); }).catch(e => { if (alive) setError(String(e)); });
    const requests = new Map<string, AbortController>();
    const subscription = listen<{ request_id: string; method: string; params: { name?: string; arguments?: unknown; text?: string; connection?: string; preview?: ChangePreview; results?: ResultSet[] }; timeout_ms: number }>('agent-bridge-request', event => {
      const { request_id, method, params, timeout_ms } = event.payload;
      const controller = new AbortController(); requests.set(request_id, controller);
      const timer = setTimeout(() => controller.abort(), timeout_ms);
      void (async () => {
        let result: unknown;
        if (method === 'approval') result = { approved: await new Promise<boolean>(resolve => {
          if (controller.signal.aborted || pending.current) { resolve(false); return; }
          const finish = (allow: boolean) => { controller.signal.removeEventListener('abort', abort); pending.current = null; setApproval(null); resolve(allow && !controller.signal.aborted); };
          const abort = () => finish(false); pending.current = finish; controller.signal.addEventListener('abort', abort, { once: true });
          setApproval({ text: params.text ?? '', connection: params.connection ?? '', preview: params.preview, finish });
        }) };
        else if (method === 'context') result = current.current.workContext?.() ?? null;
        else if (method === 'results') { setBridgeReply(value => value + 1); current.current.results(params.results ?? []); current.current.history(); result = {}; }
        else result = method === 'tools/list' ? { tools: agent.current!.listTools() } : await agent.current!.callTool(params.name ?? '', params.arguments ?? {}, { signal: controller.signal });
        await api('agent_bridge_reply', { requestId: request_id, result });
      })().catch(e => { if (alive) setError(String(e)); }).finally(() => { clearTimeout(timer); requests.delete(request_id); });
    });
    const activityEvents = listen<AgentCall>('agent-call', event => { if (alive) setCalls(previous => { const existing = previous.find(call => call.id === event.payload.id); const next = { ...event.payload, startedAt: existing?.startedAt ?? Date.now() }; return [...previous.filter(call => call.id !== next.id), next].slice(-50); }); });
    const cancellation = listen<{ request_id: string }>('agent-bridge-cancel', event => requests.get(event.payload.request_id)?.abort());
    void Promise.all([subscription, cancellation, activityEvents]).then(async () => { if (alive) await api('agent_bridge_ready', {}); }).catch(e => { if (alive) setError(String(e)); });
    return () => { alive = false; requests.forEach(c => c.abort()); void subscription.then(off => off()); void cancellation.then(off => off()); void activityEvents.then(off => off()); };
  }, [ready]);
  return <><span class={`agent-activity ${activity.running ? 'running' : ''}`} role="status" aria-label="에이전트 활동" data-testid="agent-activity">에이전트 활동 · {activity.total}{activity.running > 0 && ` · ${activity.running} 실행 중`}</span>
    <SpringCheck aria-label="Localhost MCP bridge" data-testid="agent-bridge-toggle" checked={enabled} onChange={value => { void api('agent_bridge_set', { enabled: value }).then(() => { setEnabled(value); setError(''); }).catch(e => setError(String(e))); }} label="MCP bridge" />{error && <span role="alert">{error}</span>}
    {calls.length > 0 && <div className="agent-call-feed" aria-label="MCP activity">{calls.slice(-8).map(call => <CallChip key={call.id} icon={call.name.includes('query') ? 'terminal' : call.name.includes('schema') ? 'search' : 'file'} name={call.name} argument={[host.snapshot().profiles.find(profile => profile.id === call.connection_id)?.name ?? call.connection_id, call.row_count != null ? `${call.row_count} rows` : ''].filter(Boolean).join(' · ')} status={call.status === 'running' ? 'running' : call.status === 'error' ? 'error' : 'done'} durationMs={call.duration_ms} expectedMs={Math.max(900, call.duration_ms ?? 2400)} />)}</div>}
    <Button morph={false} class="agent-rail-button" aria-label="에이전트 클라이언트" onClick={() => setClientsOpen(true)}><Bot /><span>에이전트 클라이언트</span></Button>{clientsOpen && <ClientsPanel calls={calls} bridgeReply={bridgeReply} activity={activity} queryPulse={queryPulse} approvalConnection={approval?.connection} sessions={host.snapshot().sessions} onProfileSelect={id => { setClientsOpen(false); onProfileSelect(id); }} profiles={host.snapshot().profiles} drivers={host.snapshot().drivers} onClose={() => setClientsOpen(false)} />}
    {approval && <Dialog label="에이전트 요청 승인" onClose={() => approval.finish(false)} footer={<><Button morph={false} aria-label="Approve agent query" data-testid="agent-approve" onClick={() => approval.finish(true)}>승인</Button><Button morph={false} autoFocus aria-label="Deny agent query" data-testid="agent-deny" onClick={() => approval.finish(false)}>거부</Button></>}><section class="profile-form" data-testid="agent-approval"><p>연결: {approval.connection}</p><pre>{approval.text}</pre>{approval.preview && <><p>변경 미리보기 · 아직 커밋하지 않았습니다</p><pre>{JSON.stringify(approval.preview, null, 2)}</pre></>}</section></Dialog>}
  </>;
}
