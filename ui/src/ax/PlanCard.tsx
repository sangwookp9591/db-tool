import { useEffect, useRef, useState } from 'preact/hooks';
import type { ComponentChildren } from 'preact';
import { Button } from '../ui/Button';
import { Sparkles, ClipboardList, Check, ShieldCheck, TriangleAlert, Pencil } from 'lucide-preact';
import { Dialog } from '../Dialog';
import './ax.css';
export type Risk = 'read' | 'write' | 'destructive';
export function RiskBadge({ risk }: { risk: Risk }) { const Icon = risk === 'read' ? ShieldCheck : risk === 'write' ? Pencil : TriangleAlert; return <span class="ax-risk" data-risk={risk}><Icon aria-hidden="true"/>{risk === 'read' ? '읽기' : risk === 'write' ? '변경 승인 필요' : '파괴적 변경'}</span>; }
export function PlanCard({ title, evidence, impact, risk, children, onPreview, onApply, confirmName, disabled, previewKey = '', proposal = false }: { title: string; evidence: string[]; impact: string; risk: Risk; children?: ComponentChildren; onPreview?: () => Promise<void>; onApply?: () => Promise<void>; confirmName?: string; disabled?: boolean; previewKey?: string; proposal?: boolean }) {
  const [busy, setBusy] = useState(false), [error, setError] = useState(''), [previewed, setPreviewed] = useState<string | null>(null), [confirm, setConfirm] = useState(false), [phrase, setPhrase] = useState(''), [done, setDone] = useState(false);
  const pending = useRef(false), currentKey = useRef(previewKey); currentKey.current = previewKey;
  useEffect(() => { setDone(false); setConfirm(false); setPreviewed(null); setError(''); }, [previewKey]);
  async function action(apply: boolean) {
    if (pending.current || done || disabled || (apply && (previewed !== previewKey || (risk === 'destructive' && (!confirmName || phrase !== confirmName))))) return;
    pending.current = true; setBusy(true); setError(''); setPreviewed(null);
    try { if (apply) { await onApply?.(); if(currentKey.current===previewKey)setDone(true); setConfirm(false); } else { await onPreview?.(); if(currentKey.current===previewKey)setPreviewed(previewKey); } }
    catch (e) { setError(String((e as Error).message ?? e)); if (!apply) setPreviewed(null); }
    finally { pending.current = false; setBusy(false); }
  }
  return <article class="ax-plan-card" data-proposal={proposal} aria-label={title}><header><span class="ax-plan-origin" role="status">{done?<Check aria-hidden="true"/>:proposal?<Sparkles aria-hidden="true"/>:<ClipboardList aria-hidden="true"/>}{busy?'처리 중':done?'적용 완료':proposal?'AI가 제안한 계획':'검토할 작업 계획'}</span><RiskBadge risk={risk} /></header><h3>{title}</h3><div class="ax-evidence" aria-label="계획 근거">{evidence.map(item => <span key={item}>{item}</span>)}</div><div class="ax-impact"><small>예상 영향</small><p>{impact}</p></div>{children}{proposal && <p class="muted">AI가 만든 검토용 계획입니다. 실행하지 않았습니다.</p>}{error && <p role="alert" class="invalid">{error}</p>}{done && <p role="status">적용이 완료되었습니다.</p>}<footer>{onPreview && <Button morph={false} variant={previewed === previewKey ? undefined : 'brand'} disabled={busy || done || disabled} onClick={() => void action(false)}>미리보기</Button>}{onApply && <Button morph={false} variant="brand" disabled={busy || done || disabled || previewed !== previewKey} onClick={() => { setPhrase(''); setConfirm(true); }}>검토한 계획 적용</Button>}{!onPreview && !onApply && <small>실행 전 별도의 미리보기와 승인이 필요합니다.</small>}</footer>{confirm && <Dialog title="계획 적용 승인" label="계획 적용 승인" onClose={() => { if (!busy) setConfirm(false); }} footer={<><Button morph={false} disabled={busy} onClick={() => setConfirm(false)}>취소</Button><Button morph={false} variant="brand" disabled={busy || (risk === 'destructive' && (!confirmName || phrase !== confirmName))} onClick={() => void action(true)}>승인 및 적용</Button></>}><p>{title} · {impact}</p>{risk === 'destructive' ? <label>확인을 위해 {confirmName} 입력<input aria-label="파괴적 작업 확인" value={phrase} onInput={e => setPhrase(e.currentTarget.value)} /></label> : <p>미리보기에서 확인한 변경을 적용합니다.</p>}</Dialog>}</article>;
}
