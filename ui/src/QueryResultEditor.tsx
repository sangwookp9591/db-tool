import type { ComponentChildren } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import type { ChangePreview, ResultSet } from './contract';
import { api } from './api';
import { buildEditRequest, emptyEdits, pendingCount } from './edits';
import type { EditControls } from './Results';
import { Button } from './ui/Button';
import { ChangePreviewView } from './ChangePreviewView';
import { Dialog } from './Dialog';

export interface QueryEditContext { readOnly: boolean; supported: boolean; busy: boolean; onPending: (pending:boolean) => void; sessionId: string; onRun: (id: string | null) => void; refresh: () => Promise<void>; onError: (error: unknown) => void }
export function QueryResultEditor({ result, context, children }: { result: ResultSet; context: QueryEditContext; children: (edit: EditControls) => ComponentChildren }) {
  const [editing,setEditing]=useState(false), [edits,setEdits]=useState(emptyEdits), [preview,setPreview]=useState<ChangePreview|null>(null), [fingerprint,setFingerprint]=useState(''), [confirm,setConfirm]=useState(false), [busy,setBusy]=useState(false);
  const mounted=useRef(true);
  const pending=pendingCount(edits)>0;
  useEffect(()=>{context.onPending(pending);return()=>context.onPending(false);},[pending,context.onPending]);
  useEffect(()=>()=>{mounted.current=false;},[]);
  useEffect(()=>{setEditing(false);setEdits(emptyEdits());setPreview(null);setConfirm(false);},[result.table?.columns,context.readOnly]);
  const reason=!context.supported?'이 드라이버는 결과 편집을 지원하지 않습니다':context.readOnly?'읽기 전용 연결 또는 권한 모드':context.busy?'실행이 끝난 후 편집할 수 있습니다':(result.table?.rows.length??0)>1000?'쿼리 결과 편집은 1,000행까지 지원합니다':result.truncated?'행 상한을 초과한 결과는 편집할 수 없습니다':result.edit_disabled_reason??(!result.edit_target?'편집 가능한 원본 테이블과 기본 키가 필요합니다':undefined);
  const enabled=!reason && !!result.edit_target && !!result.table;
  const request=enabled?buildEditRequest(result.edit_target!,result.table!,edits):null;
  const current=request?JSON.stringify(request):'';
  const valid=!!preview && current===fingerprint;
  async function save(apply: boolean) {
    if (!request?.edits.length || busy || (apply&&!valid)) return;
    const id=crypto.randomUUID();setBusy(true);context.onRun(id);
    try {
      if (apply) {
        await api('edit_apply',{sessionId:context.sessionId,runId:id,req:request});
        if(mounted.current){setEdits(emptyEdits());setPreview(null);setConfirm(false);setEditing(false);}
        await context.refresh();
      } else {
        const received=await api('edit_preview',{sessionId:context.sessionId,runId:id,req:request});
        if(mounted.current){setPreview(received);setFingerprint(current);}
      }
    } catch(error){context.onError(error);}
    finally {if(mounted.current)setBusy(false);context.onRun(null);}
  }
  return <>{preview?<><div class="edit-actions"><Button morph={false} disabled={busy} onClick={()=>setPreview(null)}>표로 돌아가기</Button><Button morph={false} disabled={!valid||busy||!enabled} onClick={()=>setConfirm(true)}>적용</Button></div><ChangePreviewView preview={preview}/></>:children({enabled:enabled&&!busy,reason:busy?'변경 처리 중입니다':reason,editing:editing&&enabled,onToggle:()=>setEditing(v=>!v),edits,onEditsChange:next=>{setEdits(next);setPreview(null);}})}{editing&&enabled&&!preview&&<div class="edit-actions"><Button morph={false} disabled={busy||!pendingCount(edits)} onClick={()=>void save(false)}>변경 미리보기</Button><Button morph={false} disabled={busy} onClick={()=>setEdits(emptyEdits())}>변경 취소</Button><small>{pendingCount(edits)}개 변경 대기 · 적용 전 미리보기가 필요합니다.</small></div>}<Dialog label="Apply row changes" title="변경 적용" open={confirm} onClose={()=>!busy&&setConfirm(false)} footer={<><Button morph={false} disabled={busy} onClick={()=>setConfirm(false)}>취소</Button><Button morph={false} variant="brand" disabled={busy||!valid||!enabled} onClick={()=>void save(true)}>적용</Button></>}><p>{request?.edits.length??0}개 변경을 데이터베이스에 적용합니다.</p><p>원래 값이 바뀐 행은 충돌로 처리합니다. 결과를 다시 조회하고 확인하세요.</p></Dialog></>;
}
