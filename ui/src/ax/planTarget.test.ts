import { expect, it } from 'vitest';
import type { ExplorerNode } from '../contract';
import { planTarget } from './planTarget';
const table = (id: string): ExplorerNode => ({ id, label: 'people', kind: 'table', icon: 'Table', has_children: true, detail: null, actions: [{id:'browse',label:'열기',kind:'Browse'}] });
it('uses the driver ID rather than constructing a path from a plan label', async () => {
  const node = table('["schema","public","tables","people"]');
  expect(await planTarget('people', async parent => { expect(parent).toBeNull(); return [node]; })).toBe(node);
});
it('refuses ambiguous names and missing targets', async () => {
  await expect(planTarget('people', async () => [table('a'),table('b')])).rejects.toThrow('여러 개');
  await expect(planTarget('missing', async () => [table('a')])).rejects.toThrow('직접 선택');
});
