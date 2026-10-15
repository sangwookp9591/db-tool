import type { SchemaGraph, Table, Value, BrowseTarget } from './contract';
export function referenceTarget(schema: SchemaGraph,entityId: string,table: Table,row: Value[],column: string): BrowseTarget | null {
  for(const relation of schema.relations) {
    if(relation.from!==entityId || !relation.from_fields.includes(column) || relation.from_fields.length!==relation.to_fields.length)continue;
    const indices=relation.from_fields.map(f=>table.columns.findIndex(c=>c.name===f));
    if(indices.some(i=>i<0 || !row[i] || row[i].t==='Null'))continue;
    return {nodeId:relation.to,title:schema.entities.find(e=>e.id===relation.to)?.name ?? relation.to,filter:relation.to_fields.map((f,i)=>[f,row[indices[i]]])};
  }
  return null;
}
export function rowNode(row: Value[]): string | null {return row.find(v=>v.t==='Node')?.v.id ?? null;}
