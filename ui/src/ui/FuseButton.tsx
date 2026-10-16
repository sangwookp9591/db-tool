import { Button } from './Button';
// Based on React Bits (MIT + Commons Clause), original author David Haz.
import { useLayoutEffect, useRef } from 'preact/hooks';
import { reducedMotion } from './spring';
import type { MicroAccessibility } from './accessibility';
import './micro.css';

export function startFuse(onCommit: () => void, onUndo: () => void, duration = 5000, unmountAction: 'commit' | 'cancel' = 'commit') {
  let active = true;
  const finish = (action?: () => void) => {
    if (!active) return;
    active = false;
    clearTimeout(timer);
    action?.();
  };
  const timer = setTimeout(() => finish(onCommit), Math.max(0, duration));
  return { undo: () => finish(onUndo), cancel: () => finish(), dispose: () => finish(unmountAction === 'commit' ? onCommit : undefined) };
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

export function FuseButton({ pending, onPendingChange, onCommit, onUndo, duration = 5000, unmountAction = 'commit', label = 'Delete', doneLabel = 'Done', undoLabel = 'Undo', disabled, ...accessibility }: FuseButtonProps) {
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
