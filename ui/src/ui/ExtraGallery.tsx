// 사용자 제공 레퍼런스 재구현 · interactive mock fixtures, no database calls.
import { useRef, useState } from 'preact/hooks';
import { Check, Copy, Play, Table2, Columns3, ListFilter } from 'lucide-preact';
import { Button } from './Button';
import { SuggestionCard, type Suggestion } from './SuggestionCard';
import { ToolCall, ToolCallGroup, type ToolCallProps } from './ToolCall';
import { CallChip } from './CallChip';
import { BranchedMenu } from './BranchedMenu';

const suggestions: Suggestion[] = [
  { id: 'recent', title: 'Recent orders', query: 'SELECT id, created_at FROM orders\nORDER BY created_at DESC LIMIT 20;', reason: 'Inspect a small set of recent orders before narrowing the query.', signal: 2, evidence: 'Demo fixture: orders.id and orders.created_at exist in the supplied schema; not executed.' },
  { id: 'count', title: 'Order count', query: 'SELECT count(*) FROM orders;', reason: 'Use an aggregate when you only need the size of the table.', signal: 1, evidence: 'Demo fixture: syntax check passed; schema and EXPLAIN not checked.' },
];
export function ExtraGallery() {
  const bareRef = useRef<HTMLButtonElement>(null);
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState(false);
  const [clicks, setClicks] = useState(0);
  const [acceptedId, setAcceptedId] = useState<string>();
  const [message, setMessage] = useState('No suggestion inserted');
  const [status, setStatus] = useState<ToolCallProps['status']>('success');
  const [mounted, setMounted] = useState(true);
  const [startedAt, setStartedAt] = useState(0);
  const [branchValue, setBranchValue] = useState('orders');
  const [chipStatus, setChipStatus] = useState<'idle'|'running'|'done'|'error'>('idle');
  return <>
    <section id="u2-branched-menu"><h2>21 · BranchedMenu</h2><div data-testid="bm-demo"><BranchedMenu label="Gallery branched menu" defaultOpen={['tables']} defaultActive="orders" onSelect={value => setBranchValue(value)} items={[
      { value: 'tables', label: 'Tables', children: [{ value: 'people', label: 'people', icon: <Table2 /> }, { value: 'orders', label: 'orders', icon: <Table2 /> }, { value: 'items', label: 'order_items', icon: <Table2 /> }] },
      { value: 'columns', label: 'Columns', children: [{ value: 'id', label: 'id', icon: <Columns3 /> }, { value: 'name', label: 'name', icon: <Columns3 /> }] },
      { value: 'indexes', label: 'Indexes', children: [{ value: 'pk', label: 'people_pkey', icon: <ListFilter /> }] },
      { value: 'settings', label: 'Settings' }]} /></div><output data-testid="bm-selected">{branchValue}</output><p>Sections fold with a grid-rows animation. The accent line reaches the active row and a marker sits on the rail beside its section.</p></section>
    <section id="d3-call-chip"><h2>20 · CallChip</h2><div className="micro-gallery__controls">
      <Button onClick={() => setChipStatus('idle')}>Idle</Button><Button onClick={() => setChipStatus('running')}>Start</Button><Button onClick={() => setChipStatus('done')}>Complete</Button><Button onClick={() => setChipStatus('error')}>Fail</Button>
    </div><div className="micro-gallery__controls"><CallChip name="run_query" argument="connection · long query preview that truncates" status={chipStatus} onRetry={chipStatus === 'error' ? () => setChipStatus('running') : undefined} expectedMs={2400} /><CallChip icon="search" name="get_schema_context" argument="Analytics" status="done" /></div><p>Running progress parks at 90%; completion and failure settle the status. Retry appears only after failure.</p></section>
    <section id="d3-button"><h2>17 · Button</h2><div className="micro-gallery__controls">
      <Button variant="brand" size="md" loading={loading} icon={<Play size={16} />} aria-label="D3 run" data-testid="d3-run" type="submit" name="run" value="query" onClick={() => setClicks(value => value + 1)}>{loading ? 'Running…' : 'Run query'}</Button>
      <Button variant="primary" icon={copied ? <Check size={16} /> : <Copy size={16} />} onClick={() => setCopied(!copied)}>{copied ? 'Copied' : 'Copy'}</Button>
      <Button variant="ghost">Ghost</Button><Button variant="destructive">Delete</Button><Button disabled>Disabled</Button><Button bare ref={bareRef} data-testid="d3-bare" onClick={() => { if (bareRef.current) bareRef.current.dataset.refPassed = String(bareRef.current instanceof HTMLButtonElement); }}>Bare button</Button>
    </div><label><input type="checkbox" checked={loading} onChange={event => setLoading(event.currentTarget.checked)} />D3 loading</label><output data-testid="d3-clicks">{clicks}</output><p>Finite morph on label changes. Loading blocks clicks.</p></section>
    <section id="d3-suggestion"><h2>18 · Smart Recommendation</h2><SuggestionCard suggestions={suggestions} acceptedId={acceptedId} onValidate={suggestion => setMessage(`Validated ${suggestion.id}`)} onAccept={suggestion => { setAcceptedId(suggestion.id); setMessage(`Inserted ${suggestion.id}`); }} onUndo={suggestion => { setAcceptedId(undefined); setMessage(`Undid ${suggestion.id}`); }} /><p role="status" data-testid="d3-suggestion-result">{message}</p><p>Simulated insertion only. Signal evidence comes from fixtures.</p></section>
    <section id="d3-tools" className="micro-gallery__beam"><h2>19 · Tool Call / Group</h2><div className="micro-gallery__controls">
      <Button onClick={() => { setStartedAt(Date.now()); setStatus('running'); }}>Simulate tool start</Button><Button onClick={() => setStatus('success')}>Simulate tool success</Button><Button onClick={() => setStatus('error')}>Simulate tool error</Button><Button onClick={() => setMounted(!mounted)}>{mounted ? 'Unmount tool' : 'Mount tool'}</Button>
    </div>{mounted && <ToolCall name="get_schema_context" summary="Analytics · read only" status={status} startedAt={startedAt} durationMs={2400} arguments={{ connection: 'Analytics', secret: '[REDACTED]' }} result={{ tables: ['orders', 'customers'], columns: 12 }} error="Schema context unavailable. Retry this read-only request." retrySafe onRetry={() => { setStartedAt(Date.now()); setStatus('running'); }} data-testid="d3-tool" />}
    <ToolCallGroup defaultOpen calls={[
      { id: 'denied', name: 'run_query', status: 'error', decision: 'denied', onRetry: () => setMessage('Unsafe retry'), result: { redacted: true } },
      { id: 'schema', name: 'get_schema_context', status: 'success', durationMs: 180, summary: '2 tables', result: { tables: ['orders', 'customers'] } },
      { id: 'work', name: 'get_work_context', status: 'success', durationMs: 60, summary: 'Current query', arguments: { includeHistory: false }, result: Array.from({ length: 50 }, (_, id) => ({ id, value: `Context ${id}` })) },
    ]} /><p>Mock activity. Retry is explicitly limited to safe read-only calls.</p></section>
  </>;
}
