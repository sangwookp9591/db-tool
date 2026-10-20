import { ThinkingLoader } from '../ui/ThinkingLoader';
import { useState } from 'preact/hooks';
import { Sparkles, ArrowRight } from 'lucide-preact';
import { api } from '../api';
import { Button } from '../ui/Button';
import { PlanCard } from './PlanCard';
import { useAiReady } from './useAiReady';
import { operationLabels } from './operations';
import type { Operation } from './aiContract';
const titles = {...operationLabels, explain:'실행 계획 분석', analyze:'실제 실행 분석', refactor:'쿼리 리팩터링', restore:'복원', connect_test:'연결 진단'};
export function IntentPlans({sessionId,initialIntent='',onAccounts,onInsert,purpose='operation',details}:{sessionId:string;initialIntent?:string;onAccounts:()=>void;onInsert?:(text:string)=>void;purpose?:'operation'|'explain'|'recommend_indexes';details?:Record<string,unknown>}){
 const [intent,setIntent]=useState(initialIntent),[operations,setOperations]=useState<Operation[]>([]),[busy,setBusy]=useState(false),[error,setError]=useState(''),[text,setText]=useState('');
 const ready=useAiReady();
 async function propose(){setBusy(true);setError('');setOperations([]);setText('');try{const result=await api('ai_operation',{sessionId,intent,purpose,details});setOperations(result.operations);setText(result.text);}catch(e){setError(String((e as Error).message??e));}finally{setBusy(false);}}
 return <div class="ax-intent" data-has-plan={operations.length>0}>
  <div class="ax-intent-compose ax-stack"><label class="ax-field">어떤 작업을 하고 싶으신가요?<textarea aria-label="AI 작업 의도" value={intent} placeholder="예: analyst 역할에 public 스키마 읽기 권한을 제안해 줘" onInput={e=>setIntent(e.currentTarget.value)}/></label><div class="ax-inline"><Button morph={false} variant="brand" disabled={busy||(ready&&!intent.trim())} onClick={()=>ready?void propose():onAccounts()}><Sparkles aria-hidden="true"/>{ready?'AI 계획 후보 만들기':'로그인 필요'}</Button><small>AI는 직접 실행하지 않습니다.</small></div>{busy&&<ThinkingLoader phase={0}/>}{text&&<p class="ax-intent-context">{text}</p>}{error&&<p class="ax-error" role="alert">{error}</p>}</div>
  {operations.length>0&&<div class="ax-stack ax-intent-results">{operations.map(op=><PlanCard key={op.id} title={titles[op.kind]} evidence={op.plan.evidence} impact={`${op.plan.statements.length}개 제안 문장 · 행·객체 영향은 미리보기에서 확인`} risk={op.risk} proposal><p class="ax-plan-reason">{op.plan.reason}</p><pre class="ax-code">{op.plan.statements.join('\n')}</pre>{onInsert&&op.plan.statements.length>0&&<Button class="ax-proposal-review" morph={false} variant="brand" onClick={()=>onInsert(op.plan.statements.join(';\n'))}>쿼리 탭에서 검토<ArrowRight aria-hidden="true"/></Button>}</PlanCard>)}</div>}
 </div>;
}
