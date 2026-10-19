import { useEffect, useState } from 'preact/hooks';
import { Button } from '../ui/Button';
import { JellyRadio } from '../ui/JellyRadio';
import type { DriverManifest, Profile, ExplorerNode } from '../contract';
import { AiAccounts } from './AiAccounts';
import { ImportWizard } from './ImportWizard';
import { SchemaDiffPanel } from './SchemaDiffPanel';
import { Migrations } from './Migrations';
import { BackupRestore, RolesMatrix } from './BackupRoles';
import { IntentPlans } from './IntentPlans';
import './ax.css';
import { operationLabels, supportsOperation, type AxPage } from './operations';
export function Workbench({importDraft,page,onPage,onClose,profiles,sessions,drivers,initialProfile,nodes,intent='',onInsert}:{importDraft?:{profileId:string;nodeId:string;file:File};page:AxPage;onPage:(page:AxPage)=>void;onClose:()=>void;profiles:Profile[];sessions:Record<string,string>;drivers:DriverManifest[];initialProfile?:string;nodes:Record<string,ExplorerNode[]>;intent?:string;onInsert:(profile:Profile,text:string)=>void}){
 const [selected,setSelected]=useState(initialProfile??profiles.find(p=>sessions[p.id])?.id??'');
 useEffect(()=>{if(importDraft)setSelected(importDraft.profileId);},[importDraft]);
 const profile=profiles.find(p=>p.id===selected),manifest=drivers.find(d=>d.id===profile?.driver_id),sessionId=sessions[selected];
 const cap=manifest?.capabilities;
 const supported=(key:AxPage)=>supportsOperation(key,cap);
 return <section class="ax-workbench" aria-label="AX 워크벤치"><header class="ax-heading"><div><h1>AX 워크벤치</h1><p>의도를 계획으로, 검토한 계획을 실행으로.</p></div><Button morph={false} onClick={onClose} aria-label="워크벤치 닫기">닫기</Button></header><nav class="ax-tabs"><JellyRadio segmented role="tablist" aria-label="워크벤치 기능" value={page} onChange={value=>onPage(value as AxPage)} items={(Object.keys(operationLabels) as AxPage[]).filter(supported).map(key=>({value:key,label:operationLabels[key]}))}/></nav><div class="ax-content">{page!=='accounts'&&<label class="ax-field">작업 연결<select aria-label="워크벤치 연결" value={selected} onChange={e=>setSelected(e.currentTarget.value)}><option value="">연결 선택…</option>{profiles.filter(p=>sessions[p.id]).map(p=><option value={p.id}>{p.name}</option>)}</select></label>}{page==='accounts'?<AiAccounts/>:!profile||!sessionId?<p class="ax-note">데이터베이스에 연결한 후 작업을 선택하세요. AI 계정은 연결 없이 설정할 수 있습니다.</p>:!supported(page)?<p class="ax-note">이 드라이버가 해당 능력을 제공하지 않습니다. 설치된 드라이버의 capabilities를 확인하세요.</p>:<div key={`${selected}:${page}`}>{page==='import'?<ImportWizard initialFile={importDraft?.profileId===selected?importDraft.file:undefined} initialTarget={importDraft?.profileId===selected?importDraft.nodeId:undefined} nodes={nodes[selected]??[]} sessionId={sessionId} readOnly={profile.access_mode==='read_only'}/>:page==='schema_diff'?<SchemaDiffPanel profiles={profiles.filter(p=>{const c=drivers.find(d=>d.id===p.driver_id)?.capabilities;return c?.schema_snapshot&&c.ddl_render;})} sessions={sessions} selected={selected}/>:page==='migrate'?<Migrations sessionId={sessionId} profile={profile}/>:page==='backup'?<BackupRestore name={profile.name} sessionId={sessionId} readOnly={profile.access_mode==='read_only'}/>:page==='grant'?<RolesMatrix sessionId={sessionId} readOnly={profile.access_mode==='read_only'}/>:<IntentPlans key={intent} sessionId={sessionId} initialIntent={intent} onAccounts={()=>onPage('accounts')} onInsert={text=>onInsert(profile,text)}/>}</div>}</div></section>;
}
