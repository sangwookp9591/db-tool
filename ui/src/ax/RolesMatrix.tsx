import { useEffect, useRef, useState } from 'preact/hooks';
import { RefreshCw, ShieldCheck, Users, X } from 'lucide-preact';
import { api, isMock } from '../api';
import { Button } from '../ui/Button';
import { GlideSelect } from '../ui/GlideSelect';
import { ErrorBanner } from '../ErrorBanner';
import { normalizeError } from '../values';
import type { GrantChange, GrantPlan, RolesPageQuery, RolesSnapshot } from './administrationContract';
import './roles.css';

const keyOf = (cell: Omit<GrantChange, 'revoke'>) => JSON.stringify([cell.role, cell.namespace, cell.object, cell.privilege]);
const emptyDraft = {role:'',namespace:'',object:'',privilege:'',revoke:false};
export function RolesMatrix({sessionId,readOnly=false,onPendingChange,onBusyChange}:{sessionId:string;readOnly?:boolean;onPendingChange?:(pending:boolean)=>void;onBusyChange?:(busy:boolean)=>void}) {
  const [query,setQuery]=useState<RolesPageQuery>({offset:0,limit:50});
  const [namespace,setNamespace]=useState(''),[object,setObject]=useState(''),[roleSearch,setRoleSearch]=useState('');
  const [snapshot,setSnapshot]=useState<RolesSnapshot|null>(null),[loading,setLoading]=useState(true),[retry,setRetry]=useState(0);
  const [changes,setChanges]=useState<Record<string,GrantChange>>({}),[plan,setPlan]=useState<GrantPlan|null>(null);
  const [approved,setApproved]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState<ReturnType<typeof normalizeError>|null>(null),[notice,setNotice]=useState('');
  const [draft,setDraft]=useState<GrantChange>(emptyDraft);
  const alive=useRef(true);
  useEffect(()=>{alive.current=true;return()=>{alive.current=false;};},[]);
  useEffect(()=>{
    let current=true;setLoading(true);setError(null);
    void api('roles_page',{sessionId,query}).then(value=>{if(current)setSnapshot(value);}).catch(cause=>{if(current)setError(normalizeError(cause));}).finally(()=>{if(current)setLoading(false);});
    return()=>{current=false;};
  },[sessionId,query,retry]);
  const pending=Object.values(changes),role=snapshot?.roles.find(item=>item.name===query.role);
  const page=snapshot?.page;
  const matches=!!page&&(page.role??null)===(query.role??null)&&(page.namespace??null)===(query.namespace??null)&&(page.object??null)===(query.object??null)&&page.offset===query.offset&&page.limit===query.limit;
  const cells=matches?snapshot?.matrix??[]:[];
  const fetching=loading||(snapshot?.supported===true&&!matches&&!error);
  useEffect(()=>{onBusyChange?.(busy);return()=>onBusyChange?.(false);},[busy,onBusyChange]);
  useEffect(()=>{onPendingChange?.(pending.length>0);return()=>onPendingChange?.(false);},[pending.length,onPendingChange]);
  const update=(next:Record<string,GrantChange>)=>{setChanges(next);setPlan(null);setApproved(false);setNotice('');};
  const remove=(key:string)=>{const next={...changes};delete next[key];update(next);};
  const chooseRole=(name:string)=>{setQuery(previous=>({...previous,role:name||null,offset:0}));setDraft(previous=>({...previous,role:name}));};
  async function work(action:()=>Promise<void>){setBusy(true);setError(null);setNotice('');try{await action();}catch(cause){if(alive.current)setError(normalizeError(cause));}finally{if(alive.current)setBusy(false);}}
  const preview=()=>work(async()=>{const planned=await api('plan_grants',{sessionId,changes:pending});const reviewed=planned.apply_supported?await api('grants_preview',{sessionId,changes:pending}):planned;if(alive.current){setPlan(reviewed);setApproved(false);}});
  const apply=()=>work(async()=>{await api('grants_apply',{sessionId,runId:crypto.randomUUID(),changes:pending,approved});if(alive.current){setChanges({});setPlan(null);setApproved(false);setNotice(`${pending.length}개 권한 변경을 적용했습니다.`);setRetry(value=>value+1);}});
  const roles=(snapshot?.roles??[]).filter(item=>item.name.toLocaleLowerCase().includes(roleSearch.toLocaleLowerCase()));
  return <section class="roles-page" aria-label="사용자와 권한 관리" aria-busy={loading||busy}>
    <div class="roles-heading"><div><h2><Users size={19}/>사용자·권한</h2><p>역할별 접근 권한을 확인하고, 변경 내용을 검토한 뒤 적용하세요.</p></div><Button morph={false} disabled={busy||fetching} aria-label="권한 새로고침" onClick={()=>setRetry(value=>value+1)}><RefreshCw size={15}/>새로고침</Button></div>
    {isMock&&<p class="roles-notice">예시 데이터입니다. 실제 데이터베이스의 권한을 변경하지 않습니다.</p>}
    {readOnly&&<p class="roles-notice"><ShieldCheck size={15}/>읽기 전용 연결입니다. 권한 조회만 사용할 수 있습니다.</p>}
    <ErrorBanner error={error}/>
    {error&&<Button morph={false} disabled={fetching||busy} onClick={()=>setRetry(value=>value+1)}>권한 다시 조회</Button>}
    {notice&&<p class="roles-notice" role="status">{notice}</p>}
    {snapshot&&!snapshot.supported?<div class="roles-empty"><h3>권한 조회를 지원하지 않는 연결입니다</h3><p>{snapshot.reason}</p></div>:<>
    <div class="roles-layout">
      <aside class="roles-directory" aria-label="사용자·역할 목록"><div class="roles-section-title"><h3>사용자·역할</h3><span>{snapshot?.roles.length??0}</span></div>
        <input aria-label="사용자·역할 검색" placeholder="역할 이름 검색" value={roleSearch} onInput={event=>setRoleSearch(event.currentTarget.value)}/>
        <div class="roles-list">{roles.map(item=><button key={item.name} type="button" class="roles-list-item" disabled={busy} aria-pressed={query.role===item.name} onClick={()=>chooseRole(item.name)}><span title={item.name}>{item.name}</span><small>{item.superuser?'관리자':item.login?'로그인 가능':'그룹 역할'}</small></button>)}{!loading&&!roles.length&&<p class="muted">일치하는 역할이 없습니다.</p>}{loading&&!snapshot&&<p role="status">역할을 불러오는 중…</p>}</div>
      </aside>
      <div class="roles-detail">
        <div class="roles-section-title"><div><h3>{role?.name??'역할을 선택하세요'}</h3>{role&&<p>{role.superuser?'관리자 역할':role.login?'로그인 가능한 사용자':'로그인하지 않는 그룹 역할'}{role.member_of.length?` · 소속: ${role.member_of.join(', ')}`:''}</p>}</div>{role&&<span class="roles-count">{matches?page?.total?.toLocaleString()??cells.length:'…'}개 권한</span>}</div>
        <form class="roles-filters" onSubmit={event=>{event.preventDefault();setQuery(previous=>({...previous,namespace:namespace.trim()||null,object:object.trim()||null,offset:0}));}}>
          <label class="ax-field">스키마·DB<input aria-label="권한 스키마 필터" placeholder="전체 스키마" value={namespace} onInput={event=>setNamespace(event.currentTarget.value)}/></label>
          <label class="ax-field">객체 이름<input aria-label="권한 객체 필터" placeholder="테이블·뷰 이름 검색" value={object} onInput={event=>setObject(event.currentTarget.value)}/></label>
          <Button morph={false} type="submit" disabled={!query.role||fetching||busy}>조회</Button>
        </form>
        {!query.role?<div class="roles-empty"><ShieldCheck size={28}/><h3>조회할 사용자 또는 역할을 선택하세요</h3><p>선택한 역할의 권한만 나누어 불러옵니다.<br/>테이블이 많은 DB는 스키마와 객체 이름으로 범위를 좁힐 수 있습니다.</p></div>:<>
          <div class="roles-table-wrap"><table class="roles-table"><caption class="sr-only">{query.role}의 객체별 유효 권한</caption><thead><tr><th>객체</th><th>권한</th><th>현재 상태</th><th>변경 계획</th></tr></thead><tbody>{cells.map(cell=>{const key=keyOf(cell),change=changes[key];return <tr key={key} data-pending={!!change||undefined}><td><strong title={`${cell.namespace}.${cell.object}`}>{cell.object||'—'}</strong><small>{cell.namespace}</small></td><td><code>{cell.privilege}</code></td><td><span class="roles-permission" data-allowed={cell.allowed}>{cell.allowed?'허용':'없음'}</span></td><td><Button morph={false} disabled={busy||fetching||readOnly||!!error} aria-label={`${cell.role} ${cell.namespace}.${cell.object} ${cell.privilege} ${change?'변경 취소':cell.allowed?'회수 계획':'부여 계획'}`} aria-pressed={!!change} onClick={()=>change?remove(key):update({...changes,[key]:{role:cell.role,namespace:cell.namespace,object:cell.object,privilege:cell.privilege,revoke:cell.allowed}})}>{change?`${change.revoke?'회수':'부여'} 예정 · 취소`:cell.allowed?'회수':'부여'}</Button></td></tr>;})}</tbody></table>{fetching&&<p class="roles-loading" role="status">권한을 불러오는 중…</p>}{!fetching&&!error&&!cells.length&&<p class="roles-empty">조건에 맞는 권한이 없습니다. 조회 범위를 넓혀 보세요.</p>}</div>
          <div class="roles-pagination"><span>{cells.length?`${query.offset+1}–${query.offset+cells.length}`:'0'} / {matches?page?.total?.toLocaleString()??0:'…'}</span><label>표시 수 <GlideSelect ariaLabel="권한 페이지 크기" value={String(query.limit)} disabled={busy||fetching} options={['25','50','100']} onChange={value=>setQuery(previous=>({...previous,limit:Number(value),offset:0}))}/></label><Button morph={false} disabled={busy||fetching||query.offset===0} onClick={()=>setQuery(previous=>({...previous,offset:Math.max(0,previous.offset-previous.limit)}))}>이전</Button><Button morph={false} disabled={busy||fetching||!page?.has_more} onClick={()=>setQuery(previous=>({...previous,offset:previous.offset+previous.limit}))}>다음</Button></div>
          <p class="roles-help">표는 역할 상속과 공용 권한을 포함한 유효 권한입니다. 직접 부여한 권한을 회수해도 상속된 권한은 유지될 수 있습니다.</p>
        </>}
      </div>
    </div>
    {!readOnly&&<details class="roles-manual"><summary>권한 변경 직접 입력</summary><p>표에 없는 권한을 계획에 추가합니다. MongoDB는 권한 ROLE과 대상 역할·DB를, Redis는 스키마 command와 @read 같은 권한을 입력하세요.</p><form onSubmit={event=>{event.preventDefault();update({...changes,[keyOf(draft)]:{...draft}});}}><fieldset disabled={busy}><div class="roles-manual-fields">{(['role','namespace','object','privilege'] as const).map(key=><label class="ax-field" key={key}>{({role:'사용자·역할',namespace:'스키마·DB·유형',object:'객체·부여할 역할',privilege:'권한'})[key]}<input required={key==='role'||key==='privilege'} value={draft[key]} onInput={event=>setDraft(previous=>({...previous,[key]:event.currentTarget.value}))}/></label>)}</div><label><input type="checkbox" checked={draft.revoke} onChange={event=>setDraft(previous=>({...previous,revoke:event.currentTarget.checked}))}/>권한 회수</label><Button morph={false} type="submit" disabled={!draft.role.trim()||!draft.privilege.trim()}>변경 계획에 추가</Button></fieldset></form></details>}
    <section class="roles-review" aria-label="대기 중인 권한 변경"><div class="roles-section-title"><h3>변경 계획 <span>{pending.length}</span></h3>{pending.length>0&&<Button morph={false} disabled={busy} onClick={()=>update({})}>모두 취소</Button>}</div>{!pending.length?<p>부여·회수를 선택하면 여기에 쌓입니다. 미리보기와 승인 후에만 DB에 반영됩니다.</p>:<><ul>{pending.map(change=><li key={keyOf(change)}><span class="roles-change" data-revoke={change.revoke}>{change.revoke?'회수':'부여'}</span><span><strong>{change.role}</strong> · {change.namespace}.{change.object} · <code>{change.privilege}</code></span><Button bare disabled={busy} aria-label={`${change.role} ${change.privilege} 계획 제거`} onClick={()=>remove(keyOf(change))}><X size={15}/></Button></li>)}</ul><Button morph={false} variant="brand" disabled={busy||readOnly} onClick={preview}>{busy?'처리 중…':'권한 변경 미리보기'}</Button></>}
      {plan&&<div class="roles-plan"><h3>적용 전 최종 검토</h3><pre>{plan.statements.join('\n')}</pre>{plan.warnings.map(warning=><p key={warning}>{warning}</p>)}{plan.apply_supported?<><label><input type="checkbox" checked={approved} disabled={busy||readOnly} onChange={event=>setApproved(event.currentTarget.checked)}/>위 {pending.length}개 권한 변경을 확인했습니다.</label><Button morph={false} variant="brand" disabled={busy||readOnly||!approved} onClick={apply}>승인한 권한 변경 적용</Button></>:<p>이 연결은 변경 계획만 제공합니다. SQL을 검토한 후 지원되는 DB 관리 도구에서 적용하세요.</p>}</div>}
    </section></>}
  </section>;
}
