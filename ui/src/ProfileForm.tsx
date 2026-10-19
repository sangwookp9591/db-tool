import { lazy, Suspense } from 'preact/compat';
import { Channel } from '@tauri-apps/api/core';
import { ConnectionTimeline, type StageEvent } from './ax/ConnectionStatus';
import type { ConnectionNetwork, ConnectionAuth, ConnectionSecrets, ConnectionEvent } from './ax/connectionContract';
import { Button } from './ui/Button';
import { ConnectButton } from './ui/ConnectSteps';
import { SpringCheck } from './ui/SpringCheck';
import { Check, ChevronRight } from 'lucide-preact';
import { useId, useRef, useState } from 'preact/hooks';
import type { DriverManifest, Profile } from './contract';
import { api, isMock } from './api';
import { Dialog } from './Dialog';
import { GlideSelect } from './ui/GlideSelect';
const NetworkFields = lazy(() => import('./ax/ConnectionNetwork').then(m => ({ default: m.NetworkFields })));
const IamFields = lazy(() => import('./ax/ConnectionNetwork').then(m => ({ default: m.IamFields })));
export function ProfileForm({ profile, drivers, onClose, onSaved, onError }: { profile?: Profile; drivers: DriverManifest[]; onClose: () => void; onSaved: () => Promise<void>; onError: (error: unknown) => void }) {
  const [driverId, setDriverId] = useState(profile?.driver_id ?? drivers[0]?.id ?? '');
  const profileId = useRef(profile?.id ?? crypto.randomUUID());
  const [step,setStep]=useState(0),[paste,setPaste]=useState('');
  const [network,setNetwork]=useState<ConnectionNetwork>(profile?.network??{}),[auth,setAuth]=useState<ConnectionAuth>(profile?.auth??{kind:'password'}),[networkSecrets,setNetworkSecrets]=useState<ConnectionSecrets>({}),[stages,setStages]=useState<StageEvent[]>([]);
  const manifest = drivers.find(d => d.id === driverId);
  const defaults = (d?: DriverManifest) => Object.fromEntries(d?.connection_fields.filter(f => !f.secret).map(f => [f.key, f.default ?? '']) ?? []);
  const [name, setName] = useState(profile?.name ?? '');
  const [fields, setFields] = useState(profile?.fields ?? defaults(manifest));
  const [secrets, setSecrets] = useState<Record<string, string>>({});
  const [accessMode, setAccessMode] = useState<Profile['access_mode']>(profile?.access_mode ?? 'read_write');
  const [hasStoredSecret,setHasStoredSecret]=useState(!!profile?.save_secret);
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
    setNetworkSecrets({}); setPaste(''); setSecrets({}); setName(''); setFields({}); setSaveSecret(false); setNotice('');
  }
  function close() { clearDraft(); onClose(); }

  const draft = (): Profile => ({ id: profileId.current, network, auth, name: name.trim(), driver_id: driverId, fields: Object.fromEntries(Object.entries(fields).filter(([key]) => !manifest?.connection_fields.some(f => f.key === key && f.secret))), save_secret: saveSecret, access_mode: manifest?.capabilities.read_only_exec ? accessMode : 'read_write' });
  // shortcut: host accepts one secret field; extend the host contract before supporting multiple credentials.
  const secret = () => Object.values(secrets).find(Boolean) ?? null;
  async function submit(test: boolean) {
    if (!manifest) return;
    setBusy(true); setPendingAction(test?'test':'save'); setNotice(''); if(test){setStages([]);setTestStatus('idle');}
    try { if(Object.keys(networkSecrets).length){await api('connection_credentials_save',{profileId:profileId.current,secrets:networkSecrets});setNetworkSecrets({});} if (test) { setStages([]); const onEvent = isMock ? {onmessage: (_event: ConnectionEvent) => {}} as Channel<ConnectionEvent> : new Channel<ConnectionEvent>(); onEvent.onmessage=event=>{if(event.event==='stage')setStages(previous=>[...previous.filter(item=>item.stage!==event.stage),event]);}; setNotice(await api('connection_test_steps', { profile: draft(), secret: secret(), onEvent })); setTestStatus('success'); } else { await api('profile_save', { profile: draft(), secret: secret() }); clearDraft(); await onSaved(); } }
    catch (error) { if(test){setTestStatus('error');setNotice(String((error as Error).message??error));} onError(error); } finally { setBusy(false); setPendingAction(null); }
  }
  const group = (field: import('./contract').FieldSpec) => field.secret || /^(user|username|password|auth.*|credential.*)$/i.test(field.key) ? 'Authentication' : /tls|ssl|cert|ca_file|timeout|socket|option/i.test(field.key) ? 'Advanced' : 'Connection';
  function fieldControl(field: import('./contract').FieldSpec) {
    const accessibleLabel = connectionFieldAriaLabel(field.key, field.label);
    const title = <span class="field-label">{field.label}{field.required && <span class="required-dot" aria-label="Required" />}</span>;
    if (field.kind === 'Bool' && !field.secret) return <SpringCheck key={field.key} checked={fields[field.key] === 'true'} onChange={checked => setFields(f => ({...f,[field.key]:String(checked)}))} label={field.label} aria-label={accessibleLabel} />;
    return <label key={field.key} class={/^(host|port)$/.test(field.key) ? 'field-half' : ''}>{title}{typeof field.kind === 'object' && !field.secret ? <GlideSelect ariaLabel={accessibleLabel} name={field.key} required={field.required} value={fields[field.key] ?? ''} options={[...(!field.default ? [{ value: '', label: '선택…' }] : []), ...field.kind.Select.options]} onChange={value => setFields(f => ({ ...f, [field.key]: value }))} /> : <FieldInput field={field} existing={!!profile||hasStoredSecret} value={field.secret ? secrets[field.key] ?? '' : fields[field.key] ?? ''} onInput={e => { const value = e.currentTarget.value; if (field.secret) setSecrets(s => ({ ...s, [field.key]: value })); else setFields(f => ({ ...f, [field.key]: value })); }} />}</label>;
  }
  return <Dialog closeDisabled={busy} className="connection-dialog" title={profile ? "연결 편집" : "연결 추가"} label={profile ? 'Edit connection' : 'Add connection'} closeLabel="Close connection form" onClose={close} footer={<><ConnectButton state={pendingAction==='test'?'running':testStatus==='success'?'done':testStatus==='error'?'failed':'idle'} aria-label="Test connection" data-testid="test-connection-form" type="button" disabled={busy || !manifest} onClick={() => { if (form.current?.reportValidity()) void submit(true); }}>연결 테스트</ConnectButton>{testStatus!=='idle'&&<span class="ax-test-status" data-status={testStatus} role="status">{testStatus==='success'?<Check aria-hidden="true"/>:null}{testStatus==='success'?'테스트 통과':'테스트 실패'}</span>}<span class="toolbar-spacer" /><Button morph={false} type="button" disabled={busy} onClick={close}>취소</Button><Button morph variant="brand" loading={pendingAction==='save'} form={formId} type="submit" disabled={busy || !manifest}>저장</Button></>}>
    <section class="profile-form connection-form"><nav class="ax-steps" aria-label="연결 설정 단계">{['기본','네트워크','인증','고급'].map((name,index)=><Button bare key={name} type="button" aria-label={`${index+1}. ${name}`} data-done={testStatus==='success'} aria-current={step===index?'step':undefined} onClick={()=>setStep(index)}><span class="ax-step-number" aria-hidden="true">{testStatus==='success'?<Check/>:index+1}</span><span>{name}</span></Button>)}</nav><form onInvalidCapture={e=>{const details=(e.target as HTMLElement).closest('details');if(details){details.open=true;setStep(Array.from(form.current?.querySelectorAll('.field-group')??[]).indexOf(details));}}} onInput={()=>{setTestStatus('idle');setStages([]);setNotice('');}} onChange={()=>{setTestStatus('idle');setStages([]);setNotice('');}} id={formId} ref={form} autoComplete="off" onSubmit={e => { e.preventDefault(); void submit(false); }}>
      <fieldset class="ax-form-fields" disabled={busy}><details class="field-group" open={step===0}><summary onClick={()=>setStep(0)}><ChevronRight />기본 연결</summary><label class="ax-field">스마트 붙여넣기<input aria-label="스마트 붙여넣기" type="password" autoComplete="off" value={paste} placeholder="연결 URL, JDBC, psql 명령, SSH Host" onInput={e=>setPaste(e.currentTarget.value)}/></label><Button morph={false} type="button" disabled={!paste.trim()||busy} onClick={()=>{const input=paste;setPaste('');setBusy(true);void api('connection_parse',{input,profileId:profileId.current}).then(parsed=>{const driver=drivers.find(d=>d.id===parsed.driver_id);if(!driver)throw new Error('먼저 해당 드라이버를 설치하세요.');if(profile&&profile.driver_id!==parsed.driver_id)throw new Error('기존 연결과 다른 드라이버입니다. 새 연결을 만드세요.');setDriverId(parsed.driver_id);setFields({...defaults(driver),...parsed.fields});setNetwork(parsed.network??{});setSecrets({});if(parsed.has_secret){setSaveSecret(true);setHasStoredSecret(true);}setNotice(parsed.has_secret?'연결 정보를 채웠습니다. 비밀번호는 키체인으로 이동했습니다.':'연결 정보를 채웠습니다.');}).catch(onError).finally(()=>setBusy(false));}}>안전하게 채우기</Button><div class="field-grid">
        <label><span class="field-label">이름<span class="required-dot" aria-label="Required" /></span><input aria-label="Name" required autoFocus value={name} onInput={e => setName(e.currentTarget.value)} /></label>
        <label><span class="field-label">드라이버</span><GlideSelect ariaLabel="Driver" disabled={!!profile} value={driverId} options={drivers.map(d => ({ value: d.id, label: d.display_name }))} onChange={id => { setDriverId(id); setHasStoredSecret(false); setAccessMode('read_write'); setFields(defaults(drivers.find(d => d.id === id))); setSecrets({}); }} /></label>
        {manifest?.connection_fields.filter(field => group(field) === 'Connection').map(fieldControl)}
      </div></details>
      <details class="field-group" open={step===1}><summary onClick={()=>setStep(1)}><ChevronRight />네트워크</summary><Suspense fallback={null}>{step===1 && <NetworkFields network={network} onNetwork={setNetwork} secrets={networkSecrets} onSecrets={setNetworkSecrets}/>}</Suspense></details>
      {<details class="field-group" open={step===2}><summary onClick={()=>setStep(2)}><ChevronRight />인증</summary><Suspense fallback={null}>{step===2 && <IamFields auth={auth} onChange={setAuth}/>}</Suspense><div class="field-grid">{manifest?.connection_fields.filter(field => group(field) === 'Authentication').map(fieldControl)}{manifest?.connection_fields.some(f => f.secret) && <SpringCheck checked={saveSecret} onChange={setSaveSecret} label="키체인에 저장" />}</div></details>}
      <details class="field-group" open={step===3}><summary onClick={()=>setStep(3)}><ChevronRight />고급 설정</summary><div class="field-grid">{manifest?.connection_fields.filter(field => group(field) === 'Advanced').map(fieldControl)}{manifest?.capabilities.read_only_exec && <label><span class="field-label">접근 모드</span><GlideSelect ariaLabel="Connection access mode" name="access_mode" value={accessMode} options={[{ value: 'read_write', label: '읽기 / 쓰기' }, { value: 'read_only', label: '읽기 전용' }]} onChange={value => setAccessMode(value as Profile['access_mode'])} /></label>}</div></details>
      <div class="ax-inline"><Button morph={false} type="button" disabled={step===0} onClick={()=>setStep(n=>n-1)}>이전 단계</Button><Button morph={false} type="button" disabled={step===3} onClick={()=>setStep(n=>n+1)}>다음 단계</Button></div></fieldset>{manifest&&(pendingAction==='test'||stages.length>0||testStatus!=='idle')&&<ConnectionTimeline events={stages} manifest={manifest} network={network} running={pendingAction==='test'}/>}
      {notice && <p role="status">{notice}</p>}
    </form></section>
  </Dialog>;
}
function connectionFieldAriaLabel(key: string, fallback: string) {
  return ({ host: 'Host', port: 'Port', database: 'Database', user: 'User', username: 'Username', password: 'Password', uri: 'URI', authSource: 'Authentication database', auth_source: 'Authentication database', sslmode: 'TLS mode', tls: 'TLS', sslrootcert: 'TLS CA file', tlsCAFile: 'TLS CA file', tls_ca_file: 'TLS CA file', path: 'Database file', read_only_open: 'Open read-only', db: 'db' } as Record<string, string>)[key] ?? fallback;
}
function FieldInput({ field, existing, value, onInput }: { field: import('./contract').FieldSpec; existing: boolean; value: string; onInput: (e: InputEvent & { currentTarget: HTMLInputElement }) => void }) {
  const attrs = { 'aria-label': connectionFieldAriaLabel(field.key, field.label), value, onInput, required: field.required && !(existing && field.secret), placeholder: existing && field.secret ? '비워 두면 저장된 값을 유지합니다' : field.placeholder ?? '' };
  if (field.secret || field.kind === 'Password') return <input {...attrs} type="password" autoComplete="new-password" />;
  if (field.kind === 'Number') return <input {...attrs} type="number" />;
  return <input {...attrs} type="text" autoComplete="off" />;
}

