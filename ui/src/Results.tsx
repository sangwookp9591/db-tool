import { useEffect, useState } from 'preact/hooks';
import type { ComponentType } from 'preact';
import type { DbError, ResultSet } from './contract';
import { VirtualTable } from './VirtualTable';
import { valueText } from './values';
import type { GraphView as GraphComponent } from './GraphView';
export function ErrorBox({ error }: { error: DbError | null }) {
  if (!error) return null;
  return <div class={`db-error error-${error.kind}`} role="alert"><span class="error-badge">{error.kind}</span><strong>{error.message}</strong>{error.detail && <pre>{error.detail}</pre>}{error.position !== null && <small>Position: {error.position}</small>}</div>;
}
interface Props { results: ResultSet[]; sessionId: string; canExpand: boolean; graphLimit: number; rowLimit: number; onError: (error: unknown) => void }
export function Results(props: Props) {
  const [active, setActive] = useState(0);
  useEffect(() => setActive(0), [props.results]);
  if (!props.results.length) return <div class="empty-result">Results appear here after execution.</div>;
  return <div class="results">{props.results.length > 1 && <div class="result-tabs" role="tablist" aria-label="Result sets">{props.results.map((r, i) => <button key={i} role="tab" aria-selected={active === i} onClick={() => setActive(i)}>{i + 1}. {r.title}</button>)}</div>}{props.results.map((r, i) => <div key={i} class="result-set" hidden={active !== i}><ResultView result={r} sessionId={props.sessionId} canExpand={props.canExpand} graphLimit={props.graphLimit} rowLimit={props.rowLimit} onError={props.onError} /></div>)}</div>;
}
function ResultView({ result, sessionId, canExpand, graphLimit, rowLimit, onError }: Omit<Props, 'results'> & { result: ResultSet }) {
  const available = [result.graph !== null && 'Graph', result.table !== null && 'Table', result.documents !== null && 'JSON', result.message !== null && 'Message'].filter(Boolean) as string[];
  const [view, setView] = useState(available[0]);
  const [GraphView, setGraphView] = useState<ComponentType<Parameters<typeof GraphComponent>[0]> | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [retry, setRetry] = useState(0);
  useEffect(() => { setView(available[0]); }, [result]);
  useEffect(() => {
    if (view !== 'Graph' || GraphView) return;
    let alive = true; setLoadError(false);
    void import('./GraphView').then(module => { if (alive) setGraphView(() => module.GraphView); }).catch(error => { if (alive) { setLoadError(true); onError(error); } });
    return () => { alive = false; };
  }, [view, GraphView, retry]);
  return <><div class="view-toolbar"><strong>{result.title}</strong><div class="view-switch" role="tablist" aria-label="Result views">{available.map(name => <button key={name} role="tab" aria-selected={view === name} onClick={() => setView(name)}>{name}</button>)}</div></div><div class="result-content">
    {result.graph && <div class="view-pane" hidden={view !== 'Graph'}>{GraphView ? <GraphView graph={result.graph} sessionId={sessionId} canExpand={canExpand} limit={graphLimit} onError={onError} /> : <p>{loadError ? <button onClick={() => setRetry(n => n + 1)}>Retry graph module</button> : 'Loading graph…'}</p>}</div>}
    {view === 'Table' && result.table && <VirtualTable table={result.table} />}
    {view === 'JSON' && <div class="documents">{result.documents?.map((value, i) => <pre key={i}>{valueText(value, true)}</pre>)}</div>}
    {view === 'Message' && <pre class="message-result">{result.message}</pre>}
    {!available.length && <p>No result views.</p>}
  </div><div class="result-status" role="status"><span>{result.elapsed_ms.toLocaleString()} ms</span><span>{result.row_count.toLocaleString()} rows</span>{result.affected !== null && <span>affected: {result.affected.toLocaleString()}</span>}{result.truncated && <span class="truncated">한도 {result.table ? rowLimit : graphLimit}에서 잘림{result.table && result.graph ? ` (graph limit ${graphLimit})` : ''}</span>}</div></>;
}
