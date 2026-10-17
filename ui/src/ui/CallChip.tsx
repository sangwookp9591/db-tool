// Based on React Bits (MIT + Commons Clause), independently implemented for Preact.
import { Check, File, Pencil, RotateCw, Search, Terminal } from 'lucide-preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import type { ComponentChildren } from 'preact';
import type { MicroAccessibility } from './accessibility';
import './call-chip.css';

export type CallChipStatus = 'idle' | 'running' | 'done' | 'error';
export type CallChipIcon = 'terminal' | 'file' | 'search' | 'edit';
export interface CallChipProps extends MicroAccessibility {
  icon?: CallChipIcon; name?: string; argument?: string; status?: CallChipStatus; expectedMs?: number;
  size?: number; radius?: number; showTimer?: boolean; shake?: number; durationMs?: number; onRetry?: () => void;
  className?: string; children?: ComponentChildren;
}
const icons = { terminal: Terminal, file: File, search: Search, edit: Pencil };
const words = { idle: 'queued', running: 'running', done: 'done', error: 'failed' } as const;
const shakeFrames = [0, -1, 1, -.66, .66, -.33, 0];

export function formatCallChipTime(ms: number) { return ms < 10000 ? `${Math.round(ms)} ms` : `${(ms / 1000).toFixed(1)} s`; }

export function CallChip({ icon = 'terminal', name = 'tool', argument = '', status = 'idle', expectedMs = 2500, size = 34, radius = 10, showTimer = true, shake = 6, durationMs, onRetry, className = '', children, ...accessibility }: CallChipProps) {
  const root = useRef<HTMLSpanElement>(null), fill = useRef<HTMLSpanElement>(null), frame = useRef(0), start = useRef(0), lastTime = useRef(0);
  const [elapsed, setElapsed] = useState(0);
  const label = accessibility['aria-label'] ?? accessibility.ariaLabel;
  useEffect(() => {
    if (status !== 'running') { cancelAnimationFrame(frame.current); frame.current = 0; return; }
    start.current = performance.now();
    const tick = (now: number) => { lastTime.current = Math.max(0, now - start.current); setElapsed(lastTime.current); frame.current = requestAnimationFrame(tick); };
    frame.current = requestAnimationFrame(tick);
    return () => { cancelAnimationFrame(frame.current); frame.current = 0; lastTime.current = Math.max(0, performance.now() - start.current); };
  }, [status]);
  useEffect(() => {
    if (!fill.current) return;
    const live = status === 'error' ? Number(getComputedStyle(fill.current).transform.match(/matrix\(([^,]+)/)?.[1] ?? 0) : 0;
    fill.current.style.transition = 'none';
    fill.current.style.transform = `scaleX(${status === 'running' || status === 'done' ? 0 : status === 'error' ? live : 0})`;
    void fill.current.offsetWidth;
    fill.current.style.transition = '';
    if (status === 'running') requestAnimationFrame(() => { if (fill.current) fill.current.style.transform = 'scaleX(.9)'; });
    if (status === 'done') requestAnimationFrame(() => { if (fill.current) fill.current.style.transform = 'scaleX(1)'; });
    if (status === 'error' && shake > 0 && !matchMedia('(prefers-reduced-motion: reduce)').matches && root.current) {
      root.current.animate(shakeFrames.map(x => ({ transform: `translateX(${x * shake}px)` })), { duration: 450, composite: 'add' });
    }
  }, [status, expectedMs, shake]);
  const Icon = icons[icon];
  const elapsedMs = status === 'running' ? elapsed : durationMs ?? lastTime.current;
  const announce = `${name} ${argument}, ${words[status]}${status === 'done' ? ` in ${Math.round(elapsedMs)} ms` : status === 'error' ? ` after ${Math.round(elapsedMs)} ms` : ''}`;
  return <span ref={root} className={`call-chip${className ? ` ${className}` : ''}`} data-status={status} role="status" aria-busy={status === 'running' || undefined} aria-label={label} data-testid={accessibility['data-testid'] ?? accessibility.testId} title={argument || undefined} style={{ '--cc-size': `${size}px`, '--cc-radius': `${radius}px`, '--cc-expected': `${Math.max(1, expectedMs)}ms` } as any}>
    <span ref={fill} className="call-chip__fill" aria-hidden="true" />
    <span className="call-chip__icon" aria-hidden="true">{status === 'done' ? <Check size={15} /> : status === 'error' ? <RotateCw size={15} /> : <Icon size={15} />}</span>
    <span className="call-chip__name">{name}</span>
    <span className="call-chip__arg" title={argument}>{argument || children}</span>
    {showTimer && <span className="call-chip__timer" aria-hidden="true">{status === 'idle' ? '—' : formatCallChipTime(elapsedMs)}</span>}
    {status === 'error' && onRetry && <button type="button" className="call-chip__retry" aria-label={`Retry ${name} ${argument}`} onClick={onRetry} />}
    <span className="call-chip__sr">{announce}</span>
  </span>;
}
