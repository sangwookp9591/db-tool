import { useEffect, useRef, useState } from 'preact/hooks';
import { AlertTriangle, ChevronRight, GitBranch, Sparkles } from 'lucide-preact';
import { api, isMock } from '../api';
import { Button } from '../ui/Button';
import type { PlanTree } from './editorContract';
import { useAiReady } from './useAiReady';
import { planTarget } from './planTarget';
import { IntentPlans } from './IntentPlans';

const number = (value: number | null) => value === null ? '—' : value.toLocaleString('ko-KR', { maximumFractionDigits: 2 });
function warnings(node: PlanTree) {
 const {actual_rows: actual, est_rows: estimated} = node;
 const mismatch = actual !== null && estimated !== null && Math.max(actual, estimated) > 0 && Math.max(actual, estimated) >= 10 * Math.min(actual, estimated);
 return [...node.warnings, ...(mismatch ? ['예상과 실제 행 수가 10배 이상 차이 납니다. 통계와 조건을 검토하세요.'] : [])];
}
function flatten(node: PlanTree): PlanTree[] { return [node, ...node.children.flatMap(flatten)]; }
export function QueryPlan({sessionId,query,onTarget,onInsert,canRecommendIndexes}:{sessionId:string;query:string;canRecommendIndexes?:boolean;onTarget?:(target:string)=>void;onInsert:(text:string)=>void}){
 const [plan,setPlan]=useState<PlanTree|null>(null),[selected,setSelected]=useState<PlanTree|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(''),[ai,setAi]=useState<'explain'|'recommend_indexes'|null>(null);
 const request=useRef<string|null>(null);
 useEffect(()=>{setPlan(null);setSelected(null);setAi(null);setError('');setBusy(false);return()=>{const runId=request.current;request.current=null;if(runId)void api('cancel',{runId}).catch(()=>{});};},[sessionId,query]);
 async function explain(analyze:boolean){
  if(request.current)return;
  const runId=crypto.randomUUID();request.current=runId;
  setBusy(true);setError('');setPlan(null);setSelected(null);setAi(null);
  try{const result=await api('explain_query',{sessionId,runId,query,analyze});if(request.current===runId){setPlan(result);setSelected(result);}}
  catch(e){if(request.current===runId)setError(String((e as Error).message??e));}
  finally{if(request.current===runId){request.current=null;setBusy(false);}}
 }
 function cancel(){const runId=request.current;if(runId)void api('cancel',{runId}).catch(e=>{if(request.current===runId)setError(String((e as Error).message??e));});}
 const ready=useAiReady();
 async function navigate(target:string){setBusy(true);setError('');try{const node=await planTarget(target,parent=>api('explorer_children',{sessionId,parent}));onTarget?.(node.id);}catch(e){setError(String((e as Error).message??e));}finally{setBusy(false);}}
 const nodes=plan?flatten(plan):[], costed=nodes.filter(item=>item.cost!==null);
 const expensive=costed.reduce<PlanTree|null>((best,item)=>!best||item.cost!>best.cost!?item:best,null);
 const max=expensive?.cost??0, warningCount=nodes.reduce((count,item)=>count+warnings(item).length,0);
 function node(item:PlanTree,index:number,hot:boolean){
  const issues=warnings(item), largestChild=Math.max(...item.children.map(child=>child.cost??0),0);
  return <li key={index} data-hot={hot}><Button bare class="ax-plan-node" aria-pressed={selected===item} onClick={()=>setSelected(item)}>
   <span class="ax-plan-node-title"><GitBranch aria-hidden="true"/><span><strong>{item.op}</strong><small>{item.target??'중간 결과'}</small></span>{issues.length>0&&<AlertTriangle class="ax-warning-icon" aria-label={`${issues.length}개 경고`}/>}</span>
   <span class="ax-plan-cost"><span class="ax-cost-track" role="progressbar" aria-label={`${item.op} 비용`} aria-valuemin={0} aria-valuemax={Math.max(max,1)} aria-valuenow={item.cost??0} aria-valuetext={item.cost===null?'비용 정보 없음':number(item.cost)}><span style={{width:`${max>0?Math.max(0,(item.cost??0)/max)*100:0}%`}}/></span><span>{number(item.cost)}</span></span>
   <span class="ax-row-chips"><span>예상 <b>{number(item.est_rows)}</b></span><span>실제 <b>{number(item.actual_rows)}</b></span></span>
  </Button>{!!item.children.length&&<ul class="ax-tree">{item.children.map((child,i)=>node(child,i,hot&&largestChild>0&&child.cost===largestChild))}</ul>}</li>;
 }
 return <section class="ax-query-plan"><div class="ax-inline"><Button morph={false} disabled={busy||!query.trim()} onClick={()=>void explain(false)}>실행 계획 읽기</Button><Button morph={false} disabled={busy||!query.trim()} onClick={()=>void explain(true)}>실제 분석 · 롤백</Button>{busy&&request.current&&<Button morph={false} onClick={cancel}>계획 읽기 취소</Button>}<small>ANALYZE는 호스트의 롤백 경로에서만 실행합니다.</small></div>{busy&&<p role="status">계획을 읽고 있습니다…</p>}{plan?<>
  <div class="ax-plan-summary" aria-label="실행 계획 요약"><div><small>총비용 · 루트 추정</small><strong>{number(plan.cost)}</strong></div><div><small>가장 비싼 노드</small><strong>{expensive?.op??'비용 정보 없음'}</strong></div><div data-warning={warningCount>0}><small>검토할 경고</small><strong>{warningCount}<span>개</span></strong></div></div>
  <div class="ax-plan-layout"><div class="ax-plan-tree"><div class="ax-plan-tree-heading"><span>실행 경로 <small>{nodes.length}개 노드</small></span><small>비용은 최대 노드 대비 · 강조선은 고비용 경로</small></div><ul class="ax-tree">{node(plan,0,max>0)}</ul></div>
   {selected&&<aside class="ax-plan-detail" aria-label="선택한 노드 상세"><small>노드 상세</small><h3>{selected.op}</h3><p>{selected.target??'중간 결과'}</p><dl><div><dt>추정 비용</dt><dd>{number(selected.cost)}</dd></div><div><dt>예상 행 수</dt><dd>{number(selected.est_rows)}</dd></div><div><dt>실제 행 수</dt><dd>{number(selected.actual_rows)}</dd></div></dl>{warnings(selected).map(w=><p key={w} class="ax-node-warning"><AlertTriangle aria-hidden="true"/>{w}</p>)}{selected.target&&<Button morph={false} disabled={!onTarget||busy} onClick={()=>void navigate(selected.target!)}>탐색기에서 보기<ChevronRight aria-hidden="true"/></Button>}</aside>}
  </div>{isMock&&<p class="ax-mock-note">모의 실행 계획 · 실제 데이터베이스 쿼리를 실행하지 않았습니다.</p>}
  <div class="ax-inline ax-plan-actions"><Button morph={false} onClick={()=>ready?setAi('explain'):window.dispatchEvent(new CustomEvent('ax:accounts'))}><Sparkles aria-hidden="true"/>{ready?'AI 계획 해설':'로그인 필요 · AI 계획 해설'}</Button>{canRecommendIndexes&&<Button morph={false} onClick={()=>ready?setAi('recommend_indexes'):window.dispatchEvent(new CustomEvent('ax:accounts'))}>{ready?'AI 인덱스 제안':'로그인 필요 · AI 인덱스 제안'}</Button>}</div>{ai&&<IntentPlans key={ai} sessionId={sessionId} purpose={ai} initialIntent={ai==='explain'?'이 실행 계획이 느린 이유를 세 문장으로 설명해 줘':'이 계획을 개선할 인덱스를 근거와 함께 제안해 줘'} details={plan as unknown as Record<string,unknown>} onAccounts={()=>window.dispatchEvent(new CustomEvent('ax:accounts'))} onInsert={onInsert}/>}</>:!busy&&<p class="ax-note">쿼리를 실행하지 않고 예상 비용과 접근 경로를 검토하세요.</p>}{error&&<p class="ax-error" role="alert">{error}</p>}</section>;
}
