import { useEffect, useRef, useState } from 'preact/hooks';
import type { Table, Value } from './contract';
import { valueClass, valueText, valueTitle } from './values';
import { ROW_HEIGHT, visibleRange } from './virtual';
export function VirtualTable({ table }: { table: Table }) {
  const scroll = useRef<HTMLDivElement>(null);
  const [top, setTop] = useState(0);
  const [height, setHeight] = useState(350);
  const [widths, setWidths] = useState<number[]>([]);
  const [selected, setSelected] = useState<{ value: Value; column: string; row: number } | null>(null);
  useEffect(() => {
    const el = scroll.current!;
    const observer = new ResizeObserver(() => setHeight(el.clientHeight)); observer.observe(el);
    return () => observer.disconnect();
  }, []);
  useEffect(() => { setWidths(table.columns.map(() => 190)); setSelected(null); if (scroll.current) scroll.current.scrollTop = 0; setTop(0); }, [table]);
  const range = visibleRange(table.rows.length, top, Math.max(0, height - 44));
  const template = table.columns.map((_, i) => `${widths[i] ?? 190}px`).join(' ');
  const totalWidth = table.columns.reduce((sum, _, i) => sum + (widths[i] ?? 190), 0);
  function resize(index: number, event: PointerEvent) {
    event.preventDefault();
    const target = event.currentTarget as HTMLElement;
    target.setPointerCapture(event.pointerId);
    const start = event.clientX; const initial = widths[index] ?? 190;
    const move = (e: PointerEvent) => setWidths(previous => { const next = [...previous]; next[index] = Math.max(70, Math.min(1500, initial + e.clientX - start)); return next; });
    const end = () => { target.removeEventListener('pointermove', move); target.removeEventListener('pointerup', end); target.removeEventListener('pointercancel', end); };
    target.addEventListener('pointermove', move); target.addEventListener('pointerup', end); target.addEventListener('pointercancel', end);
  }
  return <div class="table-area">
    <div class="virtual-scroll" ref={scroll} onScroll={e => setTop(e.currentTarget.scrollTop)} role="grid" aria-label="Query results" aria-rowcount={table.rows.length + 1} aria-colcount={table.columns.length}>
      <div class="table-header" role="row" style={{ gridTemplateColumns: template, width: `${totalWidth}px` }}>
        {table.columns.map((column, i) => <div role="columnheader" class="column-header" key={i}><strong>{column.name}</strong><small>{column.type_name}</small><span role="separator" tabIndex={0} aria-label={`Resize ${column.name}`} aria-orientation="vertical" onPointerDown={e => resize(i, e)} onKeyDown={e => { if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') { e.preventDefault(); setWidths(prev => { const next = [...prev]; next[i] = Math.max(70, (next[i] ?? 190) + (e.key === 'ArrowLeft' ? -16 : 16)); return next; }); } }} /></div>)}
      </div>
      <div class="virtual-spacer" style={{ height: `${table.rows.length * ROW_HEIGHT}px`, width: `${totalWidth}px` }}>
        {table.rows.slice(range.start, range.end).map((row, j) => <div class="table-row" role="row" aria-rowindex={range.start + j + 2} key={range.start + j} style={{ top: `${(range.start + j) * ROW_HEIGHT}px`, height: `${ROW_HEIGHT}px`, gridTemplateColumns: template }}>
          {row.map((value, i) => <button role="gridcell" key={i} class={`table-cell ${valueClass(value)} ${selected?.row === range.start + j && selected.column === table.columns[i].name ? 'selected' : ''}`} title={valueTitle(value)} onClick={() => setSelected({ value, column: table.columns[i].name, row: range.start + j })}>{valueText(value).slice(0, 300)}</button>)}
        </div>)}
      </div>
    </div>
    {selected && <section class="cell-detail" aria-label="Cell detail"><div class="section-title"><span>{selected.column} · row {selected.row + 1} · {valueTitle(selected.value)}</span><button onClick={() => setSelected(null)} aria-label="Close cell detail">×</button></div><pre>{valueText(selected.value, true)}</pre></section>}
  </div>;
}
