import { useEffect, useState } from 'preact/hooks';
import type { DriverManifest, Profile, ExplorerNode, NodeAction } from './contract';
import { api, isMock } from './api';
import { Dialog } from './Dialog';
interface Props { drivers: DriverManifest[]; profiles: Profile[]; sessions: Record<string, string>; dead: Set<string>; onProfiles: () => Promise<void>; onConnect: (profile: Profile, secret?: string | null) => Promise<void>; onDisconnect: (profile: Profile) => Promise<void>; onQuery: (profile: Profile, text?: string) => void; onAction: (profile: Profile, node: ExplorerNode, action: NodeAction) => void; onError: (error: unknown, profileId?: string) => void }
export function Connections(props: Props) {
  const [editing, setEditing] = useState<Profile | 'new' | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState('');
  async function task(id: string, callback: () => Promise<void>) { setBusy(id); try { await callback(); } catch (e) { props.onError(e, id); } finally { setBusy(null); } }
  return <aside class="connections" aria-label="Connections"><div class="section-title"><strong>CONNECTIONS</strong><button onClick={() => setEditing('new')} aria-label="Add connection" data-testid="add-connection">＋</button></div>
    {props.profiles.map(profile => {
      const manifest = props.drivers.find(d => d.id === profile.driver_id);
      return <section class="connection" key={profile.id}><div class="connection-name"><span class={`status-dot ${props.sessions[profile.id] && !props.dead.has(profile.id) ? 'connected' : ''}`} /><strong>{profile.name}</strong></div><small>{manifest?.display_name ?? profile.driver_id}</small>
        <div class="connection-actions"><button aria-label={`Connect or disconnect ${profile.name}`} data-testid="connect-connection" disabled={!!busy || !manifest} onClick={() => void task(profile.id, () => props.sessions[profile.id] && !props.dead.has(profile.id) ? props.onDisconnect(profile) : props.onConnect(profile))}>{busy === profile.id ? 'Working…' : props.dead.has(profile.id) ? '재연결' : props.sessions[profile.id] ? 'Disconnect' : 'Connect'}</button><button onClick={() => setEditing(profile)}>Edit</button><button aria-label={`Test ${profile.name}`} data-testid="test-connection" disabled={!!busy} onClick={() => { if (!isMock && !profile.save_secret && manifest?.connection_fields.some(f => f.secret)) setEditing(profile); else void task(profile.id, async () => setNotice(await api('connection_test', { profile, secret: null }))); }}>Test</button><button disabled={!!busy} onClick={() => { if (confirm(`Delete connection “${profile.name}”?`)) void task(profile.id, async () => { if (props.sessions[profile.id]) await props.onDisconnect(profile); await api('profile_delete', { profileId: profile.id }); await props.onProfiles(); }); }} aria-label={`Delete ${profile.name}`}>×</button></div>
        {manifest && props.sessions[profile.id] && !props.dead.has(profile.id) && <><button class="new-query" onClick={() => props.onQuery(profile)}>＋ Query · {manifest.query_language.display}</button><Explorer sessionId={props.sessions[profile.id]} manifest={manifest} onAction={(node, action) => props.onAction(profile, node, action)} onError={e => props.onError(e, profile.id)} /></>}
      </section>;
    })}
    {!props.profiles.length && <p class="muted">Add a connection to get started.</p>}
    {notice && <div class="notice" role="status">{notice}<button aria-label="Dismiss connection test" onClick={() => setNotice('')}>×</button></div>}
    {editing && <ProfileForm profile={editing === 'new' ? undefined : editing} drivers={props.drivers} onClose={() => setEditing(null)} onSaved={async () => { await props.onProfiles(); setEditing(null); }} onError={props.onError} />}
  </aside>;
}
function ProfileForm({ profile, drivers, onClose, onSaved, onError }: { profile?: Profile; drivers: DriverManifest[]; onClose: () => void; onSaved: () => Promise<void>; onError: (error: unknown) => void }) {
  const [driverId, setDriverId] = useState(profile?.driver_id ?? drivers[0]?.id ?? '');
  const manifest = drivers.find(d => d.id === driverId);
  const defaults = (d?: DriverManifest) => Object.fromEntries(d?.connection_fields.filter(f => !f.secret).map(f => [f.key, f.default ?? '']) ?? []);
  const [name, setName] = useState(profile?.name ?? '');
  const [fields, setFields] = useState(profile?.fields ?? defaults(manifest));
  const [secrets, setSecrets] = useState<Record<string, string>>({});
  const [saveSecret, setSaveSecret] = useState(profile?.save_secret ?? false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState('');
  const draft = (): Profile => ({ id: profile?.id ?? crypto.randomUUID(), name: name.trim(), driver_id: driverId, fields: Object.fromEntries(Object.entries(fields).filter(([key]) => !manifest?.connection_fields.some(f => f.key === key && f.secret))), save_secret: saveSecret });
  // shortcut: host accepts one secret field; extend the host contract before supporting multiple credentials.
  const secret = () => Object.values(secrets).find(Boolean) ?? null;
  async function submit(test: boolean) {
    if (!manifest) return;
    setBusy(true); setNotice('');
    try { if (test) setNotice(await api('connection_test', { profile: draft(), secret: secret() })); else { await api('profile_save', { profile: draft(), secret: secret() }); setSecrets({}); await onSaved(); } }
    catch (error) { onError(error); } finally { setBusy(false); }
  }
  return <Dialog label={profile ? 'Edit connection' : 'Add connection'} onClose={onClose}><section class="profile-form"><div class="section-title"><strong>{profile ? 'Edit connection' : 'Add connection'}</strong><button onClick={onClose} aria-label="Close connection form">×</button></div><form onSubmit={e => { e.preventDefault(); void submit(false); }}>
    <label>Name<input required autoFocus value={name} onInput={e => setName(e.currentTarget.value)} /></label><label>Driver<select disabled={!!profile} value={driverId} onChange={e => { const id = e.currentTarget.value; setDriverId(id); setFields(defaults(drivers.find(d => d.id === id))); setSecrets({}); }}>{drivers.map(d => <option key={d.id} value={d.id}>{d.display_name}</option>)}</select></label>
    {manifest?.connection_fields.map(field => <label key={field.key}>{field.label}{field.required ? ' *' : ''}{typeof field.kind === 'object' && !field.secret ? <select value={fields[field.key] ?? ''} required={field.required} onChange={e => setFields(f => ({ ...f, [field.key]: e.currentTarget.value }))}>{!field.default && <option value="">Choose…</option>}{field.kind.Select.options.map(option => <option key={option}>{option}</option>)}</select> : field.kind === 'Bool' && !field.secret ? <input type="checkbox" checked={fields[field.key] === 'true'} onChange={e => setFields(f => ({ ...f, [field.key]: String(e.currentTarget.checked) }))} /> : <FieldInput field={field} existing={!!profile} value={field.secret ? secrets[field.key] ?? '' : fields[field.key] ?? ''} onInput={e => { const value = e.currentTarget.value; if (field.secret) setSecrets(s => ({ ...s, [field.key]: value })); else setFields(f => ({ ...f, [field.key]: value })); }} />}</label>)}
    {manifest?.connection_fields.some(f => f.secret) && <label class="checkbox"><input type="checkbox" checked={saveSecret} onChange={e => setSaveSecret(e.currentTarget.checked)} />키체인에 저장</label>}
    <div class="toolbar"><button type="submit" disabled={busy || !manifest}>Save</button><button aria-label="Test connection" data-testid="test-connection-form" type="button" disabled={busy || !manifest} onClick={e => { if (e.currentTarget.form?.reportValidity()) void submit(true); }}>Test connection</button><button type="button" onClick={onClose}>Cancel</button></div>{notice && <p role="status">{notice}</p>}
  </form></section></Dialog>;
}
function Explorer({ sessionId, manifest, onAction, onError }: { sessionId: string; manifest: DriverManifest; onAction: (node: ExplorerNode, action: NodeAction) => void; onError: (error: unknown) => void }) {
  const [children, setChildren] = useState<Record<string, ExplorerNode[]>>({});
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
      <button class="tree-toggle" disabled={!node.has_children} onClick={() => void toggle(node)} aria-label={`${open.has(node.id) ? 'Collapse' : 'Expand'} ${node.label}`} aria-expanded={node.has_children ? open.has(node.id) : undefined}>{loading.has(node.id) ? '…' : node.has_children ? open.has(node.id) ? '▾' : '▸' : '·'}</button><button class="tree-label" data-testid="tree-item" aria-label={node.label} title={node.detail ?? node.label} onClick={() => { if (node.has_children) void toggle(node); }} onKeyDown={e => { if (e.key === 'Enter') { const action = actions(node)[0]; if (action) onAction(node, action); } }}>{node.label}<small>{node.detail}</small></button>{!!actions(node).length && <button aria-label={`Actions for ${node.label}`} onClick={e => { e.stopPropagation(); const rect = e.currentTarget.getBoundingClientRect(); setMenu({ node, x: rect.right, y: rect.bottom }); }}>⋯</button>}
    </div>{(open.has(node.id) || !!filter) && render(children[node.id] ?? [], depth + 1)}</div>);
  }
  return <div class="explorer"><div class="toolbar"><input value={filter} onInput={e => setFilter(e.currentTarget.value)} placeholder="Filter loaded nodes" aria-label="Filter explorer" /><button onClick={() => setVersion(v => v + 1)} aria-label="Refresh explorer">↻</button></div>{loading.has('$root') ? <p>Loading…</p> : render(children.$root ?? [], 0)}{menu && <div class="context-menu" role="menu" style={{ left: `${menu.x}px`, top: `${menu.y}px` }}>{actions(menu.node).map(action => <button role="menuitem" key={action.id} onClick={() => { onAction(menu.node, action); setMenu(null); }}>{action.label}</button>)}</div>}</div>;
}

function FieldInput({ field, existing, value, onInput }: { field: import('./contract').FieldSpec; existing: boolean; value: string; onInput: (e: InputEvent & { currentTarget: HTMLInputElement }) => void }) {
  const attrs = { value, onInput, required: field.required && !(existing && field.secret), placeholder: existing && field.secret ? 'Leave blank to retain existing secret' : field.placeholder ?? '' };
  if (field.secret || field.kind === 'Password') return <input {...attrs} type="password" autoComplete="new-password" />;
  if (field.kind === 'Number') return <input {...attrs} type="number" />;
  return <input {...attrs} type="text" autoComplete="off" />;
}
