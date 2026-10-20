import { describe, expect, it } from 'vitest';
import { buildEditRequest, emptyEdits, pendingCount } from './edits';
import type { Table } from './contract';

const table: Table = { columns: [{ name: 'id', type_name: 'int8' }, { name: 'name', type_name: 'text' }], rows: [[{ t: 'Int', v: '1' }, { t: 'Text', v: 'Ada' }], [{ t: 'Int', v: '2' }, { t: 'Text', v: 'Lin' }]] };
describe('pending row edits', () => {
  it('counts row operations and builds key locked update, delete, and insert requests', () => {
    const pending = { updates: { 0: { name: { t: 'Text' as const, v: 'Ada Lovelace' } } }, deleted: [1], added: [{ id: 'new', values: [{ t: 'Null' as const }, { t: 'Text' as const, v: 'Grace' }] }] };
    expect(pendingCount(pending)).toBe(3);
    expect(buildEditRequest({ node_id: 'people', key_columns: ['id'] }, table, pending)).toEqual({ node_id: 'people', timeout_ms: 30000, preview_sample_limit: 50, originals: [ [['id',table.rows[0][0]],['name',table.rows[0][1]]], [['id',table.rows[1][0]],['name',table.rows[1][1]]], [] ], edits: [
      { op: 'Update', key: [['id', { t: 'Int', v: '1' }]], set: [['name', { t: 'Text', v: 'Ada Lovelace' }]] },
      { op: 'Delete', key: [['id', { t: 'Int', v: '2' }]] },
      { op: 'Insert', values: [['id', { t: 'Null' }], ['name', { t: 'Text', v: 'Grace' }]] },
    ] });
  });
  it('starts empty and rejects an invalid key target', () => {
    expect(pendingCount(emptyEdits())).toBe(0);
    expect(() => buildEditRequest({ node_id: 'people', key_columns: ['missing'] }, table, { ...emptyEdits(), deleted: [0] })).toThrow(/키 컬럼/);
  });
});

it('maps aliases to original names and never sends computed values', () => {
  const result: Table={columns:[{name:'alias_id',type_name:'int'},{name:'label',type_name:'text'},{name:'computed',type_name:'int'}],rows:[[ {t:'Int',v:'1'},{t:'Text',v:'old'},{t:'Int',v:'2'} ]]};
  const request=buildEditRequest({node_id:'t',key_columns:['id'],columns:['id','name',null]},result,{...emptyEdits(),updates:{0:{alias_id:{t:'Int',v:'99'},label:{t:'Text',v:'new'},computed:{t:'Int',v:'4'}}}});
  expect(request.edits).toEqual([{op:'Update',key:[['id',{t:'Int',v:'1'}]],set:[['name',{t:'Text',v:'new'}]]}]);
  expect(request.originals).toEqual([[['id',{t:'Int',v:'1'}],['name',{t:'Text',v:'old'}]]]);
});
