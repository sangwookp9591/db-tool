// Interaction reference: affectUI Shimmer Loader by Flo; independent finite entrance, no third-party source copied.
import type { MicroAccessibility } from './accessibility';
import './micro.css';

export function Skeleton({ running, rows = 3, ...accessibility }: MicroAccessibility & { running: boolean; rows?: number }) {
  if (!running) return null;
  return <div className="micro-skeleton micro" role="status" aria-label={accessibility['aria-label'] ?? accessibility.ariaLabel ?? 'Loading results'} data-testid={accessibility['data-testid'] ?? accessibility.testId}>
    {Array.from({ length: Math.max(1, Math.min(10, Math.floor(rows) || 1)) }, (_, i) => <span key={i} aria-hidden="true" />)}
  </div>;
}
