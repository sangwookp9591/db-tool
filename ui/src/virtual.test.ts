import { expect, it } from 'vitest';
import type { Table } from './contract';
import { columnWidths } from './virtual';
import { fitAspect, fitGraph } from './graph';

const table: Table = {
  columns: [{ name: 'id', type_name: 'int8' }, { name: 'amount', type_name: 'numeric' }, { name: 'note', type_name: 'text' }],
  rows: [[{ t: 'Int', v: '9007199254740993' }, { t: 'Decimal', v: '12345678901234567890.123400' }, { t: 'Text', v: 'x' }]] as Table['rows'],
};
it('sizes columns from header and sampled values, shared by browse and edit', () => {
  const widths = columnWidths(table);
  expect(widths).toHaveLength(3);
  expect(widths[1]).toBeGreaterThan(widths[0]);
  expect(widths.every(w => w >= 110 && w <= 340)).toBe(true);
  expect(columnWidths(table)).toEqual(widths);
});
it('fit aspect matches the padded canvas and a stretched layout fills both axes', () => {
  expect(fitAspect(960, 330)).toBeCloseTo((960 - 96) / (330 - 96));
  const a = fitAspect(960, 330), nodes = [{ x: -100 * a, y: -100 }, { x: 100 * a, y: 100 }];
  const { scale } = fitGraph(nodes, 960, 330), usedX = (200 * a + 40) * scale;
  expect(usedX / (960 - 96)).toBeGreaterThan(.85);
});
