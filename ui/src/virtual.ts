export const ROW_HEIGHT = 28;
export function visibleRange(count: number, scrollTop: number, height: number, rowHeight = ROW_HEIGHT, overscan = 6): { start: number; end: number } {
  const start = Math.min(count, Math.max(0, Math.floor(scrollTop / rowHeight) - overscan));
  return { start, end: Math.min(count, Math.max(start, Math.ceil((Math.max(0, scrollTop) + height) / rowHeight) + overscan)) };
}
