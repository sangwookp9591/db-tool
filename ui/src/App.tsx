import { useUpdater } from './update/useUpdater';
import { UpdateBadge, UpdateDialog, UpdateSettings } from './update/Updater';
import { lazy, Suspense } from 'preact/compat';
import { DriversSkeleton } from './drivers/DriverGallery';
const DriversPanel = lazy(() => import('./drivers/DriversPanel').then(module => ({ default: module.DriversPanel })));
import { DriverRequired } from './drivers/DriverRequired';
import { Onboarding, useOnboarding } from './onboarding/Onboarding';
import { Button } from './ui/Button';
import { IntegrationBeam, type IntegrationBeamHandle } from './ui/IntegrationBeam';
import { CommandPalette } from './ui/CommandPalette';
import { ToastStack, type ToastItem } from './ui/ToastStack';
import { Tooltip } from './ui/Tooltip';
import { SlidingIndicator } from './ui/SlidingIndicator';
import { SwipeRow } from './ui/SwipeRow';
import { FuseButton } from './ui/FuseButton';
import { WakeSlider } from './ui/WakeSlider';
import { GlideSelect } from './ui/GlideSelect';
import { Bot, Search, Database, Download, History, Sparkles, Settings2, Plus, X, ShieldCheck, Terminal, PanelLeftClose, RefreshCw, SquarePlus, Copy, Trash2 } from 'lucide-preact';
import { savedTheme, setTheme, type Theme } from './theme';
import { useEffect, useRef, useState } from 'preact/hooks';
import type { DriverManifest, Profile, HistoryEntry, ExplorerNode, NodeAction, DbError, ResultSet } from './contract';
import { api, isMock } from './api';
import { Connections } from './Connections';
import { Dialog } from './Dialog';
import { NavigationTab } from './NavigationTab';
import { ErrorBanner } from './ErrorBanner';
import { shortServerVersion, serverVersionTitle } from './serverVersion';
import { normalizeError } from './values';
import type { EditorHandle } from './Editor';
import { painted, start } from './perf';
import { AssistantPanel } from './ai/AssistantPanel';
import type { WorkInput } from './ai/types';
import { AgentPanel } from './agent/AgentPanel';
const MAX_TOASTS = 4, TOAST_MS = 4500, SIDEBAR_MIN = 220;
interface Tab { aria?: string; dirty?: boolean; aiSuggested?: boolean; id: string; profileId: string; kind: 'query' | 'browse' | 'schema'; scope?: string | null; text: string; nodeId?: string; title: string; runId: string | null }
export function App() {
  const [driversOpen, setDriversOpen] = useState(false);
  const [requestedDriver, setRequestedDriver] = useState<string | undefined>();
  const [drivers, setDrivers] = useState<DriverManifest[]>([]);
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [sessions, setSessions] = useState<Record<string, string>>({});
  const [serverInfo, setServerInfo] = useState<Record<string,string|null|undefined>>({});
  const [dead, setDead] = useState<Set<string>>(new Set());
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [active, setActive] = useState('');
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [error, setError] = useState<DbError | null>(null);
  const errorProfile = useRef<string | null>(null);
  const [graphLimit, setGraphLimit] = useState(300);
  const [theme, updateTheme] = useState<Theme>(savedTheme);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const updater = useUpdater();
  const [historyOpen, setHistoryOpen] = useState(false);
  const [historyActions, setHistoryActions] = useState<string | null>(null);
  const [historyDelete, setHistoryDelete] = useState<HistoryEntry | null>(null);
  const [deletePending, setDeletePending] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [sidebarWidth, setSidebarWidth] = useState(270);
  const [viewportWidth, setViewportWidth] = useState(innerWidth);
  const [addRequest, setAddRequest] = useState(0);
  const [editRequest, setEditRequest] = useState<{ id: string; sequence: number } | null>(null);
  const [agentRequest, setAgentRequest] = useState(0);
  const [queryPulse, setQueryPulse] = useState<{ profileId: string; running: boolean; sequence: number } | null>(null);
  const emptyBeam = useRef<IntegrationBeamHandle>(null);
  const tabStrip = useRef<HTMLDivElement>(null);
  const [tabResults, setTabResults] = useState<Record<string, ResultSet | undefined>>({});
  const [loaded, setLoaded] = useState(false);
  const [commandOpen, setCommandOpen] = useState(false);
  const [searchTip, setSearchTip] = useState(false);
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const [explorerNodes, setExplorerNodes] = useState<Record<string, ExplorerNode[]>>({});
  function notify(message: string, tone: ToastItem['tone'] = 'success') {
    const id = crypto.randomUUID();
    // Cap the stack: older success/info toasts go first, an unread error is only dropped when nothing else is left to drop.
    setToasts(previous => { const next = [...previous, { id, message, tone }]; while (next.length > MAX_TOASTS) { const at = next.findIndex(item => item.tone !== 'error'); next.splice(at < 0 ? 0 : at, 1); } return next; });
  }
  const lastResults = useRef<ResultSet[]>([]);
  const lastResultsTab = useRef('');
  const lastRuns = useRef(new Map<string, string>());
  const [assistantOpen, setAssistantOpen] = useState(false);
  const [assistantMounted, setAssistantMounted] = useState(false);
  const credentialReply = useRef<((secret: string | null) => void) | null>(null);
  const handles = useRef(new Map<string, EditorHandle>());
  const activeRef = useRef(active); activeRef.current = active;
  const tabsRef = useRef(tabs); tabsRef.current = tabs;
  const sessionsRef = useRef(sessions); sessionsRef.current = sessions;
  const [connectPrompt, setConnectPrompt] = useState<Profile | null>(null);
  const [credential, setCredential] = useState('');
  // One surface per error: connection-level errors get the global banner, everything else a persistent toast. Errors raised inside a tab (reportInTab) are shown by that tab's own banner.
  function report(e: unknown, profileId?: string) { const normalized = normalizeError(e); if (normalized.kind === 'Connection') { setError(normalized); errorProfile.current = profileId ?? null; } else notify(normalized.message, "error"); if (normalized.kind === 'Connection' && profileId) setDead(previous => new Set([...previous, profileId])); }
  function reportInTab(e: unknown, profileId: string) { if (normalizeError(e).kind === 'Connection') report(e, profileId); }
  async function refreshProfiles() { setProfiles(await api('profiles_list', {})); }
  async function refreshHistory() { try { setHistory(await api('history_list', { limit: 100 })); } catch (e) { report(e); } }
  async function load() {
    setLoaded(false); setError(null);
    try { const [d, p, h] = await Promise.all([api('drivers_list', {}), api('profiles_list', {}), api('history_list', { limit: 100 })]); setDrivers(d); setProfiles(p); setHistory(h); }
    catch (e) { report(e); } finally { setLoaded(true); }
  }
  useEffect(() => { void load(); }, []);
  useEffect(() => {
    const show = (event: Event) => { setRequestedDriver((event as CustomEvent<string | undefined>).detail); setDriversOpen(true); };
    const changed = () => { void api('drivers_list', {}).then(setDrivers).catch(report); };
    window.addEventListener('axdb-open-drivers', show);
    window.addEventListener('axdb-drivers-changed', changed);
    return () => { window.removeEventListener('axdb-open-drivers', show); window.removeEventListener('axdb-drivers-changed', changed); };
  }, []);
  function newQuery(profile: Profile, text?: string, nodeId?: string, aiSuggested = false) {
    const manifest = drivers.find(d => d.id === profile.driver_id); if (!manifest) return;
    const since = start('new-query-tab'), number = tabsRef.current.filter(t => t.profileId === profile.id && t.kind === 'query').length + 1;
    const tab: Tab = { id: crypto.randomUUID(), profileId: profile.id, kind: 'query', aiSuggested, nodeId, text: text ?? manifest.default_query, title: `쿼리 ${number}`, aria: `Query ${number}`, runId: null };
    tabsRef.current = [...tabsRef.current, tab]; setTabs(tabsRef.current); activeRef.current = tab.id; setActive(tab.id); void painted('new-query-tab', since); return tab.id;
  }
  function onRun(tabId: string, runId: string | null) { const tab = tabsRef.current.find(item => item.id === tabId); if (tab) setQueryPulse(previous => ({ profileId: tab.profileId, running: !!runId, sequence: (previous?.sequence ?? 0) + 1 })); if (runId) { setError(null); lastRuns.current.set(tabId, runId); } setTabs(prev => prev.map(t => t.id === tabId ? { ...t, runId } : t)); }
  async function closeTab(tab: Tab) {
    if (tab.runId) { try { await api('cancel', { runId: tab.runId }); } catch (e) { report(e, tab.profileId); } }
    lastRuns.current.delete(tab.id);
    setTabResults(previous => { const next={...previous}; delete next[tab.id]; return next; });
    if(lastResultsTab.current===tab.id){lastResults.current=[];lastResultsTab.current='';}
    const rest = tabsRef.current.filter(t => t.id !== tab.id); setTabs(rest); if (active === tab.id) setActive(rest.at(-1)?.id ?? '');
  }
  async function disconnect(profile: Profile) {
    const id = sessionsRef.current[profile.id];
    if (errorProfile.current === profile.id) { errorProfile.current = null; setError(null); }
    for (const tab of tabsRef.current.filter(t => t.profileId === profile.id && t.runId)) await api('cancel', { runId: tab.runId! });
    if (id) await api('session_close', { sessionId: id });
    setServerInfo(previous=>{const next={...previous};delete next[profile.id];return next;});
    const nextSessions = { ...sessionsRef.current }; delete nextSessions[profile.id]; sessionsRef.current = nextSessions; setSessions(nextSessions);
    const closedIds = new Set(tabsRef.current.filter(tab=>tab.profileId===profile.id).map(tab=>tab.id));
    for (const id of closedIds) lastRuns.current.delete(id);
    setTabResults(previous=>Object.fromEntries(Object.entries(previous).filter(([id])=>!closedIds.has(id))));
    if(closedIds.has(lastResultsTab.current)){lastResults.current=[];lastResultsTab.current='';}
    const rest = tabsRef.current.filter(t => t.profileId !== profile.id); setTabs(rest); if (!rest.some(t => t.id === active)) setActive(rest.at(-1)?.id ?? '');
  }
  async function connect(profile: Profile, secret?: string | null) {
    const manifest = drivers.find(d => d.id === profile.driver_id);
    if (!manifest) { setRequestedDriver(profile.driver_id); setDriversOpen(true); throw new Error('드라이버를 먼저 설치하세요.'); }
    if (secret === undefined && !profile.save_secret && manifest?.connection_fields.some(f => f.secret) && !isMock) { if (credentialReply.current) throw new Error('다른 연결이 인증 정보를 기다리고 있습니다.'); setConnectPrompt(profile); secret = await new Promise<string | null>(resolve => { credentialReply.current = resolve; }); if (secret === null) throw new Error('사용자가 연결을 취소했습니다.'); }
    if (sessionsRef.current[profile.id]) await disconnect(profile);
    const id = await api('session_open', { profileId: profile.id, secret: secret ?? null });
    notify(`${profile.name} 연결됨`);
    sessionsRef.current = { ...sessionsRef.current, [profile.id]: id }; setSessions(sessionsRef.current);
    setServerInfo(previous=>({...previous,[profile.id]:undefined}));
    void api('session_info',{sessionId:id}).then(info=>{if(sessionsRef.current[profile.id]===id)setServerInfo(previous=>({...previous,[profile.id]:info}));}).catch(()=>{if(sessionsRef.current[profile.id]===id)setServerInfo(previous=>({...previous,[profile.id]:null}));});
    setDead(previous => { const next = new Set(previous); next.delete(profile.id); return next; }); setError(null);
    newQuery(profile); return id;
  }
  function action(profile: Profile, node: ExplorerNode, selected: NodeAction) {
    if (selected.kind === 'Browse') {
      const tab: Tab = { id: crypto.randomUUID(), profileId: profile.id, kind: 'browse', nodeId: node.id, text: '', title: node.label, runId: null };
      setTabs(prev => [...prev, tab]); setActive(tab.id);
    } else if ('OpenSchemaGraph' in selected.kind) {
      const tab: Tab={id:crypto.randomUUID(),profileId:profile.id,kind:'schema',scope:selected.kind.OpenSchemaGraph.scope,text:'',title:`다이어그램 · ${node.label}`,aria:`Diagram · ${node.label}`,runId:null};setTabs(prev=>[...prev,tab]);setActive(tab.id);
    } else if ('OpenQuery' in selected.kind) newQuery(profile, selected.kind.OpenQuery.text,node.id);
    else {
      const target = tabsRef.current.find(t => t.id === active && t.kind === 'query' && t.profileId === profile.id);
      if (target && handles.current.has(target.id)) handles.current.get(target.id)!.insert(selected.kind.Insert.text);
      else newQuery(profile, selected.kind.Insert.text);
    }
  }
  function fromHistory(entry: HistoryEntry, newTab = false) {
    const target = tabsRef.current.find(t => t.id === active && t.kind === 'query');
    if (!newTab && target && handles.current.has(target.id)) { handles.current.get(target.id)!.replace(entry.text); return; }
    const profile = profiles.find(p => p.name === entry.profile_name && sessions[p.id]); if (profile) newQuery(profile, entry.text);
    else setError({ kind: 'Connection', message: '이력을 불러오려면 연결한 뒤 쿼리 탭을 여세요.', detail: null, position: null });
  }
  function workContext(): WorkInput | null {
    const tab = tabsRef.current.find(t => t.id === activeRef.current);
    const session = tab && sessionsRef.current[tab.profileId];
    return tab && session ? { session_id: session, scope: { text: handles.current.get(tab.id)?.text() ?? tab.text, run: lastRuns.current.get(tab.id) } } : null;
  }
  const activeTab = tabs.find(tab => tab.id === active);
  const statusResult = tabResults[active];
  const activeProfile = profiles.find(profile => profile.id === activeTab?.profileId);
  const activeDriver = drivers.find(driver => driver.id === activeProfile?.driver_id);
  // At the minimum window the sidebar may take at most 40% so the editor and results stay usable.
  const sidebarMax = Math.max(SIDEBAR_MIN, Math.min(420, Math.floor(viewportWidth * .4)));
  useEffect(() => { const resize = () => setViewportWidth(innerWidth); addEventListener('resize', resize); return () => removeEventListener('resize', resize); }, []);
  function resizeSidebar(event: PointerEvent) {
    event.preventDefault(); const handle = event.currentTarget as HTMLElement; handle.setPointerCapture(event.pointerId);
    const x = event.clientX, width = sidebarWidth;
    const move = (e: PointerEvent) => setSidebarWidth(Math.max(SIDEBAR_MIN, Math.min(sidebarMax, width + e.clientX - x)));
    const end = () => { handle.removeEventListener('pointermove', move); handle.removeEventListener('pointerup', end); handle.removeEventListener('pointercancel', end); };
    handle.addEventListener('pointermove', move); handle.addEventListener('pointerup', end); handle.addEventListener('pointercancel', end);
  }

  useEffect(() => {
    if (!loaded || tabs.length) return;
    let second = 0;
    const first = requestAnimationFrame(() => { second = requestAnimationFrame(() => emptyBeam.current?.pulse('databases', 'axdb')); });
    return () => { cancelAnimationFrame(first); cancelAnimationFrame(second); };
  }, [loaded, tabs.length === 0]);
  // Keep the active tab inside the (scrollbar-less) strip whenever it changes or a tab is added.
  useEffect(() => {
    const strip = tabStrip.current, tab = strip?.querySelector<HTMLElement>('.workspace-tab.active');
    if (!strip || !tab) return;
    const a = tab.getBoundingClientRect(), s = strip.getBoundingClientRect();
    if (a.left < s.left) strip.scrollLeft += a.left - s.left; else if (a.right > s.right) strip.scrollLeft += a.right - s.right;
  }, [active, tabs.length, viewportWidth]);
  // Tab strip: edge fade hints (data-edge) and settle on a tab boundary once scrolling stops, so no half tab is left at the left edge.
  useEffect(() => {
    const strip = tabStrip.current;
    if (!strip) return;
    let settle = 0;
    const edges = () => { const max = strip.scrollWidth - strip.clientWidth; strip.dataset.edge = [strip.scrollLeft > 1 && 'start', strip.scrollLeft < max - 1 && 'end'].filter(Boolean).join(' '); };
    // Size tabs so a whole number of them fills the strip: every scroll stop (including the last) is then a tab boundary, with no end gap or left fragment.
    const align = () => {
      const count = strip.children.length, width = strip.clientWidth, fit = Math.max(1, Math.floor(width / 190));
      const before = strip.style.getPropertyValue('--tab-w');
      if (count * 224 <= width) strip.style.removeProperty('--tab-w'); else strip.style.setProperty('--tab-w', `${width / fit}px`);
      if (strip.style.getPropertyValue('--tab-w') !== before) { const tab = strip.querySelector<HTMLElement>('.workspace-tab.active'); if (tab) { const a = tab.getBoundingClientRect(), s = strip.getBoundingClientRect(); if (a.left < s.left - 1) strip.scrollLeft += a.left - s.left; else if (a.right > s.right + 1) strip.scrollLeft += a.right - s.right; } }
      edges();
    };
    const snap = () => {
      const max = strip.scrollWidth - strip.clientWidth;
      if (strip.scrollLeft <= 1 || strip.scrollLeft >= max - 1) return;
      const bounds = strip.getBoundingClientRect(), current = strip.querySelector<HTMLElement>('.workspace-tab.active')?.getBoundingClientRect();
      const active = current && current.left >= bounds.left - 1 && current.right <= bounds.right + 1 ? current : undefined;
      let best: number | undefined;
      for (const child of Array.from(strip.children)) {
        const d = child.getBoundingClientRect().left - bounds.left;
        if (active && (active.left - d < bounds.left - 1 || active.right - d > bounds.right + 1)) continue;
        if (best === undefined || Math.abs(d) < Math.abs(best)) best = d;
      }
      if (best !== undefined && Math.abs(best) > 1) strip.scrollBy({ left: best, behavior: 'smooth' });
    };
    const onScroll = () => { edges(); clearTimeout(settle); settle = window.setTimeout(snap, 140); };
    strip.addEventListener('scroll', onScroll, { passive: true });
    const observer = new ResizeObserver(align); observer.observe(strip);
    align();
    return () => { clearTimeout(settle); strip.removeEventListener('scroll', onScroll); observer.disconnect(); };
  }, [loaded, drivers.length, tabs.length]);
  function editConnection(id: string) { setSidebarOpen(true); setEditRequest(previous => ({id, sequence:(previous?.sequence ?? 0) + 1})); }
  function addConnection() { if (!drivers.length) { setDriversOpen(true); return; } setDriversOpen(false); setSidebarOpen(true); setAddRequest(value => value + 1); }
  async function sampleQuery() {
    const profile = profiles.find(p => sessions[p.id]) ?? profiles[0];
    if (!profile) { addConnection(); return; }
    try { if (sessions[profile.id]) newQuery(profile); else await connect(profile); } catch (e) { report(e, profile.id); }
  }
  const onboarding = useOnboarding(loaded, report);
  const errorBanner = <div class="workspace-error"><ErrorBanner scope="global" error={error} onDismiss={() => { errorProfile.current = null; setError(null); }} /></div>;
  return <div class="app" data-sidebar-open={sidebarOpen} style={{ '--sidebar-width': `${Math.min(sidebarWidth, sidebarMax)}px` }}>

    <UpdateDialog updater={updater} /><div class="workspace"><nav class="rail" aria-label="Workspace tools"><div class="brand-mark" title="AxDB"><span class="axdb-mark" aria-hidden="true" /><span class="sr-only">AxDB</span></div><Button morph={false} aria-label="Connections panel" title="연결 · ⌘K" aria-pressed={sidebarOpen} onClick={() => setSidebarOpen(value => !value)}><Database /></Button><Button morph={false} aria-label="Execution history panel" title="실행 이력" aria-pressed={historyOpen} onClick={() => setHistoryOpen(value => !value)}><History /></Button><Tooltip open={searchTip} onOpenChange={setSearchTip} content="작업공간 검색 · ⌘K"><Button morph={false} aria-label="Command palette" onClick={() => setCommandOpen(true)}><Search /></Button></Tooltip><div class="rail-agent-slot" /><Button morph={false} aria-label="드라이버" title="드라이버 설치 및 관리" aria-pressed={driversOpen} onClick={() => setDriversOpen(value => !value)}><Download /></Button><Button morph={false} aria-label="AI 도우미" title="AI 도우미" onClick={() => {setAssistantMounted(true);setAssistantOpen(true);}}><Sparkles /></Button><span class="rail-spacer" /><Button morph={false} aria-label="Workspace settings" title="설정" aria-pressed={settingsOpen} onClick={() => setSettingsOpen(value => !value)}><Settings2 /></Button></nav>
    {settingsOpen && <section class="settings-popover" aria-label="Workspace settings"><div class="section-title"><h2>작업공간 설정</h2><Button morph={false} aria-label="Close settings" onClick={() => setSettingsOpen(false)}><X /></Button></div><label>테마<GlideSelect ariaLabel="Appearance" fitTrigger value={theme} options={[{ value: "midnight", label: "미드나이트" }, { value: "daylight", label: "데이라이트" }, { value: "system", label: "시스템" }]} onChange={value => { const next = value as Theme; updateTheme(next); setTheme(next); }} /></label><WakeSlider value={graphLimit} onChange={setGraphLimit} min={1} max={5000} step={1} aria-label="Graph node limit slider" label="노드 한도" /><label>그래프 노드 한도 <input aria-label="Graph node limit" type="number" min="1" max="5000" value={graphLimit} onChange={e => setGraphLimit(Math.max(1, Math.min(5000, Math.trunc(Number(e.currentTarget.value)) || 300)))} /></label><Button morph={false} onClick={() => { setSettingsOpen(false); onboarding.show(); }}>사용법 다시 보기</Button><UpdateSettings updater={updater} /><small>테마는 이 기기에 저장됩니다.</small></section>}
    <div class="sidebar-container" hidden={!sidebarOpen}><Connections editRequest={editRequest} onNodes={(profileId, nodes) => setExplorerNodes(previous => ({ ...previous, [profileId]: nodes }))} addRequest={addRequest} drivers={drivers} profiles={profiles} sessions={sessions} dead={dead} onProfiles={refreshProfiles} onConnect={async (profile, secret) => { await connect(profile, secret); }} onDisconnect={disconnect} onQuery={newQuery} onAction={action} onError={report} /></div><div hidden={!sidebarOpen} class="sidebar-splitter" role="separator" tabIndex={0} aria-label="Resize connections sidebar" aria-orientation="vertical" aria-valuemin={SIDEBAR_MIN} aria-valuemax={sidebarMax} aria-valuenow={Math.min(sidebarWidth, sidebarMax)} onPointerDown={resizeSidebar} onKeyDown={e => { if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) { e.preventDefault(); setSidebarWidth(value => e.key === 'Home' ? SIDEBAR_MIN : e.key === 'End' ? sidebarMax : Math.max(SIDEBAR_MIN, Math.min(sidebarMax, value + (e.key === 'ArrowLeft' ? -16 : 16)))); } }} />
      <main><div hidden={!driversOpen} style={{height:"100%",minHeight:0}}>{driversOpen && <Suspense fallback={<DriversSkeleton />}><DriversPanel requestedId={requestedDriver} onClose={() => setDriversOpen(false)} onAddConnection={addConnection} /></Suspense>}</div><div hidden={driversOpen} style={{height:"100%",display:driversOpen ? "none" : "contents"}}>{(!loaded || !drivers.length || !tabs.length) && errorBanner}
        {!loaded ? <p class="empty-workspace">작업공간 불러오는 중…</p> : !drivers.length ? <div class="empty-workspace"><h2>드라이버부터 설치하세요</h2><p>필요한 데이터베이스 드라이버를 설치하면 연결을 추가할 수 있습니다.</p><DriverRequired /></div> : <>{tabs.length > 0 && <SlidingIndicator value={active}><div class="workspace-tabs" role="tablist" aria-label="Workspace tabs" ref={tabStrip} onWheel={e => { if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) e.currentTarget.scrollLeft += e.deltaY; }}>{tabs.map(tab => { const p = profiles.find(p => p.id === tab.profileId); const manifest = drivers.find(d => d.id === p?.driver_id); return <div class={`workspace-tab ${active === tab.id ? 'active' : ''}`} data-paradigm={manifest?.paradigm} data-indicator-scope key={tab.id}><Button morph={false} role="tab" title={`${p?.name} / ${tab.title}`} aria-selected={active === tab.id} onClick={() => { const since = start('tab-switch'); setActive(tab.id); void painted('tab-switch', since); }}><span class="paradigm-dot" /><span class="run-indicator" aria-label={tab.runId ? "Running" : undefined}></span><span class="tab-title">{p?.name} / {tab.title}</span>{tab.dirty && <span class="dirty-dot" aria-label="Modified query" />}{p?.access_mode === 'read_only' && <small class="read-only-badge">읽기 전용</small>}</Button><Button morph={false} onClick={() => void closeTab(tab)} aria-label={`Close ${tab.aria ?? tab.title}`}><X /></Button></div>; })}</div></SlidingIndicator>}
          {tabs.length > 0 && errorBanner}
          {!tabs.length && <div class="empty-workspace"><IntegrationBeam controllerRef={emptyBeam} nodes={[{id:"databases",label:"데이터베이스",side:"source",state:"disconnected",icon:<Database />},{id:"agents",label:"MCP 클라이언트",side:"target",state:"disconnected",icon:<Bot />}]} status="연결할 준비가 됐습니다" onSelect={id => id === "databases" ? addConnection() : setAgentRequest(value=>value+1)} /><h2>데이터를 한 곳에서.</h2><p>데이터베이스를 연결하고 구조를 탐색하며 쿼리를 실행하세요.</p><div class="toolbar"><Button morph={false} variant="primary" onClick={addConnection}><Plus />연결 추가</Button><Button morph={false} onClick={() => void sampleQuery()}><Terminal />샘플 쿼리</Button></div><div class="shortcut-hints"><span><kbd>⌘ K</kbd> 작업공간 검색</span><span><kbd>⌘ ↵</kbd> 쿼리 실행</span></div>{isMock && <p class="mock-badge">모의 · DB 연결 없음</p>}</div>}

          {tabs.map(tab => { const p = profiles.find(p => p.id === tab.profileId); const manifest = drivers.find(d => d.id === p?.driver_id); const sessionId = sessions[tab.profileId]; return manifest && sessionId && <section class="tab-body" hidden={active !== tab.id} key={tab.id} aria-label={tab.aria ?? tab.title}>{<NavigationTab onDirty={dirty => setTabs(previous => previous.map(item => item.id === tab.id && item.dirty !== dirty ? { ...item, dirty } : item))} aiSuggested={tab.aiSuggested} readOnly={p?.access_mode === 'read_only'} kind={tab.kind} scope={tab.scope} nodeId={tab.nodeId} title={tab.title} initialText={tab.text} onResults={results => { lastResults.current = results; lastResultsTab.current = tab.id; setTabResults(previous => ({ ...previous, [tab.id]: results[0] })); if (results[0]) notify(`${results[0].row_count.toLocaleString()}행 · ${results[0].elapsed_ms.toLocaleString()} ms`); }} active={active===tab.id} sessionId={sessionId} manifest={manifest} graphLimit={graphLimit} onRun={runId=>onRun(tab.id,runId)} onReady={handle=>{if(handle)handles.current.set(tab.id,handle);else handles.current.delete(tab.id);}} onHistory={()=>void refreshHistory()} onError={e=>reportInTab(e,tab.profileId)}/>}</section>; })}</>}
        <section class="history" hidden={!historyOpen} aria-label="Execution history"><div class="section-title"><strong>이력 <small>{history.length.toLocaleString()}</small></strong><span><Button morph={false} onClick={() => void refreshHistory()} aria-label="Refresh history"><RefreshCw /></Button><Button morph={false} onClick={() => setHistoryOpen(false)} aria-label="Close history"><PanelLeftClose /></Button></span></div><div class="history-list">{history.map((entry, i) => <SwipeRow key={`${entry.timestamp}:${entry.profile_name}:${i}`} label={`History ${i + 1}`} open={historyActions === `${i}`} onOpenChange={open => setHistoryActions(open ? `${i}` : null)} actions={[{id:'open',label:'새 탭에서 열기',icon:<SquarePlus />,onSelect:()=>fromHistory(entry,true)},{id:'copy',label:'쿼리 복사',icon:<Copy />,onSelect:()=>{void navigator.clipboard.writeText(entry.text).then(() => notify('쿼리를 복사했습니다')).catch(report);}},{id:'delete',label:'이력 삭제',icon:<Trash2 />,onSelect:()=>{setHistoryDelete(entry);setDeletePending(false);}}]}><Button bare class="history-entry" onClick={() => fromHistory(entry)} title={entry.text}><time title={new Date(entry.timestamp).toLocaleString()}>{new Date(entry.timestamp).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}</time><span>{entry.profile_name} · {entry.source === 'agent' ? '에이전트' : 'UI'}</span><code>{entry.text}</code><small class={entry.error ? 'invalid' : ''}>{entry.error ?? `${entry.row_count.toLocaleString()}행 · ${entry.elapsed_ms.toLocaleString()} ms`}</small></Button></SwipeRow>)}{!history.length && <p class="muted">실행 이력이 없습니다.</p>}</div></section>
      </div></main>
    </div>
    <footer class="statusbar" aria-label="Workspace status"><span class={`status-dot ${activeProfile && sessions[activeProfile.id] && !dead.has(activeProfile.id) ? 'connected' : ''}`} /><span class="status-name" title={activeProfile?.name}>{activeProfile?.name ?? '연결을 선택하세요'}</span><span class="status-driver" title={activeProfile ? serverVersionTitle(serverInfo[activeProfile.id]) ?? (serverInfo[activeProfile.id]===null?"서버 정보를 읽을 수 없습니다":"서버 정보 확인 중") : undefined}>{shortServerVersion(activeProfile && serverInfo[activeProfile.id], activeDriver?.display_name || "AxDB")}</span>{activeProfile?.access_mode === 'read_only' && <span class="read-only-badge"><ShieldCheck />읽기 전용</span>}<span class="status-spacer" /><UpdateBadge updater={updater} />{toasts.length > 0 && <Button morph={false} class="notification-count" aria-label="Show notifications" onClick={() => setSidebarOpen(true)}>알림 {toasts.length.toLocaleString()}</Button>}{statusResult && <span>{statusResult.elapsed_ms.toLocaleString()} ms · {statusResult.row_count.toLocaleString()}행</span>}<div class="agent-controls"><AgentPanel queryPulse={queryPulse} openRequest={agentRequest} onProfileSelect={editConnection} ready={loaded} registrationKey={JSON.stringify([drivers, profiles.map(p => [p.id, p.driver_id, p.name, p.access_mode]), sessions])} host={{
      workContext,
      snapshot: () => ({ drivers, profiles, sessions: sessionsRef.current }), invoke: api,
      open: async profileId => { const profile = profiles.find(p => p.id === profileId); if (!profile) throw new Error('Saved connection not found'); return sessionsRef.current[profileId] ?? await connect(profile); },
      close: async profileId => { const profile = profiles.find(p => p.id === profileId); if (profile) await disconnect(profile); },
      openTab: (profileId, text) => { const profile = profiles.find(p => p.id === profileId); if (!profile) throw new Error('Connection not found'); return newQuery(profile, text); },
      uiState: () => { const tab = tabsRef.current.find(t => t.id === activeRef.current); return { connections: profiles.filter(p => sessionsRef.current[p.id]).map(p => ({ id: p.id, name: p.name, session_id: sessionsRef.current[p.id] })), active_tab: tab ? { id: tab.id, connection: profiles.find(p => p.id === tab.profileId)?.name, text: handles.current.get(tab.id)?.text() ?? tab.text } : null, last_result: lastResults.current.map(r => ({ title: r.title, row_count: r.row_count, truncated: r.truncated, views: ['table', 'graph', 'documents', 'message'].filter(k => r[k as keyof ResultSet] !== null) })) }; },
      results: results => { lastResults.current = results; lastResultsTab.current = ''; }, history: () => { void refreshHistory(); }
    }} /></div>{isMock && <span class="mock-badge">모의</span>}</footer>
    {assistantMounted && <AssistantPanel open={assistantOpen} driver={session => { const profile=profiles.find(profile=>sessionsRef.current[profile.id]===session); return drivers.find(driver=>driver.id===profile?.driver_id); }} undo={async id => { const tab=tabsRef.current.find(tab=>tab.id===id); if(!tab)return true; const edited=(handles.current.get(id)?.text()??tab.text)!==tab.text; const executed=lastRuns.current.has(id); if((edited||executed)&&!confirm(executed?"탭만 닫습니다. 이미 실행된 DB 변경은 되돌리지 않습니다. 닫을까요?":"수정한 쿼리 탭을 닫고 삽입을 취소할까요?"))return false; await closeTab(tab); return true; }} work={workContext} table={() => lastResultsTab.current === activeRef.current ? lastResults.current.find(r => r.table)?.table ?? undefined : undefined} insert={(session, text) => { const profile = profiles.find(p => sessionsRef.current[p.id] === session); if (profile) return newQuery(profile, text, undefined, true); }} close={() => setAssistantOpen(false)} />}
    {onboarding.open && <Onboarding onClose={onboarding.close} onConnect={addConnection} onQuery={() => void sampleQuery()} onExplore={() => setSidebarOpen(true)} onClients={() => setAgentRequest(value => value + 1)} onAssistant={() => { setAssistantMounted(true); setAssistantOpen(true); }} />}
    <ToastStack autoDismissMs={TOAST_MS} items={toasts} onDismiss={id => setToasts(previous => previous.filter(item => item.id !== id))} />
    <CommandPalette open={commandOpen} onOpenChange={setCommandOpen} items={[
      {id:'onboarding',label:'사용법 다시 보기',group:'작업공간',keywords:'onboarding help tutorial 가이드',onSelect:onboarding.show},
      {id:'add',label:'연결 추가',group:'작업공간',keywords:'add connection',onSelect:addConnection},
      {id:'history',label:'실행 이력',group:'작업공간',onSelect:()=>setHistoryOpen(true)},
      ...profiles.map(profile => ({id:`connection:${profile.id}`,label:profile.name,kind:'connection' as const,group:'연결',keywords:drivers.find(driver => driver.id === profile.driver_id)?.display_name,onSelect:()=>{if(sessions[profile.id])newQuery(profile);else void connect(profile).catch(e=>report(e,profile.id));}})),
      ...profiles.filter(profile=>sessions[profile.id]).flatMap(profile=>(explorerNodes[profile.id]??[]).filter(node=>node.actions.length).map(node=>({id:`node:${profile.id}:${node.id}`,label:node.label,group:profile.name,keywords:node.detail??'',onSelect:()=>action(profile,node,node.actions[0])}))),
      ...history.slice(0,30).map((entry,i)=>({id:`history:${i}`,label:entry.text,kind:'query' as const,group:`최근 · ${entry.profile_name}`,onSelect:()=>fromHistory(entry,true)}))
    ]} />
    {historyDelete && <Dialog title="이력 삭제" label="이력 삭제" footer={<><FuseButton pending={deletePending} onPendingChange={setDeletePending} unmountAction="cancel" onUndo={() => setHistoryDelete(null)} onCommit={() => { const entry = historyDelete; setHistoryDelete(null); void api('history_delete', { timestamp: String(entry.timestamp), profileName: entry.profile_name, text: entry.text }).then(refreshHistory).catch(report); }} label="이력 삭제" doneLabel="삭제 대기 중" /><Button morph={false} onClick={() => { setHistoryDelete(null); setDeletePending(false); }}>취소</Button></>} onClose={() => { setHistoryDelete(null); setDeletePending(false); }}><section class="profile-form"><pre>{historyDelete.text}</pre><p>5초의 실행 취소 시간이 끝나면 삭제합니다.</p></section></Dialog>}
    {connectPrompt && <Dialog title="연결 인증" label="Session credential" footer={<><Button morph={false} variant="brand" type="submit" form="session-credential-form">연결</Button><Button morph={false} type="button" onClick={() => { setCredential(''); setConnectPrompt(null); credentialReply.current?.(null); credentialReply.current = null; }}>취소</Button></>} onClose={() => { setCredential(''); setConnectPrompt(null); credentialReply.current?.(null); credentialReply.current = null; }}><form id="session-credential-form" class="profile-form" onSubmit={e => { e.preventDefault(); const secret = String(new FormData(e.currentTarget).get('credential') ?? ''); setCredential(''); setConnectPrompt(null); credentialReply.current?.(secret); credentialReply.current = null; }}><h2>연결 · {connectPrompt.name}</h2><label>비밀번호<input name="credential" type="password" autoFocus autoComplete="off" value={credential} onInput={e => setCredential(e.currentTarget.value)} /></label><p class="muted">이 세션에서만 사용합니다.</p></form></Dialog>}
  </div>;
}
