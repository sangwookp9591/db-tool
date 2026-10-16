// Based on React Bits (MIT + Commons Clause), original author David Haz.
import { useLayoutEffect, useRef } from 'preact/hooks';
import { spring } from './spring';
import type { MicroAccessibility } from './accessibility';
import './micro.css';

function Digit({ char }: { char: string }) {
  const element = useRef<HTMLSpanElement>(null);
  const previous = useRef(char);
  useLayoutEffect(() => {
    if (previous.current === char) return;
    previous.current = char;
    return spring(0, 1, value => {
      if (element.current) { element.current.style.transform = `translateY(${(1 - value) * 12}px) scale(${.85 + value * .15})`; element.current.style.opacity = String(value); }
    });
  }, [char]);
  return <span className="code-slots__slot"><span ref={element}>{char || '\u00a0'}</span></span>;
}

export interface CodeSlotsProps extends MicroAccessibility { value: string; onChange: (value: string) => void; length?: number; disabled?: boolean; mask?: boolean; status?: 'idle' | 'error' | 'success' }
export function CodeSlots({ value, onChange, length = 6, disabled, mask, status = 'idle', ...accessibility }: CodeSlotsProps) {
  const clean = value.replace(/\D/g, '').slice(0, length);
  const inputProps = {
    value: clean, inputMode: 'numeric' as const, autoComplete: 'one-time-code', 'aria-label': accessibility['aria-label'] ?? accessibility.ariaLabel ?? 'One-time code', 'data-testid': accessibility['data-testid'] ?? accessibility.testId, 'aria-invalid': status === 'error',
    disabled, readOnly: status === 'success',
    onInput: (e: { currentTarget: HTMLInputElement }) => {
      const next = e.currentTarget.value.replace(/\D/g, '').slice(0, length);
      onChange(next);
      e.currentTarget.value = clean;
    },
  };
  return <span className="code-slots micro" data-status={status}>
    {mask ? <input type="password" {...inputProps} /> : <input type="text" {...inputProps} />}
    <span className="code-slots__digits" aria-hidden="true">{Array.from({ length }, (_, i) => <Digit key={i} char={clean[i] ? mask ? '\u2022' : clean[i] : ''} />)}</span>
  </span>;
}

export function NumberSlots({ value, suffix = '', ...accessibility }: MicroAccessibility & { value: number | string; suffix?: string }) {
  const text = String(value);
  return <span className="number-slots micro" role="img" aria-label={accessibility['aria-label'] ?? `${accessibility.ariaLabel ?? 'Value'}: ${text}${suffix}`} data-testid={accessibility['data-testid'] ?? accessibility.testId}>
    <span aria-hidden="true">{Array.from(text, (char, i) => <Digit key={i} char={char} />)}{suffix && <span className="number-slots__suffix">{suffix}</span>}</span>
  </span>;
}
