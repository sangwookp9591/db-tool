import { Button } from './ui/Button';
import { ArrowLeft, ArrowRight } from 'lucide-preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import type { BrowseTarget, SchemaGraph } from './contract';
import { api } from './api';
import { ErrorBanner } from './ErrorBanner';
import { normalizeError } from './values';
import { QueryTab, BrowseTab } from './QueryTab';
import type { SchemaView as SchemaComponent } from './SchemaView';
import type { ComponentType } from 'preact';
import type { EditorHandle } from './Editor';
import type { DriverManifest, ResultSet } from './contract';
type Step = {kind:'query';title:string} | {kind:'schema';scope:string|null;title:string} | ({kind:'browse'} & BrowseTarget);
export function NavigationTab(props: {onDirty?:(dirty:boolean)=>void;aiSuggested?:boolean;readOnly?:boolean;kind:'query'|'browse'|'schema';scope?:string|null;nodeId?:string;title:string;initialText:string;sessionId:string;manifest:DriverManifest;graphLimit:number;active:boolean;onRun:(id:string|null)=>void;onReady:(handle:EditorHandle|null)=>void;onHistory:()=>void;onResults:(results:ResultSet[])=>void;onError:(error:unknown)=>void}) {
  const [busy,setBusy]=useState(false);
  // Schema loading/diagram errors have no view of their own, so this tab shows them (query and browse views keep their own banners).
  const [schemaError,setSchemaError]=useState<ReturnType<typeof normalizeError>|null>(null);
  const failSchema=(e:unknown)=>{setSchemaError(normalizeError(e));props.onError(e);};
  function run(id:string|null){setBusy(!!id);props.onRun(id);}
  const [needsSchema,setNeedsSchema]=useState(props.kind!=='query'||!!props.nodeId);
  const editor=useRef<EditorHandle|null>(null),draft=useRef(props.initialText);
  function editorReady(handle:EditorHandle|null){if(!handle&&editor.current)draft.current=editor.current.text();editor.current=handle;props.onReady(handle);}
  const [steps,setSteps]=useState<Step[]>([props.kind==='schema'?{kind:'schema',scope:props.scope??null,title:props.title}:props.kind==='browse'?{kind:'browse',nodeId:props.nodeId!,title:props.title}:{kind:'query',title:props.title}]);
  const [index,setIndex]=useState(0),[schema,setSchema]=useState<SchemaGraph|null>(null),[retry,setRetry]=useState(0);
  const [SchemaView,setSchemaView]=useState<ComponentType<Parameters<typeof SchemaComponent>[0]>|null>(null);
  const step=steps[index];
  useEffect(()=>{if(step.kind==='schema'&&!SchemaView) {let alive=true;void import('./SchemaView').then(m=>{if(alive)setSchemaView(()=>m.SchemaView);}).catch(failSchema);return()=>{alive=false;};}},[step.kind,SchemaView,retry]);
  const scope=step.kind==='schema'?step.scope:step.kind==='browse'?step.nodeId:props.nodeId??null;
  useEffect(()=>{
    if(!props.manifest.capabilities.schema_graph || (!needsSchema && step.kind==='query'))return;
    let alive=true;void api('schema_graph',{sessionId:props.sessionId,scope}).then(s=>{if(alive){setSchema(s);setSchemaError(null);}}).catch(e=>{if(alive)failSchema(e);});return()=>{alive=false;};
  },[props.sessionId,scope,retry,needsSchema]);
  function browse(target:BrowseTarget){setSteps(previous=>[...previous.slice(0,index+1),{kind:'browse',...target}]);setIndex(index+1);}
  return <div class="navigation-tab">{steps.length>1&&<nav class="toolbar" aria-label="Navigation history"><Button morph={false} aria-label="Back in tab" disabled={busy||index===0} onClick={()=>setIndex(i=>i-1)}><ArrowLeft /></Button><Button morph={false} aria-label="Forward in tab" disabled={busy||index===steps.length-1} onClick={()=>setIndex(i=>i+1)}><ArrowRight /></Button>{steps.map((s,i)=><Button morph={false} key={i} disabled={busy} aria-current={i===index?'page':undefined} onClick={()=>setIndex(i)}>{s.title}{s.kind==='browse'&&s.filter?.length?' · 필터됨':''}</Button>)}</nav>}<ErrorBanner scope="tab" error={schemaError} onDismiss={()=>setSchemaError(null)} dismissLabel="Dismiss schema error"/><div class="navigation-content">
    {step.kind==='schema' ? schema && SchemaView ? <SchemaView schema={schema} sessionId={props.sessionId} active={props.active} onBrowse={browse} onError={failSchema}/> : <Button morph={false} onClick={()=>setRetry(n=>n+1)}>스키마 그래프 불러오기 / 다시 시도</Button> : step.kind==='query' ? <QueryTab {...props} onRun={run} onResults={received=>{if(received.some(r=>r.graph))setNeedsSchema(true);props.onResults(received);}} initialText={draft.current} sourceText={props.initialText} onReady={editorReady} entityId={props.nodeId} onBrowse={browse} schema={schema ?? undefined}/> : <BrowseTab key={index} active={props.active} readOnly={props.readOnly} sessionId={props.sessionId} nodeId={step.nodeId} filter={step.filter} manifest={props.manifest} graphLimit={props.graphLimit} schema={schema ?? undefined} onBrowse={browse} onRun={run} onError={props.onError} onResults={props.onResults}/>}
  </div></div>;
}
