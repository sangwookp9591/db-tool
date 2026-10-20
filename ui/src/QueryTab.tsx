import type { QueryDraft } from './recycle';
import './results.css';
import { EditorTools } from './ax/EditorTools';
import { lazy, Suspense } from 'preact/compat';
const QueryPlan = lazy(() => import('./ax/QueryPlan').then(m => ({ default: m.QueryPlan })));
import type { Diagnostic } from './ax/editorContract';
import { Button } from './ui/Button';
import { JellyRadio } from './ui/JellyRadio';
import { Skeleton } from './ui/Skeleton';
import { CometDial } from './ui/CometDial';
import { WakeSlider } from './ui/WakeSlider';
import { BorderGlow } from './ui/BorderGlow';
import { LatticeLoader } from './ui/LatticeLoader';
import { ChevronRight, Play, Square, ScanEye, Settings2 } from 'lucide-preact';
import { useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';
import type { DriverManifest, ExecRequest, ResultSet, ChangePreview, SchemaGraph, BrowseTarget, Value } from './contract';
import { api } from './api';
import { Editor, type EditorHandle } from './Editor';
import { Results } from './Results';
import { ErrorBanner } from './ErrorBanner';
import { normalizeError } from './values';
import { GlideSelect } from './ui/GlideSelect';
import { parseParams } from './params';
import { executeQuery } from './perf/stream';
import { ChangePreviewView } from './ChangePreviewView';
import { Dialog } from './Dialog';
import { pendingCount, emptyEdits, buildEditRequest, type PendingEdits } from './edits';
interface Props { draft?: QueryDraft; onDraft?: (draft: QueryDraft) => void; onDirty?: (dirty: boolean) => void; aiSuggested?: boolean; sourceText?: string; entityId?: string; schema?: SchemaGraph; onBrowse?: (target:BrowseTarget)=>void; readOnly?: boolean; onResults?: (results: ResultSet[]) => void; active?: boolean; initialText: string; sessionId: string; manifest: DriverManifest; graphLimit: number; onRun: (runId: string | null) => void; onReady: (handle: EditorHandle | null) => void; onHistory: () => void; onError: (error: unknown) => void }
export function QueryTab(props: Props) {
  const container = useRef<HTMLDivElement>(null);
  const runShortcut = /Mac|iPhone|iPad/.test(navigator.platform) ? '⌘↵' : 'Ctrl+↵';
  const [paneRatio, setPaneRatio] = useState(props.draft?.paneRatio ?? .28), [paneHeight, setPaneHeight] = useState(500), [firstReceived, setFirstReceived] = useState(false);
  const available = Math.max(320, paneHeight - (props.manifest.capabilities.params ? 72 : 38) - 6);
  const editorHeight = Math.max(80, Math.min(available - 300, available * paneRatio));
  useLayoutEffect(() => { const element = container.current!; const resize = () => { if (element.clientHeight) setPaneHeight(element.clientHeight); }; resize(); const observer = new ResizeObserver(resize); observer.observe(element); return () => observer.disconnect(); }, []);
  function resizeEditor(event: PointerEvent) {
    event.preventDefault(); const handle = event.currentTarget as HTMLElement; handle.setPointerCapture(event.pointerId); const y = event.clientY, initial = editorHeight;
    const move = (e: PointerEvent) => setPaneRatio(Math.max(80, Math.min(available - 300, initial + e.clientY - y)) / available);
    const end = () => { handle.removeEventListener('pointermove', move); handle.removeEventListener('pointerup', end); handle.removeEventListener('pointercancel', end); };
    handle.addEventListener('pointermove', move); handle.addEventListener('pointerup', end); handle.addEventListener('pointercancel', end);
  }
  const editor = useRef<EditorHandle | null>(null);
  const text = useRef(props.initialText);
  const [queryText,setQueryText]=useState(props.initialText), [diagnostics,setDiagnostics]=useState<Diagnostic[]>([]), [refactorOpen,setRefactorOpen]=useState(false), [resultPane,setResultPane]=useState('results');
  useEffect(()=>{let alive=true;setDiagnostics([]);if(!props.manifest.capabilities.lint)return;const timer=window.setTimeout(()=>{void api('lint_query',{sessionId:props.sessionId,query:queryText}).then(value=>{if(alive)setDiagnostics(value);}).catch(()=>{});},300);return()=>{alive=false;clearTimeout(timer);};},[queryText,props.sessionId,props.manifest.capabilities.lint]);
  useEffect(()=>{const operation=(event:Event)=>{if(!props.active)return;const kind=(event as CustomEvent<string>).detail;if(kind==='plan')setResultPane('plan');else if(kind==='refactor')setRefactorOpen(true);};window.addEventListener('ax:editor',operation);return()=>window.removeEventListener('ax:editor',operation);},[props.active]);
  const [ranText, setRanText] = useState('');
  const dirty = useRef(false);
  useEffect(()=>{const next=text.current!==props.initialText;dirty.current=next;props.onDirty?.(next);},[props.initialText]);
  const running = useRef<string | null>(null);
  const mounted = useRef(true);
  const [runId, setRunId] = useState<string | null>(null);
  const previewRequest = useRef<ExecRequest | null>(null);
  const [preview, setPreview] = useState<ChangePreview | null>(null);
  const [resultEntity,setResultEntity]=useState<string|undefined>();
  const [results, setResults] = useState<ResultSet[]>([]);
  const [error, setError] = useState<ReturnType<typeof normalizeError> | null>(null);
  const [timeout, setTimeoutSeconds] = useState(props.draft?.timeout ?? 30);
  const [rowLimit, setRowLimit] = useState(props.draft?.rowLimit ?? 1000);
  const [resultRowLimit, setResultRowLimit] = useState(1000);
  const [params, setParams] = useState(props.draft?.params ?? '{}');
  useEffect(() => { props.onDraft?.({params, rowLimit, timeout, paneRatio}); }, [params, rowLimit, timeout, paneRatio]);
  // Every error raised inside this tab (run, editor, result views) is shown once here; the parent only reacts to connection-level ones.
  const fail = (e: unknown) => { setError(normalizeError(e)); props.onError(e); };
  let paramError = '';
  try { if (props.manifest.capabilities.params) parseParams(params); } catch (e) { paramError = (e as Error).message; }
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; running.current = null; }; }, []);
  async function run(previewOnly = false) {
    if (running.current || paramError) return;
    const query = editor.current?.selection() || editor.current?.text() || text.current;
    if (!query.trim()) return;
    const id = crypto.randomUUID(); running.current = id; setRunId(id); props.onRun(id); setError(null); setPreview(null); setFirstReceived(false);
    // shortcut: edited queries lack source metadata, enable their FK links when drivers return column provenance.
    setResultEntity(query===(props.sourceText??props.initialText)?props.entityId:undefined); setRanText(query);
    const req: ExecRequest = { text: query, params: props.manifest.capabilities.params ? parseParams(params) : {}, row_limit: rowLimit, graph_node_limit: props.graphLimit, timeout_ms: props.manifest.capabilities.timeout ? timeout * 1000 : 30000 };
    if (props.aiSuggested && !previewOnly) req.read_only = true;
    previewRequest.current = null;
    try { if (previewOnly) {
      const received = await api('preview', { sessionId: props.sessionId, runId: id, req: { ...req, preview_sample_limit: 50 }, source: props.aiSuggested ? 'agent' : 'ui' });
      previewRequest.current = { ...req, preview_sample_limit: 50 };
      if (mounted.current && running.current === id) { setPreview(received); setFirstReceived(true); }
      return;
    }
    const received = props.aiSuggested ? await api('execute_agent', { sessionId: props.sessionId, runId: id, req }) : await executeQuery({ sessionId: props.sessionId, runId: id, req }, partial => { if (mounted.current && running.current === id) { setResults(partial); if (partial.some(r => r.table?.rows.length || r.graph || r.documents || r.message)) setFirstReceived(true); setResultRowLimit(req.row_limit); } }); if (mounted.current && running.current === id) { setResults(received); props.onResults?.(received); setResultRowLimit(req.row_limit); } }
    catch (e) { if (mounted.current && running.current === id) { setError(normalizeError(e)); props.onError(e); } }
    finally { if (mounted.current && running.current === id) { running.current = null; setRunId(null); props.onRun(null); props.onHistory(); } }
  }
  async function approveSuggestion() {
    const req = previewRequest.current;
    if (!req || running.current) return;
    const id = crypto.randomUUID(); running.current = id; setRunId(id); props.onRun(id); setError(null);
    try {
      await api('agent_approve_write', {sessionId: props.sessionId, runId: id, req});
      previewRequest.current = null;
      const received = await api('execute_agent', {sessionId: props.sessionId, runId: id, req});
      if (mounted.current) { setPreview(null); setResults(received); props.onResults?.(received); }
    } catch (e) { if (mounted.current) { setError(normalizeError(e)); props.onError(e); } }
    finally { if (mounted.current) { running.current = null; setRunId(null); props.onRun(null); props.onHistory(); } }
  }
  return <div class="query-tab" ref={container}><div class="query-toolbar toolbar"><Button morph variant="brand" loading={!!runId} icon={<Play size={13} fill="currentColor" />} class="primary" aria-label="Run query" data-testid="run-query" onClick={() => void run()} disabled={!!runId || !!paramError}>{runId ? '실행 중…' : props.aiSuggested ? '읽기 전용 실행' : '실행'} <kbd>{runShortcut}</kbd></Button>{!props.readOnly && props.manifest.capabilities.preview_changes && props.manifest.capabilities.preview_changes !== 'None' && <Button morph={false} class="preview-button" aria-label="Preview changes" data-testid="preview-changes" disabled={!!runId || !!paramError} onClick={() => void run(true)}><ScanEye />미리보기</Button>}{props.manifest.capabilities.cancel !== 'None' && <Button morph={false} aria-label="Cancel query" data-testid="cancel-query" disabled={!runId} onClick={() => { if (runId) void api('cancel', { runId }).catch(fail); }}><Square size={12} fill="currentColor" />취소</Button>}<span class="execution-chip">{timeout}초 · {rowLimit.toLocaleString()}행</span><span class="toolbar-spacer" /><details class="options-menu"><summary aria-label="Execution options" title="실행 옵션"><Settings2 /></summary><div class="execution-options"><h3>실행 옵션</h3>{props.manifest.capabilities.timeout && <div class="timeout-control"><CometDial value={timeout} onChange={setTimeoutSeconds} aria-label="Timeout dial" /><label>시간 제한 <input aria-label="Timeout seconds" type="number" min="1" max="600" value={timeout} onInput={e => setTimeoutSeconds(Math.max(1, Math.min(600, Number(e.currentTarget.value) || 30)))} />초</label></div>}<WakeSlider value={rowLimit} onChange={setRowLimit} min={100} max={100000} step={100} aria-label="Row limit slider" label="행 한도" /><label>최대 행 수 <input aria-label="Row limit" type="number" min="100" max="100000" value={rowLimit} onInput={e => setRowLimit(Math.max(100, Math.min(100000, Math.trunc(Number(e.currentTarget.value)) || 1000)))} /></label><small>다음 실행부터 적용됩니다.</small></div></details><span class="muted">{props.manifest.query_language.display}</span>{props.readOnly && <strong class="read-only-badge">읽기 전용</strong>}</div>
    <EditorTools canRefactor={props.manifest.capabilities.lint&&props.manifest.query_language.editor_mode==='sql'} sessionId={props.sessionId} query={queryText} diagnostics={diagnostics} onReplace={value=>editor.current?.replace(value)} onError={fail} menuOpen={refactorOpen} onMenuClose={()=>setRefactorOpen(false)}/>
    <div class="ai-editor-ask"><Button variant="ghost" onClick={()=>window.dispatchEvent(new Event('ax:ask-panel'))}>패널에 묻기 <kbd>⌘L</kbd></Button></div><div class="editor-pane" style={{ height: `${editorHeight}px` }}><BorderGlow><Editor diagnostics={diagnostics} onRefactor={props.manifest.capabilities.lint&&props.manifest.query_language.editor_mode==='sql'?()=>setRefactorOpen(value=>!value):undefined} text={props.initialText} mode={props.manifest.query_language.editor_mode} onChange={value => { text.current = value; setQueryText(value); const next = value !== props.initialText; if (dirty.current !== next) { dirty.current = next; props.onDirty?.(next); } }} onRun={() => void run()} onReady={handle => { editor.current = handle; props.onReady(handle); }} onError={fail} /></BorderGlow></div>
    <div class="editor-splitter" role="separator" tabIndex={0} aria-label="Resize editor and results" aria-orientation="horizontal" aria-valuemin={80} aria-valuemax={available - 300} aria-valuenow={Math.round(editorHeight)} onPointerDown={resizeEditor} onKeyDown={e => { if (['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(e.key)) { e.preventDefault(); const next = e.key === 'Home' ? 80 : e.key === 'End' ? available - 300 : editorHeight + (e.key === 'ArrowUp' ? -16 : 16); setPaneRatio(Math.max(80, Math.min(available - 300, next)) / available); } }} />
    {props.manifest.capabilities.params && <details class="params"><summary><ChevronRight class="summary-chevron" aria-hidden="true" />파라미터 · JSON 객체 <span class={paramError ? 'invalid' : 'valid'}>{paramError ? '오류' : '정상'}</span></summary><textarea aria-label="Query parameters" value={params} onInput={e => setParams(e.currentTarget.value)} spellcheck={false} />{paramError && <p role="alert">{paramError}</p>}<small>큰 숫자 파라미터는 문자열로 전달하고 쿼리에서 형변환하세요.</small></details>}
    <div class="ax-result-switch"><JellyRadio segmented role="tablist" aria-label="쿼리 출력" value={resultPane} onChange={setResultPane} items={[{value:'results',label:'결과'},...(props.manifest.capabilities.explain?[{value:'plan',label:'계획'}]:[])]}/></div>
    <ErrorBanner scope="tab" error={error} onDismiss={() => setError(null)} dismissLabel="Dismiss query error" /><div class="results-wrap" aria-busy={!!runId}>{resultPane==='plan'?<Suspense fallback={<p role="status">계획 화면을 불러오는 중…</p>}><QueryPlan canRecommendIndexes={props.manifest.capabilities.ddl_render} key={queryText} sessionId={props.sessionId} query={queryText} onTarget={props.onBrowse?target=>props.onBrowse!({nodeId:target,title:target}):undefined} onInsert={value=>window.dispatchEvent(new CustomEvent('ax:proposal',{detail:{sessionId:props.sessionId,text:value}}))}/></Suspense>:preview ? <ChangePreviewView preview={preview}>{props.aiSuggested && !props.readOnly && <Button morph={false} disabled={!!runId || !previewRequest.current} onClick={() => void approveSuggestion()}>위 미리보기의 쓰기 승인 및 실행</Button>}</ChangePreviewView> : <Results queryText={ranText} active={props.active} results={results} sessionId={props.sessionId} canExpand={props.manifest.capabilities.graph_expand} graphLimit={props.graphLimit} rowLimit={resultRowLimit} onError={fail} schema={props.schema} entityId={resultEntity} filteredBrowse={props.manifest.capabilities.filtered_browse} onBrowse={runId?undefined:props.onBrowse} />}{!!runId && !firstReceived && <div class="result-loading"><LatticeLoader running={!!runId && (props.active ?? true)} label="결과를 불러오는 중…" /><Skeleton running={!!runId && (props.active ?? true)} rows={3} aria-label="Result skeleton" /></div>}</div>
  </div>;
}
export function BrowseTab({ active = true, sessionId, nodeId, manifest, graphLimit, onRun, onError, onResults, filter, schema, onBrowse, readOnly = false }: { filter?: [string,Value][]; schema?: SchemaGraph; onBrowse?: (target:BrowseTarget)=>void; onResults?: (results: ResultSet[]) => void; active?: boolean; readOnly?: boolean; sessionId: string; nodeId: string; manifest: DriverManifest; graphLimit: number; onRun: (runId: string | null) => void; onError: (error: unknown) => void }) {
  const [offset, setOffset] = useState(0);
  const [limit, setLimit] = useState(100);
  const [result, setResult] = useState<ResultSet | null>(null);
  const [error, setError] = useState<ReturnType<typeof normalizeError> | null>(null);
  const [busy, setBusy] = useState(false);
  const [retry, setRetry] = useState(0);
  const [editing, setEditing] = useState(false);
  const [edits, setEdits] = useState<PendingEdits>(emptyEdits);
  const [editPreview, setEditPreview] = useState<ChangePreview | null>(null);
  const [previewFingerprint, setPreviewFingerprint] = useState('');
  const [editBusy, setEditBusy] = useState(false);
  const [editError, setEditError] = useState<ReturnType<typeof normalizeError> | null>(null);
  const [confirmApply, setConfirmApply] = useState(false);
  const currentRequest = result?.edit_target && result.table ? buildEditRequest(result.edit_target, result.table, edits) : null;
  const currentFingerprint = currentRequest ? JSON.stringify(currentRequest) : '';
  const previewCurrent = !!editPreview && previewFingerprint === currentFingerprint;
  const latest = useRef({ onRun, onError, onResults }); latest.current = { onRun, onError, onResults };
  useEffect(() => {
    const runId = crypto.randomUUID(); let alive = true; setBusy(true); setError(null); latest.current.onRun(runId);
    void api('browse', { sessionId, nodeId, offset, limit, runId, filter }).then(value => { if (alive) { setResult(value); latest.current.onResults?.([value]); } }).catch(e => { if (alive) { setError(normalizeError(e)); latest.current.onError(e); } }).finally(() => { if (alive) { setBusy(false); latest.current.onRun(null); } });
    return () => { alive = false; };
  }, [sessionId, nodeId, offset, limit, retry, filter]);
  async function previewEdits() { if (!currentRequest || !currentRequest.edits.length || editBusy) return; const id = crypto.randomUUID(); setEditBusy(true); setEditError(null); onRun(id); try { const received = await api('edit_preview', { sessionId, runId: id, req: currentRequest }); setEditPreview(received); setPreviewFingerprint(currentFingerprint); } catch (cause) { setEditError(normalizeError(cause)); onError(cause); } finally { setEditBusy(false); onRun(null); } }
  async function applyEdits() { if (!currentRequest || !previewCurrent || editBusy) return; const id = crypto.randomUUID(); setEditBusy(true); setEditError(null); onRun(id); try { await api('edit_apply', { sessionId, runId: id, req: currentRequest }); setConfirmApply(false); setEditing(false); setEdits(emptyEdits()); setEditPreview(null); setPreviewFingerprint(''); setRetry(n => n + 1); } catch (cause) { setEditError(normalizeError(cause)); onError(cause); } finally { setEditBusy(false); onRun(null); } }
  const hasPendingEdits = pendingCount(edits) > 0;
  return <div class="browse-tab" onDragOver={e=>{if(manifest.capabilities.import_rows)e.preventDefault();}} onDrop={e=>{if(!manifest.capabilities.import_rows)return;e.preventDefault();const file=e.dataTransfer?.files[0];if(file)window.dispatchEvent(new CustomEvent('ax:import',{detail:{sessionId,nodeId,file}}));}}><div class="toolbar"><Button morph={false} aria-label="Previous page" data-testid="previous-page" disabled={busy || editBusy || hasPendingEdits || offset === 0} onClick={() => setOffset(o => Math.max(0, o - limit))}>이전</Button><Button morph={false} aria-label="Next page" data-testid="next-page" disabled={busy || editBusy || hasPendingEdits || !result?.page?.has_more} onClick={() => setOffset(o => o + limit)}>다음</Button><label>페이지 크기 <GlideSelect ariaLabel="Page size" size="sm" value={String(limit)} disabled={busy || editBusy || hasPendingEdits} options={[100, 500, 1000].map(n => ({ value: String(n), label: n.toLocaleString() }))} onChange={value => { setOffset(0); setLimit(Number(value)); }} /></label><span>{offset.toLocaleString()} – {(offset + (result?.row_count ?? 0)).toLocaleString()}</span><Button morph={false} disabled={busy || editBusy || hasPendingEdits} onClick={() => setRetry(n => n + 1)}>새로고침</Button>{busy && <span>불러오는 중…</span>}</div><ErrorBanner scope="tab" error={error} onDismiss={() => setError(null)} dismissLabel="Dismiss browse error" /><ErrorBanner scope="tab" error={editError} onDismiss={() => setEditError(null)} dismissLabel="Dismiss edit error" />{editPreview ? <><div class="edit-actions"><Button morph={false} onClick={() => setEditPreview(null)}>표로 돌아가기</Button><Button morph={false} class="preview-button" disabled={!previewCurrent || editBusy} onClick={() => setConfirmApply(true)}>적용</Button></div><ChangePreviewView preview={editPreview} /><Dialog label="Apply row changes" title="변경 적용" onClose={() => setConfirmApply(false)} open={confirmApply} footer={<><Button morph={false} disabled={editBusy} onClick={() => setConfirmApply(false)}>취소</Button><Button morph={false} variant="brand" loading={editBusy} disabled={editBusy} onClick={() => void applyEdits()}>적용</Button></>}><p>{(currentRequest?.edits.length ?? 0).toLocaleString()}개 변경을 데이터베이스에 적용합니다.</p><p>미리보기에서 확인한 내용과 같은 변경만 적용됩니다.</p></Dialog></> : <><Results active={active} results={result ? [result] : []} sessionId={sessionId} canExpand={manifest.capabilities.graph_expand} graphLimit={graphLimit} rowLimit={limit} initialView="Table" onError={cause => { setError(normalizeError(cause)); onError(cause); }} schema={schema} entityId={nodeId} filteredBrowse={manifest.capabilities.filtered_browse} onBrowse={busy?undefined:onBrowse} edit={{ enabled: manifest.capabilities.edit_rows && !!result?.edit_target && !!result?.table && !readOnly, editing, onToggle: () => { setEditing(value => !value); setEditPreview(null); setEditError(null); }, edits, onEditsChange: value => { setEdits(value); setEditPreview(null); } }} />{editing && <div class="edit-actions"><Button morph={false} disabled={editBusy || !hasPendingEdits} onClick={() => void previewEdits()}>변경 미리보기</Button><small>{pendingCount(edits).toLocaleString()}개 변경 대기 · 적용 전 미리보기가 필요합니다.</small></div>}</>}</div>;
}
