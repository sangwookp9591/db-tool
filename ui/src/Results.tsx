import { useEffect, useRef, useState } from 'preact/hooks';
import type { ComponentType } from 'preact';
import type { DbError, ResultSet } from './contract';
import { VirtualTable } from './VirtualTable';
import { valueText } from './values';
import { painted, start } from './perf';
import type { GraphView as GraphComponent } from './GraphView';
export function ErrorBox({ error }: { error: DbError | null }) {
  if (!error) return null;
  return <div class={`db-error error-${error.kind}`} role="alert"><span class="error-badge">{error.kind}</span><strong>{error.message}</strong>{error.detail && <pre>{error.detail}</pre>}{error.position !== null && <small>Position: {error.position}</small>}</div>;
}
interface Props { active?: boolean; results: ResultSet[]; sessionId: string; canExpand: boolean; graphLimit: number; rowLimit: number; onError: (error: unknown) => void }
export function Results(props: Props) {
  const [active, setActive] = useState(0);
  const [opened, setOpened] = useState(new Set([0]));
  useEffect(() => { setActive(0); setOpened(new Set([0])); }, [props.results[0]?.table?.columns, props.results[0]?.graph, props.results[0]?.documents, props.results[0]?.message]);
  if (!props.results.length) return <div class="empty-result">Results appear here after execution.</div>;
  return <div class="results">{props.results.length > 1 && <div class="result-tabs" role="tablist" aria-label="Result sets">{props.results.map((r, i) => <button key={i} role="tab" aria-selected={active === i} onClick={() => { const since = start('result-switch'); setActive(i); setOpened(prev => new Set([...prev, i])); void painted('result-switch', since); }}>{i + 1}. {r.title}</button>)}</div>}{props.results.map((r, i) => <div key={i} class="result-set" hidden={active !== i}>{opened.has(i) && <ResultView active={(props.active ?? true) && active === i} result={r} sessionId={props.sessionId} canExpand={props.canExpand} graphLimit={props.graphLimit} rowLimit={props.rowLimit} onError={props.onError} />}</div>)}</div>;
}
function ResultView({ active = true, result, sessionId, canExpand, graphLimit, rowLimit, onError }: Omit<Props, 'results'> & { result: ResultSet }) {
  const available = [result.graph !== null && 'Graph', result.table !== null && 'Table', result.documents !== null && 'JSON', result.message !== null && 'Message'].filter(Boolean) as string[];
  const [view, setView] = useState(available[0]);
  const [opened, setOpened] = useState(new Set([available[0]]));
  const [GraphView, setGraphView] = useState<ComponentType<Parameters<typeof GraphComponent>[0]> | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [retry, setRetry] = useState(0);
  useEffect(() => { setView(available[0]); setOpened(new Set([available[0]])); }, [result.table?.columns, result.graph, result.documents, result.message]);
  useEffect(() => {
    if (view !== 'Graph' || GraphView) return;
    let alive = true; setLoadError(false);
    void import('./GraphView').then(module => { if (alive) setGraphView(() => module.GraphView); }).catch(error => { if (alive) { setLoadError(true); onError(error); } });
    return () => { alive = false; };
  }, [view, GraphView, retry]);
  return <><div class="view-toolbar"><strong>{result.title}</strong><div class="view-switch" role="tablist" aria-label="Result views">{available.map(name => <button key={name} aria-label={`Show ${name} results`} data-testid={`result-view-${name.toLowerCase()}`} role="tab" aria-selected={view === name} onClick={() => { const since = start('view-switch'); setOpened(prev => new Set([...prev, name])); setView(name); void painted('view-switch', since); }}>{name}</button>)}</div></div><div class="result-content">
    {result.graph && opened.has('Graph') && <div class="view-pane" hidden={view !== 'Graph'}>{GraphView ? <GraphView active={active && view === 'Graph'} graph={result.graph} sessionId={sessionId} canExpand={canExpand} limit={graphLimit} onError={onError} /> : <p>{loadError ? <button onClick={() => setRetry(n => n + 1)}>Retry graph module</button> : 'Loading graph…'}</p>}</div>}
    {opened.has('Table') && result.table && <div class="view-pane" hidden={view !== 'Table'}><VirtualTable table={result.table} /></div>}
    {opened.has('JSON') && <div class="documents" hidden={view !== 'JSON'}>{result.documents?.map((value, i) => <pre key={i}>{valueText(value, true)}</pre>)}</div>}
    {opened.has('Message') && <pre class="message-result" hidden={view !== 'Message'}>{result.message}</pre>}
    {!available.length && <p>No result views.</p>}
  </div><ResultStatus result={result} rowLimit={rowLimit} graphLimit={graphLimit} /></>;
}

function ResultStatus({ result, rowLimit, graphLimit }: { result: ResultSet; rowLimit: number; graphLimit: number }) {
  const current = { elapsed: result.elapsed_ms, rows: result.row_count };
  const latest = useRef(current); latest.current = current;
  const [counter, setCounter] = useState(current);
  const last = useRef(0), timer = useRef(0);
  useEffect(() => { if (!timer.current) timer.current = window.setTimeout(() => { timer.current = 0; last.current = performance.now(); setCounter(latest.current); }, Math.max(0, 100 - (performance.now() - last.current))); }, [result.elapsed_ms, result.row_count]);
  useEffect(() => () => clearTimeout(timer.current), []);
  useEffect(() => { setCounter(latest.current); last.current = performance.now(); }, [result.table?.columns, result.graph, result.documents, result.message]);
  return <div class="result-status" role="status"><span>{counter.elapsed.toLocaleString()} ms</span><span>{counter.rows.toLocaleString()} rows</span>{result.affected !== null && <span>affected: {result.affected.toLocaleString()}</span>}{result.truncated && <span class="truncated">한도 {result.table ? rowLimit : graphLimit}에서 잘림{result.table && result.graph ? ` (graph limit ${graphLimit})` : ''}</span>}</div>;
}
