import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import type { Value } from './contract';
import { documentRows } from './documents';
import { valueText } from './values';
import { visibleRange } from './virtual';
export function DocumentTree({values,onField,onError}: {values: Value[];onField?: (column: string)=>void;onError?: (error: unknown)=>void}) {
  const [open,setOpen]=useState(new Set<string>(values.length===1?['0']:[])),[viewport,setViewport]=useState({top:0,height:400});
  const scroll=useRef<HTMLDivElement>(null),frame=useRef(0);
  useEffect(()=>{setOpen(new Set(values.length===1?['0']:[]));if(scroll.current)scroll.current.scrollTop=0;},[values]);
  function schedule(){if(!frame.current)frame.current=requestAnimationFrame(()=>{frame.current=0;const e=scroll.current;if(e)setViewport({top:e.scrollTop,height:e.clientHeight});});}
  useEffect(()=>{const observer=new ResizeObserver(schedule);observer.observe(scroll.current!);return()=>{observer.disconnect();cancelAnimationFrame(frame.current);};},[]);
  const rows=useMemo(()=>documentRows(values,open),[values,open]);
  const range=visibleRange(rows.length,viewport.top,viewport.height,28);
  return <div class="document-tree" role="tree" aria-label="Document tree" ref={scroll} onScroll={schedule}><div style={{height:`${rows.length*28}px`,position:'relative'}}>{rows.slice(range.start,range.end).map((r,j)=><div role="treeitem" aria-level={r.depth+1} aria-expanded={r.expandable?open.has(r.id):undefined} class="document-row" key={r.id} style={{position:'absolute',top:`${(range.start+j)*28}px`,paddingLeft:`${r.depth*16}px`,height:'28px'}}><button aria-label={`${open.has(r.id)?'Collapse':'Expand'} ${r.path||r.name}`} disabled={!r.expandable} onClick={()=>setOpen(previous=>{const next=new Set(previous);if(next.has(r.id))next.delete(r.id);else next.add(r.id);return next;})}>{r.expandable?(open.has(r.id)?'▾':'▸'):'·'}</button><button class="document-field" onClick={()=>onField?.(r.column)}>{r.name}</button><span class="type-badge">{r.value.t==='Other'?r.value.v.type_name:r.value.t}</span><span class="document-value">{r.expandable ? (r.value.t==='List'?`${r.value.v.length} items`:'') : valueText(r.value,true)}</span>{r.path&&<button aria-label={`Copy path ${r.path}`} onClick={async()=>{try{await navigator.clipboard.writeText(r.path);}catch(e){onError?.(e);}}}>Copy path</button>}</div>)}</div></div>;
}
