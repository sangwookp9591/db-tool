import { Button } from './ui/Button';
import { ArrowLeft, ArrowRight } from 'lucide-preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import type { BrowseTarget, SchemaGraph } from './contract';
import { api } from './api';
import { QueryTab, BrowseTab } from './QueryTab';
import type { SchemaView as SchemaComponent } from './SchemaView';
import type { ComponentType } from 'preact';
import type { EditorHandle } from './Editor';
import type { DriverManifest, ResultSet } from './contract';
type Step = {kind:'query';title:string} | {kind:'schema';scope:string|null;title:string} | ({kind:'browse'} & BrowseTarget);
export function NavigationTab(props: {onDirty?:(dirty:boolean)=>void;aiSuggested?:boolean;readOnly?:boolean;kind:'query'|'browse'|'schema';scope?:string|null;nodeId?:string;title:string;initialText:string;sessionId:string;manifest:DriverManifest;graphLimit:number;active:boolean;onRun:(id:string|null)=>void;onReady:(handle:EditorHandle|null)=>void;onHistory:()=>void;onResults:(results:ResultSet[])=>void;onError:(error:unknown)=>void}) {
  const [busy,setBusy]=useState(false);
  function run(id:string|null){setBusy(!!id);props.onRun(id);}
  const [needsSchema,setNeedsSchema]=useState(props.kind!=='query'||!!props.nodeId);
  const editor=useRef<EditorHandle|null>(null),draft=useRef(props.initialText);
  function editorReady(handle:EditorHandle|null){if(!handle&&editor.current)draft.current=editor.current.text();editor.current=handle;props.onReady(handle);}
  const [steps,setSteps]=useState<Step[]>([props.kind==='schema'?{kind:'schema',scope:props.scope??null,title:props.title}:props.kind==='browse'?{kind:'browse',nodeId:props.nodeId!,title:props.title}:{kind:'query',title:props.title}]);
  const [index,setIndex]=useState(0),[schema,setSchema]=useState<SchemaGraph|null>(null),[retry,setRetry]=useState(0);
  const [SchemaView,setSchemaView]=useState<ComponentType<Parameters<typeof SchemaComponent>[0]>|null>(null);
  const step=steps[index];
  useEffect(()=>{if(step.kind==='schema'&&!SchemaView) {let alive=true;void import('./SchemaView').then(m=>{if(alive)setSchemaView(()=>m.SchemaView);}).catch(props.onError);return()=>{alive=false;};}},[step.kind,SchemaView,retry]);
  const scope=step.kind==='schema'?step.scope:step.kind==='browse'?step.nodeId:props.nodeId??null;
  useEffect(()=>{
    if(!props.manifest.capabilities.schema_graph || (!needsSchema && step.kind==='query'))return;
    let alive=true;void api('schema_graph',{sessionId:props.sessionId,scope}).then(s=>{if(alive)setSchema(s);}).catch(e=>{if(alive)props.onError(e);});return()=>{alive=false;};
  },[props.sessionId,scope,retry,needsSchema]);
  function browse(target:BrowseTarget){setSteps(previous=>[...previous.slice(0,index+1),{kind:'browse',...target}]);setIndex(index+1);}
  return <div class="navigation-tab"><nav class="toolbar" aria-label="Navigation history"><Button morph={false} aria-label="Back in tab" disabled={busy||index===0} onClick={()=>setIndex(i=>i-1)}><ArrowLeft /></Button><Button morph={false} aria-label="Forward in tab" disabled={busy||index===steps.length-1} onClick={()=>setIndex(i=>i+1)}><ArrowRight /></Button>{steps.map((s,i)=><Button morph={false} key={i} disabled={busy} aria-current={i===index?'page':undefined} onClick={()=>setIndex(i)}>{s.title}{s.kind==='browse'&&s.filter?.length?' · filtered':''}</Button>)}</nav><div class="navigation-content">
    {step.kind==='schema' ? schema && SchemaView ? <SchemaView schema={schema} sessionId={props.sessionId} active={props.active} onBrowse={browse} onError={props.onError}/> : <Button morph={false} onClick={()=>setRetry(n=>n+1)}>Load / retry schema graph</Button> : step.kind==='query' ? <QueryTab {...props} onRun={run} onResults={received=>{if(received.some(r=>r.graph))setNeedsSchema(true);props.onResults(received);}} initialText={draft.current} sourceText={props.initialText} onReady={editorReady} entityId={props.nodeId} onBrowse={browse} schema={schema ?? undefined}/> : <BrowseTab key={index} active={props.active} sessionId={props.sessionId} nodeId={step.nodeId} filter={step.filter} manifest={props.manifest} graphLimit={props.graphLimit} schema={schema ?? undefined} onBrowse={browse} onRun={run} onError={props.onError} onResults={props.onResults}/>}
  </div></div>;
}
