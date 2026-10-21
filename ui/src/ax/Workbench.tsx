import { useEffect, useId, useMemo, useState } from 'preact/hooks';
import { Database, ShieldCheck } from 'lucide-preact';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';
import { radioKey } from '../ui/JellyRadio';
import { Dialog } from '../Dialog';
import type { DriverManifest, Profile, ExplorerNode } from '../contract';
import { AiAccounts } from './AiAccounts';
import { ImportWizard } from './ImportWizard';
import { SchemaDiffPanel } from './SchemaDiffPanel';
import { Migrations } from './Migrations';
import { BackupRestore } from './BackupRoles';
import { RolesMatrix } from './RolesMatrix';
import { IntentPlans } from './IntentPlans';
import './ax.css';
import './workbench.css';
import { operationLabels, supportsOperation, type AxPage } from './operations';
const descriptions:Record<AxPage,string>={import:'파일의 데이터를 검토하고 테이블로 가져옵니다.',schema_diff:'두 연결의 스키마 차이를 비교합니다.',migrate:'스키마 변경 계획을 만들고 실행 이력을 확인합니다.',backup:'데이터베이스를 파일로 백업하거나 검토한 백업을 복원합니다.',grant:'사용자와 역할의 접근 권한을 조회하고 변경합니다.',accounts:'AI 서비스 계정을 연결하고 사용할 모델을 설정합니다.',intent:'원하는 작업을 설명하고 실행 계획을 검토합니다.'};
export function Workbench({importDraft,page,onPage,onClose,profiles,sessions,drivers,initialProfile,nodes,intent='',onInsert}:{importDraft?:{profileId:string;nodeId:string;file:File};page:AxPage;onPage:(page:AxPage)=>void;onClose:()=>void;profiles:Profile[];sessions:Record<string,string>;drivers:DriverManifest[];initialProfile?:string;nodes:Record<string,ExplorerNode[]>;intent?:string;onInsert:(profile:Profile,text:string)=>void}) {
  const [selected,setSelected]=useState(initialProfile??profiles.find(profile=>sessions[profile.id])?.id??'');
  const [busy,setBusy]=useState(false);
  const [pending,setPending]=useState(false),[navigation,setNavigation]=useState<(()=>void)|null>(null);
  const id=useId();
  const connectionOptions=useMemo(()=>profiles.filter(profile=>sessions[profile.id]).map(profile=>({value:profile.id,label:profile.name,tag:drivers.find(driver=>driver.id===profile.driver_id)?.display_name})),[profiles,sessions,drivers]);
  const diffProfiles=useMemo(()=>profiles.filter(profile=>{const cap=drivers.find(driver=>driver.id===profile.driver_id)?.capabilities;return cap?.schema_snapshot&&cap.ddl_render;}),[profiles,drivers]);
  useEffect(()=>{if(importDraft)setSelected(importDraft.profileId);},[importDraft]);
  const profile=profiles.find(profile=>profile.id===selected),manifest=drivers.find(driver=>driver.id===profile?.driver_id),sessionId=sessions[selected];
  const supported=(key:AxPage)=>supportsOperation(key,manifest?.capabilities);
  const pages=(Object.keys(operationLabels) as AxPage[]).filter(supported);
  const navigate=(action:()=>void)=>{if(busy)return;if(pending)setNavigation(()=>action);else action();};
  return <section class="ax-workbench" aria-label="AX 워크벤치">
    <header class="ax-heading"><div><span class="workbench-eyebrow">데이터베이스 관리</span><h1>AX 워크벤치</h1><p>{descriptions[page]}</p></div><Button morph={false} onClick={()=>navigate(onClose)} aria-label="워크벤치 닫기" disabled={busy}>닫기</Button></header>
    <nav class="ax-tabs workbench-tabs" role="tablist" aria-label="워크벤치 기능">{pages.map((key,index)=><button key={key} type="button" role="tab" disabled={busy} id={`${id}-${key}`} aria-selected={page===key} aria-controls={`${id}-panel`} tabIndex={page===key?0:-1} onClick={()=>navigate(()=>onPage(key))} onKeyDown={event=>{const next=radioKey(event.key,index,pages.map(()=>({})));if(next===undefined)return;event.preventDefault();navigate(()=>{onPage(pages[next]);document.getElementById(`${id}-${pages[next]}`)?.focus();});}}>{operationLabels[key]}</button>)}</nav>
    <div class="ax-content" role="tabpanel" id={`${id}-panel`} aria-labelledby={pages.includes(page)?`${id}-${page}`:undefined}>
      {page!=='accounts'&&<div class="workbench-connection"><label class="ax-field wb-field"><span class="wb-field__label"><Database size={14}/>작업 연결</span><Select ariaLabel="워크벤치 연결" disabled={busy} placeholder="연결 선택…" emptyText="활성 연결이 없습니다" value={selected} options={connectionOptions} onChange={value=>navigate(()=>setSelected(value))}/></label><div class="workbench-connection-status">{profile&&sessionId?<><span class="status-dot connected"/><span>{manifest?.display_name}</span>{profile.access_mode==='read_only'&&<span class="read-only-badge"><ShieldCheck size={13}/>읽기 전용</span>}</>:<span>활성 연결이 필요합니다.</span>}</div></div>}
      {page==='accounts'?<AiAccounts/>:!profile||!sessionId?<div class="workbench-empty"><Database size={30}/><h2>데이터베이스를 먼저 연결하세요</h2><p>작업공간의 연결 목록에서 DB에 연결하면 관리 작업을 시작할 수 있습니다.<br/>AI 계정은 연결 없이 설정할 수 있습니다.</p><Button morph={false} onClick={onClose}>작업공간으로 돌아가기</Button></div>:!supported(page)?<div class="workbench-empty"><h2>이 연결은 {operationLabels[page]} 기능을 지원하지 않습니다</h2><p>다른 연결이나 지원되는 관리 작업을 선택하세요.</p></div>:<div key={`${selected}:${page}`}>
        {page==='import'?<ImportWizard initialFile={importDraft?.profileId===selected?importDraft.file:undefined} initialTarget={importDraft?.profileId===selected?importDraft.nodeId:undefined} nodes={nodes[selected]??[]} sessionId={sessionId} readOnly={profile.access_mode==='read_only'}/>:page==='schema_diff'?<SchemaDiffPanel profiles={diffProfiles} sessions={sessions} selected={selected}/>:page==='migrate'?<Migrations sessionId={sessionId} profile={profile}/>:page==='backup'?<BackupRestore name={profile.name} sessionId={sessionId} readOnly={profile.access_mode==='read_only'}/>:page==='grant'?<RolesMatrix sessionId={sessionId} readOnly={profile.access_mode==='read_only'} onPendingChange={setPending} onBusyChange={setBusy}/>:<IntentPlans key={intent} sessionId={sessionId} initialIntent={intent} onAccounts={()=>onPage('accounts')} onInsert={text=>onInsert(profile,text)}/>}
      </div>}
    </div>
    {navigation&&<Dialog label="권한 변경 계획 유지" title="작성한 권한 변경 계획을 버릴까요?" onClose={()=>setNavigation(null)} footer={<><Button morph={false} onClick={()=>setNavigation(null)}>계속 검토</Button><Button morph={false} variant="destructive" onClick={()=>{const next=navigation;setNavigation(null);setPending(false);next();}}>계획 버리고 이동</Button></>}><p>아직 적용하지 않은 변경 계획이 있습니다. 데이터베이스에는 반영되지 않았습니다.</p></Dialog>}
  </section>;
}
