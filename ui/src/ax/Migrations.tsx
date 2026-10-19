import { Check, Clock3 } from 'lucide-preact';
import { useState } from 'preact/hooks';
import { api } from '../api';
import { Button } from '../ui/Button';
import { ChangePreviewView } from '../ChangePreviewView';
import type { ChangePreview, Profile } from '../contract';
import type { MigrationList, Migration } from './managementContract';
import { PlanCard } from './PlanCard';
export function Migrations({sessionId,profile}:{sessionId:string;profile:Profile}){
 const [path,setPath]=useState(''),[list,setList]=useState<MigrationList|null>(null),[selected,setSelected]=useState<Migration|null>(null),[preview,setPreview]=useState<ChangePreview|null>(null),[error,setError]=useState(''),[busy,setBusy]=useState(false);
 async function scan(){setBusy(true);setError('');setSelected(null);setPreview(null);try{setList(await api('migration_scan',{sessionId,path}));}catch(e){setError(String((e as Error).message??e));}finally{setBusy(false);}}
 return <div class="ax-stack"><label class="ax-field">마이그레이션 폴더<input value={path} placeholder="/project/migrations" onInput={e=>{setPath(e.currentTarget.value);setList(null);setSelected(null);}}/></label><div class="ax-inline"><Button morph={false} disabled={busy||!path.trim()} onClick={()=>void scan()}>폴더 읽기</Button><small>Flyway · Prisma · Rails · NNN_*.sql</small></div>{list&&<div class="ax-grid"><ol class="ax-timeline">{list.migrations.map(file=><li key={file.id} data-status={file.applied?'passed':'waiting'} data-selected={selected?.id===file.id}><span class="ax-timeline-dot" aria-hidden="true">{file.applied?<Check/>:<Clock3/>}</span><div><Button morph={false} onClick={()=>{setSelected(file);setPreview(null);}}>{file.name}</Button><p>{file.applied?'적용됨':'대기 중'}</p></div></li>)}</ol>{selected&&<PlanCard key={selected.name} title={selected.name} evidence={['로컬 마이그레이션 폴더',selected.checksum??'파일 내용 검토 필요']} impact={`${profile.name} · 선택한 파일`} risk="destructive" confirmName={profile.name} disabled={profile.access_mode==='read_only'||selected.applied} previewKey={selected.text} onPreview={async()=>setPreview(await api('migration_preview',{sessionId,runId:crypto.randomUUID(),text:selected.text}))} onApply={async()=>{await api('migration_apply',{sessionId,runId:crypto.randomUUID(),text:selected.text,confirmation:profile.name,migrationId:selected.id});await scan();}}><pre class="ax-code">{selected.text}</pre></PlanCard>}</div>}{list?.warnings.map(w=><p class="ax-note">{w}</p>)}{preview&&<ChangePreviewView preview={preview}/ >}{error&&<p class="ax-error" role="alert">{error}</p>}</div>;
}
