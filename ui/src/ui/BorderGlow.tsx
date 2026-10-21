// A plain bordered surface for the editor pane and dialogs. It started as React Bits BorderGlow (MIT + Commons Clause,
// David Haz) with a gradient that followed the pointer; that effect was removed: no gradient, no pointermove work,
// just a flat border that takes the focus colour while something inside has focus.
import type { ComponentChildren } from 'preact';
import type { MicroAccessibility } from './accessibility';
import './micro.css';

export function BorderGlow({ children, active = true, ...accessibility }: MicroAccessibility & { children: ComponentChildren; active?: boolean }) {
  const ariaLabel = accessibility['aria-label'] ?? accessibility.ariaLabel;
  return <div className="border-glow micro" data-active={active} role={ariaLabel ? 'group' : undefined} aria-label={ariaLabel} data-testid={accessibility['data-testid'] ?? accessibility.testId}>
    <div className="border-glow__content">{children}</div>
  </div>;
}
