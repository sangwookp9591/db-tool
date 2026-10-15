import { expect,it } from 'vitest';
import { referenceTarget } from './navigation';
import type { SchemaGraph,Table,Value } from './contract';
it('maps composite FK keys in relation order and refuses missing or null keys',()=>{
 const schema:SchemaGraph={entities:[],relations:[{id:'fk',name:'fk',from:'child',to:'parent',from_fields:['tenant','parent_id'],to_fields:['tenant','id'],kind:'foreign_key'}]};
 const table:Table={columns:[{name:'parent_id',type_name:'Int'},{name:'tenant',type_name:'Text'}],rows:[]};
 const row:Value[]=[{t:'Int',v:'9223372036854775807'},{t:'Text',v:'a'}];
 expect(referenceTarget(schema,'child',table,row,'parent_id')?.filter).toEqual([['tenant',row[1]],['id',row[0]]]);
 expect(referenceTarget(schema,'child',table,[{t:'Null'},row[1]],'parent_id')).toBeNull();
 expect(referenceTarget(schema,'other',table,row,'parent_id')).toBeNull();
 expect(referenceTarget(schema,'child',{...table,columns:table.columns.slice(0,1)},row,'parent_id')).toBeNull();
});
