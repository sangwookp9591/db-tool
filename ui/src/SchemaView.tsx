import { useMemo } from 'preact/hooks';
import type { SchemaGraph, BrowseTarget } from './contract';
import { GraphView } from './GraphView';
export function SchemaView({schema,sessionId,active,onBrowse,onError}: {schema:SchemaGraph;sessionId:string;active?:boolean;onBrowse:(target:BrowseTarget)=>void;onError:(error:unknown)=>void}) {
  const graph=useMemo(()=>({nodes:schema.entities.map(e=>({id:e.id,labels:[e.name],props:e.fields.map(f=>[f.name,{t:'Text' as const,v:`${f.pk?'PK · ':''}${f.type_name} · ${f.nullable?'NULL':'NOT NULL'}`}] as [string,{t:'Text';v:string}])})),rels:schema.relations.map(r=>({id:r.id,rel_type:`${r.name}${r.from_fields.length?` (${r.from_fields.join(', ')} → ${r.to_fields.join(', ')})`:''}`,start:r.from,end:r.to,props:[]}))}),[schema]);
  return <GraphView graph={graph} schema={schema} sessionId={sessionId} active={active} canExpand={false} limit={Math.max(1,schema.entities.length)} onBrowse={onBrowse} onError={onError}/>;
}
