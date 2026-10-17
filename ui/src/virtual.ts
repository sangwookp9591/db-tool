import type { Table } from './contract';
import { valueText } from './values';
export const ROW_HEIGHT = 26;
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

const NUMERIC = /int|float|double|numeric|decimal|real|serial/i;
// One width rule for browse (VirtualTable) and edit (EditableTable): header and the first 50 rows decide, so the same result never changes column width between modes.
export function columnWidths(table: Table): number[] {
  const sample = table.rows.slice(0, 50);
  return table.columns.map((column, index) => {
    const typeChars = /bigint|int8|long/i.test(column.type_name) ? 20 : /smallint|int2/i.test(column.type_name) ? 6 : /int|serial/i.test(column.type_name) ? 11 : NUMERIC.test(column.type_name) ? 24 : 0;
    let chars = Math.max(column.name.length, column.type_name.length, typeChars);
    for (const row of sample) { const v = row[index]; if (v) chars = Math.max(chars, Math.min(valueText(v).length, 60)); }
    return Math.round(Math.min(NUMERIC.test(column.type_name) ? 280 : 340, Math.max(110, chars * 8.2 + 28)));
  });
}
