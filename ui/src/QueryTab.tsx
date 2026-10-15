import { useEffect, useRef, useState } from 'preact/hooks';
import type { DriverManifest, ExecRequest, ResultSet } from './contract';
import { api } from './api';
import { Editor, type EditorHandle } from './Editor';
import { ErrorBox, Results } from './Results';
import { normalizeError } from './values';
import { parseParams } from './params';
interface Props { initialText: string; sessionId: string; manifest: DriverManifest; graphLimit: number; onRun: (runId: string | null) => void; onReady: (handle: EditorHandle | null) => void; onHistory: () => void; onError: (error: unknown) => void }
export function QueryTab(props: Props) {
  const editor = useRef<EditorHandle | null>(null);
  const text = useRef(props.initialText);
  const running = useRef<string | null>(null);
  const mounted = useRef(true);
  const [runId, setRunId] = useState<string | null>(null);
  const [results, setResults] = useState<ResultSet[]>([]);
  const [error, setError] = useState<ReturnType<typeof normalizeError> | null>(null);
  const [timeout, setTimeoutSeconds] = useState(30);
  const [rowLimit, setRowLimit] = useState(1000);
  const [resultRowLimit, setResultRowLimit] = useState(1000);
  const [params, setParams] = useState('{}');
  let paramError = '';
  try { if (props.manifest.capabilities.params) parseParams(params); } catch (e) { paramError = (e as Error).message; }
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; running.current = null; }; }, []);
  async function run() {
    if (running.current || paramError) return;
    const query = editor.current?.selection() || editor.current?.text() || text.current;
    if (!query.trim()) return;
    const id = crypto.randomUUID(); running.current = id; setRunId(id); props.onRun(id); setError(null);
    const req: ExecRequest = { text: query, params: props.manifest.capabilities.params ? parseParams(params) : {}, row_limit: rowLimit, graph_node_limit: props.graphLimit, timeout_ms: props.manifest.capabilities.timeout ? timeout * 1000 : 30000 };
    try { const received = await api('execute', { sessionId: props.sessionId, runId: id, req }); if (mounted.current && running.current === id) { setResults(received); setResultRowLimit(req.row_limit); } }
    catch (e) { if (mounted.current && running.current === id) { setError(normalizeError(e)); props.onError(e); } }
    finally { if (mounted.current && running.current === id) { running.current = null; setRunId(null); props.onRun(null); props.onHistory(); } }
  }
  return <div class="query-tab"><div class="query-toolbar toolbar"><button class="primary" onClick={() => void run()} disabled={!!runId || !!paramError}>▶ Run <kbd>⌘/Ctrl ↵</kbd></button>{props.manifest.capabilities.cancel !== 'None' && <button disabled={!runId} onClick={() => { if (runId) void api('cancel', { runId }).catch(e => { setError(normalizeError(e)); props.onError(e); }); }}>Cancel</button>}{props.manifest.capabilities.timeout && <label>Timeout <input aria-label="Timeout seconds" type="number" min="1" max="3600" value={timeout} onInput={e => setTimeoutSeconds(Math.max(1, Math.min(3600, Number(e.currentTarget.value) || 30)))} />s</label>}<label>Row limit <input aria-label="Row limit" type="number" min="1" max="100000" value={rowLimit} onInput={e => setRowLimit(Math.max(1, Math.min(100000, Math.trunc(Number(e.currentTarget.value)) || 1000)))} /></label><span class="muted">{props.manifest.query_language.display}</span></div>
    <Editor text={props.initialText} mode={props.manifest.query_language.editor_mode} onChange={value => { text.current = value; }} onRun={() => void run()} onReady={handle => { editor.current = handle; props.onReady(handle); }} onError={e => { setError(normalizeError(e)); props.onError(e); }} />
    {props.manifest.capabilities.params && <details class="params"><summary>Parameters · JSON object <span class={paramError ? 'invalid' : 'valid'}>{paramError ? 'Invalid' : 'Valid'}</span></summary><textarea aria-label="Query parameters" value={params} onInput={e => setParams(e.currentTarget.value)} spellcheck={false} />{paramError && <p role="alert">{paramError}</p>}<small>Large numeric parameters: use strings and cast in the query.</small></details>}
    <ErrorBox error={error} /><Results results={results} sessionId={props.sessionId} canExpand={props.manifest.capabilities.graph_expand} graphLimit={props.graphLimit} rowLimit={resultRowLimit} onError={props.onError} />
  </div>;
}
export function BrowseTab({ sessionId, nodeId, manifest, graphLimit, onRun, onError }: { sessionId: string; nodeId: string; manifest: DriverManifest; graphLimit: number; onRun: (runId: string | null) => void; onError: (error: unknown) => void }) {
  const [offset, setOffset] = useState(0);
  const [limit, setLimit] = useState(100);
  const [result, setResult] = useState<ResultSet | null>(null);
  const [error, setError] = useState<ReturnType<typeof normalizeError> | null>(null);
  const [busy, setBusy] = useState(false);
  const [retry, setRetry] = useState(0);
  const latest = useRef({ onRun, onError }); latest.current = { onRun, onError };
  useEffect(() => {
    const runId = crypto.randomUUID(); let alive = true; setBusy(true); setError(null); latest.current.onRun(runId);
    void api('browse', { sessionId, nodeId, offset, limit, runId }).then(value => { if (alive) setResult(value); }).catch(e => { if (alive) { setError(normalizeError(e)); latest.current.onError(e); } }).finally(() => { if (alive) { setBusy(false); latest.current.onRun(null); } });
    return () => { alive = false; };
  }, [sessionId, nodeId, offset, limit, retry]);
  return <div class="browse-tab"><div class="toolbar"><button disabled={busy || offset === 0} onClick={() => setOffset(o => Math.max(0, o - limit))}>Previous</button><button disabled={busy || !result?.page?.has_more} onClick={() => setOffset(o => o + limit)}>Next</button><label>Page size <select aria-label="Page size" disabled={busy} value={limit} onChange={e => { setOffset(0); setLimit(Number(e.currentTarget.value)); }}>{[100, 500, 1000].map(n => <option key={n}>{n}</option>)}</select></label><span>{offset.toLocaleString()} – {(offset + (result?.row_count ?? 0)).toLocaleString()}</span><button disabled={busy} onClick={() => setRetry(n => n + 1)}>Refresh</button>{busy && <span>Loading…</span>}</div><ErrorBox error={error} /><Results results={result ? [result] : []} sessionId={sessionId} canExpand={manifest.capabilities.graph_expand} graphLimit={graphLimit} rowLimit={limit} onError={onError} /></div>;
}
