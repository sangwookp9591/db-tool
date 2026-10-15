import { useEffect, useRef, useState } from 'preact/hooks';
import type { DriverManifest, Profile, HistoryEntry, ExplorerNode, NodeAction, DbError, ResultSet } from './contract';
import { api, isMock } from './api';
import { Connections } from './Connections';
import { Dialog } from './Dialog';
import { QueryTab, BrowseTab } from './QueryTab';
import { ErrorBox } from './Results';
import { normalizeError } from './values';
import type { EditorHandle } from './Editor';
import { painted, start } from './perf';
import { AgentPanel } from './agent/AgentPanel';
interface Tab { id: string; profileId: string; kind: 'query' | 'browse'; text: string; nodeId?: string; title: string; runId: string | null }
export function App() {
  const [drivers, setDrivers] = useState<DriverManifest[]>([]);
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [sessions, setSessions] = useState<Record<string, string>>({});
  const [dead, setDead] = useState<Set<string>>(new Set());
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [active, setActive] = useState('');
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [error, setError] = useState<DbError | null>(null);
  const [graphLimit, setGraphLimit] = useState(300);
  const [loaded, setLoaded] = useState(false);
  const lastResults = useRef<ResultSet[]>([]);
  const credentialReply = useRef<((secret: string | null) => void) | null>(null);
  const handles = useRef(new Map<string, EditorHandle>());
  const activeRef = useRef(active); activeRef.current = active;
  const tabsRef = useRef(tabs); tabsRef.current = tabs;
  const sessionsRef = useRef(sessions); sessionsRef.current = sessions;
  const [connectPrompt, setConnectPrompt] = useState<Profile | null>(null);
  const [credential, setCredential] = useState('');
  function report(e: unknown, profileId?: string) { const normalized = normalizeError(e); setError(normalized); if (normalized.kind === 'Connection' && profileId) setDead(previous => new Set([...previous, profileId])); }
  async function refreshProfiles() { setProfiles(await api('profiles_list', {})); }
  async function refreshHistory() { try { setHistory(await api('history_list', { limit: 100 })); } catch (e) { report(e); } }
  async function load() {
    setLoaded(false); setError(null);
    try { const [d, p, h] = await Promise.all([api('drivers_list', {}), api('profiles_list', {}), api('history_list', { limit: 100 })]); setDrivers(d); setProfiles(p); setHistory(h); }
    catch (e) { report(e); } finally { setLoaded(true); }
  }
  useEffect(() => { void load(); }, []);
  function newQuery(profile: Profile, text?: string) {
    const manifest = drivers.find(d => d.id === profile.driver_id); if (!manifest) return;
    const since = start('new-query-tab');
    const tab: Tab = { id: crypto.randomUUID(), profileId: profile.id, kind: 'query', text: text ?? manifest.default_query, title: `Query ${tabsRef.current.filter(t => t.profileId === profile.id && t.kind === 'query').length + 1}`, runId: null };
    tabsRef.current = [...tabsRef.current, tab]; setTabs(tabsRef.current); activeRef.current = tab.id; setActive(tab.id); void painted('new-query-tab', since); return tab.id;
  }
  function onRun(tabId: string, runId: string | null) { if (runId) setError(null); setTabs(prev => prev.map(t => t.id === tabId ? { ...t, runId } : t)); }
  async function closeTab(tab: Tab) {
    if (tab.runId) { try { await api('cancel', { runId: tab.runId }); } catch (e) { report(e, tab.profileId); } }
    const rest = tabsRef.current.filter(t => t.id !== tab.id); setTabs(rest); if (active === tab.id) setActive(rest.at(-1)?.id ?? '');
  }
  async function disconnect(profile: Profile) {
    const id = sessionsRef.current[profile.id];
    for (const tab of tabsRef.current.filter(t => t.profileId === profile.id && t.runId)) await api('cancel', { runId: tab.runId! });
    if (id) await api('session_close', { sessionId: id });
    const nextSessions = { ...sessionsRef.current }; delete nextSessions[profile.id]; sessionsRef.current = nextSessions; setSessions(nextSessions);
    const rest = tabsRef.current.filter(t => t.profileId !== profile.id); setTabs(rest); if (!rest.some(t => t.id === active)) setActive(rest.at(-1)?.id ?? '');
  }
  async function connect(profile: Profile, secret?: string | null) {
    const manifest = drivers.find(d => d.id === profile.driver_id);
    if (secret === undefined && !profile.save_secret && manifest?.connection_fields.some(f => f.secret) && !isMock) { if (credentialReply.current) throw new Error('Another connection is awaiting credentials'); setConnectPrompt(profile); secret = await new Promise<string | null>(resolve => { credentialReply.current = resolve; }); if (secret === null) throw new Error('Connection cancelled by user'); }
    if (sessionsRef.current[profile.id]) await disconnect(profile);
    const id = await api('session_open', { profileId: profile.id, secret: secret ?? null });
    sessionsRef.current = { ...sessionsRef.current, [profile.id]: id }; setSessions(sessionsRef.current);
    setDead(previous => { const next = new Set(previous); next.delete(profile.id); return next; }); setError(null);
    newQuery(profile); return id;
  }
  function action(profile: Profile, node: ExplorerNode, selected: NodeAction) {
    if (selected.kind === 'Browse') {
      const tab: Tab = { id: crypto.randomUUID(), profileId: profile.id, kind: 'browse', nodeId: node.id, text: '', title: node.label, runId: null };
      setTabs(prev => [...prev, tab]); setActive(tab.id);
    } else if ('OpenQuery' in selected.kind) newQuery(profile, selected.kind.OpenQuery.text);
    else {
      const target = tabsRef.current.find(t => t.id === active && t.kind === 'query' && t.profileId === profile.id);
      if (target && handles.current.has(target.id)) handles.current.get(target.id)!.insert(selected.kind.Insert.text);
      else newQuery(profile, selected.kind.Insert.text);
    }
  }
  function fromHistory(entry: HistoryEntry) {
    const target = tabsRef.current.find(t => t.id === active && t.kind === 'query');
    if (target && handles.current.has(target.id)) { handles.current.get(target.id)!.replace(entry.text); return; }
    const profile = profiles.find(p => p.name === entry.profile_name && sessions[p.id]); if (profile) newQuery(profile, entry.text);
    else setError({ kind: 'Connection', message: 'Connect a profile and open a query tab to load history.', detail: null, position: null });
  }
  return <div class="app"><header class="app-header"><h1>db-tool</h1><span>DATABASE WORKSPACE</span>{isMock && <span class="mock-badge">MOCK · no database traffic</span>}<AgentPanel registrationKey={JSON.stringify([drivers, profiles.map(p => [p.id, p.driver_id, p.name]), sessions])} host={{
      snapshot: () => ({ drivers, profiles, sessions: sessionsRef.current }), invoke: api,
      open: async profileId => { const profile = profiles.find(p => p.id === profileId); if (!profile) throw new Error('Saved connection not found'); return sessionsRef.current[profileId] ?? await connect(profile); },
      close: async profileId => { const profile = profiles.find(p => p.id === profileId); if (profile) await disconnect(profile); },
      openTab: (profileId, text) => { const profile = profiles.find(p => p.id === profileId); if (!profile) throw new Error('Connection not found'); return newQuery(profile, text); },
      uiState: () => { const tab = tabsRef.current.find(t => t.id === activeRef.current); return { connections: profiles.filter(p => sessionsRef.current[p.id]).map(p => ({ id: p.id, name: p.name, session_id: sessionsRef.current[p.id] })), active_tab: tab ? { id: tab.id, connection: profiles.find(p => p.id === tab.profileId)?.name, text: handles.current.get(tab.id)?.text() ?? tab.text } : null, last_result: lastResults.current.map(r => ({ title: r.title, row_count: r.row_count, truncated: r.truncated, views: ['table', 'graph', 'documents', 'message'].filter(k => r[k as keyof ResultSet] !== null) })) }; },
      results: results => { lastResults.current = results; }, history: () => { void refreshHistory(); }
    }} /><label>Graph node limit <input aria-label="Graph node limit" type="number" min="1" max="5000" value={graphLimit} onChange={e => setGraphLimit(Math.max(1, Math.min(5000, Math.trunc(Number(e.currentTarget.value)) || 300)))} /></label></header>
    <div class="workspace"><Connections drivers={drivers} profiles={profiles} sessions={sessions} dead={dead} onProfiles={refreshProfiles} onConnect={async (profile, secret) => { await connect(profile, secret); }} onDisconnect={disconnect} onQuery={newQuery} onAction={action} onError={report} />
      <main><div class="workspace-error"><ErrorBox error={error} />{error && <button onClick={() => setError(null)} aria-label="Dismiss error">×</button>}</div>
        {!loaded ? <p class="empty-workspace">Loading workspace…</p> : !drivers.length ? <div class="empty-workspace"><p>No drivers loaded.</p><button onClick={() => void load()}>Retry</button></div> : <><div class="workspace-tabs" role="tablist" aria-label="Workspace tabs">{tabs.map(tab => { const p = profiles.find(p => p.id === tab.profileId); return <div class={`workspace-tab ${active === tab.id ? 'active' : ''}`} key={tab.id}><button role="tab" aria-selected={active === tab.id} onClick={() => { const since = start('tab-switch'); setActive(tab.id); void painted('tab-switch', since); }}><span class="run-indicator" aria-label={tab.runId ? "Running" : undefined}>{tab.runId ? "●" : ""}</span>{p?.name} / {tab.title}</button><button onClick={() => void closeTab(tab)} aria-label={`Close ${tab.title}`}>×</button></div>; })}</div>
          {!tabs.length && <div class="empty-workspace"><h2>Query your data.</h2><p>Connect a database on the left, then open a query or browse a table.</p>{isMock && <p>Demo queries: <code>SELECT large;</code> · <code>SELECT error;</code> · <code>SELECT slow;</code><br />Set row limit to 100000 to inspect the large fixture.</p>}</div>}
          {tabs.map(tab => { const p = profiles.find(p => p.id === tab.profileId); const manifest = drivers.find(d => d.id === p?.driver_id); const sessionId = sessions[tab.profileId]; return manifest && sessionId && <section class="tab-body" hidden={active !== tab.id} key={tab.id} aria-label={tab.title}>{tab.kind === 'query' ? <QueryTab onResults={results => { lastResults.current = results; }} active={active === tab.id} initialText={tab.text} sessionId={sessionId} manifest={manifest} graphLimit={graphLimit} onRun={runId => onRun(tab.id, runId)} onReady={handle => { if (handle) handles.current.set(tab.id, handle); else handles.current.delete(tab.id); }} onHistory={() => void refreshHistory()} onError={e => report(e, tab.profileId)} /> : <BrowseTab onResults={results => { lastResults.current = results; }} active={active === tab.id} sessionId={sessionId} nodeId={tab.nodeId!} manifest={manifest} graphLimit={graphLimit} onRun={runId => onRun(tab.id, runId)} onError={e => report(e, tab.profileId)} />}</section>; })}</>}
        <section class="history" aria-label="Execution history"><div class="section-title"><strong>HISTORY <small>{history.length}</small></strong><button onClick={() => void refreshHistory()}>Refresh</button></div><div class="history-list">{history.map((entry, i) => <button key={i} onClick={() => fromHistory(entry)} title={entry.text}><time>{new Date(entry.timestamp).toLocaleTimeString()}</time><span>{entry.profile_name} · {entry.source ?? "ui"}</span><code>{entry.text}</code><small class={entry.error ? 'invalid' : ''}>{entry.error ?? `${entry.row_count.toLocaleString()} rows · ${entry.elapsed_ms} ms`}</small></button>)}{!history.length && <p class="muted">No executions yet.</p>}</div></section>
      </main>
    </div>
    {connectPrompt && <Dialog label="Session credential" onClose={() => { setCredential(''); setConnectPrompt(null); credentialReply.current?.(null); credentialReply.current = null; }}><form class="profile-form" onSubmit={e => { e.preventDefault(); const secret = String(new FormData(e.currentTarget).get('credential') ?? ''); setCredential(''); setConnectPrompt(null); credentialReply.current?.(secret); credentialReply.current = null; }}><h2>Connect · {connectPrompt.name}</h2><label>Password<input name="credential" type="password" autoFocus autoComplete="off" value={credential} onInput={e => setCredential(e.currentTarget.value)} /></label><p class="muted">Used for this session only.</p><div class="toolbar"><button type="submit">Connect</button><button type="button" onClick={() => { setCredential(''); setConnectPrompt(null); credentialReply.current?.(null); credentialReply.current = null; }}>Cancel</button></div></form></Dialog>}
  </div>;
}
