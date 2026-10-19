import { useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';
import { ArrowLeft, ArrowRight, Check, Copy, Sparkles } from 'lucide-preact';
import { Button } from '../ui/Button';
import { SpringCheck } from '../ui/SpringCheck';
import { Dialog } from '../Dialog';
import { isMock } from '../api';
import { normalizeError } from '../values';
import { bridgeStatus, completeOnboarding, copyMcpConfig, setBridge } from './host';
import { Scene, scenes } from './scenes';
import './onboarding.css';

export { useOnboarding } from './useOnboarding';

interface Props { onClose:()=>void; onConnect:()=>void; onQuery:()=>void; onExplore:()=>void; onClients:()=>void; onAssistant:()=>void }
export function Onboarding(props: Props) {
  const [step, setStep] = useState(0);
  const [busy, setBusy] = useState(false);
  const [enabled, setEnabled] = useState(false);
  const [bridgeBusy, setBridgeBusy] = useState(false);
  const [driverBusy, setDriverBusy] = useState(false);
  const locked = busy || bridgeBusy || driverBusy;
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  const [exportMissing, setExportMissing] = useState(false);
  const content = useRef<HTMLDivElement>(null);
  const saving = useRef(false);
  const alive = useRef(true);
  useEffect(() => () => { alive.current = false; }, []);
  useEffect(() => {
    let current = true;
    if (step === 5) void bridgeStatus().then(value => { if (current) setEnabled(value); }).catch(error => { if (current) setError(normalizeError(error).message); });
    return () => { current = false; };
  }, [step]);
  async function finish(action?: () => void) {
    if (saving.current || locked) return;
    saving.current = true; setBusy(true); setError('');
    try { await completeOnboarding(); content.current?.closest('dialog')?.close(); props.onClose(); action?.(); }
    catch (error) { if (alive.current) setError(normalizeError(error).message); }
    finally { saving.current = false; if (alive.current) setBusy(false); }
  }
  function move(next: number) { setStep(Math.max(0, Math.min(scenes.length - 1, next))); setMessage(''); setError(''); setExportMissing(false); }
  const next = () => step === scenes.length - 1 ? void finish() : move(step + 1);
  useEffect(() => { content.current?.focus({ preventScroll: true }); }, [step]);
  async function toggleBridge(value: boolean) {
    setBridgeBusy(true); setError(''); setMessage('');
    try { await setBridge(value); if (alive.current) setEnabled(value); }
    catch (error) { if (alive.current) setError(normalizeError(error).message); }
    finally { if (alive.current) setBridgeBusy(false); }
  }
  async function copy() {
    setError(''); setMessage('');
    if (!enabled) { setMessage('먼저 MCP 브리지를 켜세요.'); return; }
    setBridgeBusy(true);
    try { await copyMcpConfig(); if (alive.current) setMessage('현재 URL과 토큰이 포함된 설정을 복사했습니다.'); }
    catch (error) { if (alive.current) setError(normalizeError(error).message); }
    finally { if (alive.current) setBridgeBusy(false); }
  }
  useLayoutEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (locked || event.defaultPrevented || event.isComposing || event.altKey || event.metaKey || event.ctrlKey) return;
      const target = event.target as HTMLElement;
      if (!target.closest('.onboarding') || target.closest('input,[role=radiogroup],[role=tablist]')) return;
      if (event.key === 'ArrowRight' || (event.key === 'Enter' && !target.closest('button'))) {
        event.preventDefault(); next();
      } else if (event.key === 'ArrowLeft') { event.preventDefault(); move(step - 1); }
    };
    document.addEventListener('keydown', key);
    return () => document.removeEventListener('keydown', key);
  }, [step, locked]);
  function showExport(image: boolean) {
    const label = image ? '이미지 내보내기' : '결과 내보내기';
    const target = Array.from(document.querySelectorAll<HTMLButtonElement>(`button[aria-label="${label}"]`))
      .find(button => !button.disabled && button.getClientRects().length > 0);
    if (!target) {
      setExportMissing(true);
      setMessage(image ? '연결한 뒤 ERD 또는 그래프 결과를 여세요. 그 툴바에서 이미지 내보내기를 사용할 수 있습니다.' : '먼저 연결하고 샘플 쿼리를 실행하세요. 결과가 표시되면 툴바에서 내보낼 수 있습니다.');
      return;
    }
    void finish(() => {
      target.scrollIntoView({ block: 'nearest' });
      target.classList.add('ob-export-target');
      target.focus({ preventScroll: true });
      target.addEventListener('blur', () => target.classList.remove('ob-export-target'), { once: true });
    });
  }
  const scene = scenes[step];
  const actions = [next, () => void finish(props.onConnect), () => void finish(props.onQuery), () => void finish(props.onExplore), () => void finish(props.onClients), () => void finish(props.onClients), () => void finish(props.onAssistant)];
  return <Dialog label="AxDB 사용법" title="AxDB 시작하기" className="onboarding" closeLabel="사용법 건너뛰기" closeDisabled={locked} onClose={() => void finish()} footer={<div class="ob-footer"><Button morph={false} variant="ghost" disabled={locked} onClick={() => void finish()}>건너뛰기</Button><nav class="ob-dots" aria-label="온보딩 진행">{scenes.map((item,i)=><Button bare key={item.label} disabled={locked} aria-label={`${i+1}. ${item.label}`} aria-current={i===step?'step':undefined} onClick={()=>move(i)}><span/></Button>)}</nav><div class="ob-pagination"><Button morph={false} disabled={step===0 || locked} onClick={()=>move(step-1)}><ArrowLeft size={14}/>이전</Button><Button morph={false} variant="primary" disabled={locked} onClick={next}>{step===6?'시작하기':'다음'}{step===6?<Check size={14}/>:<ArrowRight size={14}/>}</Button></div></div>}>
    <div class="ob-content" ref={content} tabIndex={-1}>
      <div class="ob-copy" key={`copy-${step}`}><span class="ob-eyebrow"><span>{String(step+1).padStart(2,'0')} / 07</span>{scene.label}</span><h1 class="ob-enter">{scene.title}</h1><p class="ob-description ob-enter">{scene.description}</p>{step !== 5 && <div class="ob-scene-actions"><Button morph={false} variant="primary" disabled={locked} onClick={actions[step]}>{step===6?<Sparkles size={15}/>:<ArrowRight size={15}/>} {scene.action}</Button></div>}{step === 6 && <div class="ob-export-controls"><Button morph={false} disabled={locked} onClick={()=>showExport(false)}>결과 내보내기 보여 주기</Button><Button morph={false} disabled={locked} onClick={()=>showExport(true)}>이미지 내보내기 보여 주기</Button>{exportMissing && <><Button morph={false} disabled={locked} onClick={()=>void finish(props.onQuery)}>샘플 쿼리 열기</Button><Button morph={false} disabled={locked} onClick={()=>void finish(props.onConnect)}>연결 추가</Button></>}</div>}<p class="ob-hint">{scene.hint}</p>{step === 5 && <div class="ob-bridge-controls"><SpringCheck aria-label="온보딩 MCP 브리지" checked={enabled} disabled={locked} onChange={value=>void toggleBridge(value)} label={isMock?'1. MCP 브리지 켜기 (모의)':'1. MCP 브리지 켜기'}/><Button morph={false} disabled={locked} onClick={()=>void copy()}><Copy size={14}/>{enabled?'클라이언트 설정 JSON 복사':'먼저 켜기'}</Button><Button morph={false} variant="primary" disabled={locked} onClick={actions[step]}><ArrowRight size={14}/>권한 범위 토큰 발급</Button></div>}</div>
      <Scene key={step} step={step} onDriverBusy={setDriverBusy} onClients={()=>void finish(props.onClients)}/>
      <div class="ob-feedback" aria-live="polite">{error?<span role="alert">{error}</span>:message||<span>← → 단계 이동　·　Enter 다음　·　Esc 건너뛰기</span>}</div>
    </div>
  </Dialog>;
}
