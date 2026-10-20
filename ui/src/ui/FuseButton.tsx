import type { ComponentChildren } from 'preact';
import { ShieldCheck, Undo2, Check } from 'lucide-preact';
import { Button } from './Button';
// Based on React Bits (MIT + Commons Clause), original author David Haz.
import { useEffect, useId, useLayoutEffect, useRef, useState } from 'preact/hooks';
import { reducedMotion } from './spring';
import type { MicroAccessibility } from './accessibility';
import './micro.css';
import './FuseButton.css';

export function startFuse(onCommit: () => void, onUndo: () => void, duration = 5000, unmountAction: 'commit' | 'cancel' = 'commit') {
  let active = true, remaining = Math.max(0, Number.isFinite(duration) ? duration : 5000), started = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;
  const finish = (action?: () => void) => { if (!active) return; active = false; if (timer !== null) clearTimeout(timer); timer = null; action?.(); };
  const resume = () => { if (!active || timer !== null) return; started = performance.now(); timer = setTimeout(() => finish(onCommit), remaining); };
  const pause = () => { if (!active || timer === null) return; remaining = Math.max(0, remaining - (performance.now() - started)); clearTimeout(timer); timer = null; };
  resume();
  return { undo: () => finish(onUndo), cancel: () => finish(), dispose: () => finish(unmountAction === 'commit' ? onCommit : undefined), pause, resume, remaining: () => active ? Math.max(0, remaining - (timer !== null ? performance.now() - started : 0)) : 0 };
}

export interface FuseButtonProps extends MicroAccessibility {
  pending: boolean;
  onPendingChange: (pending: boolean) => void;
  onCommit: () => void;
  onUndo: () => void;
  duration?: number;
  unmountAction?: 'commit' | 'cancel';
  label?: string;
  doneLabel?: string;
  undoLabel?: string;
  disabled?: boolean;
}

function ControlledFuseButton({ pending, onPendingChange, onCommit, onUndo, duration = 5000, unmountAction = 'commit', label = 'Delete', doneLabel = 'Done', undoLabel = 'Undo', disabled, ...accessibility }: FuseButtonProps) {
  const line = useRef<HTMLSpanElement>(null);
  const fuse = useRef<ReturnType<typeof startFuse> | null>(null);
  const animation = useRef<Animation | null>(null);
  const latest = useRef({ onPendingChange, onCommit, onUndo });
  latest.current = { onPendingChange, onCommit, onUndo };
  useLayoutEffect(() => {
    if (!pending) { fuse.current?.cancel(); animation.current?.cancel(); return; }
    fuse.current = startFuse(() => {
      animation.current?.cancel();
      latest.current.onPendingChange(false);
      latest.current.onCommit();
    }, () => {
      animation.current?.cancel();
      latest.current.onPendingChange(false);
      latest.current.onUndo();
    }, duration, unmountAction);
    if (!reducedMotion()) animation.current = line.current?.animate?.([{ transform: 'scaleX(1)' }, { transform: 'scaleX(0)' }], { duration: Math.max(1, duration), easing: 'linear' }) ?? null;
    // The deadline and unmount policy belong to the armed action, not subsequent rerenders.
  }, [pending]);
  useLayoutEffect(() => () => { animation.current?.cancel(); fuse.current?.dispose(); }, []);
  useLayoutEffect(() => {
    const preference = matchMedia('(prefers-reduced-motion: reduce)');
    const change = () => { if (preference.matches) animation.current?.cancel(); };
    preference.addEventListener('change', change);
    return () => preference.removeEventListener('change', change);
  }, []);
  return <span className="fuse-button micro" data-pending={pending}>
    {pending && <span role="status">{doneLabel}</span>}
    <Button bare type="button" aria-label={pending ? undoLabel : accessibility['aria-label'] ?? accessibility.ariaLabel ?? label} data-testid={accessibility['data-testid'] ?? accessibility.testId} disabled={!pending && disabled} aria-keyshortcuts={pending ? 'Escape' : undefined}
      onClick={() => { if (pending) fuse.current?.undo(); else onPendingChange(true); }}
      onKeyDown={e => { if (pending && e.key === 'Escape') { e.preventDefault(); fuse.current?.undo(); } }}>
      {pending ? undoLabel : label}
    </Button>
    {pending && <span ref={line} className="fuse-button__line" aria-hidden="true" />}
  </span>;
}

export interface DeferredFuseButtonProps extends MicroAccessibility {
  label?: string; undoLabel?: string; doneLabel?: string; icon?: ComponentChildren;
  color?: string; background?: string; fuseColor?: string; size?: 'sm'|'md'|'lg'; radius?: number;
  undoWindow?: number; fuse?: 'outline'|'bottom'|'top'; fuseThickness?: number; crossfadeMs?: number;
  commitOn: 'fuseEnd'; pauseOnHover?: boolean; settle?: 'reset'|'stay'; disabled?: boolean;
  onCommit: (reason: 'fuseEnd') => void; onUndo?: () => void; onFuseEnd?: () => void;
  onPhaseChange?: (phase: 'idle'|'armed'|'settled') => void; className?: string; type?: 'button'|'submit'|'reset';
}
function DeferredFuseButton({label='허용',undoLabel='취소',doneLabel='실행됨',icon,color='var(--accent-error)',background='color-mix(in srgb, var(--accent-error) 12%, transparent)',fuseColor='var(--accent-warning)',size='sm',radius=22,undoWindow=4000,fuse='outline',fuseThickness=1.5,crossfadeMs=200,commitOn,pauseOnHover=true,settle='stay',disabled,onCommit,onUndo,onFuseEnd,onPhaseChange,className='',type='button',...accessibility}:DeferredFuseButtonProps) {
  const [phase,setPhase]=useState<'idle'|'armed'|'settled'>('idle'),[instant,setInstant]=useState(false),[reduced,setReduced]=useState(reducedMotion),[seconds,setSeconds]=useState(Math.ceil(undoWindow/1000)),[paused,setPaused]=useState(false),[status,setStatus]=useState('');
  const root=useRef<HTMLSpanElement>(null),idle=useRef<HTMLButtonElement>(null),cancel=useRef<HTMLButtonElement>(null),rim=useRef<SVGRectElement>(null),line=useRef<HTMLElement>(null);
  const phaseRef=useRef(phase),clock=useRef<ReturnType<typeof startFuse>|null>(null),animation=useRef<Animation|null>(null),windowMs=useRef(undoWindow),pause=useRef({hover:false,hidden:false,canHoverPause:false}),lastInput=useRef<'pointer'|'keyboard'>('pointer'),statusId=useId();
  const latest=useRef({onCommit,onUndo,onFuseEnd,onPhaseChange,settle});latest.current={onCommit,onUndo,onFuseEnd,onPhaseChange,settle};
  const preset={sm:{height:36,font:13,icon:14,px:16},md:{height:44,font:14,icon:15,px:20},lg:{height:52,font:15,icon:17,px:24}}[size];
  function go(next:typeof phase){phaseRef.current=next;setInstant(lastInput.current==='keyboard');setPhase(next);latest.current.onPhaseChange?.(next);}
  function stopAnimation(){animation.current?.cancel();animation.current=null;}
  function sync(){if(phaseRef.current!=='armed')return;const frozen=pause.current.hover||pause.current.hidden;setPaused(frozen);if(frozen){clock.current?.pause();animation.current?.pause();}else{clock.current?.resume();if(animation.current?.playState==='paused')animation.current.play();}setSeconds(Math.ceil((clock.current?.remaining()??0)/1000));}
  function finish(){if(phaseRef.current!=='armed')return;stopAnimation();lastInput.current='pointer';go(latest.current.settle==='stay'?'settled':'idle');setStatus(doneLabel);latest.current.onFuseEnd?.();latest.current.onCommit('fuseEnd');}
  function undo(){if(phaseRef.current!=='armed')return;clock.current?.cancel();stopAnimation();go('idle');setPaused(false);setStatus('취소했습니다.');latest.current.onUndo?.();}
  function arm(){if(commitOn!=='fuseEnd'||disabled||phaseRef.current!=='idle')return;windowMs.current=Math.max(1,Number.isFinite(undoWindow)?undoWindow:4000);pause.current={hover:false,hidden:document.hidden,canHoverPause:false};clock.current=startFuse(finish,undo,windowMs.current,'cancel');go('armed');setStatus(`${label} 대기 중. 취소할 수 있습니다.`);sync();}
  useLayoutEffect(()=>{if(phase!=='armed'){stopAnimation();return;}if(!reduced){const target=fuse==='outline'?rim.current:line.current;if(target){const a=target.animate(fuse==='outline'?[{strokeDashoffset:0},{strokeDashoffset:-1}]:[{transform:'scaleX(1)'},{transform:'scaleX(0)'}],{duration:windowMs.current,easing:'linear',fill:'forwards'});a.pause();a.currentTime=windowMs.current-(clock.current?.remaining()??0);animation.current=a;}}sync();return stopAnimation;},[phase,reduced,fuse]);
  useLayoutEffect(()=>{const inside=root.current?.contains(document.activeElement);if(phase==='armed')cancel.current?.focus({preventScroll:true});else if(inside)(phase==='idle'?idle.current:root.current)?.focus({preventScroll:true});},[phase]);
  useEffect(()=>{if(!reduced||phase!=='armed'||paused)return;const timer=setInterval(()=>setSeconds(Math.ceil((clock.current?.remaining()??0)/1000)),100);return()=>clearInterval(timer);},[phase,reduced,paused]);
  useEffect(()=>{if(disabled)undo();},[disabled]);
  useEffect(()=>{if(!pauseOnHover){pause.current.hover=false;sync();}},[pauseOnHover]);
  useEffect(()=>{const preference=matchMedia('(prefers-reduced-motion: reduce)'),change=()=>setReduced(preference.matches);preference.addEventListener('change',change);const visibility=()=>{pause.current.hidden=document.hidden;sync();};const escape=(event:KeyboardEvent)=>{if(event.key==='Escape'&&phaseRef.current==='armed'){event.preventDefault();lastInput.current='keyboard';undo();}};document.addEventListener('visibilitychange',visibility);window.addEventListener('keydown',escape);return()=>{clock.current?.cancel();stopAnimation();preference.removeEventListener('change',change);document.removeEventListener('visibilitychange',visibility);window.removeEventListener('keydown',escape);};},[]);
  const release=()=>{if(root.current)delete root.current.dataset.pressed;};
  return <span ref={root} tabIndex={-1} className={`fuse-button fuse-button--deferred ${className}`} data-phase={phase} data-fuse={fuse} data-paused={paused} data-reduced={reduced} data-instant={instant?'':undefined} aria-disabled={phase==='settled'||undefined} style={{'--fb-ink':color,'--fb-bg':background,'--fb-fuse':fuseColor,'--fb-fuse-h':`${fuseThickness}px`,'--fb-radius':`${radius}px`,'--fb-fade':`${crossfadeMs}ms`,'--fb-h':`${preset.height}px`,'--fb-fs':`${preset.font}px`,'--fb-icon':`${preset.icon}px`,'--fb-px':`${preset.px}px`}} onPointerDown={event=>{lastInput.current='pointer';if(event.button===0&&(phaseRef.current==='armed'||phaseRef.current==='idle'&&!disabled))event.currentTarget.dataset.pressed='';}} onPointerUp={release} onPointerCancel={release} onPointerEnter={event=>{if(pauseOnHover&&event.pointerType==='mouse'&&pause.current.canHoverPause){pause.current.hover=true;sync();}}} onPointerLeave={event=>{release();if(event.pointerType==='mouse'){pause.current.canHoverPause=true;pause.current.hover=false;sync();}}} onKeyDown={event=>{if(event.key==='Enter'||event.key===' ')lastInput.current='keyboard';if(event.key==='Escape'&&phaseRef.current==='armed'){event.preventDefault();lastInput.current='keyboard';undo();}}}>
    <Button bare ref={idle} type={type} className="fuse-button__face fuse-button__idle" disabled={disabled} inert={phase!=='idle'} aria-label={accessibility['aria-label']??accessibility.ariaLabel??label} data-testid={accessibility['data-testid']??accessibility.testId} onClick={arm}><span className="fuse-button__icon" aria-hidden="true">{icon??<ShieldCheck/>}</span>{label}</Button>
    <Button bare ref={cancel} type="button" className="fuse-button__face fuse-button__undo" aria-describedby={statusId} aria-keyshortcuts="Escape" inert={phase!=='armed'} onClick={undo}><span className="fuse-button__icon fuse-button__icon--undo" aria-hidden="true"><Undo2/></span>{undoLabel}{reduced&&<span className="fuse-button__countdown">{seconds}초</span>}{fuse!=='outline'&&!reduced&&<i ref={line} className="fuse-button__fuse" aria-hidden="true"/>}</Button>
    <span className="fuse-button__face fuse-button__settled" inert={phase!=='settled'}><span className="fuse-button__icon" aria-hidden="true"><Check/></span>{doneLabel}</span>
    {fuse==='outline'&&!reduced&&<svg className="fuse-button__rim" aria-hidden="true"><rect ref={rim} pathLength="1"/></svg>}
    <span className="fuse-button__status" id={statusId} role="status" aria-live="polite">{status}</span>
  </span>;
}
export function FuseButton(props:FuseButtonProps|DeferredFuseButtonProps){return 'pending' in props?<ControlledFuseButton {...props}/>:<DeferredFuseButton {...props}/>;}
