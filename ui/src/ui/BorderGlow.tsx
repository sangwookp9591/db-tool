// Based on React Bits (MIT + Commons Clause), original author David Haz.
import type { ComponentChildren } from 'preact';
import { useLayoutEffect, useRef } from 'preact/hooks';
import { reducedMotion } from './spring';
import type { MicroAccessibility } from './accessibility';
import './micro.css';

export function BorderGlow({ children, active = true, ...accessibility }: MicroAccessibility & { children: ComponentChildren; active?: boolean }) {
  const ariaLabel = accessibility['aria-label'] ?? accessibility.ariaLabel;
  const root = useRef<HTMLDivElement>(null);
  const frame = useRef(0);
  const point = useRef({ x: 0, y: 0 });
  const cancel = () => { cancelAnimationFrame(frame.current); frame.current = 0; };
  useLayoutEffect(() => cancel, [active]);
  return <div ref={root} className="border-glow micro" data-active={active} role={ariaLabel ? 'group' : undefined} aria-label={ariaLabel} data-testid={accessibility['data-testid'] ?? accessibility.testId}
    onPointerMove={e => {
      if (!active || reducedMotion()) return;
      point.current = { x: e.clientX, y: e.clientY };
      if (frame.current) return;
      frame.current = requestAnimationFrame(() => {
        frame.current = 0;
        const el = root.current;
        if (!el) return;
        const rect = el.getBoundingClientRect();
        el.style.setProperty('--glow-angle', `${Math.atan2(point.current.y - rect.top - rect.height / 2, point.current.x - rect.left - rect.width / 2)}rad`);
      });
    }} onPointerLeave={cancel}>
    <span className="border-glow__mesh" aria-hidden="true" /><div className="border-glow__content">{children}</div>
  </div>;
}
