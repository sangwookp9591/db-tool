import { blockStream } from './chat';
export interface FileChange { id:string; path:string; after:string; before?:string; tabId?:string; targetPath?:string; decision:'pending'|'accepted'|'rejected'; revision?:number }
export interface FileReviewActions {
 targets:(session:string)=>(Pick<FileChange,'before'|'tabId'|'targetPath'>&{label:string})[];
 snapshot:(session:string,path:string)=>Pick<FileChange,'before'|'tabId'|'targetPath'>;
 resolve:(session:string,file:FileChange,source?:'tab'|'file')=>Promise<FileChange|null>;
 apply:(session:string,file:FileChange)=>Promise<FileChange>;
 undo:(session:string,file:FileChange)=>FileChange;
}
export interface ToolRequest { name:string; arguments:Record<string,unknown>; client_id?:string }
export interface ToolPolicy { allowed:boolean;reason?:string;read_only:boolean;destructive:boolean;can_apply:boolean;connection:string;mode:string;client:string }
export interface ToolProposal { id:string; request:ToolRequest; reason:string; session:string; readOnly:boolean }
export function fileChanges(text:string):FileChange[] {
 const blocks=blockStream()(text,true).blocks;let heading='';const files:FileChange[]=[];
 for(const block of blocks){
  if(block.kind==='text'){heading=block.text.match(/(?:^|\n)\s*(?:#{1,6}\s*)?`?([^\n`]+\.sql)`?\s*$/i)?.[1]?.trim()??'';continue;}
  const meta=block.language??'';
  if(!/^sql(?:\s|$)|\.sql$/i.test(meta))continue;
  const path=meta.match(/(?:file(?:name)?=)?["']?([^\s"']+\.sql)["']?/i)?.[1]??heading??'';
  files.push({id:String(files.length),path:path||`proposal-${files.length+1}.sql`,after:block.text,decision:'pending'});heading='';
 }
 return files;
}
export function toolProposals(text:string):{request:ToolRequest;reason:string}[] {
 return blockStream()(text,true).blocks.filter(b=>b.kind==='code'&&/^(mcp|tool)$/i.test(b.language??'')).flatMap(b=>{
  try{const value=JSON.parse(b.text);if(typeof value.name!=='string'||!value.arguments||Array.isArray(value.arguments)||typeof value.arguments!=='object')return [];
   return [{request:{name:value.name,arguments:value.arguments,...(typeof value.client_id==='string'?{client_id:value.client_id}:{})},reason:typeof value.reason==='string'?value.reason:'AI가 이 도구 호출을 제안했습니다.'}];
  }catch{return [];}
 });
}
export function lineCounts(before:string,after:string){
 const a=before?before.split('\n'):[],b=after?after.split('\n'):[];let start=0,end=0;
 while(start<a.length&&start<b.length&&a[start]===b[start])start++;
 while(end<a.length-start&&end<b.length-start&&a[a.length-end-1]===b[b.length-end-1])end++;
 return {added:b.length-start-end,removed:a.length-start-end};
}

export interface DiffLine {text:string;kind:'context'|'add'|'remove';number?:number}
export function diffLines(before:string,after:string):DiffLine[]{
 const a=before?before.split('\n'):[],b=after?after.split('\n'):[];let start=0,end=0;
 while(start<a.length&&start<b.length&&a[start]===b[start])start++;
 while(end<a.length-start&&end<b.length-start&&a[a.length-end-1]===b[b.length-end-1])end++;
 return [...a.slice(0,start).map((text,i)=>({text,kind:'context' as const,number:i+1})),...a.slice(start,a.length-end).map((text,i)=>({text,kind:'remove' as const,number:start+i+1})),...b.slice(start,b.length-end).map((text,i)=>({text,kind:'add' as const,number:start+i+1})),...b.slice(b.length-end).map((text,i)=>({text,kind:'context' as const,number:b.length-end+i+1}))];
}
