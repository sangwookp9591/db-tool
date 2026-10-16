// Based on React Bits SpotlightCard (MIT + Commons Clause), original author David Haz.
import type { ComponentChildren } from 'preact';
import { useLayoutEffect, useRef } from 'preact/hooks';
import { reducedMotion } from './spring';
import type { MicroAccessibility } from './accessibility';
import './micro.css';

export function SpotlightCard({ children, active = true, ...accessibility }: MicroAccessibility & { children: ComponentChildren; active?: boolean }) {
  const spot = useRef<HTMLSpanElement>(null);
  const frame = useRef(0);
  const point = useRef({ x: 0, y: 0 });
  const cancel = () => { cancelAnimationFrame(frame.current); frame.current = 0; };
  useLayoutEffect(() => cancel, [active]);
  const label = accessibility['aria-label'] ?? accessibility.ariaLabel;
  return <div className="spotlight-card micro" data-active={active} role={label ? 'group' : undefined} aria-label={label} data-testid={accessibility['data-testid'] ?? accessibility.testId}
    onPointerMove={e => {
      if (!active || reducedMotion()) return;
      const root = e.currentTarget;
      point.current = { x: e.clientX, y: e.clientY };
      if (frame.current) return;
      frame.current = requestAnimationFrame(() => {
        frame.current = 0;
        const rect = root.getBoundingClientRect();
        if (spot.current) spot.current.style.transform = `translate(${point.current.x - rect.left - spot.current.offsetWidth / 2}px, ${point.current.y - rect.top - spot.current.offsetHeight / 2}px)`;
      });
    }} onPointerLeave={cancel}>
    <span className="spotlight-card__light" aria-hidden="true"><span ref={spot} className="spotlight-card__spot" /></span><div className="spotlight-card__content">{children}</div>
  </div>;
}
