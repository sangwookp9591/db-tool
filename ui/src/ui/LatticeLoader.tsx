// Based on React Bits (MIT + Commons Clause), original author David Haz.
import { useLayoutEffect, useRef } from 'preact/hooks';
import type { MicroAccessibility } from './accessibility';
import './micro.css';

export function LatticeLoader({ running, label = 'Running', ...accessibility }: MicroAccessibility & { running: boolean; label?: string }) {
  const timer = useRef<HTMLSpanElement>(null);
  useLayoutEffect(() => {
    if (!running) return;
    const started = performance.now();
    const id = setInterval(() => { if (timer.current) timer.current.textContent = `${((performance.now() - started) / 1000).toFixed(1)}s`; }, 100);
    return () => clearInterval(id);
  }, [running]);
  if (!running) return null;
  return <span className="lattice-loader micro" role="status" aria-label={accessibility['aria-label'] ?? accessibility.ariaLabel ?? label} data-testid={accessibility['data-testid'] ?? accessibility.testId}>
    <span className="lattice-loader__grid" aria-hidden="true">{Array.from({ length: 9 }, (_, i) => <span key={i} style={{ animationDelay: `${(i % 3 + Math.floor(i / 3)) * -120}ms` }} />)}</span>
    <span>{label}</span><span ref={timer} className="lattice-loader__timer" aria-hidden="true">0.0s</span>
  </span>;
}
