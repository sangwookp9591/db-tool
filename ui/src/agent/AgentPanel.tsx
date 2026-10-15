import { useEffect, useRef, useState } from 'preact/hooks';
import { listen } from '@tauri-apps/api/event';
import { api, isMock } from '../api';
import { Dialog } from '../Dialog';
import { createRegistry, type AgentHost } from './registry';
import { expose, register } from './webmcp';
export function AgentPanel({ host, registrationKey, ready }: { ready: boolean; host: Omit<AgentHost, 'approve' | 'activity'>; registrationKey: string }) {
  const current = useRef(host); current.current = host;
  const [activity, setActivity] = useState({ running: 0, total: 0 });
  const [enabled, setEnabled] = useState(false);
  const [error, setError] = useState('');
  const [approval, setApproval] = useState<{ text: string; connection: string; finish: (allow: boolean) => void } | null>(null);
  const pending = useRef<((allow: boolean) => void) | null>(null);
  const registry = useRef<ReturnType<typeof createRegistry> | null>(null);
  if (!registry.current) registry.current = createRegistry({
    snapshot: () => current.current.snapshot(), invoke: (name, args) => current.current.invoke(name, args),
    open: id => current.current.open(id), close: id => current.current.close(id), openTab: (id, text) => current.current.openTab(id, text), uiState: () => current.current.uiState(), results: r => current.current.results(r), history: () => current.current.history(),
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
    const subscription = listen<{ request_id: string; method: string; params: { name?: string; arguments?: unknown }; timeout_ms: number }>('agent-bridge-request', event => {
      const { request_id, method, params, timeout_ms } = event.payload;
      const controller = new AbortController(); requests.set(request_id, controller);
      const timer = setTimeout(() => controller.abort(), timeout_ms);
      void (async () => {
        const result = method === 'tools/list' ? { tools: agent.current!.listTools() } : await agent.current!.callTool(params.name ?? '', params.arguments ?? {}, { signal: controller.signal });
        await api('agent_bridge_reply', { requestId: request_id, result });
      })().catch(e => { if (alive) setError(String(e)); }).finally(() => { clearTimeout(timer); requests.delete(request_id); });
    });
    const cancellation = listen<{ request_id: string }>('agent-bridge-cancel', event => requests.get(event.payload.request_id)?.abort());
    void Promise.all([subscription, cancellation]).then(async () => { if (alive) await api('agent_bridge_ready', {}); }).catch(e => { if (alive) setError(String(e)); });
    return () => { alive = false; requests.forEach(c => c.abort()); void subscription.then(off => off()); void cancellation.then(off => off()); };
  }, [ready]);
  return <><span class={`agent-activity ${activity.running ? 'running' : ''}`} role="status" aria-label="에이전트 활동" data-testid="agent-activity">에이전트 활동 · {activity.total}{activity.running > 0 && ` · ${activity.running} 실행 중`}</span>
    <label><input type="checkbox" aria-label="Localhost MCP bridge" data-testid="agent-bridge-toggle" checked={enabled} onChange={e => { const value = e.currentTarget.checked; void api('agent_bridge_set', { enabled: value }).then(() => { setEnabled(value); setError(''); }).catch(e => setError(String(e))); }} />MCP bridge</label>{error && <span role="alert">{error}</span>}
    {approval && <Dialog label="에이전트 요청 승인" onClose={() => approval.finish(false)}><section class="profile-form" data-testid="agent-approval"><h2>에이전트 요청</h2><p>연결: {approval.connection}</p><pre>{approval.text}</pre><div class="toolbar"><button aria-label="Approve agent query" data-testid="agent-approve" onClick={() => approval.finish(true)}>승인</button><button autoFocus aria-label="Deny agent query" data-testid="agent-deny" onClick={() => approval.finish(false)}>거부</button></div></section></Dialog>}
  </>;
}
