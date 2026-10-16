// Based on React Bits (MIT + Commons Clause), original author David Haz.
import { useLayoutEffect, useRef } from 'preact/hooks';
import { spring, reducedMotion } from './spring';
import { clampValue, sliderKey, type SliderProps } from './slider';
import './micro.css';

export function CometDial({ value, onChange, min = 1, max = 600, step = 1, disabled, unit = 's', ...accessibility }: SliderProps) {
  const ariaLabel = accessibility['aria-label'] ?? accessibility.ariaLabel ?? 'Timeout';
  const root = useRef<HTMLDivElement>(null);
  const tail = useRef<SVGGElement>(null);
  const pointer = useRef<number | null>(null);
  const stop = useRef<() => void>(() => {});
  const clean = clampValue(value, min, max, step);
  const logarithmic = min > 0 && max > min;
  const ratio = max <= min ? 0 : logarithmic ? Math.log(clean / min) / Math.log(max / min) : (clean - min) / (max - min);
  const angle = -135 + ratio * 270;
  useLayoutEffect(() => () => stop.current(), []);
  const glow = () => {
    stop.current();
    stop.current = spring(reducedMotion() ? 0 : 1, 0, opacity => { if (tail.current) tail.current.style.opacity = String(opacity); }, 300);
  };
  const change = (next: number) => { onChange(clampValue(next, min, max, step)); glow(); };
  const fromPoint = (x: number, y: number) => {
    const rect = root.current!.getBoundingClientRect();
    const deg = Math.atan2(x - rect.left - rect.width / 2, -(y - rect.top - rect.height / 2)) * 180 / Math.PI;
    const p = Math.min(1, Math.max(0, (deg + 135) / 270));
    change(logarithmic ? min * (max / min) ** p : min + p * (max - min));
  };
  useLayoutEffect(() => {
    const element = root.current!;
    const wheel = (event: WheelEvent) => { if (disabled || !event.deltaY) return; event.preventDefault(); change(clean + (event.deltaY < 0 ? step : -step)); };
    element.addEventListener('wheel', wheel, { passive: false });
    return () => element.removeEventListener('wheel', wheel);
  }, [clean, disabled, min, max, step, onChange]);
  return <div ref={root} className="comet-dial micro" role="slider" tabIndex={disabled ? -1 : 0} aria-label={ariaLabel} data-testid={accessibility['data-testid'] ?? accessibility.testId} aria-valuemin={min} aria-valuemax={max} aria-valuenow={clean} aria-valuetext={`${clean}${unit}`} aria-disabled={disabled || undefined}
    onKeyDown={e => { if (disabled) return; const next = sliderKey(e.key, clean, min, max, step, e.shiftKey); if (next !== undefined) { e.preventDefault(); change(next); } }}
    onPointerDown={e => { if (disabled || e.button !== 0 || pointer.current !== null) return; e.preventDefault(); pointer.current = e.pointerId; e.currentTarget.focus(); e.currentTarget.setPointerCapture(e.pointerId); fromPoint(e.clientX, e.clientY); }}
    onPointerMove={e => { if (!disabled && pointer.current === e.pointerId) fromPoint(e.clientX, e.clientY); }} onPointerUp={() => { pointer.current = null; }} onPointerCancel={() => { pointer.current = null; }} onLostPointerCapture={() => { pointer.current = null; }}>
    <svg viewBox="0 0 120 120" aria-hidden="true">
      <path className="comet-dial__track" d="M 27.5 92.5 A 46 46 0 1 1 92.5 92.5" />
      <g style={{ transform: `rotate(${angle}deg)`, transformOrigin: '60px 60px' }}>
        <g ref={tail} className="comet-dial__tail" style={{ opacity: 0 }}>{[1, 2, 3, 4, 5, 6].map(i => <circle key={i} cx="60" cy="14" r={4 - i * .4} style={{ transform: `rotate(${-i * 7}deg)`, transformOrigin: '60px 60px', opacity: 1 - i / 7 }} />)}</g>
        <circle className="comet-dial__head" cx="60" cy="14" r="5" />
      </g>
    </svg>
    <span className="comet-dial__value" aria-hidden="true">{clean}<small>{unit}</small></span>
    <span className="micro-sr">{ariaLabel}</span>
  </div>;
}
