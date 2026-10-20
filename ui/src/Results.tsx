import './results.css';
import { QueryResultEditor, type QueryEditContext } from './QueryResultEditor';
import { ExportButton } from './ExportButton';
import type { DataFormat } from './export';
import { Button } from './ui/Button';
import { SlidingIndicator } from './ui/SlidingIndicator';
import { JellyRadio } from './ui/JellyRadio';
import { NumberSlots } from './ui/CodeSlots';
import { X, Table2, Share2, Braces, MessageSquare, Terminal, Copy, Check } from 'lucide-preact';
import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import type { ComponentType, ComponentChildren } from 'preact';
import type { DbError, ResultSet, SchemaGraph, BrowseTarget, Value } from './contract';
import type { PendingEdits } from './edits';
import { VirtualTable } from './VirtualTable';
import { DocumentTree } from './DocumentTree';
import { referenceTarget, rowNode } from './navigation';
import { painted, start } from './perf';
import { valueText } from './values';
import { errorKindLabel } from './previewCopy';
import type { GraphView as GraphComponent } from './GraphView';
export function ErrorBox({ error }: { error: DbError | null }) {
  if (!error) return null;
  return <div class={`db-error error-${error.kind}`} role="alert"><span class="error-badge">{errorKindLabel(error.kind)}</span><strong>{error.message}</strong>{error.detail && <pre>{error.detail}</pre>}{error.position !== null && <small>위치: {error.position}</small>}</div>;
}
export interface EditControls { reason?: string; enabled: boolean; editing: boolean; onToggle: () => void; edits: PendingEdits; onEditsChange: (edits: PendingEdits) => void }
interface Props { queryText?: string; initialView?: string; schema?: SchemaGraph; entityId?: string; filteredBrowse?: boolean; onBrowse?: (target:BrowseTarget)=>void; active?: boolean; results: ResultSet[]; sessionId: string; canExpand: boolean; graphLimit: number; rowLimit: number; onError: (error: unknown) => void; edit?: EditControls; queryEdit?: QueryEditContext }
export function Results(props: Props) {
  const [active, setActive] = useState(0);
  const [opened, setOpened] = useState(new Set([0]));
  useEffect(() => { setActive(0); setOpened(new Set([0])); }, [props.results[0]?.table?.columns, props.results[0]?.graph, props.results[0]?.documents, props.results[0]?.message]);
  if (!props.results.length) return <div class="empty-result"><Terminal />쿼리를 실행하면 여기에 결과가 표시됩니다.<small>⌘/Ctrl ↵로 쿼리를 실행하세요.</small></div>;
  const header = () => props.results.length > 1 ? <div class="result-tabs" role="tablist" aria-label="Result sets">{props.results.map((r, i) => <Button morph={false} key={i} title={r.title} role="tab" aria-selected={active === i} onClick={() => { const since = start('result-switch'); setActive(i); setOpened(prev => new Set([...prev, i])); void painted('result-switch', since); }}>{i + 1}. {r.title}</Button>)}</div> : null;
  return <div class="results">{props.results.map((r, i) => {
    const view = (edit?: EditControls) => <ResultView queryText={props.results.length === 1 ? props.queryText : undefined} header={header()} active={(props.active ?? true) && active === i} result={r} sessionId={props.sessionId} canExpand={props.canExpand} graphLimit={props.graphLimit} rowLimit={props.rowLimit} onError={props.onError} schema={props.schema} entityId={props.entityId} filteredBrowse={props.filteredBrowse} onBrowse={props.onBrowse} initialView={props.initialView} edit={edit} />;
    return <div key={i} class="result-set" hidden={active !== i}>{opened.has(i) && (props.queryEdit ? <QueryResultEditor result={r} context={props.queryEdit}>{edit => view(edit)}</QueryResultEditor> : view(props.edit))}</div>;
  })}</div>;

}
function visibleResultTitle(result: ResultSet, schema?: SchemaGraph, entityId?: string, filteredBrowse?: boolean) {
  if (!filteredBrowse || !schema || !entityId || !result.table?.rows.length || !/\s+WHERE\b/i.test(result.title)) return result.title;
  const incoming = schema.relations.find(relation => relation.to === entityId && relation.from_fields.length === relation.to_fields.length);
  if (!incoming) return result.title;
  const row = result.table.rows[0];
  const predicates = incoming.to_fields.map((field, index) => {
    const column = result.table!.columns.findIndex(item => item.name === field);
    const value = row[column];
    if (column < 0 || !value) return null;
    if (value.t === 'Null') return `${field} IS NULL`;
    const rendered = ['Text', 'Uuid', 'Date', 'Time', 'DateTime', 'DateTimeTz'].includes(value.t) ? `'${valueText(value, true).replaceAll("'", "''")}'` : valueText(value, true);
    return `${field} = ${rendered}`;
  });
  if (predicates.some(predicate => predicate === null)) return result.title;
  const prefix = result.title.split(/\s+WHERE\b/i)[0];
  return `${prefix} WHERE ${predicates.join(' AND ')}`;
}
function ResultView({ queryText, header, active = true, result, sessionId, canExpand, graphLimit, rowLimit, onError, schema, entityId, filteredBrowse, onBrowse, initialView, edit }: Omit<Props, 'results'> & { result: ResultSet; header?: ComponentChildren }) {
  const available = [result.graph !== null && 'Graph', result.table !== null && 'Table', result.documents !== null && 'JSON', result.message !== null && 'Message'].filter(Boolean) as string[];
  const [loadedGraph, setLoadedGraph] = useState(result.graph);
  useEffect(() => setLoadedGraph(result.graph), [result.graph]);
  const [viewOrder, setViewOrder] = useState<Uint32Array | null>(null);
  useEffect(() => setViewOrder(null), [result.table?.columns]);
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
  const [copied, setCopied] = useState(false);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(copyTimer.current), []);
  useEffect(() => { setView(firstView); setOpened(new Set([firstView])); }, [result.table?.columns, result.graph, result.documents, result.message]);
  useEffect(() => {
    if (view !== 'Graph' || GraphView) return;
    let alive = true; setLoadError(false);
    void import('./GraphView').then(module => { if (alive) setGraphView(() => module.GraphView); }).catch(error => { if (alive) { setLoadError(true); onError(error); } });
    return () => { alive = false; };
  }, [view, GraphView, retry]);
  const viewControls = <JellyRadio role="tablist" aria-label="Result views" value={view} items={available.map(name => ({ value: name, 'aria-label': `Show ${name} results`, 'data-testid': `result-view-${name.toLowerCase()}`, label: <>{name === 'Table' ? <Table2 /> : name === 'Graph' ? <Share2 /> : name === 'JSON' ? <Braces /> : <MessageSquare />}{name === 'Table' ? '표' : name === 'Graph' ? '그래프' : name === 'Message' ? '메시지' : name}</> }))} onChange={name => { const since = start('view-switch'); setOpened(prev => new Set([...prev, name])); setView(name); void painted('view-switch', since); }} />;
  const resultTitle = visibleResultTitle(result, schema, entityId, filteredBrowse);
  return <><div class="view-toolbar">{header || <strong class="result-title" title={queryText && resultTitle === result.title && resultTitle.endsWith('…') ? queryText : resultTitle}>{resultTitle}</strong>}{edit && result.table && <div class="edit-toolbar"><span title={edit.reason}><Button morph={false} disabled={!edit.enabled} title={edit.reason} aria-pressed={edit.editing} aria-label={edit.editing ? 'Exit edit mode' : 'Edit rows'} onClick={edit.onToggle}>{edit.editing ? '편집 종료' : '편집'}</Button></span>{edit.editing && <>{!result.edit_target?.columns?.length && <Button morph={false} onClick={() => edit.onEditsChange({ ...edit.edits, added: [...edit.edits.added, { id: crypto.randomUUID(), values: result.table!.columns.map(() => ({ t: 'Null' })) }] })}>행 추가</Button>}<small>{(Object.keys(edit.edits.updates).length + edit.edits.deleted.length + edit.edits.added.length).toLocaleString()}개 변경 대기</small></>}</div>}{(result.table || result.documents || result.graph) && <ExportButton formats={view === 'JSON' && result.documents ? ['JSON', 'NDJSON'] : view === 'Graph' && result.graph ? ['JSON'] : result.table ? ['CSV', 'JSON', 'Markdown'] : ['JSON']} count={view === 'JSON' ? result.documents?.length ?? 0 : view === 'Graph' ? loadedGraph?.nodes.length ?? 0 : result.table?.rows.length ?? 0} truncated={result.truncated || !!result.page?.has_more} table={view === 'Table'} makeFile={async ({format, visible, nullLabel}) => {
      const { serializeTable, exportGraph, exportValue } = await import('./export');
      let text: string, count: number;
      if (view === 'JSON' && result.documents) { const documents = result.documents.map(exportValue); count = documents.length; text = format === 'NDJSON' ? documents.map(item => JSON.stringify(item)).join('\n') + (documents.length ? '\n' : '') : JSON.stringify(documents, null, 2); }
      else if (view === 'Graph' && loadedGraph) { count = loadedGraph.nodes.length; text = JSON.stringify(exportGraph(loadedGraph), null, 2); }
      else if (result.table) { const rows = visible && viewOrder && !edit?.editing ? Array.from(viewOrder, index => result.table!.rows[index]) : result.table.rows; count = rows.length; text = serializeTable({...result.table, rows}, format as DataFormat, nullLabel); }
      else { count = result.graph?.nodes.length ?? 0; text = JSON.stringify(result.graph ? exportGraph(result.graph) : [], null, 2); }
      return { bytes: new TextEncoder().encode(text), count };
    }} />}<div class="view-switch">{available.length > 1 ? <SlidingIndicator value={view}>{viewControls}</SlidingIndicator> : viewControls}</div></div><div class="result-content">
    {result.graph && opened.has('Graph') && <div class="view-pane" hidden={view !== 'Graph'}>{GraphView ? <GraphView onGraphChange={setLoadedGraph} active={active && view === 'Graph'} graph={result.graph} sessionId={sessionId} canExpand={canExpand} limit={graphLimit} onError={onError} labelEntities={schema?.entities} selectedNodeId={selectedNode} onSelect={node=>setSelectedNode(node?.id ?? null)} onBrowse={onBrowse && schema ? target=>onBrowse(target) : undefined} /> : <p>{loadError ? <Button morph={false} onClick={() => setRetry(n => n + 1)}>그래프 모듈 다시 불러오기</Button> : '그래프를 불러오는 중…'}</p>}</div>}
    {opened.has('Table') && result.table && <div class="view-pane" hidden={view !== 'Table'}><VirtualTable table={result.table} onViewOrder={setViewOrder} selectedNodeId={selectedNode} highlightedColumn={highlightedColumn} onError={onError} onRow={row=>{setSelectedRow(row);setSelectedNode(rowNode(row));}} reference={schema && entityId && filteredBrowse && onBrowse ? (row,column)=>{const target=referenceTarget(schema,entityId,result.table!,row,column);return target?()=>onBrowse(target):null;} : undefined} editing={edit?.editing && result.edit_target ? { target: result.edit_target, edits: edit.edits, onChange: edit.onEditsChange } : undefined} /></div>}
    {opened.has('JSON') && <div class="documents" hidden={view !== 'JSON'}><div class="document-toolbar"><span><Braces />문서 트리 · {(result.documents?.length ?? 0).toLocaleString()}개</span><Button morph icon={copied?<Check />:<Copy />} aria-label="Copy result documents" onClick={() => { void navigator.clipboard.writeText(JSON.stringify(result.documents ?? [], null, 2)).then(()=>{setCopied(true);clearTimeout(copyTimer.current);copyTimer.current=setTimeout(()=>setCopied(false),1400);}).catch(onError); }}>{copied?'복사됨':'JSON 복사'}</Button></div><DocumentTree values={result.documents ?? []} onField={setHighlightedColumn} onError={onError}/></div>}
    {opened.has('Message') && <pre class="message-result" hidden={view !== 'Message'}>{result.message}</pre>}
    {selectedRow && result.table && <aside class="row-inspector" aria-label="Row document inspector"><Button morph={false} aria-label="Close row inspector" onClick={()=>setSelectedRow(null)}><X /></Button><DocumentTree values={inspectorValues} onField={setHighlightedColumn} onError={onError}/></aside>}
    {!available.length && <p>표시할 결과 뷰가 없습니다.</p>}
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
  return <div class="result-status" role="status"><span class="metric-chip">시간 <NumberSlots value={counter.elapsed.toLocaleString()} ariaLabel="Elapsed milliseconds" /> ms</span><span class="metric-chip">행 <NumberSlots value={counter.rows.toLocaleString()} ariaLabel="Rows" /></span>{result.affected !== null && <span>영향 {result.affected.toLocaleString()}행</span>}{result.truncated && <span class="truncated">한도 {(result.table ? rowLimit : graphLimit).toLocaleString()}에서 잘림{result.table && result.graph ? ` (그래프 한도 ${graphLimit.toLocaleString()})` : ''}</span>}</div>;
}
