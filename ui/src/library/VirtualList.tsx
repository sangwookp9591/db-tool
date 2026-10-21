import type { ComponentChildren } from 'preact';
import { useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';
import { visibleRange } from '../virtual';
export const LIBRARY_ROW_HEIGHT = 28;

interface Props {
  id: string; label: string; count: number; selected: number; rowHeight?: number;
  /** Visible rows changed; the owner loads or evicts pages here. */
  onRange: (first: number, last: number) => void;
  renderRow: (index: number) => ComponentChildren;
}

/** Fixed-height windowed list. Only the visible rows exist in the DOM, scrolling updates state only when the range changes, and nothing runs while idle. */
export function VirtualList({ id, label, count, selected, rowHeight = LIBRARY_ROW_HEIGHT, onRange, renderRow }: Props) {
  const box = useRef<HTMLDivElement>(null);
  const frame = useRef(0);
  const [view, setView] = useState({ top: 0, height: 240 });
  const range = visibleRange(count, view.top, view.height, rowHeight, 4);
  const onRangeRef = useRef(onRange); onRangeRef.current = onRange;
  const live = useRef({ range, count }); live.current = { range, count };

  useLayoutEffect(() => {
    const el = box.current!;
    setView({ top: el.scrollTop, height: el.clientHeight || 240 });
    const observer = typeof ResizeObserver === 'function' ? new ResizeObserver(() => setView(v => v.height === el.clientHeight ? v : { top: el.scrollTop, height: el.clientHeight })) : undefined;
    observer?.observe(el);
    return () => { observer?.disconnect(); cancelAnimationFrame(frame.current); };
  }, []);
  useEffect(() => { onRangeRef.current(range.start, Math.max(range.start, range.end - 1)); }, [range.start, range.end]);
  useLayoutEffect(() => {
    const el = box.current!; if (selected < 0 || selected >= count) return;
    const top = selected * rowHeight, bottom = top + rowHeight;
    if (top < el.scrollTop) el.scrollTop = top; else if (bottom > el.scrollTop + el.clientHeight) el.scrollTop = bottom - el.clientHeight;
  }, [selected]);

  const onScroll = () => {
    if (frame.current) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = 0; const el = box.current; if (!el) return;
      const { range: shown, count: total } = live.current, next = visibleRange(total, el.scrollTop, el.clientHeight, rowHeight, 4);
      if (next.start !== shown.start || next.end !== shown.end) setView({ top: el.scrollTop, height: el.clientHeight });
    });
  };
  const rows: ComponentChildren[] = [];
  for (let i = range.start; i < range.end; i++) rows.push(renderRow(i));
  return <div ref={box} id={id} class="lib-scroll" role="listbox" aria-label={label} aria-rowcount={count} onScroll={onScroll}>
    <div class="lib-spacer" style={{ height: count * rowHeight }}><div class="lib-window" style={{ transform: `translateY(${range.start * rowHeight}px)` }}>{rows}</div></div>
  </div>;
}
