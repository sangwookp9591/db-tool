import { Check, ChevronRight, Undo2, X } from 'lucide-preact';
import { useMemo, useRef, useState } from 'preact/hooks';
import { Button } from '../ui/Button';
import { QueryCode } from './QueryCode';
import type { FileChange, FileReviewActions } from './review';
import { diffLines, lineCounts } from './review';
export function QueryDiff({before,after}:{before:string;after:string}) {
 const lines=useMemo(()=>diffLines(before,after),[before,after]);
 return <div class="ai-unified-diff"><QueryCode text={lines.map(line=>line.text).join('\n')} language="sql" diff={lines}/></div>;
}
export function FileReview({files,session,actions,onChange}:{files:FileChange[];session:string;actions:FileReviewActions;onChange:(files:FileChange[])=>void}){
 const busyRef=useRef(false),filesRef=useRef(files);filesRef.current=files;
 const [busy,setBusy]=useState(false),[error,setError]=useState(''),[target,setTarget]=useState(''),[accepted,setAccepted]=useState<string[]>([]);
 function change(next:FileChange[]){filesRef.current=next;onChange(next);}
 const counts=files.map(f=>lineCounts(f.before??'',f.after)).reduce((a,b)=>({added:a.added+b.added,removed:a.removed+b.removed}),{added:0,removed:0});
 const pending=files.filter(f=>f.decision==='pending');
 async function decide(selected:FileChange[],kind:'accept'|'resolve'|'undo',source?:'tab'|'file'){
  if(busyRef.current)return;busyRef.current=true;setBusy(true);setError('');setTarget('');const applied:string[]=[];
  try{for(const selectedFile of selected){const file=filesRef.current.find(f=>f.id===selectedFile.id);if(!file||kind!=='undo'&&file.decision!=='pending')continue;let updated:FileChange|null;
   if(kind==='undo')updated=file.decision==='accepted'?actions.undo(session,file):{...file,decision:'pending'};
   else if(kind==='resolve')updated=await actions.resolve(session,file,source);
   else if(file.before!==undefined){updated=await actions.apply(session,file);applied.push(file.id);}
   else continue;
   if(updated)change(filesRef.current.map(f=>f.id===file.id?updated!:f));
  }}catch(e){setError(String((e as Error).message??e));}finally{if(applied.length)setAccepted(applied);else if(kind==='undo')setAccepted([]);busyRef.current=false;setBusy(false);}
 }
 function reject(selected:FileChange[]){change(filesRef.current.map(f=>selected.some(s=>s.id===f.id)&&f.decision==='pending'?{...f,decision:'rejected'}:f));setTarget('');}
 return <section class="ai-file-review" aria-label="파일 변경 검토" aria-busy={busy}>
  <header><div class="ai-review-title"><strong>{files.length}개 파일 변경</strong><span class="ai-added">+{counts.added}</span><span class="ai-removed">−{counts.removed}</span></div><div class="ai-review-header-actions"><Button morph={false} variant="ghost" disabled={busy||!pending.length} onClick={()=>reject(pending)}>전체 거절</Button><Button morph={false} class="ai-review-primary" disabled={busy||!pending.length||pending.some(f=>f.before===undefined)} title={pending.some(f=>f.before===undefined)?'먼저 각 파일의 대상을 선택하세요.':undefined} onClick={()=>void decide(pending,'accept')}>전체 수락</Button></div></header>
  <div class="ai-file-list">{files.map(file=>{
   const split=file.path.lastIndexOf('/'),directory=file.path.slice(0,split+1),name=file.path.slice(split+1),count=lineCounts(file.before??'',file.after),total=count.added+count.removed;
   return <div class="ai-file" data-decision={file.decision} key={file.id}><details><summary><ChevronRight class="ai-file-chevron" size={12}/>{file.decision==='accepted'&&<Check class="ai-file-accepted" size={14} aria-label="수락됨"/>}<span class="ai-file-path" title={file.targetPath??file.path}><span>{directory}</span><strong>{name}</strong></span>{file.before===''&&<span class="ai-file-new">new</span>}
    {file.decision==='pending'&&file.before===undefined&&<Button bare class="ai-file-target" title="적용할 탭 또는 파일 위치를 선택하세요" aria-expanded={target===file.id} onClick={e=>{e.preventDefault();e.stopPropagation();setTarget(t=>t===file.id?'':file.id);}}>대상 선택</Button>}
    <span class="ai-file-count"><span class="ai-added">+{count.added}</span><span class="ai-removed">−{count.removed}</span></span><span class="ai-file-meter" aria-hidden="true">{Array.from({length:5},(_,i)=><i class={total===0?'':i<Math.round(count.added/total*5)?'add':'remove'}/>)}</span>
    <span class="ai-file-actions" onClick={e=>{e.preventDefault();e.stopPropagation();}}>{file.decision==='pending'?<><Button bare class="ai-review-icon" disabled={busy} title={`${file.path} 거절`} aria-label="거절" onClick={()=>reject([file])}><X size={14}/></Button><Button bare class="ai-review-icon" disabled={busy||file.before===undefined} title={file.before===undefined?'먼저 대상을 선택하세요':`${file.path} 수락`} aria-label="수락" onClick={()=>void decide([file],'accept')}><Check size={14}/></Button></>:<Button bare class="ai-review-undo" disabled={busy} onClick={()=>void decide([file],'undo')}><Undo2 size={12}/>되돌리기</Button>}</span>
   </summary><QueryDiff before={file.before??''} after={file.after}/></details>
    {target===file.id&&<div class="ai-file-target-menu" role="dialog" aria-label="파일 대상 선택"><small>열린 쿼리 탭</small>{actions.targets(session).map(choice=><Button variant="ghost" title={choice.targetPath??choice.label} disabled={busy} onClick={()=>{change(filesRef.current.map(f=>f.id===file.id?{...f,...choice}:f));setTarget('');}}>{choice.label}</Button>)}<hr/><Button variant="ghost" disabled={busy} onClick={()=>void decide([file],'resolve','file')}>파일 위치 선택…</Button></div>}
   </div>;
  })}</div>
  {error&&<p role="alert" class="ai-review-error">{error}</p>}
  {accepted.some(id=>files.some(f=>f.id===id&&f.decision==='accepted'))&&<div class="ai-review-toast" role="status"><Check size={14}/><span>{accepted.filter(id=>files.some(f=>f.id===id&&f.decision==='accepted')).length}개 파일에 적용됨</span><Button bare aria-label="수락 되돌리기" onClick={()=>void decide(files.filter(f=>accepted.includes(f.id)&&f.decision==='accepted').reverse(),'undo')}>되돌리기</Button></div>}
  <p class="ai-review-footnote">편집기에만 적용 · 저장·실행은 직접</p>
 </section>;
}
