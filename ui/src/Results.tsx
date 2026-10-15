import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import type { ComponentType, ComponentChildren } from 'preact';
import type { DbError, ResultSet, SchemaGraph, BrowseTarget, Value } from './contract';
import { VirtualTable } from './VirtualTable';
import { DocumentTree } from './DocumentTree';
import { referenceTarget, rowNode } from './navigation';
import { painted, start } from './perf';
import type { GraphView as GraphComponent } from './GraphView';
export function ErrorBox({ error }: { error: DbError | null }) {
  if (!error) return null;
  return <div class={`db-error error-${error.kind}`} role="alert"><span class="error-badge">{error.kind}</span><strong>{error.message}</strong>{error.detail && <pre>{error.detail}</pre>}{error.position !== null && <small>Position: {error.position}</small>}</div>;
}
interface Props { initialView?: string; schema?: SchemaGraph; entityId?: string; filteredBrowse?: boolean; onBrowse?: (target:BrowseTarget)=>void; active?: boolean; results: ResultSet[]; sessionId: string; canExpand: boolean; graphLimit: number; rowLimit: number; onError: (error: unknown) => void }
export function Results(props: Props) {
  const [active, setActive] = useState(0);
  const [opened, setOpened] = useState(new Set([0]));
  useEffect(() => { setActive(0); setOpened(new Set([0])); }, [props.results[0]?.table?.columns, props.results[0]?.graph, props.results[0]?.documents, props.results[0]?.message]);
  if (!props.results.length) return <div class="empty-result">Results appear here after execution.</div>;
  const header = () => props.results.length > 1 ? <div class="result-tabs" role="tablist" aria-label="Result sets">{props.results.map((r, i) => <button key={i} role="tab" aria-selected={active === i} onClick={() => { const since = start('result-switch'); setActive(i); setOpened(prev => new Set([...prev, i])); void painted('result-switch', since); }}>{i + 1}. {r.title}</button>)}</div> : null;
  return <div class="results">{props.results.map((r, i) => <div key={i} class="result-set" hidden={active !== i}>{opened.has(i) && <ResultView header={header()} active={(props.active ?? true) && active === i} result={r} sessionId={props.sessionId} canExpand={props.canExpand} graphLimit={props.graphLimit} rowLimit={props.rowLimit} onError={props.onError} schema={props.schema} entityId={props.entityId} filteredBrowse={props.filteredBrowse} onBrowse={props.onBrowse} initialView={props.initialView} />}</div>)}</div>;

}
function ResultView({ header, active = true, result, sessionId, canExpand, graphLimit, rowLimit, onError, schema, entityId, filteredBrowse, onBrowse, initialView }: Omit<Props, 'results'> & { result: ResultSet; header?: ComponentChildren }) {
  const available = [result.graph !== null && 'Graph', result.table !== null && 'Table', result.documents !== null && 'JSON', result.message !== null && 'Message'].filter(Boolean) as string[];
  const [selectedNode,setSelectedNode]=useState<string|null>(null);
  const [selectedRow,setSelectedRow]=useState<Value[]|null>(null);
  const [highlightedColumn,setHighlightedColumn]=useState('');
  const inspectorValues=useMemo<Value[]>(()=>selectedRow && result.table ? [{t:'Map',v:result.table.columns.map((c,i)=>[c.name,selectedRow[i]])}]:[],[selectedRow,result.table?.columns]);
  useEffect(()=>{setSelectedNode(null);setSelectedRow(null);setHighlightedColumn('');},[result.table?.columns,result.graph]);
  const firstView=initialView && available.includes(initialView)?initialView:available[0];
  const [view, setView] = useState(firstView);
  const [opened, setOpened] = useState(new Set([firstView]));
  const [GraphView, setGraphView] = useState<ComponentType<Parameters<typeof GraphComponent>[0]> | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [retry, setRetry] = useState(0);
  useEffect(() => { setView(firstView); setOpened(new Set([firstView])); }, [result.table?.columns, result.graph, result.documents, result.message]);
  useEffect(() => {
    if (view !== 'Graph' || GraphView) return;
    let alive = true; setLoadError(false);
    void import('./GraphView').then(module => { if (alive) setGraphView(() => module.GraphView); }).catch(error => { if (alive) { setLoadError(true); onError(error); } });
    return () => { alive = false; };
  }, [view, GraphView, retry]);
  return <><div class="view-toolbar">{header || <strong>{result.title}</strong>}<div class="view-switch" role="tablist" aria-label="Result views">{available.map(name => <button key={name} aria-label={`Show ${name} results`} data-testid={`result-view-${name.toLowerCase()}`} role="tab" aria-selected={view === name} onClick={() => { const since = start('view-switch'); setOpened(prev => new Set([...prev, name])); setView(name); void painted('view-switch', since); }}>{name}</button>)}</div></div><div class="result-content">
    {result.graph && opened.has('Graph') && <div class="view-pane" hidden={view !== 'Graph'}>{GraphView ? <GraphView active={active && view === 'Graph'} graph={result.graph} sessionId={sessionId} canExpand={canExpand} limit={graphLimit} onError={onError} labelEntities={schema?.entities} selectedNodeId={selectedNode} onSelect={node=>setSelectedNode(node?.id ?? null)} onBrowse={onBrowse && schema ? target=>onBrowse(target) : undefined} /> : <p>{loadError ? <button onClick={() => setRetry(n => n + 1)}>Retry graph module</button> : 'Loading graph…'}</p>}</div>}
    {opened.has('Table') && result.table && <div class="view-pane" hidden={view !== 'Table'}><VirtualTable table={result.table} selectedNodeId={selectedNode} highlightedColumn={highlightedColumn} onError={onError} onRow={row=>{setSelectedRow(row);setSelectedNode(rowNode(row));}} reference={schema && entityId && filteredBrowse && onBrowse ? (row,column)=>{const target=referenceTarget(schema,entityId,result.table!,row,column);return target?()=>onBrowse(target):null;} : undefined} /></div>}
    {opened.has('JSON') && <div class="documents" hidden={view !== 'JSON'}><DocumentTree values={result.documents ?? []} onField={setHighlightedColumn} onError={onError}/></div>}
    {opened.has('Message') && <pre class="message-result" hidden={view !== 'Message'}>{result.message}</pre>}
    {selectedRow && result.table && <aside class="row-inspector" aria-label="Row document inspector"><button aria-label="Close row inspector" onClick={()=>setSelectedRow(null)}>×</button><DocumentTree values={inspectorValues} onField={setHighlightedColumn} onError={onError}/></aside>}
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
  return <div class="result-status" role="status"><span><span class="metric-number">{counter.elapsed.toLocaleString()}</span> ms</span><span><span class="metric-number">{counter.rows.toLocaleString()}</span> rows</span>{result.affected !== null && <span>affected: {result.affected.toLocaleString()}</span>}{result.truncated && <span class="truncated">한도 {result.table ? rowLimit : graphLimit}에서 잘림{result.table && result.graph ? ` (graph limit ${graphLimit})` : ''}</span>}</div>;
}
