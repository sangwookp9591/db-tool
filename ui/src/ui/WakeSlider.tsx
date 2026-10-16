// Based on React Bits (MIT + Commons Clause), original author David Haz.
import { useLayoutEffect, useRef } from 'preact/hooks';
import { spring, reducedMotion } from './spring';
import { clampValue, sliderKey, type SliderProps } from './slider';
import './micro.css';

export function WakeSlider({ value, onChange, min = 100, max = 100000, step = 100, disabled, unit = '', ...accessibility }: SliderProps) {
  const ariaLabel = accessibility['aria-label'] ?? accessibility.ariaLabel ?? 'Row limit';
  const track = useRef<HTMLDivElement>(null);
  const bars = useRef<(HTMLSpanElement | null)[]>([]);
  const pointer = useRef<number | null>(null);
  const last = useRef({ x: 0, time: 0 });
  const stop = useRef<() => void>(() => {});
  const clean = clampValue(value, min, max, step);
  const ratio = max > min ? (clean - min) / (max - min) : 0;
  useLayoutEffect(() => () => stop.current(), []);
  const fromX = (x: number) => {
    const rect = track.current!.getBoundingClientRect();
    if (!rect.width) return;
    const position = Math.min(1, Math.max(0, (x - rect.left) / rect.width));
    onChange(clampValue(min + position * (max - min), min, max, step));
    const now = performance.now();
    const speed = last.current.time ? Math.min(1, Math.abs(x - last.current.x) / Math.max(1, now - last.current.time)) : 0;
    last.current = { x, time: now };
    stop.current();
    if (reducedMotion()) return;
    stop.current = spring(speed, 0, amplitude => bars.current.forEach((bar, i) => {
      if (bar) bar.style.transform = `scaleY(${1 + amplitude * 2.5 * Math.max(0, 1 - Math.abs(i - position * 31) / 7)})`;
    }), 300);
  };
  const end = () => { pointer.current = null; last.current.time = 0; };
  return <div className="wake-slider micro">
    <div ref={track} className="wake-slider__track" role="slider" tabIndex={disabled ? -1 : 0}
      aria-label={ariaLabel} data-testid={accessibility['data-testid'] ?? accessibility.testId} aria-valuenow={clean} aria-valuemin={min} aria-valuemax={max} aria-valuetext={`${clean}${unit}`} aria-disabled={disabled || undefined}
      onKeyDown={e => { if (disabled) return; const next = sliderKey(e.key, clean, min, max, step, e.shiftKey); if (next !== undefined) { e.preventDefault(); onChange(next); } }}
      onPointerDown={e => { if (disabled || e.button !== 0 || pointer.current !== null) return; e.preventDefault(); pointer.current = e.pointerId; e.currentTarget.setPointerCapture(e.pointerId); e.currentTarget.focus(); fromX(e.clientX); }}
      onPointerMove={e => { if (!disabled && pointer.current === e.pointerId) fromX(e.clientX); }} onPointerUp={end} onPointerCancel={end} onLostPointerCapture={end}>
      {Array.from({ length: 32 }, (_, i) => <span key={i} ref={el => { bars.current[i] = el; }} aria-hidden="true" data-filled={i <= ratio * 31} />)}
    </div>
    <label className="wake-slider__number"><span>{ariaLabel}</span><input type="number" aria-label={`${ariaLabel} value`} min={min} max={max} step={step} value={clean} disabled={disabled}
      onChange={e => { onChange(clampValue(e.currentTarget.valueAsNumber, min, max, step)); e.currentTarget.value = String(clean); }} />{unit}</label>
  </div>;
}
