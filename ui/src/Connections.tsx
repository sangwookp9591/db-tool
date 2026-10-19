import { DriverLogo } from './drivers/DriverLogo';
import { lazy, Suspense } from 'preact/compat';
const ProfileForm = lazy(() => import('./ProfileForm').then(m => ({ default: m.ProfileForm })));
import { DriverRequired, openDrivers } from './drivers/DriverRequired';
import './sidebar.css';
import { loadSidebarPreferences, sidebarRatio, SIDEBAR_STORAGE_KEY } from './sidebar';
import { Button } from './ui/Button';
import { SpotlightCard } from './ui/SpotlightCard';
import { FuseButton } from './ui/FuseButton';
import { Database, Plus, X, Search, MoreHorizontal, PlugZap, Unplug, Pencil, FlaskConical, ChevronRight, ChevronDown, Minus, RefreshCw, Folder, Table2, Columns3, Key, ListFilter, Layers, Network, FileCode2 } from 'lucide-preact';
import { useEffect, useId, useRef, useState } from 'preact/hooks';
import type { DriverManifest, Profile, ExplorerNode, NodeAction } from './contract';
import { api, isMock } from './api';
import { Skeleton } from './ui/Skeleton';
import { BranchGroup, BranchItem, reachIndex, reachState, toggleOpen, treeKey } from './ui/BranchedMenu';
interface Props { editRequest?: {id:string;sequence:number} | null; onNodes?: (profileId: string, nodes: ExplorerNode[]) => void; addRequest?: number; drivers: DriverManifest[]; profiles: Profile[]; sessions: Record<string, string>; dead: Set<string>; onProfiles: () => Promise<void>; onConnect: (profile: Profile, secret?: string | null) => Promise<void>; onDisconnect: (profile: Profile) => Promise<void>; onQuery: (profile: Profile, text?: string) => void; onAction: (profile: Profile, node: ExplorerNode, action: NodeAction) => void; onError: (error: unknown, profileId?: string) => void }
export function Connections(props: Props) {
  const [view, setView] = useState(loadSidebarPreferences);
  const panels = useRef<HTMLDivElement>(null);
  const sectionId = useId();
  useEffect(() => { try { localStorage.setItem(SIDEBAR_STORAGE_KEY, JSON.stringify(view)); } catch { /* Viewer preferences remain usable without storage. */ } }, [view]);
  const resize = (y: number) => { const rect = panels.current!.getBoundingClientRect(); setView(v => ({ ...v, ratio: sidebarRatio((y - rect.top) / rect.height * 100) })); };
  const rows = view.connectionsCollapsed ? `36px ${view.explorerCollapsed ? '0px 36px' : '0px minmax(0,1fr)'}` : view.explorerCollapsed ? 'minmax(0,1fr) 0px 36px' : `minmax(165px,${view.ratio}fr) 8px minmax(200px,${100 - view.ratio}fr)`;
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);
  const [editing, setEditing] = useState<Profile | 'new' | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState('');
  const [search, setSearch] = useState('');
  const [focusId, setFocusId] = useState<string | null>(null);
  const isLive = (id: string) => !!props.sessions[id] && !props.dead.has(id);
  // Only one connection's explorer is shown at a time; the rest collapse to their card.
  const activeId = focusId && isLive(focusId) ? focusId : props.profiles.find(profile => isLive(profile.id))?.id;
  const list = useRef<HTMLDivElement>(null);
  // The connection list scrolls, so its action menu is fixed to the viewport next to the trigger instead of clipped by the list.
  function placeMenu(e: Event) {
    const details = e.currentTarget as HTMLDetailsElement, menu = details.querySelector<HTMLElement>('.connection-menu');
    if (!details.open || !menu) return;
    const rect = details.querySelector('summary')!.getBoundingClientRect();
    menu.style.right = `${Math.max(8, innerWidth - rect.right)}px`;
    menu.style.top = `${rect.bottom + 3 + menu.offsetHeight > innerHeight - 8 ? Math.max(8, rect.top - 3 - menu.offsetHeight) : rect.bottom + 3}px`;
  }
  const closeMenus = () => list.current?.querySelectorAll('details[open]').forEach(d => d.removeAttribute('open'));
  // Fade the clipped edges of the scrolling list so a neighbouring card never reads as a text fragment overlapping its sibling.
  const listEdges = () => { const el = list.current; if (el) el.dataset.edge = [el.scrollTop > 1 && 'start', el.scrollTop < el.scrollHeight - el.clientHeight - 1 && 'end'].filter(Boolean).join(' '); };
  useEffect(() => { const el = list.current; if (!el) return; const observer = new ResizeObserver(listEdges); observer.observe(el); listEdges(); return () => observer.disconnect(); }, [props.profiles.length, search]);
  useEffect(() => { list.current?.querySelector('.connection[data-active]')?.scrollIntoView({ block: 'nearest' }); listEdges(); }, [activeId, props.profiles.length]);
  useEffect(() => { if (props.addRequest) addConnection(); }, [props.addRequest]);
  useEffect(() => { if (props.editRequest) { const profile = props.profiles.find(profile => profile.id === props.editRequest?.id); if (profile && !props.drivers.some(driver => driver.id === profile.driver_id)) openDrivers(profile.driver_id); else setEditing(profile ?? null); } }, [props.editRequest]);
  function addConnection() { if (!props.drivers.length) openDrivers(); else setEditing('new'); }
  async function task(id: string, callback: () => Promise<void>) { setBusy(id); try { await callback(); } catch (e) { props.onError(e, id); } finally { setBusy(null); } }
  return <aside class={`connections ${view.compact ? 'connections--compact' : ''}`} aria-label="Connections"><div class="sidebar-heading">작업공간<small>연결된 데이터베이스를 탐색하세요.</small></div>
    <div class="sidebar-panels" ref={panels} style={{ gridTemplateRows: rows }}>
    <section class="sidebar-section" aria-label="연결 섹션"><div class="section-title sidebar-section-heading"><Button bare class="sidebar-section-toggle" aria-expanded={!view.connectionsCollapsed} aria-controls={`${sectionId}-connections`} onClick={() => setView(v => ({ ...v, connectionsCollapsed: !v.connectionsCollapsed }))}>{view.connectionsCollapsed ? <ChevronRight /> : <ChevronDown />}연결 <small>{props.profiles.length}</small></Button><Button morph={false} onClick={addConnection} aria-label="Add connection" data-testid="add-connection"><Plus /></Button></div>
    <div class="sidebar-section-body" id={`${sectionId}-connections`} hidden={view.connectionsCollapsed}><div class="connection-search"><Search /><input aria-label="Search connections" placeholder="연결 검색…" value={search} onInput={e => setSearch(e.currentTarget.value)} /><kbd>⌘ K</kbd></div><Button bare class="sidebar-compact" aria-pressed={view.compact} onClick={() => setView(v => ({ ...v, compact: !v.compact }))}>컴팩트 보기</Button>
    <div class="connection-list" ref={list} onScroll={() => { closeMenus(); listEdges(); }}>{props.profiles.filter(profile => `${profile.name} ${props.drivers.find(d => d.id === profile.driver_id)?.display_name ?? ''}`.toLocaleLowerCase().includes(search.toLocaleLowerCase())).sort((a, b) => Number(b.id === activeId) - Number(a.id === activeId)).map(profile => {
      const manifest = props.drivers.find(d => d.id === profile.driver_id);
      const connected = !!props.sessions[profile.id] && !props.dead.has(profile.id);
      return <section class="connection" data-paradigm={manifest?.paradigm} data-active={connected && activeId === profile.id ? '' : undefined} key={profile.id} onClick={() => { if (connected) setFocusId(profile.id); }} onFocusIn={() => { if (connected) setFocusId(profile.id); }}><SpotlightCard><div class="connection-heading"><div class="connection-tile"><DriverLogo iconKey={manifest?.icon_key} paradigm={manifest?.paradigm} size={24} /></div><div class="connection-info"><div class="connection-name"><strong title={profile.name}>{profile.name}</strong><span class={`status-dot ${connected ? 'connected' : ''}`} title={connected ? '연결됨' : '연결 끊김'} /></div><small>{manifest?.display_name ?? profile.driver_id}{profile.access_mode === 'read_only' && ' · 읽기 전용'}</small></div>
        <div class="connection-actions"><Button morph={false} class="connect-button" title={busy === profile.id ? '작업 중…' : connected ? '연결 해제' : '연결하기'} aria-label={`Connect or disconnect ${profile.name}`} data-testid="connect-connection" disabled={!!busy || !manifest} onClick={() => void task(profile.id, () => connected ? props.onDisconnect(profile) : props.onConnect(profile).then(() => setFocusId(profile.id)))}>{connected ? <Unplug /> : <PlugZap />}</Button><details onToggle={placeMenu}><summary aria-label={`Connection actions for ${profile.name}`} title="연결 작업"><MoreHorizontal /></summary><div class="connection-menu"><Button morph={false} disabled={!!busy||!manifest} onClick={e=>{e.currentTarget.closest('details')?.removeAttribute('open');void task(profile.id,()=>connected?props.onDisconnect(profile):props.onConnect(profile));}}>{connected?'연결 해제':'연결하기'}</Button><Button morph={false} onClick={e => { e.currentTarget.closest('details')?.removeAttribute('open'); if (manifest) setEditing(profile); else openDrivers(profile.driver_id); }}><Pencil />편집</Button><Button morph={false} aria-label={`Test ${profile.name}`} data-testid="test-connection" disabled={!!busy || !manifest} onClick={e => { e.currentTarget.closest('details')?.removeAttribute('open'); if (!isMock && !profile.save_secret && manifest?.connection_fields.some(f => f.secret)) setEditing(profile); else void task(profile.id, async () => setNotice(await api('connection_test', { profile, secret: null }))); }}><FlaskConical />테스트</Button><FuseButton pending={pendingDelete === profile.id} onPendingChange={pending => setPendingDelete(pending ? profile.id : null)} onUndo={() => {}} unmountAction="cancel" disabled={!!busy} aria-label={`Delete ${profile.name}`} label="삭제" doneLabel="삭제 예약" undoLabel="실행 취소" onCommit={() => void task(profile.id, async () => { if (props.sessions[profile.id]) await props.onDisconnect(profile); await api("profile_delete", { profileId: profile.id }); await props.onProfiles(); })} /></div></details></div></div>{!manifest && <DriverRequired driverId={profile.driver_id} />}</SpotlightCard>

      </section>;
    })}</div></div></section>
    <div class="sidebar-section-splitter" role="separator" aria-label="연결과 탐색기 높이 조절" aria-orientation="horizontal" aria-valuemin={20} aria-valuemax={65} aria-valuenow={Math.round(view.ratio)} aria-controls={`${sectionId}-connections ${sectionId}-explorer`} tabIndex={0} hidden={view.connectionsCollapsed || view.explorerCollapsed}
      onPointerDown={e => { if (e.button !== 0) return; e.preventDefault(); closeMenus(); e.currentTarget.setPointerCapture(e.pointerId); resize(e.clientY); }}
      onPointerMove={e => { if (e.currentTarget.hasPointerCapture(e.pointerId)) resize(e.clientY); }}
      onPointerUp={e => { if (e.currentTarget.hasPointerCapture(e.pointerId)) e.currentTarget.releasePointerCapture(e.pointerId); }}
      onKeyDown={e => { if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return; e.preventDefault(); setView(v => ({ ...v, ratio: sidebarRatio(v.ratio + (e.key === 'ArrowDown' ? 5 : -5)) })); }} />
    <section class="sidebar-section" aria-label="탐색기 섹션"><div class="section-title sidebar-section-heading"><Button bare class="sidebar-section-toggle" aria-expanded={!view.explorerCollapsed} aria-controls={`${sectionId}-explorer`} onClick={() => setView(v => ({ ...v, explorerCollapsed: !v.explorerCollapsed }))}>{view.explorerCollapsed ? <ChevronRight /> : <ChevronDown />}탐색기</Button></div><div class="sidebar-section-body" id={`${sectionId}-explorer`} hidden={view.explorerCollapsed}>
    {props.profiles.map(profile => {
      const manifest = props.drivers.find(d => d.id === profile.driver_id);
      if (!manifest || !isLive(profile.id)) return null;
      return <div class="explorer-region" key={profile.id} hidden={profile.id !== activeId}><Button morph={false} class="new-query" onClick={() => props.onQuery(profile)}><Plus />쿼리 · {manifest.query_language.display}</Button><Explorer onNodes={nodes => props.onNodes?.(profile.id, nodes)} sessionId={props.sessions[profile.id]} manifest={manifest} onAction={(node, action) => props.onAction(profile, node, action)} onError={e => props.onError(e, profile.id)} /></div>;
    })}
    {!activeId && <p class="sidebar-explorer-empty">연결하면 탐색 항목이 표시됩니다.</p>}
    </div></section></div>
    {!props.profiles.length && <div class="sidebar-empty"><Database /><p>연결을 추가해 시작하세요.</p><Button morph={false} onClick={addConnection}><Plus />연결 추가</Button></div>}
    {notice && <div class="notice" role="status">{notice}<Button morph={false} aria-label="Dismiss connection test" onClick={() => setNotice('')}><X /></Button></div>}
    <Suspense fallback={null}>{editing && <ProfileForm key={editing === 'new' ? 'new' : editing.id} profile={editing === 'new' ? undefined : editing} drivers={props.drivers} onClose={() => setEditing(null)} onSaved={async () => { await props.onProfiles(); setEditing(null); }} onError={props.onError} />}</Suspense>
  </aside>;
}
const GROUP_PAGE = 300;
function Explorer({ sessionId, manifest, onAction, onError, onNodes }: { onNodes?: (nodes: ExplorerNode[]) => void; sessionId: string; manifest: DriverManifest; onAction: (node: ExplorerNode, action: NodeAction) => void; onError: (error: unknown) => void }) {
  const [children, setChildren] = useState<Record<string, ExplorerNode[]>>({});
  useEffect(() => { onNodes?.(Object.values(children).flat()); }, [children]);
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState('');
  const [active, setActive] = useState('');
  // Long groups render one page of rows at a time so a schema with hundreds of tables stays light.
  const [limits, setLimits] = useState<Record<string, number>>({});
  const [menu, setMenu] = useState<{ node: ExplorerNode; x: number; y: number } | null>(null);
  const [version, setVersion] = useState(0);
  const tree = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let current = true; setChildren({}); setOpen(new Set()); setActive(''); setLimits({}); setLoading(new Set(['$root']));
    void api('explorer_children', { sessionId, parent: null }).then(nodes => { if (current) setChildren({ $root: nodes }); }).catch(error => { if (current) onError(error); }).finally(() => { if (current) setLoading(new Set()); });
    return () => { current = false; };
  }, [sessionId, version]);
  useEffect(() => { if (!menu) return; const close = () => setMenu(null); window.addEventListener('click', close); window.addEventListener('keydown', close); return () => { window.removeEventListener('click', close); window.removeEventListener('keydown', close); }; }, [menu]);
  async function toggle(node: ExplorerNode) {
    if (open.has(node.id)) { setOpen(s => toggleOpen(s, node.id)); return; }
    setOpen(s => new Set([...s, node.id]));
    if (children[node.id] || loading.has(node.id)) return;
    setLoading(s => new Set([...s, node.id]));
    try { const nodes = await api('explorer_children', { sessionId, parent: node.id }); setChildren(prev => ({ ...prev, [node.id]: nodes })); }
    catch (e) { onError(e); setOpen(s => { const next = new Set(s); next.delete(node.id); return next; }); }
    finally { setLoading(s => { const next = new Set(s); next.delete(node.id); return next; }); }
  }
  const actions = (node: ExplorerNode) => node.actions.filter(a => a.kind !== 'Browse' || manifest.capabilities.paged_browse);
  const matches = (node: ExplorerNode): boolean => !filter || `${node.label} ${node.detail ?? ''}`.toLocaleLowerCase().includes(filter.toLocaleLowerCase()) || (children[node.id] ?? []).some(matches);
  const byId = new Map<string, ExplorerNode>(), parentOf = new Map<string, string>();
  for (const [parent, nodes] of Object.entries(children)) for (const node of nodes) { byId.set(node.id, node); parentOf.set(node.id, parent); }
  const path = new Set<string>();
  for (let id: string | undefined = active; id && id !== '$root' && !path.has(id); id = parentOf.get(id)) path.add(id);
  const run = (node: ExplorerNode, action: NodeAction) => { setActive(node.id); onAction(node, action); };
  function onKeyDown(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    const label = target.closest('.tree-node')?.querySelector<HTMLElement>('.tree-label');
    if (!label || !target.matches('.tree-label,.tree-toggle')) return;
    const labels = [...tree.current!.querySelectorAll<HTMLElement>('.tree-label')].filter(el => !el.closest('[inert]'));
    const node = byId.get(label.dataset.nodeId ?? '');
    const move = treeKey(e.key, { index: labels.indexOf(label), count: labels.length, hasChildren: !!node?.has_children, open: label.dataset.open === 'true', parentIndex: labels.findIndex(el => el.dataset.nodeId === label.dataset.parent) });
    if (!move) return;
    e.preventDefault();
    if (move.type === 'focus') labels[move.index]?.focus();
    else if (node) void toggle(node);
  }
  function render(nodes: ExplorerNode[], depth: number, parentId: string): preact.ComponentChildren {
    const shown = nodes.filter(matches), limit = limits[parentId] ?? GROUP_PAGE, part = shown.slice(0, limit);
    const reached = reachIndex(part.map(node => node.id), path), more = shown.length - part.length;
    return <>{part.map((node, index) => {
      const isOpen = open.has(node.id) || !!filter;
      return <BranchItem key={node.id} last={index === part.length - 1 && !more} reach={depth ? reachState(index, reached) : undefined} section={!depth && path.has(node.id) && isOpen}
        row={<div class="tree-node" onDragOver={e=>{if(manifest.capabilities.import_rows&&node.actions.some(a=>a.kind==='Browse'))e.preventDefault();}} onDrop={e=>{if(!manifest.capabilities.import_rows||!node.actions.some(a=>a.kind==='Browse'))return;e.preventDefault();const file=e.dataTransfer?.files[0];if(file)window.dispatchEvent(new CustomEvent('ax:import',{detail:{sessionId,nodeId:node.id,file}}));}} onContextMenu={e => { e.preventDefault(); setActive(node.id); setMenu({ node, x: Math.min(e.clientX, innerWidth - 180), y: Math.min(e.clientY, innerHeight - 140) }); }} onDblClick={() => { const action = actions(node)[0]; if (action) run(node, action); }}>
          <Button bare class="tree-toggle" disabled={!node.has_children} onClick={() => void toggle(node)} aria-label={`${open.has(node.id) ? 'Collapse' : 'Expand'} ${node.label}`} aria-expanded={node.has_children ? open.has(node.id) : undefined}>{node.has_children ? open.has(node.id) ? <ChevronDown /> : <ChevronRight /> : <Minus />}</Button><Button bare class="tree-label" data-testid="tree-item" data-node-id={node.id} data-parent={parentId} data-open={node.has_children ? String(open.has(node.id)) : undefined} data-active={active === node.id ? '' : undefined} aria-current={active === node.id ? 'true' : undefined} aria-label={node.label} title={node.detail ?? node.label} onClick={() => { setActive(node.id); if (node.has_children) void toggle(node); }} onKeyDown={e => { if (e.key === 'Enter') { const action = actions(node)[0]; if (action) run(node, action); } }}><ExplorerIcon icon={node.icon} /><span>{node.label}</span><small>{node.detail}</small></Button>{!!actions(node).length && <Button morph={false} aria-label={`Actions for ${node.label}`} onClick={e => { e.stopPropagation(); setActive(node.id); const rect = e.currentTarget.getBoundingClientRect(); setMenu({ node, x: rect.right, y: rect.bottom }); }}><MoreHorizontal /></Button>}
        </div>}
        nested={node.has_children ? <BranchGroup open={isOpen}>{!children[node.id] && loading.has(node.id) ? <div class="tree-loading"><Skeleton running rows={2} aria-label="Loading children" /></div> : render(children[node.id] ?? [], depth + 1, node.id)}</BranchGroup> : undefined} />;
    })}{more > 0 && <BranchItem last row={<Button bare class="tree-more" aria-label={`Show more in ${byId.get(parentId)?.label ?? 'explorer'}`} onClick={() => setLimits(l => ({ ...l, [parentId]: limit + GROUP_PAGE }))}>+{more} 더 보기</Button>} />}</>;
  }
  return <div class="explorer"><div class="toolbar"><input value={filter} onInput={e => setFilter(e.currentTarget.value)} placeholder="탐색 항목 필터" aria-label="Filter explorer" /><Button morph={false} onClick={() => setVersion(v => v + 1)} aria-label="Refresh explorer"><RefreshCw /></Button></div><div class="explorer-scroll" ref={tree} onKeyDown={onKeyDown}>{loading.has('$root') ? <p>불러오는 중…</p> : <div class="bm-root">{render(children.$root ?? [], 0, '$root')}</div>}</div>{menu && <div class="context-menu" role="menu" style={{ left: `${menu.x}px`, top: `${menu.y}px` }}>{actions(menu.node).map(action => <Button morph={false} role="menuitem" key={action.id} onClick={() => { run(menu.node, action); setMenu(null); }}>{action.label}</Button>)}</div>}</div>;
}

function ExplorerIcon({ icon }: { icon: ExplorerNode['icon'] }) {
  const Icon = icon === 'Folder' ? Folder : icon === 'Table' || icon === 'View' ? Table2 : icon === 'Column' || icon === 'Property' ? Columns3 : icon === 'Key' ? Key : icon === 'Index' ? ListFilter : icon === 'Label' ? Layers : icon === 'Relationship' ? Network : icon === 'Function' ? FileCode2 : Database;
  return <Icon class="explorer-icon" />;
}
