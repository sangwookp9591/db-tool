import { Button } from './ui/Button';
import { X, ArrowUpRight } from 'lucide-preact';
import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import type { Table, Value } from './contract';
import { valueClass, valueText, valueTitle } from './values';
import { ROW_HEIGHT, visibleRange, columnRange } from './virtual';
import { tableIndex } from './perf/tableIndex';
import { painted } from './perf';
export function VirtualTable({ table, onRow, selectedNodeId, highlightedColumn, reference, onError }: { table: Table; onRow?: (row: Value[])=>void; selectedNodeId?: string | null; highlightedColumn?: string; reference?: (row: Value[],column: string)=> (()=>void) | null; onError?: (error:unknown)=>void }) {
  const callbacks = useRef({ onRow, reference }); callbacks.current = { onRow, reference };
  const scroll = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState({ top: 0, left: 0, height: 350, width: 800 });
  const defaultWidths = () => table.columns.map((column, index) => index === 0 && /int|float|double|numeric|decimal|real|serial/i.test(column.type_name) ? Math.min(220, Math.max(96, column.name.length * 8 + 24, (/bigint|int8|long/i.test(column.type_name) ? 20 : /smallint|int2/i.test(column.type_name) ? 6 : /int|serial/i.test(column.type_name) ? 11 : 24) * 7.5 + 24, ...table.rows.slice(0, 50).map(row => valueText(row[0]).length * 7.5 + 24))) : 190);
  const [widths, setWidths] = useState<number[]>(defaultWidths);
  const previousColumns = useRef(table.columns);
  const [filter, setFilter] = useState('');
  const [sort, setSort] = useState({ column: -1, descending: false });
  const [selected, setSelected] = useState<{ value: Value; column: string; row: number } | null>(null);
  const frame = useRef(0);
  const cache = useRef(new WeakMap<Value, string>());
  const cached = useRef(0);
  function schedule() {
    if (frame.current) return;
    frame.current = requestAnimationFrame(() => { frame.current = 0; const el = scroll.current; if (el) setViewport({ top: el.scrollTop, left: el.scrollLeft, height: el.clientHeight, width: el.clientWidth }); });
  }
  useEffect(() => { const observer = new ResizeObserver(schedule); observer.observe(scroll.current!); return () => { observer.disconnect(); cancelAnimationFrame(frame.current); }; }, []);
  useEffect(() => { if (previousColumns.current === table.columns) return; previousColumns.current = table.columns; setWidths(defaultWidths()); setSelected(null); setFilter(''); setSort({ column: -1, descending: false }); cache.current = new WeakMap(); cached.current = 0; if (scroll.current) { scroll.current.scrollTop = 0; scroll.current.scrollLeft = 0; } schedule(); }, [table.columns]);
  useEffect(() => { if (table.rows.length) void painted('table-first-paint'); }, [table.columns]);
  const indices = useMemo(() => filter || sort.column >= 0 ? tableIndex(table, filter, sort.column, sort.descending) : null, [table.rows.length, table.rows, filter, sort]);
  const count = indices?.length ?? table.rows.length;
  const [displayCount, setDisplayCount] = useState(count), latestCount = useRef(count), counterTimer = useRef(0);
  latestCount.current = count;
  useEffect(() => { if (!counterTimer.current) counterTimer.current = window.setTimeout(() => { counterTimer.current = 0; setDisplayCount(latestCount.current); }, 100); }, [count]);
  useEffect(() => () => clearTimeout(counterTimer.current), []);
  const range = visibleRange(count, viewport.top, Math.max(0, viewport.height - 44));
  const actualWidths = useMemo(() => table.columns.map((_, i) => widths[i] ?? 190), [table.columns, widths]);
  const columns = useMemo(() => columnRange(actualWidths, viewport.left, viewport.width), [actualWidths, viewport.left, viewport.width]);
  const shownColumns = useMemo(() => table.columns.slice(columns.start, columns.end), [table.columns, columns]);
  const positions = useMemo(() => { let left = columns.offset; return shownColumns.map((_, j) => { const position = left; left += actualWidths[columns.start + j]; return position; }); }, [shownColumns, columns, actualWidths]);
  function cell(value: Value) { let text = cache.current.get(value); if (text === undefined) { text = valueText(value).slice(0, 300); if (cached.current >= 4096) { cache.current = new WeakMap(); cached.current = 0; } cache.current.set(value, text); cached.current++; } return text; }
  function resize(index: number, event: PointerEvent) {
    event.preventDefault(); const target = event.currentTarget as HTMLElement; target.setPointerCapture(event.pointerId);
    const start = event.clientX, initial = widths[index] ?? 190;
    const move = (e: PointerEvent) => setWidths(previous => { const next = [...previous]; next[index] = Math.max(70, Math.min(1500, initial + e.clientX - start)); return next; });
    const end = () => { target.removeEventListener('pointermove', move); target.removeEventListener('pointerup', end); target.removeEventListener('pointercancel', end); };
    target.addEventListener('pointermove', move); target.addEventListener('pointerup', end); target.addEventListener('pointercancel', end);
  }
  // The pure bare Button renderer avoids per-cell component instances; streamed prefixes stay cached.
  const visibleRows = useMemo(() => Array.from({ length: range.end - range.start }, (_, j) => { const visible = range.start + j; const index = indices?.[visible] ?? visible; const row = table.rows[index]; return <div class={`table-row ${row.some(v=>v.t==='Node'&&v.v.id===selectedNodeId)?'selected':''}`} role="row" aria-rowindex={visible + 2} key={j} style={{ top: 0, transform: `translateY(${visible * ROW_HEIGHT}px)`, willChange: 'transform', height: `${ROW_HEIGHT}px`, width: `${columns.total}px`, display: 'block' }}>
          {shownColumns.map((column, c) => { const i = columns.start + c, value = row[i]; return <>{Button({bare:true,role:'gridcell','aria-colindex':i+1,key:i,style:{position:'absolute',left:`${positions[c]}px`,width:`${actualWidths[i]}px`},class:`table-cell ${valueClass(value)} ${selected?.row === index && selected.column === column.name ? 'selected' : ''}`,title:valueTitle(value),onClick:()=>{setSelected({value,column:column.name,row:index});callbacks.current.onRow?.(row);},children:value.t==='Null'?<span class="null-pill">NULL</span>:value.t==='Text'&&value.v===''?'""':cell(value)})}{callbacks.current.reference?.(row,column.name) && <Button bare class="fk-link" style={{position:'absolute',left:`${positions[c]+actualWidths[i]-30}px`,width:'28px'}} aria-label={`Follow reference ${column.name} row ${index+1}`} onClick={()=>callbacks.current.reference?.(row,column.name)?.()}><ArrowUpRight /></Button>}</>; })}
        </div>; }), [table.rows, range.start, range.end, indices, shownColumns, positions, actualWidths, columns.total, selectedNodeId, selected, reference]);
  return <div class="table-area">
    <div class="toolbar"><input aria-label="Filter loaded rows" placeholder="불러온 행에서 검색…" value={filter} onInput={e => { setFilter(e.currentTarget.value); scroll.current!.scrollTop = 0; schedule(); }} /><small class="loaded-chip">불러온 행 <span>{displayCount.toLocaleString()}</span></small><small>헤더를 클릭해 정렬</small></div>
    <div class="virtual-scroll" ref={scroll} onScroll={schedule} role="grid" aria-label="Query results" aria-rowcount={count + 1} aria-colcount={table.columns.length} style={{ contain: 'strict' }}>
      <div class="table-header" role="row" style={{ width: `${columns.total}px`, display: 'block' }}>
        <div style={{ position: 'relative', width: `${columns.total}px`, height: '44px' }}>
        {shownColumns.map((column, j) => { const i = columns.start + j; return <div role="columnheader" aria-colindex={i + 1} aria-sort={sort.column === i ? sort.descending ? 'descending' : 'ascending' : 'none'} class={`column-header ${columnType(column.type_name)} ${highlightedColumn===column.name?'selected':''}`} key={i} style={{ position: 'absolute', left: `${positions[j]}px`, width: `${actualWidths[i]}px`, height: '44px' }}><Button bare style={{ padding: 0, textAlign: 'left', border: 0, overflow: 'hidden', textOverflow: 'ellipsis' }} onClick={() => setSort(s => ({ column: i, descending: s.column === i && !s.descending }))}>{column.name}</Button><small class={columnType(column.type_name)}>{column.type_name}</small><span role="separator" tabIndex={0} aria-label={`Resize ${column.name}`} aria-orientation="vertical" onPointerDown={e => resize(i, e)} onKeyDown={e => { if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') { e.preventDefault(); setWidths(prev => { const next = [...prev]; next[i] = Math.max(70, Math.min(1500, (next[i] ?? 190) + (e.key === 'ArrowLeft' ? -16 : 16))); return next; }); } }} /></div>; })}
        </div>
      </div>
      <div class="virtual-spacer" style={{ height: `${count * ROW_HEIGHT}px`, width: `${columns.total}px` }}>
        {visibleRows}
      </div>
    </div>
    {selected && <section class="cell-detail" aria-label="Cell detail"><div class="section-title"><span>{selected.column} · row {selected.row + 1} · {valueTitle(selected.value)}</span><Button bare onClick={() => setSelected(null)} aria-label="Close cell detail"><X /></Button></div><pre>{valueText(selected.value,true)}</pre></section>}
  </div>;
}

function columnType(name: string) {
  const type = name.toLowerCase();
  return /int|float|double|numeric|decimal|real|serial/.test(type) ? 'value-number' : /bool/.test(type) ? 'value-boolean' : /date|time|interval/.test(type) ? 'value-date' : /json|map|list|array|node|relationship/.test(type) ? 'value-json' : /byte|binary|blob/.test(type) ? 'value-binary' : 'value-string';
}
