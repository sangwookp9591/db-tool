import { describe, expect, it } from 'vitest';
import { buildEditRequest, emptyEdits, pendingCount } from './edits';
import type { Table } from './contract';

const table: Table = { columns: [{ name: 'id', type_name: 'int8' }, { name: 'name', type_name: 'text' }], rows: [[{ t: 'Int', v: '1' }, { t: 'Text', v: 'Ada' }], [{ t: 'Int', v: '2' }, { t: 'Text', v: 'Lin' }]] };
describe('pending row edits', () => {
  it('counts row operations and builds key locked update, delete, and insert requests', () => {
    const pending = { updates: { 0: { name: { t: 'Text' as const, v: 'Ada Lovelace' } } }, deleted: [1], added: [{ id: 'new', values: [{ t: 'Null' as const }, { t: 'Text' as const, v: 'Grace' }] }] };
    expect(pendingCount(pending)).toBe(3);
    expect(buildEditRequest({ node_id: 'people', key_columns: ['id'] }, table, pending)).toEqual({ node_id: 'people', timeout_ms: 30000, preview_sample_limit: 50, edits: [
      { op: 'Update', key: [['id', { t: 'Int', v: '1' }]], set: [['name', { t: 'Text', v: 'Ada Lovelace' }]] },
      { op: 'Delete', key: [['id', { t: 'Int', v: '2' }]] },
      { op: 'Insert', values: [['id', { t: 'Null' }], ['name', { t: 'Text', v: 'Grace' }]] },
    ] });
  });
  it('starts empty and rejects an invalid key target', () => {
    expect(pendingCount(emptyEdits())).toBe(0);
    expect(() => buildEditRequest({ node_id: 'people', key_columns: ['missing'] }, table, { ...emptyEdits(), deleted: [0] })).toThrow(/Key column/);
  });
});
