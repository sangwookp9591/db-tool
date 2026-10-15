import { useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';
import type { DriverManifest, ExecRequest, ResultSet, ChangePreview, SchemaGraph, BrowseTarget, Value } from './contract';
import { api } from './api';
import { Editor, type EditorHandle } from './Editor';
import { ErrorBox, Results } from './Results';
import { normalizeError } from './values';
import { parseParams } from './params';
import { executeQuery } from './perf/stream';
import { VirtualTable } from './VirtualTable';
interface Props { sourceText?: string; entityId?: string; schema?: SchemaGraph; onBrowse?: (target:BrowseTarget)=>void; readOnly?: boolean; onResults?: (results: ResultSet[]) => void; active?: boolean; initialText: string; sessionId: string; manifest: DriverManifest; graphLimit: number; onRun: (runId: string | null) => void; onReady: (handle: EditorHandle | null) => void; onHistory: () => void; onError: (error: unknown) => void }
export function QueryTab(props: Props) {
  const container = useRef<HTMLDivElement>(null);
  const [paneRatio, setPaneRatio] = useState(.28), [paneHeight, setPaneHeight] = useState(500), [firstReceived, setFirstReceived] = useState(false);
  const available = Math.max(320, paneHeight - (props.manifest.capabilities.params ? 72 : 38) - 6);
  const editorHeight = Math.max(80, Math.min(available - 240, available * paneRatio));
  useLayoutEffect(() => { const element = container.current!; const resize = () => { if (element.clientHeight) setPaneHeight(element.clientHeight); }; resize(); const observer = new ResizeObserver(resize); observer.observe(element); return () => observer.disconnect(); }, []);
  function resizeEditor(event: PointerEvent) {
    event.preventDefault(); const handle = event.currentTarget as HTMLElement; handle.setPointerCapture(event.pointerId); const y = event.clientY, initial = editorHeight;
    const move = (e: PointerEvent) => setPaneRatio(Math.max(80, Math.min(available - 240, initial + e.clientY - y)) / available);
    const end = () => { handle.removeEventListener('pointermove', move); handle.removeEventListener('pointerup', end); handle.removeEventListener('pointercancel', end); };
    handle.addEventListener('pointermove', move); handle.addEventListener('pointerup', end); handle.addEventListener('pointercancel', end);
  }
  const editor = useRef<EditorHandle | null>(null);
  const text = useRef(props.initialText);
  const running = useRef<string | null>(null);
  const mounted = useRef(true);
  const [runId, setRunId] = useState<string | null>(null);
  const [preview, setPreview] = useState<ChangePreview | null>(null);
  const [resultEntity,setResultEntity]=useState<string|undefined>();
  const [results, setResults] = useState<ResultSet[]>([]);
  const [error, setError] = useState<ReturnType<typeof normalizeError> | null>(null);
  const [timeout, setTimeoutSeconds] = useState(30);
  const [rowLimit, setRowLimit] = useState(1000);
  const [resultRowLimit, setResultRowLimit] = useState(1000);
  const [params, setParams] = useState('{}');
  let paramError = '';
  try { if (props.manifest.capabilities.params) parseParams(params); } catch (e) { paramError = (e as Error).message; }
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; running.current = null; }; }, []);
  async function run(previewOnly = false) {
    if (running.current || paramError) return;
    const query = editor.current?.selection() || editor.current?.text() || text.current;
    if (!query.trim()) return;
    const id = crypto.randomUUID(); running.current = id; setRunId(id); props.onRun(id); setError(null); setPreview(null); setFirstReceived(false);
    // shortcut: edited queries lack source metadata, enable their FK links when drivers return column provenance.
    setResultEntity(query===(props.sourceText??props.initialText)?props.entityId:undefined);
    const req: ExecRequest = { text: query, params: props.manifest.capabilities.params ? parseParams(params) : {}, row_limit: rowLimit, graph_node_limit: props.graphLimit, timeout_ms: props.manifest.capabilities.timeout ? timeout * 1000 : 30000 };
    try { if (previewOnly) {
      const received = await api('preview', { sessionId: props.sessionId, runId: id, req: { ...req, preview_sample_limit: 50 } });
      if (mounted.current && running.current === id) { setPreview(received); setFirstReceived(true); }
      return;
    }
    const received = await executeQuery({ sessionId: props.sessionId, runId: id, req }, partial => { if (mounted.current && running.current === id) { setResults(partial); if (partial.some(r => r.table?.rows.length || r.graph || r.documents || r.message)) setFirstReceived(true); setResultRowLimit(req.row_limit); } }); if (mounted.current && running.current === id) { setResults(received); props.onResults?.(received); setResultRowLimit(req.row_limit); } }
    catch (e) { if (mounted.current && running.current === id) { setError(normalizeError(e)); props.onError(e); } }
    finally { if (mounted.current && running.current === id) { running.current = null; setRunId(null); props.onRun(null); props.onHistory(); } }
  }
  return <div class="query-tab" ref={container}><div class="query-toolbar toolbar"><button class="primary" aria-label="Run query" data-testid="run-query" onClick={() => void run()} disabled={!!runId || !!paramError}>▶ Run <kbd>⌘/Ctrl ↵</kbd></button>{!props.readOnly && props.manifest.capabilities.preview_changes && props.manifest.capabilities.preview_changes !== 'None' && <button aria-label="Preview changes" data-testid="preview-changes" disabled={!!runId || !!paramError} onClick={() => void run(true)}>미리보기</button>}{props.manifest.capabilities.cancel !== 'None' && <button aria-label="Cancel query" data-testid="cancel-query" disabled={!runId} onClick={() => { if (runId) void api('cancel', { runId }).catch(e => { setError(normalizeError(e)); props.onError(e); }); }}>Cancel</button>}{props.manifest.capabilities.timeout && <label>Timeout <input aria-label="Timeout seconds" type="number" min="1" max="3600" value={timeout} onInput={e => setTimeoutSeconds(Math.max(1, Math.min(3600, Number(e.currentTarget.value) || 30)))} />s</label>}<label>Row limit <input aria-label="Row limit" type="number" min="1" max="100000" value={rowLimit} onInput={e => setRowLimit(Math.max(1, Math.min(100000, Math.trunc(Number(e.currentTarget.value)) || 1000)))} /></label><span class="muted">{props.manifest.query_language.display}</span>{props.readOnly && <strong class="read-only-badge">읽기 전용</strong>}</div>
    <div class="editor-pane" style={{ height: `${editorHeight}px` }}><Editor text={props.initialText} mode={props.manifest.query_language.editor_mode} onChange={value => { text.current = value; }} onRun={() => void run()} onReady={handle => { editor.current = handle; props.onReady(handle); }} onError={e => { setError(normalizeError(e)); props.onError(e); }} /></div>
    <div class="editor-splitter" role="separator" tabIndex={0} aria-label="Resize editor and results" aria-orientation="horizontal" aria-valuemin={80} aria-valuemax={available - 240} aria-valuenow={Math.round(editorHeight)} onPointerDown={resizeEditor} onKeyDown={e => { if (['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(e.key)) { e.preventDefault(); const next = e.key === 'Home' ? 80 : e.key === 'End' ? available - 240 : editorHeight + (e.key === 'ArrowUp' ? -16 : 16); setPaneRatio(Math.max(80, Math.min(available - 240, next)) / available); } }} />
    {props.manifest.capabilities.params && <details class="params"><summary>Parameters · JSON object <span class={paramError ? 'invalid' : 'valid'}>{paramError ? 'Invalid' : 'Valid'}</span></summary><textarea aria-label="Query parameters" value={params} onInput={e => setParams(e.currentTarget.value)} spellcheck={false} />{paramError && <p role="alert">{paramError}</p>}<small>Large numeric parameters: use strings and cast in the query.</small></details>}
    <ErrorBox error={error} /><div class="results-wrap" aria-busy={!!runId}>{preview ? <div class="change-preview" style={{ overflow: 'auto', height: '100%', width: '100%', minWidth: 0, padding: '0 10px' }}><p><strong>미리보기 · 커밋하지 않았습니다</strong></p>{preview.notes.map((note, i) => <p key={i}>{note}</p>)}{preview.statements.map((statement, i) => <section key={i}><h3>{statement.kind} · affected: {statement.affected ?? 'unknown'}</h3><pre>{statement.text}</pre>{statement.warnings.map((warning, j) => <p key={j}>{warning}</p>)}{statement.before && <div><h4>Before (sample)</h4><div style={{ height: '240px', display: 'flex' }}><VirtualTable table={statement.before} /></div></div>}{statement.after && <div><h4>After (sample)</h4><div style={{ height: '240px', display: 'flex' }}><VirtualTable table={statement.after} /></div></div>}{statement.schema_diff && <ul>{(['added', 'removed', 'changed'] as const).flatMap(kind => statement.schema_diff![kind].map(item => <li key={`${kind}:${item}`}>{kind}: {item}</li>))}</ul>}{statement.counters.filter(([, value]) => value !== 0).map(([name, value]) => <p key={name}>{name}: {value}</p>)}</section>)}</div> : <Results active={props.active} results={results} sessionId={props.sessionId} canExpand={props.manifest.capabilities.graph_expand} graphLimit={props.graphLimit} rowLimit={resultRowLimit} onError={props.onError} schema={props.schema} entityId={resultEntity} filteredBrowse={props.manifest.capabilities.filtered_browse} onBrowse={runId?undefined:props.onBrowse} />}{!!runId && !firstReceived && <div class="result-loading" role="status">Loading results…</div>}</div>
  </div>;
}
export function BrowseTab({ active = true, sessionId, nodeId, manifest, graphLimit, onRun, onError, onResults, filter, schema, onBrowse }: { filter?: [string,Value][]; schema?: SchemaGraph; onBrowse?: (target:BrowseTarget)=>void; onResults?: (results: ResultSet[]) => void; active?: boolean; sessionId: string; nodeId: string; manifest: DriverManifest; graphLimit: number; onRun: (runId: string | null) => void; onError: (error: unknown) => void }) {
  const [offset, setOffset] = useState(0);
  const [limit, setLimit] = useState(100);
  const [result, setResult] = useState<ResultSet | null>(null);
  const [error, setError] = useState<ReturnType<typeof normalizeError> | null>(null);
  const [busy, setBusy] = useState(false);
  const [retry, setRetry] = useState(0);
  const latest = useRef({ onRun, onError, onResults }); latest.current = { onRun, onError, onResults };
  useEffect(() => {
    const runId = crypto.randomUUID(); let alive = true; setBusy(true); setError(null); latest.current.onRun(runId);
    void api('browse', { sessionId, nodeId, offset, limit, runId, filter }).then(value => { if (alive) { setResult(value); latest.current.onResults?.([value]); } }).catch(e => { if (alive) { setError(normalizeError(e)); latest.current.onError(e); } }).finally(() => { if (alive) { setBusy(false); latest.current.onRun(null); } });
    return () => { alive = false; };
  }, [sessionId, nodeId, offset, limit, retry, filter]);
  return <div class="browse-tab"><div class="toolbar"><button aria-label="Previous page" data-testid="previous-page" disabled={busy || offset === 0} onClick={() => setOffset(o => Math.max(0, o - limit))}>Previous</button><button aria-label="Next page" data-testid="next-page" disabled={busy || !result?.page?.has_more} onClick={() => setOffset(o => o + limit)}>Next</button><label>Page size <select aria-label="Page size" disabled={busy} value={limit} onChange={e => { setOffset(0); setLimit(Number(e.currentTarget.value)); }}>{[100, 500, 1000].map(n => <option key={n}>{n}</option>)}</select></label><span>{offset.toLocaleString()} – {(offset + (result?.row_count ?? 0)).toLocaleString()}</span><button disabled={busy} onClick={() => setRetry(n => n + 1)}>Refresh</button>{busy && <span>Loading…</span>}</div><ErrorBox error={error} /><Results active={active} results={result ? [result] : []} sessionId={sessionId} canExpand={manifest.capabilities.graph_expand} graphLimit={graphLimit} rowLimit={limit} initialView="Table" onError={onError} schema={schema} entityId={nodeId} filteredBrowse={manifest.capabilities.filtered_browse} onBrowse={busy?undefined:onBrowse} /></div>;
}
