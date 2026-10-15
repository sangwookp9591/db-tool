export const ROW_HEIGHT = 28;
export function visibleRange(count: number, scrollTop: number, height: number, rowHeight = ROW_HEIGHT, overscan = 6): { start: number; end: number } {
  const start = Math.min(count, Math.max(0, Math.floor(scrollTop / rowHeight) - overscan));
  return { start, end: Math.min(count, Math.max(start, Math.ceil((Math.max(0, scrollTop) + height) / rowHeight) + overscan)) };
}
export function columnRange(widths: number[], left: number, width: number, overscan = 1) {
  const offsets = [0]; for (const w of widths) offsets.push(offsets.at(-1)! + w);
  const first = offsets.findIndex((offset, i) => i < widths.length && offset + widths[i] > Math.max(0, left));
  const start = Math.max(0, (first < 0 ? widths.length : first) - overscan);
  let end = start; while (end < widths.length && offsets[end] < left + width) end++;
  end = Math.min(widths.length, end + overscan);
  return { start, end, offset: offsets[start], total: offsets.at(-1)! };
}
