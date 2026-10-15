import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import type { Table, Value } from './contract';
import { valueClass, valueText, valueTitle } from './values';
import { ROW_HEIGHT, visibleRange, columnRange } from './virtual';
import { tableIndex } from './perf/tableIndex';
import { painted } from './perf';
export function VirtualTable({ table }: { table: Table }) {
  const scroll = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState({ top: 0, left: 0, height: 350, width: 800 });
  const [widths, setWidths] = useState<number[]>([]);
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
  useEffect(() => { setWidths(table.columns.map(() => 190)); setSelected(null); setFilter(''); setSort({ column: -1, descending: false }); cache.current = new WeakMap(); cached.current = 0; if (scroll.current) { scroll.current.scrollTop = 0; scroll.current.scrollLeft = 0; } schedule(); }, [table.columns]);
  useEffect(() => { if (table.rows.length) void painted('table-first-paint'); }, [table.columns]);
  const indices = useMemo(() => filter || sort.column >= 0 ? tableIndex(table, filter, sort.column, sort.descending) : null, [table.rows.length, table.rows, filter, sort]);
  const count = indices?.length ?? table.rows.length;
  const [displayCount, setDisplayCount] = useState(count), latestCount = useRef(count), counterTimer = useRef(0);
  latestCount.current = count;
  useEffect(() => { if (!counterTimer.current) counterTimer.current = window.setTimeout(() => { counterTimer.current = 0; setDisplayCount(latestCount.current); }, 100); }, [count]);
  useEffect(() => () => clearTimeout(counterTimer.current), []);
  const range = visibleRange(count, viewport.top, Math.max(0, viewport.height - 44));
  const actualWidths = table.columns.map((_, i) => widths[i] ?? 190);
  const columns = columnRange(actualWidths, viewport.left, viewport.width);
  const shownColumns = table.columns.slice(columns.start, columns.end);
  let columnLeft = columns.offset;
  const positions = shownColumns.map((_, j) => { const left = columnLeft; columnLeft += actualWidths[columns.start + j]; return left; });
  function cell(value: Value) { let text = cache.current.get(value); if (text === undefined) { text = valueText(value).slice(0, 300); if (cached.current >= 4096) { cache.current = new WeakMap(); cached.current = 0; } cache.current.set(value, text); cached.current++; } return text; }
  function resize(index: number, event: PointerEvent) {
    event.preventDefault(); const target = event.currentTarget as HTMLElement; target.setPointerCapture(event.pointerId);
    const start = event.clientX, initial = widths[index] ?? 190;
    const move = (e: PointerEvent) => setWidths(previous => { const next = [...previous]; next[index] = Math.max(70, Math.min(1500, initial + e.clientX - start)); return next; });
    const end = () => { target.removeEventListener('pointermove', move); target.removeEventListener('pointerup', end); target.removeEventListener('pointercancel', end); };
    target.addEventListener('pointermove', move); target.addEventListener('pointerup', end); target.addEventListener('pointercancel', end);
  }
  return <div class="table-area">
    <div class="toolbar"><input aria-label="Filter loaded rows" placeholder="Filter loaded rows" value={filter} onInput={e => { setFilter(e.currentTarget.value); scroll.current!.scrollTop = 0; schedule(); }} /><small><span class="metric-number">{displayCount.toLocaleString()}</span> loaded rows · click header to sort</small></div>
    <div class="virtual-scroll" ref={scroll} onScroll={schedule} role="grid" aria-label="Query results" aria-rowcount={count + 1} aria-colcount={table.columns.length} style={{ contain: 'strict' }}>
      <div class="table-header" role="row" style={{ width: `${columns.total}px`, display: 'block' }}>
        <div style={{ position: 'relative', width: `${columns.total}px`, height: '44px' }}>
        {shownColumns.map((column, j) => { const i = columns.start + j; return <div role="columnheader" aria-colindex={i + 1} aria-sort={sort.column === i ? sort.descending ? 'descending' : 'ascending' : 'none'} class="column-header" key={i} style={{ position: 'absolute', left: `${positions[j]}px`, width: `${actualWidths[i]}px`, height: '44px' }}><button style={{ padding: 0, textAlign: 'left', border: 0, overflow: 'hidden', textOverflow: 'ellipsis' }} onClick={() => setSort(s => ({ column: i, descending: s.column === i && !s.descending }))}>{column.name}</button><small>{column.type_name}</small><span role="separator" tabIndex={0} aria-label={`Resize ${column.name}`} aria-orientation="vertical" onPointerDown={e => resize(i, e)} onKeyDown={e => { if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') { e.preventDefault(); setWidths(prev => { const next = [...prev]; next[i] = Math.max(70, Math.min(1500, (next[i] ?? 190) + (e.key === 'ArrowLeft' ? -16 : 16))); return next; }); } }} /></div>; })}
        </div>
      </div>
      <div class="virtual-spacer" style={{ height: `${count * ROW_HEIGHT}px`, width: `${columns.total}px` }}>
        {Array.from({ length: range.end - range.start }, (_, j) => { const visible = range.start + j; const index = indices?.[visible] ?? visible; const row = table.rows[index]; return <div class="table-row" role="row" aria-rowindex={visible + 2} key={j} style={{ top: 0, transform: `translateY(${visible * ROW_HEIGHT}px)`, willChange: 'transform', height: `${ROW_HEIGHT}px`, width: `${columns.total}px`, display: 'block' }}>
          {shownColumns.map((column, c) => { const i = columns.start + c, value = row[i]; return <button role="gridcell" aria-colindex={i + 1} key={i} style={{ position: 'absolute', left: `${positions[c]}px`, width: `${actualWidths[i]}px` }} class={`table-cell ${valueClass(value)} ${selected?.row === index && selected.column === column.name ? 'selected' : ''}`} title={valueTitle(value)} onClick={() => setSelected({ value, column: column.name, row: index })}>{cell(value)}</button>; })}
        </div>; })}
      </div>
    </div>
    {selected && <section class="cell-detail" aria-label="Cell detail"><div class="section-title"><span>{selected.column} · row {selected.row + 1} · {valueTitle(selected.value)}</span><button onClick={() => setSelected(null)} aria-label="Close cell detail">×</button></div><pre>{valueText(selected.value, true)}</pre></section>}
  </div>;
}
