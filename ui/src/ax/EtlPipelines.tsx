import { Check, Copy, Plus, Trash2, ArrowUp, ArrowDown, X } from 'lucide-preact';
import { when } from '../library/parts';
import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import type { ComponentChildren } from 'preact';
import { listen } from '@tauri-apps/api/event';
import { api, isMock } from '../api';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';
import { JellyRadio } from '../ui/JellyRadio';
import { SpringCheck } from '../ui/SpringCheck';
import { Dialog } from '../Dialog';
import { ChangePreviewView } from '../ChangePreviewView';
import type { DriverManifest, ExplorerNode, Profile, Value } from '../contract';
import { normalizeError, valueText as cellText } from '../values';
import { CAST_TYPES, CMPS, STEP_OPS, TEXT_FUNCS, VALUE_KINDS, canBeSource, canBeTarget, emptyPipeline, makeValue, needsValue, newStep, specKey, statusLabel, stepLabel, summarizeRun, validatePipeline, valueKind, valueText, MAX_STEPS, type EtlProgressEvent, type Pipeline, type PreviewReport, type Progress, type RunEntry, type RunReport, type Step, type StepOp, type ValueKind } from './etlContract';
import './etlContract';
import './etl.css';

const STEPS = ['소스', '변환', '대상', '미리보기', '실행'] as const;
const SAMPLE_CAP = 50, RECEIPT_MS = 55_000;
const browsable = (nodes: ExplorerNode[]) => nodes.filter(node => node.actions.some(action => action.kind === 'Browse')).map(node => ({ value: node.id, label: node.label }));
const sameJson = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

function Field({ label, children, help }: { label: string; children: ComponentChildren; help?: string }) {
  return <label class="ax-field wb-field"><span class="wb-field__label">{label}</span>{children}{help && <small class="wb-help">{help}</small>}</label>;
}
/** 쉼표로 구분한 컬럼 이름 목록. 입력 중인 문자열을 보존하기 위해 로컬 상태를 둔다. */
function ListInput({ label, value, onChange, disabled }: { label: string; value: string[]; onChange: (value: string[]) => void; disabled?: boolean }) {
  const parse = (text: string) => text.split(',').map(item => item.trim()).filter(Boolean);
  const [text, setText] = useState(value.join(', '));
  useEffect(() => { if (!sameJson(parse(text), value)) setText(value.join(', ')); }, [value]);
  return <Field label={label} help="쉼표로 구분합니다."><input aria-label={label} disabled={disabled} value={text} onInput={event => { setText(event.currentTarget.value); onChange(parse(event.currentTarget.value)); }} /></Field>;
}
function ValueInput({ label, value, onChange, disabled }: { label: string; value: Value | undefined; onChange: (value: Value | null) => void; disabled?: boolean }) {
  const [kind, setKind] = useState<ValueKind>(valueKind(value)), [text, setText] = useState(value ? valueText(value) === 'NULL' ? '' : valueText(value) : '');
  const parsed = makeValue(kind, text);
  const update = (nextKind: ValueKind, nextText: string) => { setKind(nextKind); setText(nextText); onChange(makeValue(nextKind, nextText)); };
  return <div class="wb-pair etl-value"><Field label={`${label} 종류`}><Select ariaLabel={`${label} 종류`} disabled={disabled} value={kind} options={VALUE_KINDS} onChange={next => update(next as ValueKind, text)} /></Field>
    {kind !== 'null' && <Field label={label} help={parsed ? undefined : kind === 'bool' ? 'true 또는 false를 입력하세요.' : '숫자 형식이 올바르지 않습니다.'}><input aria-label={label} disabled={disabled} value={text} onInput={event => update(kind, event.currentTarget.value)} /></Field>}</div>;
}
function StepEditor({ step, onChange, disabled }: { step: Step; onChange: (step: Step) => void; disabled?: boolean }) {
  const column = (value: string, set: (value: string) => void, label = '컬럼') => <Field label={label}><input aria-label={label} disabled={disabled} value={value} onInput={event => set(event.currentTarget.value)} /></Field>;
  switch (step.op) {
    case 'select': return <div class="ax-stack">{step.columns.map((c, i) => <div class="wb-pair etl-select-row" key={i}>
      <Field label={`원본 컬럼 ${i + 1}`}><input aria-label={`원본 컬럼 ${i + 1}`} disabled={disabled} value={c.from} onInput={event => onChange({ ...step, columns: step.columns.map((x, j) => j === i ? { ...x, from: event.currentTarget.value } : x) })} /></Field>
      <div class="etl-row"><Field label={`결과 이름 ${i + 1}`}><input aria-label={`결과 이름 ${i + 1}`} disabled={disabled} value={c.to} placeholder="원본과 같게" onInput={event => onChange({ ...step, columns: step.columns.map((x, j) => j === i ? { ...x, to: event.currentTarget.value } : x) })} /></Field>
        <Button morph={false} ariaLabel={`컬럼 ${i + 1} 제거`} disabled={disabled || step.columns.length < 2} onClick={() => onChange({ ...step, columns: step.columns.filter((_, j) => j !== i) })}><X size={14} /></Button></div></div>)}
      <div class="wb-row"><Button morph={false} disabled={disabled} onClick={() => onChange({ ...step, columns: [...step.columns, { from: '', to: '' }] })}>컬럼 추가</Button></div></div>;
    case 'rename': return <div class="wb-pair">{column(step.from, from => onChange({ ...step, from }), '현재 이름')}{column(step.to, to => onChange({ ...step, to }), '새 이름')}</div>;
    case 'drop': case 'dedupe': return <ListInput label={step.op === 'drop' ? '제거할 컬럼' : '중복 판단 키 컬럼'} disabled={disabled} value={step.columns} onChange={columns => onChange({ ...step, columns })} />;
    case 'cast': return <div class="wb-pair">{column(step.column, c => onChange({ ...step, column: c }))}<Field label="변환할 형"><Select ariaLabel="변환할 형" disabled={disabled} value={step.to} options={CAST_TYPES} onChange={to => onChange({ ...step, to: to as typeof step.to })} /></Field></div>;
    case 'filter': return <div class="ax-stack"><div class="wb-pair">{column(step.column, c => onChange({ ...step, column: c }))}<Field label="조건"><Select ariaLabel="조건" disabled={disabled} value={step.cmp} options={CMPS.map(c => ({ value: c.value, label: c.label }))} onChange={cmp => onChange({ ...step, cmp: cmp as typeof step.cmp, value: needsValue(cmp as typeof step.cmp) ? step.value ?? { t: 'Text', v: '' } : undefined })} /></Field></div>
      {needsValue(step.cmp) && <ValueInput label="비교할 값" disabled={disabled} value={step.value} onChange={value => onChange({ ...step, value: value ?? undefined })} />}<small class="wb-help">필터는 조건에 맞는 행만 남기며, 여러 필터는 AND로 결합합니다. NULL과의 비교는 제외됩니다.</small></div>;
    case 'constant': case 'fill_null': return <div class="ax-stack">{column(step.column, c => onChange({ ...step, column: c }))}<ValueInput label={step.op === 'constant' ? '상수 값' : '대체 값'} disabled={disabled} value={step.value} onChange={value => { if (value) onChange({ ...step, value }); }} /></div>;
    case 'text': return <div class="wb-pair">{column(step.column, c => onChange({ ...step, column: c }))}<Field label="변환"><Select ariaLabel="텍스트 변환" disabled={disabled} value={step.func} options={TEXT_FUNCS} onChange={func => onChange({ ...step, func: func as typeof step.func })} /></Field></div>;
    case 'concat': return <div class="ax-stack"><ListInput label="합칠 컬럼" disabled={disabled} value={step.inputs} onChange={inputs => onChange({ ...step, inputs })} /><div class="wb-pair">{column(step.column, c => onChange({ ...step, column: c }), '결과 컬럼')}<Field label="구분자"><input aria-label="구분자" disabled={disabled} value={step.separator} onInput={event => onChange({ ...step, separator: event.currentTarget.value })} /></Field></div></div>;
  }
}
function Summary({ progress }: { progress: Progress }) {
  return <span>읽음 {progress.rows_read.toLocaleString()}행 · 반영 {progress.rows_written.toLocaleString()}행 · 제외 {progress.rows_filtered.toLocaleString()}행 · 건너뜀 {progress.rows_rejected.toLocaleString()}행 · 배치 {progress.batches_committed.toLocaleString()}개</span>;
}

export function EtlPipelines({ profiles, sessions, drivers, nodes, selected }: { profiles: Profile[]; sessions: Record<string, string>; drivers: DriverManifest[]; nodes: Record<string, ExplorerNode[]>; selected: string }) {
  const [list, setList] = useState<Pipeline[]>([]), [draft, setDraft] = useState<Pipeline>(() => init(selected)), [saved, setSaved] = useState<string>(() => specKey(init(selected)));
  const [step, setStep] = useState(0), [busy, setBusy] = useState<'preview' | 'run' | 'save' | null>(null), [error, setError] = useState(''), [notice, setNotice] = useState('');
  const [preview, setPreview] = useState<{ key: string; report: PreviewReport } | null>(null), [expired, setExpired] = useState(false);
  const [progress, setProgress] = useState<Progress | null>(null), [report, setReport] = useState<RunReport | null>(null), [runs, setRuns] = useState<RunEntry[]>([]);
  const [newStepOp, setNewStepOp] = useState<StepOp>('filter'), [createTable, setCreateTable] = useState(false), [namespace, setNamespace] = useState(''), [tableName, setTableName] = useState('');
  const [confirm, setConfirm] = useState<{ title: string; body: string; label: string; action: () => void } | null>(null);
  const runId = useRef<string | null>(null), expiry = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  // The id is part of the previewed spec, so assign it here instead of on first save.
  function init(profile: string): Pipeline { const p = emptyPipeline(); p.id = crypto.randomUUID(); p.source.profile_id = profile; return p; }
  const capabilities = (profileId: string) => drivers.find(driver => driver.id === profiles.find(profile => profile.id === profileId)?.driver_id)?.capabilities;
  const connected = useMemo(() => profiles.filter(profile => sessions[profile.id]), [profiles, sessions]);
  const tag = (profile: Profile) => drivers.find(driver => driver.id === profile.driver_id)?.display_name;
  const sourceOptions = useMemo(() => connected.filter(p => canBeSource(draft.source.kind, capabilities(p.id))).map(p => ({ value: p.id, label: p.name, tag: tag(p) })), [connected, drivers, draft.source.kind]);
  const targetOptions = useMemo(() => connected.filter(p => canBeTarget(p, capabilities(p.id))).map(p => ({ value: p.id, label: p.name, tag: tag(p) })), [connected, drivers]);
  const sourceTables = useMemo(() => browsable(nodes[draft.source.profile_id] ?? []), [nodes, draft.source.profile_id]);
  const targetTables = useMemo(() => browsable(nodes[draft.target.profile_id] ?? []), [nodes, draft.target.profile_id]);
  const sourceSession = sessions[draft.source.profile_id] ?? '', targetSession = sessions[draft.target.profile_id] ?? '';
  const resolving = createTable && !!tableName.trim();
  // 새 테이블은 미리보기 때 import_target으로 노드 id를 얻는다. 그 전에는 자리 표시 값으로 검증한다.
  const problems = useMemo(() => [...validatePipeline(resolving && !draft.target.node_id ? { ...draft, target: { ...draft.target, node_id: 'pending' } } : draft), ...(sourceSession ? [] : ['소스 연결이 활성 상태가 아닙니다']), ...(targetSession ? [] : ['대상 연결이 활성 상태가 아닙니다'])], [draft, resolving, sourceSession, targetSession]);
  const key = `${specKey(draft)}|${sourceSession}|${targetSession}`;
  const previewValid = !!preview && preview.key === key && !expired;
  const dirty = specKey(draft) !== saved;
  const locked = busy !== null;

  async function reload() { try { setList(await api('etl_list', {})); setRuns(await api('etl_runs', { limit: 20 })); } catch (e) { setError(normalizeError(e).message); } }
  useEffect(() => { void reload(); }, []);
  useEffect(() => { if (isMock) return; let alive = true; const sub = listen<EtlProgressEvent>('etl-progress', event => { if (alive && event.payload.run_id === runId.current) setProgress(event.payload); }); sub.catch(() => setError('진행률 알림을 연결하지 못했습니다.')); return () => { alive = false; void sub.then(off => off()).catch(() => {}); }; }, []);
  useEffect(() => () => clearTimeout(expiry.current), []);

  function edit(change: (p: Pipeline) => void) { const next = structuredClone(draft); change(next); setDraft(next); setNotice(''); setReport(null); }
  function load(p: Pipeline) { setDraft(structuredClone(p)); setSaved(specKey(p)); setPreview(null); setExpired(false); setReport(null); setProgress(null); setError(''); setNotice(''); setStep(0); setCreateTable(p.target.create_table); setTableName(''); setNamespace(''); }
  const guarded = (action: () => void) => dirty ? setConfirm({ title: '저장하지 않은 변경을 버릴까요?', body: '이 파이프라인의 수정 내용은 저장되지 않았습니다.', label: '변경 버리고 이동', action }) : action();
  const stepMove = (index: number, delta: number) => edit(p => { const [item] = p.steps.splice(index, 1); p.steps.splice(index + delta, 0, item); });

  async function save() {
    if (problems.some(m => !m.includes('활성 상태'))) { setError(problems.join(' · ')); return; }
    if (resolving && !draft.target.node_id) { setError('새 테이블은 미리보기를 한 번 실행해 대상 노드가 정해진 뒤에 저장할 수 있습니다.'); return; }
    setBusy('save'); setError('');
    try { const result = await api('etl_save', { pipeline: draft }); setDraft(result); setSaved(specKey(result)); setNotice('저장했습니다.'); if (result.id !== draft.id) setPreview(null); await reload(); }
    catch (e) { setError(normalizeError(e).message); } finally { setBusy(null); }
  }
  async function runPreview() {
    if (locked || problems.length) { setError(problems.join(' · ')); return; }
    const id = crypto.randomUUID(); runId.current = id; setBusy('preview'); setError(''); setPreview(null); setExpired(false); clearTimeout(expiry.current);
    try {
      let pipeline = draft;
      if (resolving) { const nodeId = await api('import_target', { sessionId: targetSession, namespace: namespace.trim() || undefined, name: tableName.trim() }); pipeline = { ...draft, target: { ...draft.target, node_id: nodeId, create_table: true } }; setDraft(pipeline); }
      const result = await api('etl_preview', { pipeline, sourceSessionId: sourceSession, targetSessionId: targetSession, runId: id });
      setPreview({ key: `${specKey(pipeline)}|${sourceSession}|${targetSession}`, report: result });
      expiry.current = setTimeout(() => setExpired(true), RECEIPT_MS);
    } catch (e) { setError(normalizeError(e).message); } finally { runId.current = null; setBusy(null); }
  }
  async function runPipeline() {
    if (locked || !previewValid) return;
    const id = crypto.randomUUID(); runId.current = id; setBusy('run'); setError(''); setReport(null); setProgress(null); setStep(4);
    try { const result = await api('etl_run', { pipeline: draft, sourceSessionId: sourceSession, targetSessionId: targetSession, runId: id }); setReport(result); }
    catch (e) { setError(normalizeError(e).message); }
    finally { runId.current = null; setBusy(null); setPreview(null); clearTimeout(expiry.current); void reload(); }
  }
  const cancel = () => { if (runId.current) void api('cancel', { runId: runId.current }).catch(e => setError(normalizeError(e).message)); };
  async function remove(p: Pipeline) { try { await api('etl_delete', { id: p.id }); if (draft.id === p.id) load(init(selected)); await reload(); } catch (e) { setError(normalizeError(e).message); } }

  const sourcePanel = <div class="ax-stack">
    <div class="ax-field wb-field"><span class="wb-field__label">소스 종류</span><JellyRadio ariaLabel="소스 종류" disabled={locked} value={draft.source.kind} items={[{ value: 'query', label: '쿼리' }, { value: 'table', label: '테이블' }]} onChange={kind => edit(p => { p.source = kind === 'query' ? { kind: 'query', profile_id: p.source.profile_id, text: '' } : { kind: 'table', profile_id: p.source.profile_id, node_id: '' }; })} /></div>
    <Field label="소스 연결" help={draft.source.kind === 'query' ? '읽기 전용 쿼리를 실행할 수 있는 연결만 표시합니다.' : '테이블을 페이지 단위로 읽을 수 있는 연결만 표시합니다.'}><Select ariaLabel="소스 연결" disabled={locked} value={draft.source.profile_id} options={sourceOptions} placeholder="연결 선택…" emptyText="사용할 수 있는 활성 연결이 없습니다" onChange={value => edit(p => { p.source.profile_id = value; if (p.source.kind === 'table') p.source.node_id = ''; })} /></Field>
    {draft.source.kind === 'query'
      ? <Field label="소스 쿼리" help="읽기 전용으로 한 번 실행하며 최대 100,000행까지 가져옵니다."><textarea aria-label="소스 쿼리" disabled={locked} rows={6} value={draft.source.text} onInput={event => { const text = event.currentTarget.value; edit(p => { if (p.source.kind === 'query') p.source.text = text; }); }} /></Field>
      : <Field label="소스 테이블" help="탐색기에서 테이블을 펼친 후 선택하세요."><Select ariaLabel="소스 테이블" disabled={locked} value={draft.source.node_id} options={sourceTables} placeholder="테이블 선택…" emptyText="탐색기에서 소스 테이블을 펼친 후 선택하세요" onChange={value => edit(p => { if (p.source.kind === 'table') p.source.node_id = value; })} /></Field>}
  </div>;

  const transformPanel = <div class="ax-stack">
    {!draft.steps.length && <p class="wb-help">변환 단계가 없으면 소스 데이터를 그대로 적재합니다. 단계는 위에서 아래로 순서대로 실행됩니다.</p>}
    <ol class="etl-steps">{draft.steps.map((s, i) => <li key={i} class="etl-step">
      <div class="etl-step__head"><strong>{i + 1}. {stepLabel(s)}</strong><div class="etl-row">
        <Button morph={false} ariaLabel={`단계 ${i + 1} 위로`} disabled={locked || i === 0} onClick={() => stepMove(i, -1)}><ArrowUp size={14} /></Button>
        <Button morph={false} ariaLabel={`단계 ${i + 1} 아래로`} disabled={locked || i === draft.steps.length - 1} onClick={() => stepMove(i, 1)}><ArrowDown size={14} /></Button>
        <Button morph={false} variant="destructive" ariaLabel={`단계 ${i + 1} 제거`} disabled={locked} onClick={() => edit(p => { p.steps.splice(i, 1); })}><Trash2 size={14} /></Button></div></div>
      <StepEditor step={s} disabled={locked} onChange={next => edit(p => { p.steps[i] = next; })} /></li>)}</ol>
    <div class="wb-pair"><Field label="추가할 단계"><Select ariaLabel="추가할 단계" disabled={locked} value={newStepOp} options={STEP_OPS} onChange={value => setNewStepOp(value as StepOp)} /></Field>
      <div class="wb-row"><Button morph={false} icon={<Plus size={14} />} disabled={locked || draft.steps.length >= MAX_STEPS} onClick={() => edit(p => { p.steps.push(newStep(newStepOp)); })}>단계 추가</Button></div></div>
  </div>;

  const targetPanel = <div class="ax-stack">
    <Field label="대상 연결" help="쓰기 가능한 연결만 표시합니다. 읽기 전용 연결은 대상으로 쓸 수 없습니다."><Select ariaLabel="대상 연결" disabled={locked} value={draft.target.profile_id} options={targetOptions} placeholder="연결 선택…" emptyText="쓰기 가능한 활성 연결이 없습니다" onChange={value => edit(p => { p.target.profile_id = value; p.target.node_id = ''; })} /></Field>
    <div class="ax-field wb-field"><span class="wb-field__label">적재 방식</span><JellyRadio ariaLabel="적재 방식" disabled={locked} value={createTable ? 'new' : 'existing'} items={[{ value: 'existing', label: '기존 테이블' }, { value: 'new', label: '새 테이블 생성' }]} onChange={value => { setCreateTable(value === 'new'); edit(p => { p.target.create_table = value === 'new'; p.target.node_id = ''; }); }} /></div>
    {createTable
      ? <><div class="wb-pair"><Field label="네임스페이스"><input aria-label="네임스페이스" disabled={locked} value={namespace} placeholder="기본 스키마" onInput={event => { setNamespace(event.currentTarget.value); edit(p => { p.target.node_id = ''; }); }} /></Field><Field label="새 테이블 이름"><input aria-label="새 테이블 이름" disabled={locked} value={tableName} onInput={event => { setTableName(event.currentTarget.value); edit(p => { p.target.node_id = ''; }); }} /></Field></div>
        {draft.target.node_id && !tableName.trim() && <p class="wb-help">생성할 테이블: {draft.target.node_id}</p>}
        <small class="wb-help">미리보기를 실행할 때 대상 노드가 결정됩니다. 같은 이름의 테이블이 이미 있으면 실행이 실패하므로, 저장한 파이프라인을 다시 실행할 때는 기존 테이블을 선택하세요.</small></>
      : <Field label="대상 테이블"><Select ariaLabel="대상 테이블" disabled={locked} value={draft.target.node_id} options={targetTables} placeholder="테이블 선택…" emptyText="탐색기에서 대상 테이블을 펼친 후 선택하세요" onChange={value => edit(p => { p.target.node_id = value; })} /></Field>}
    <div class="wb-pair">
      <Field label="배치 크기(행)" help="1..10,000. 배치마다 별도 트랜잭션으로 커밋합니다."><input type="number" aria-label="배치 크기" min={1} max={10000} disabled={locked} value={draft.options.batch_size} onInput={event => { const n = event.currentTarget.valueAsNumber; edit(p => { p.options.batch_size = Number.isNaN(n) ? 0 : n; }); }} /></Field>
      <Field label="최대 행 수" help="비워두면 제한하지 않습니다."><input type="number" aria-label="최대 행 수" min={1} disabled={locked} value={draft.options.max_rows ?? ''} onInput={event => { const n = event.currentTarget.valueAsNumber; edit(p => { p.options.max_rows = Number.isNaN(n) ? null : n; }); }} /></Field></div>
    <SpringCheck label="오류가 난 행은 건너뛰고 계속 진행" disabled={locked} checked={draft.options.on_error === 'skip_row'} onChange={checked => edit(p => { p.options.on_error = checked ? 'skip_row' : 'abort'; })} />
    <small class="wb-help">끄면 변환 오류가 나는 즉시 실행을 중단합니다.</small>
  </div>;

  const r = preview?.report;
  const previewPanel = <div class="ax-stack">
    <div class="wb-row"><Button morph={false} variant="brand" loading={busy === 'preview'} disabled={locked || problems.length > 0} onClick={() => void runPreview()}>미리보기 실행</Button>{busy === 'preview' && <Button morph={false} onClick={cancel}>취소</Button>}</div>
    {problems.length > 0 && <ul class="etl-problems" aria-label="입력 확인">{problems.map(m => <li key={m} class="wb-error">{m}</li>)}</ul>}
    <p class="wb-help">미리보기는 표본 최대 200행을 변환하고 대상에서 롤백하여 결과를 보여줍니다. 대상에는 아무것도 기록되지 않습니다. 명세를 바꾸면 다시 실행해야 합니다.</p>
    {preview && !previewValid && <p class="wb-error" role="status">{expired ? '미리보기가 만료되었습니다. 다시 실행하세요.' : '명세가 바뀌었습니다. 미리보기를 다시 실행하세요.'}</p>}
    {r && <div class="ax-stack" data-stale={!previewValid}>
      {r.warnings.map((w, i) => <p key={i} class="wb-help" role="note">{w}</p>)}
      {r.source_truncated && <p class="wb-help">쿼리 결과가 행 수 상한에서 잘렸습니다.</p>}
      <p>표본 {r.rows_read.toLocaleString()}행 읽음 · 필터 제외 {r.rows_filtered.toLocaleString()}행 · 오류 {r.rows_rejected.toLocaleString()}행 · 출력 {r.sample.length.toLocaleString()}행</p>
      <div><h3 class="etl-h">출력 컬럼 ({r.output_columns.length})</h3><p class="etl-columns">{r.output_columns.map(c => <span key={c.name} class="etl-chip">{c.name} <small>{c.type_name}</small></span>)}</p></div>
      <div class="etl-table-wrap" tabIndex={0} role="region" aria-label="변환 결과 표본"><table class="ax-table"><thead><tr>{r.output_columns.map(c => <th key={c.name}>{c.name}</th>)}</tr></thead><tbody>{r.sample.slice(0, SAMPLE_CAP).map((row, i) => <tr key={i}>{row.map((cell, j) => <td key={j}>{cellText(cell)}</td>)}</tr>)}</tbody></table></div>
      {r.sample.length > SAMPLE_CAP && <small class="wb-help">처음 {SAMPLE_CAP}행만 표시합니다.</small>}
      {r.rejected.length > 0 && <div><h3 class="etl-h">오류 행 표본</h3><ul>{r.rejected.map((x, i) => <li key={i} class="wb-error">{x.row}행 · 단계 {x.step + 1}: {x.message}</li>)}</ul></div>}
      {r.target && <div class="etl-preview-target"><ChangePreviewView preview={r.target} /></div>}
    </div>}
  </div>;

  const last = report ?? null;
  const runPanel = <div class="ax-stack">
    <p class="wb-help">배치마다 별도로 커밋합니다. 실행 도중 실패하거나 취소하면 앞서 커밋된 배치는 대상에 그대로 남으며 자동으로 되돌리지 않습니다.</p>
    <div class="wb-row"><Button morph={false} variant="brand" loading={busy === 'run'} disabled={locked || !previewValid} onClick={() => void runPipeline()}>실행</Button>{busy === 'run' && <Button morph={false} variant="destructive" onClick={cancel}>취소</Button>}</div>
    {!previewValid && !locked && <p class="wb-help">{last ? '미리보기 승인은 한 번만 쓸 수 있습니다. 다시 실행하려면 미리보기를 다시 하세요.' : '실행하려면 현재 명세로 미리보기를 먼저 성공시켜야 합니다.'}</p>}
    {busy === 'run' && <div role="status"><progress aria-label="파이프라인 진행률" value={draft.options.max_rows && progress ? Math.min(progress.rows_read, draft.options.max_rows) : undefined} max={draft.options.max_rows ?? undefined} /><br />{progress ? <><Summary progress={progress} /> · {progress.phase === 'extract' ? '추출 중' : progress.phase === 'load' ? '적재 중' : '마무리 중'}</> : <span>시작하는 중…</span>}</div>}
    {last && <div role="status" class={last.status === 'completed' ? 'etl-result' : 'etl-result wb-error'}><strong>{summarizeRun(last)}</strong><br /><Summary progress={last} /> · {(last.elapsed_ms / 1000).toFixed(1)}초
      {last.status === 'failed' && last.batches_committed > 0 && <p>연결 오류로 끝났다면 마지막 배치의 반영 여부를 대상에서 직접 확인하세요.</p>}
      {last.rejected.length > 0 && <ul>{last.rejected.map((x, i) => <li key={i}>{x.row}행 · 단계 {x.step + 1}: {x.message}</li>)}</ul>}</div>}
    <div><h3 class="etl-h">최근 실행</h3>{runs.length ? <ul class="etl-runs">{runs.map(run => <li key={run.run_id}><strong>{run.pipeline_name}</strong> · {statusLabel(run.status)} · {run.rows_written.toLocaleString()}행 / {run.batches_committed}개 배치 · <time dateTime={run.finished_at}>{when(run.finished_at)}</time></li>)}</ul> : <p class="wb-help">실행 기록이 없습니다.</p>}</div>
  </div>;

  const panels = [sourcePanel, transformPanel, targetPanel, previewPanel, runPanel];
  return <div class="etl-layout">
    <aside class="etl-list" aria-label="저장된 파이프라인">
      <div class="wb-row"><Button morph={false} icon={<Plus size={14} />} disabled={locked} onClick={() => guarded(() => load(init(selected)))}>새 파이프라인</Button>
        <Button morph={false} icon={<Copy size={14} />} disabled={locked || !draft.id} onClick={() => guarded(() => { const copy = structuredClone(draft); copy.id = crypto.randomUUID(); copy.name = `${draft.name} 복사본`; load(copy); setSaved(''); })}>복제</Button></div>
      {list.length ? <ul>{list.map(p => <li key={p.id}><button type="button" class="etl-list__item" aria-current={p.id === draft.id ? 'true' : undefined} disabled={locked} onClick={() => p.id !== draft.id && guarded(() => load(p))}>{p.name}</button>
        <Button morph={false} variant="destructive" ariaLabel={`${p.name} 삭제`} disabled={locked} onClick={() => setConfirm({ title: `"${p.name}" 파이프라인을 삭제할까요?`, body: '저장된 명세만 삭제됩니다. 이미 대상에 적재된 데이터는 그대로 남습니다.', label: '삭제', action: () => void remove(p) })}><Trash2 size={14} /></Button></li>)}</ul> : <p class="wb-help">저장된 파이프라인이 없습니다. 오른쪽에서 소스·변환·대상을 정하고 저장하면 여기에 쌓입니다.</p>}
    </aside>
    <div class="etl-editor ax-stack">
      <Field label="파이프라인 이름"><input aria-label="파이프라인 이름" disabled={locked} value={draft.name} placeholder="예: 사용자 테이블 복사" onInput={event => { const name = event.currentTarget.value; edit(p => { p.name = name; }); }} /></Field>
      <nav class="ax-steps" aria-label="파이프라인 단계">{STEPS.map((name, index) => <Button bare key={name} aria-label={`${index + 1}. ${name}`} data-done={index < step} disabled={locked} aria-current={index === step ? 'step' : undefined} onClick={() => setStep(index)}><span class="ax-step-number" aria-hidden="true">{index < step ? <Check /> : index + 1}</span><span>{name}</span></Button>)}</nav>
      <div role="group" aria-label={`${step + 1}단계 ${STEPS[step]}`}>{panels[step]}</div>
      {error && <p class="ax-error wb-error" role="alert">{error}</p>}
      {notice && <p class="wb-help" role="status">{notice}</p>}
      <div class="wb-row etl-actions"><Button morph={false} disabled={locked || step === 0} onClick={() => setStep(step - 1)}>이전</Button><Button morph={false} disabled={locked || step === STEPS.length - 1} onClick={() => setStep(step + 1)}>다음</Button><Button morph={false} loading={busy === 'save'} disabled={locked || !dirty} onClick={() => void save()}>저장</Button>{dirty && <small class="wb-help">저장하지 않은 변경이 있습니다.</small>}</div>
    </div>
    {confirm && <Dialog label={confirm.title} title={confirm.title} onClose={() => setConfirm(null)} footer={<><Button morph={false} onClick={() => setConfirm(null)}>취소</Button><Button morph={false} variant="destructive" onClick={() => { const action = confirm.action; setConfirm(null); action(); }}>{confirm.label}</Button></>}><p>{confirm.body}</p></Dialog>}
  </div>;
}
