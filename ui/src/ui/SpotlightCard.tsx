// A plain surface for a connection card. It started as React Bits SpotlightCard (MIT + Commons Clause, David Haz) with a
// pointer-following radial gradient; that effect was removed: no gradient, no pointermove work, only a flat hover tone.
import type { ComponentChildren } from 'preact';
import type { MicroAccessibility } from './accessibility';
import './micro.css';

export function SpotlightCard({ children, active = true, ...accessibility }: MicroAccessibility & { children: ComponentChildren; active?: boolean }) {
  const label = accessibility['aria-label'] ?? accessibility.ariaLabel;
  return <div className="spotlight-card micro" data-active={active} role={label ? 'group' : undefined} aria-label={label} data-testid={accessibility['data-testid'] ?? accessibility.testId}>
    <div className="spotlight-card__content">{children}</div>
  </div>;
}
