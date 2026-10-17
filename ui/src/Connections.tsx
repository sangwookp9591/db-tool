import { Button } from './ui/Button';
import { SpotlightCard } from './ui/SpotlightCard';
import { SpringCheck } from './ui/SpringCheck';
import { FuseButton } from './ui/FuseButton';
import { Database, Share2, FileJson, KeyRound, Plus, X, Search, MoreHorizontal, PlugZap, Unplug, Pencil, FlaskConical, Trash2, ChevronRight, ChevronDown, Minus, RefreshCw, Folder, Table2, Columns3, Key, ListFilter, Layers, Network, FileCode2 } from 'lucide-preact';
import { useEffect, useId, useRef, useState } from 'preact/hooks';
import type { DriverManifest, Profile, ExplorerNode, NodeAction } from './contract';
import { api, isMock } from './api';
import { Dialog } from './Dialog';
interface Props { editRequest?: {id:string;sequence:number} | null; onNodes?: (profileId: string, nodes: ExplorerNode[]) => void; addRequest?: number; drivers: DriverManifest[]; profiles: Profile[]; sessions: Record<string, string>; dead: Set<string>; onProfiles: () => Promise<void>; onConnect: (profile: Profile, secret?: string | null) => Promise<void>; onDisconnect: (profile: Profile) => Promise<void>; onQuery: (profile: Profile, text?: string) => void; onAction: (profile: Profile, node: ExplorerNode, action: NodeAction) => void; onError: (error: unknown, profileId?: string) => void }
export function Connections(props: Props) {
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);
  const [editing, setEditing] = useState<Profile | 'new' | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState('');
  const [search, setSearch] = useState('');
  useEffect(() => { if (props.addRequest) setEditing('new'); }, [props.addRequest]);
  useEffect(() => { if (props.editRequest) setEditing(props.profiles.find(profile => profile.id === props.editRequest?.id) ?? null); }, [props.editRequest]);
  async function task(id: string, callback: () => Promise<void>) { setBusy(id); try { await callback(); } catch (e) { props.onError(e, id); } finally { setBusy(null); } }
  return <aside class="connections" aria-label="Connections"><div class="sidebar-heading">작업공간<small>연결된 데이터베이스를 탐색하세요.</small></div><div class="connection-search"><Search /><input aria-label="Search connections" placeholder="연결 검색…" value={search} onInput={e => setSearch(e.currentTarget.value)} /><kbd>⌘ K</kbd></div><div class="section-title"><strong>연결</strong><Button morph={false} onClick={() => setEditing('new')} aria-label="Add connection" data-testid="add-connection"><Plus /></Button></div>
    {props.profiles.filter(profile => `${profile.name} ${props.drivers.find(d => d.id === profile.driver_id)?.display_name ?? ''}`.toLocaleLowerCase().includes(search.toLocaleLowerCase())).map(profile => {
      const manifest = props.drivers.find(d => d.id === profile.driver_id);
      const Tile = manifest?.paradigm === 'Graph' ? Share2 : manifest?.paradigm === 'Document' ? FileJson : manifest?.paradigm === 'KeyValue' ? KeyRound : Database;
      const connected = !!props.sessions[profile.id] && !props.dead.has(profile.id);
      return <section class="connection" data-paradigm={manifest?.paradigm} key={profile.id}><SpotlightCard><div class="connection-heading"><div class="connection-tile"><Tile /></div><div class="connection-info"><div class="connection-name"><strong title={profile.name}>{profile.name}</strong><span class={`status-dot ${connected ? 'connected' : ''}`} title={connected ? 'Connected' : 'Disconnected'} /></div><small>{manifest?.display_name ?? profile.driver_id}{profile.access_mode === 'read_only' && ' · 읽기 전용'}</small></div>
        <div class="connection-actions"><Button morph={false} class="connect-button" title={busy === profile.id ? 'Working…' : connected ? 'Disconnect' : 'Connect'} aria-label={`Connect or disconnect ${profile.name}`} data-testid="connect-connection" disabled={!!busy || !manifest} onClick={() => void task(profile.id, () => connected ? props.onDisconnect(profile) : props.onConnect(profile))}>{connected ? <Unplug /> : <PlugZap />}</Button><details><summary aria-label={`Connection actions for ${profile.name}`} title="Connection actions"><MoreHorizontal /></summary><div class="connection-menu"><Button morph={false} onClick={e => { e.currentTarget.closest('details')?.removeAttribute('open'); setEditing(profile); }}><Pencil />편집</Button><Button morph={false} aria-label={`Test ${profile.name}`} data-testid="test-connection" disabled={!!busy} onClick={e => { e.currentTarget.closest('details')?.removeAttribute('open'); if (!isMock && !profile.save_secret && manifest?.connection_fields.some(f => f.secret)) setEditing(profile); else void task(profile.id, async () => setNotice(await api('connection_test', { profile, secret: null }))); }}><FlaskConical />테스트</Button><FuseButton pending={pendingDelete === profile.id} onPendingChange={pending => setPendingDelete(pending ? profile.id : null)} onUndo={() => {}} unmountAction="cancel" disabled={!!busy} aria-label={`Delete ${profile.name}`} label="삭제" doneLabel="삭제 예약" undoLabel="실행 취소" onCommit={() => void task(profile.id, async () => { if (props.sessions[profile.id]) await props.onDisconnect(profile); await api("profile_delete", { profileId: profile.id }); await props.onProfiles(); })} /></div></details></div></div></SpotlightCard>

        {manifest && props.sessions[profile.id] && !props.dead.has(profile.id) && <><Button morph={false} class="new-query" onClick={() => props.onQuery(profile)}><Plus />쿼리 · {manifest.query_language.display}</Button><Explorer onNodes={nodes => props.onNodes?.(profile.id, nodes)} sessionId={props.sessions[profile.id]} manifest={manifest} onAction={(node, action) => props.onAction(profile, node, action)} onError={e => props.onError(e, profile.id)} /></>}
      </section>;
    })}
    {!props.profiles.length && <div class="sidebar-empty"><Database /><p>연결을 추가해 시작하세요.</p><Button morph={false} onClick={() => setEditing('new')}><Plus />연결 추가</Button></div>}
    {notice && <div class="notice" role="status">{notice}<Button morph={false} aria-label="Dismiss connection test" onClick={() => setNotice('')}><X /></Button></div>}
    {editing && <ProfileForm key={editing === 'new' ? 'new' : editing.id} profile={editing === 'new' ? undefined : editing} drivers={props.drivers} onClose={() => setEditing(null)} onSaved={async () => { await props.onProfiles(); setEditing(null); }} onError={props.onError} />}
  </aside>;
}
function ProfileForm({ profile, drivers, onClose, onSaved, onError }: { profile?: Profile; drivers: DriverManifest[]; onClose: () => void; onSaved: () => Promise<void>; onError: (error: unknown) => void }) {
  const [driverId, setDriverId] = useState(profile?.driver_id ?? drivers[0]?.id ?? '');
  const manifest = drivers.find(d => d.id === driverId);
  const defaults = (d?: DriverManifest) => Object.fromEntries(d?.connection_fields.filter(f => !f.secret).map(f => [f.key, f.default ?? '']) ?? []);
  const [name, setName] = useState(profile?.name ?? '');
  const [fields, setFields] = useState(profile?.fields ?? defaults(manifest));
  const [secrets, setSecrets] = useState<Record<string, string>>({});
  const [accessMode, setAccessMode] = useState<Profile['access_mode']>(profile?.access_mode ?? 'read_write');
  const [saveSecret, setSaveSecret] = useState(profile?.save_secret ?? false);
  const [busy, setBusy] = useState(false);
  const [pendingAction, setPendingAction] = useState<'test'|'save'|null>(null);
  const [testStatus, setTestStatus] = useState<'idle'|'success'|'error'>('idle');
  const [notice, setNotice] = useState('');
  const form = useRef<HTMLFormElement>(null);
  const formId = useId();
  function clearDraft() {
    form.current?.querySelectorAll('input').forEach(input => { input.value = ''; input.checked = false; });
    for (const key of Object.keys(secrets)) delete secrets[key];
    setSecrets({}); setName(''); setFields({}); setSaveSecret(false); setNotice('');
  }
  function close() { clearDraft(); onClose(); }

  const draft = (): Profile => ({ id: profile?.id ?? crypto.randomUUID(), name: name.trim(), driver_id: driverId, fields: Object.fromEntries(Object.entries(fields).filter(([key]) => !manifest?.connection_fields.some(f => f.key === key && f.secret))), save_secret: saveSecret, access_mode: manifest?.capabilities.read_only_exec ? accessMode : 'read_write' });
  // shortcut: host accepts one secret field; extend the host contract before supporting multiple credentials.
  const secret = () => Object.values(secrets).find(Boolean) ?? null;
  async function submit(test: boolean) {
    if (!manifest) return;
    setBusy(true); setPendingAction(test?'test':'save'); setNotice('');
    try { if (test) { setNotice(await api('connection_test', { profile: draft(), secret: secret() })); setTestStatus('success'); } else { await api('profile_save', { profile: draft(), secret: secret() }); clearDraft(); await onSaved(); } }
    catch (error) { if(test)setTestStatus('error'); onError(error); } finally { setBusy(false); setPendingAction(null); }
  }
  const group = (field: import('./contract').FieldSpec) => field.required ? 'Connection' : field.secret || /^(user|username|password|auth.*|credential.*)$/i.test(field.key) ? 'Authentication' : /tls|ssl|cert|ca_file|timeout|socket|option/i.test(field.key) ? 'Advanced' : 'Connection';
  function fieldControl(field: import('./contract').FieldSpec) {
    const title = <span class="field-label">{field.label}{field.required && <span class="required-dot" aria-label="Required" />}</span>;
    if (field.kind === 'Bool' && !field.secret) return <SpringCheck key={field.key} checked={fields[field.key] === 'true'} onChange={checked => setFields(f => ({...f,[field.key]:String(checked)}))} label={field.label} />;
    return <label key={field.key} class={/^(host|port)$/.test(field.key) ? 'field-half' : ''}>{title}{typeof field.kind === 'object' && !field.secret ? <span class="select-control"><select value={fields[field.key] ?? ''} required={field.required} onChange={e => setFields(f => ({ ...f, [field.key]: e.currentTarget.value }))}>{!field.default && <option value="">Choose…</option>}{field.kind.Select.options.map(option => <option key={option}>{option}</option>)}</select><ChevronDown /></span> : <FieldInput field={field} existing={!!profile} value={field.secret ? secrets[field.key] ?? '' : fields[field.key] ?? ''} onInput={e => { const value = e.currentTarget.value; if (field.secret) setSecrets(s => ({ ...s, [field.key]: value })); else setFields(f => ({ ...f, [field.key]: value })); }} />}</label>;
  }
  return <Dialog className="connection-dialog" title={profile ? "연결 편집" : "연결 추가"} label={profile ? 'Edit connection' : 'Add connection'} closeLabel="Close connection form" onClose={close} footer={<><Button morph loading={pendingAction==='test'} aria-label="Test connection" data-testid="test-connection-form" type="button" disabled={busy || !manifest} onClick={() => { if (form.current?.reportValidity()) void submit(true); }}><FlaskConical />{testStatus==='success'?'연결됨':testStatus==='error'?'연결 실패':'연결 테스트'}</Button><span class="toolbar-spacer" /><Button morph={false} type="button" onClick={close}>취소</Button><Button morph loading={pendingAction==='save'} form={formId} type="submit" disabled={busy || !manifest}>저장</Button></>}>
    <section class="profile-form connection-form"><form onInput={()=>setTestStatus('idle')} id={formId} ref={form} autoComplete="off" onSubmit={e => { e.preventDefault(); void submit(false); }}>
      <details class="field-group" open><summary><ChevronRight />연결</summary><div class="field-grid">
        <label><span class="field-label">이름<span class="required-dot" aria-label="Required" /></span><input aria-label="Name" required autoFocus value={name} onInput={e => setName(e.currentTarget.value)} /></label>
        <label><span class="field-label">드라이버</span><span class="select-control"><select aria-label="Driver" disabled={!!profile} value={driverId} onChange={e => { const id = e.currentTarget.value; setDriverId(id); setAccessMode('read_write'); setFields(defaults(drivers.find(d => d.id === id))); setSecrets({}); }}>{drivers.map(d => <option key={d.id} value={d.id}>{d.display_name}</option>)}</select><ChevronDown /></span></label>
        {manifest?.connection_fields.filter(field => group(field) === 'Connection').map(fieldControl)}
      </div></details>
      {!!manifest?.connection_fields.some(field => group(field) === 'Authentication') && <details class="field-group" open><summary><ChevronRight />인증</summary><div class="field-grid">{manifest.connection_fields.filter(field => group(field) === 'Authentication').map(fieldControl)}{manifest.connection_fields.some(f => f.secret) && <SpringCheck checked={saveSecret} onChange={setSaveSecret} label="키체인에 저장" />}</div></details>}
      <details class="field-group"><summary><ChevronRight />고급 설정</summary><div class="field-grid">{manifest?.connection_fields.filter(field => group(field) === 'Advanced').map(fieldControl)}{manifest?.capabilities.read_only_exec && <label><span class="field-label">접근 모드</span><span class="select-control"><select aria-label="Connection access mode" value={accessMode} onChange={e => setAccessMode(e.currentTarget.value as Profile['access_mode'])}><option value="read_write">읽기 / 쓰기</option><option value="read_only">읽기 전용</option></select><ChevronDown /></span></label>}</div></details>
      {notice && <p role="status">{notice}</p>}
    </form></section>
  </Dialog>;
}
function Explorer({ sessionId, manifest, onAction, onError, onNodes }: { onNodes?: (nodes: ExplorerNode[]) => void; sessionId: string; manifest: DriverManifest; onAction: (node: ExplorerNode, action: NodeAction) => void; onError: (error: unknown) => void }) {
  const [children, setChildren] = useState<Record<string, ExplorerNode[]>>({});
  useEffect(() => { onNodes?.(Object.values(children).flat()); }, [children]);
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState('');
  const [menu, setMenu] = useState<{ node: ExplorerNode; x: number; y: number } | null>(null);
  const [version, setVersion] = useState(0);
  useEffect(() => {
    let current = true; setChildren({}); setOpen(new Set()); setLoading(new Set(['$root']));
    void api('explorer_children', { sessionId, parent: null }).then(nodes => { if (current) setChildren({ $root: nodes }); }).catch(error => { if (current) onError(error); }).finally(() => { if (current) setLoading(new Set()); });
    return () => { current = false; };
  }, [sessionId, version]);
  useEffect(() => { if (!menu) return; const close = () => setMenu(null); window.addEventListener('click', close); window.addEventListener('keydown', close); return () => { window.removeEventListener('click', close); window.removeEventListener('keydown', close); }; }, [menu]);
  async function toggle(node: ExplorerNode) {
    if (open.has(node.id)) { setOpen(s => { const next = new Set(s); next.delete(node.id); return next; }); return; }
    setOpen(s => new Set([...s, node.id]));
    if (children[node.id] || loading.has(node.id)) return;
    setLoading(s => new Set([...s, node.id]));
    try { const nodes = await api('explorer_children', { sessionId, parent: node.id }); setChildren(prev => ({ ...prev, [node.id]: nodes })); }
    catch (e) { onError(e); setOpen(s => { const next = new Set(s); next.delete(node.id); return next; }); }
    finally { setLoading(s => { const next = new Set(s); next.delete(node.id); return next; }); }
  }
  const actions = (node: ExplorerNode) => node.actions.filter(a => a.kind !== 'Browse' || manifest.capabilities.paged_browse);
  const matches = (node: ExplorerNode): boolean => !filter || `${node.label} ${node.detail ?? ''}`.toLocaleLowerCase().includes(filter.toLocaleLowerCase()) || (children[node.id] ?? []).some(matches);
  function render(nodes: ExplorerNode[], depth: number): preact.ComponentChildren {
    return nodes.filter(matches).map(node => <div key={node.id}><div class="tree-node" style={{ paddingLeft: `${depth * 12}px` }} onContextMenu={e => { e.preventDefault(); setMenu({ node, x: Math.min(e.clientX, innerWidth - 180), y: Math.min(e.clientY, innerHeight - 140) }); }} onDblClick={() => { const action = actions(node)[0]; if (action) onAction(node, action); }}>
      <Button bare class="tree-toggle" disabled={!node.has_children} onClick={() => void toggle(node)} aria-label={`${open.has(node.id) ? 'Collapse' : 'Expand'} ${node.label}`} aria-expanded={node.has_children ? open.has(node.id) : undefined}>{node.has_children ? open.has(node.id) ? <ChevronDown /> : <ChevronRight /> : <Minus />}</Button><Button bare class="tree-label" data-testid="tree-item" aria-label={node.label} title={node.detail ?? node.label} onClick={() => { if (node.has_children) void toggle(node); }} onKeyDown={e => { if (e.key === 'Enter') { const action = actions(node)[0]; if (action) onAction(node, action); } }}><ExplorerIcon icon={node.icon} /><span>{node.label}</span><small>{node.detail}</small></Button>{!!actions(node).length && <Button morph={false} aria-label={`Actions for ${node.label}`} onClick={e => { e.stopPropagation(); const rect = e.currentTarget.getBoundingClientRect(); setMenu({ node, x: rect.right, y: rect.bottom }); }}><MoreHorizontal /></Button>}
    </div>{(open.has(node.id) || !!filter) && render(children[node.id] ?? [], depth + 1)}</div>);
  }
  return <div class="explorer"><div class="toolbar"><input value={filter} onInput={e => setFilter(e.currentTarget.value)} placeholder="탐색 항목 필터" aria-label="Filter explorer" /><Button morph={false} onClick={() => setVersion(v => v + 1)} aria-label="Refresh explorer"><RefreshCw /></Button></div>{loading.has('$root') ? <p>Loading…</p> : render(children.$root ?? [], 0)}{menu && <div class="context-menu" role="menu" style={{ left: `${menu.x}px`, top: `${menu.y}px` }}>{actions(menu.node).map(action => <Button morph={false} role="menuitem" key={action.id} onClick={() => { onAction(menu.node, action); setMenu(null); }}>{action.label}</Button>)}</div>}</div>;
}

function FieldInput({ field, existing, value, onInput }: { field: import('./contract').FieldSpec; existing: boolean; value: string; onInput: (e: InputEvent & { currentTarget: HTMLInputElement }) => void }) {
  const attrs = { 'aria-label': field.label, value, onInput, required: field.required && !(existing && field.secret), placeholder: existing && field.secret ? 'Leave blank to retain existing secret' : field.placeholder ?? '' };
  if (field.secret || field.kind === 'Password') return <input {...attrs} type="password" autoComplete="new-password" />;
  if (field.kind === 'Number') return <input {...attrs} type="number" />;
  return <input {...attrs} type="text" autoComplete="off" />;
}

function ExplorerIcon({ icon }: { icon: ExplorerNode['icon'] }) {
  const Icon = icon === 'Folder' ? Folder : icon === 'Table' || icon === 'View' ? Table2 : icon === 'Column' || icon === 'Property' ? Columns3 : icon === 'Key' ? Key : icon === 'Index' ? ListFilter : icon === 'Label' ? Layers : icon === 'Relationship' ? Network : icon === 'Function' ? FileCode2 : Database;
  return <Icon class="explorer-icon" />;
}
