// 사용자 제공 레퍼런스 재구현
import type { ComponentChildren, JSX } from 'preact';
import { useCallback, useLayoutEffect, useRef } from 'preact/hooks';
import { LoaderCircle } from 'lucide-preact';
import type { MicroAccessibility } from './accessibility';
import { reducedMotion } from './spring';
import './micro-extra.css';

export interface ButtonProps extends Omit<JSX.IntrinsicElements['button'], 'size' | keyof MicroAccessibility>, MicroAccessibility {
  variant?: 'primary' | 'secondary' | 'ghost' | 'destructive' | 'brand';
  size?: 'sm' | 'md' | 'lg';
  loading?: boolean;
  icon?: ComponentChildren;
  iconRight?: ComponentChildren;
  morph?: boolean;
  bare?: boolean;
}

export function Button(props: ButtonProps) {
  if (!props.bare) return <AnimatedButton {...props} />;
  const { key: nodeKey, bare: _bare, variant: _variant, size: _size, icon: _icon, iconRight: _right, morph: _morph, loading, disabled, children, onClick, ariaLabel, testId, type = 'button', ...native } = props;
  return <button key={nodeKey} {...native} type={type} disabled={disabled || loading} aria-busy={loading || native['aria-busy']} aria-label={native['aria-label'] ?? ariaLabel} data-testid={native['data-testid'] ?? testId} onClick={disabled || loading ? undefined : onClick}>{children}</button>;
}

function AnimatedButton({ variant = 'secondary', size = 'sm', loading = false, icon, iconRight, morph = true, bare = false, ref: forwardedRef, children, disabled, onClick, type = 'button', class: className, className: legacyClass, ariaLabel, testId, ...props }: ButtonProps) {
  const root = useRef<HTMLButtonElement>(null);
  const setRef = useCallback((element: HTMLButtonElement | null) => {
    root.current = element;
    if (typeof forwardedRef === 'function') return forwardedRef(element);
    if (forwardedRef) forwardedRef.current = element;
  }, [forwardedRef]);
  const content = useRef<HTMLSpanElement>(null);
  const previous = useRef<{ width: number; signature: string } | undefined>(undefined);
  useLayoutEffect(() => {
    const el = root.current, inner = content.current;
    if (!el || !inner || !morph || bare || typeof ResizeObserver === 'undefined') return;
    const media = matchMedia('(prefers-reduced-motion: reduce)');
    let animations: Animation[] = [];
    const cancel = () => { animations.forEach(animation => animation.cancel()); animations = []; };
    const measure = () => {
      const width = inner.getBoundingClientRect().width;
      const signature = inner.innerHTML;
      const old = previous.current;
      previous.current = { width, signature };
      if (!old || old.signature === signature || reducedMotion() || !el.animate) return;
      cancel();
      const outer = el.getBoundingClientRect().width;
      // DESIGN exception: one small button width morph, outside result grids/canvases.
      if (old.width !== width) animations.push(el.animate([{ width: `${outer + old.width - width}px` }, { width: `${outer}px` }], { duration: 480, easing: 'cubic-bezier(.2,.8,.2,1)' }));
      animations.push(inner.animate([{ opacity: .35, transform: 'translateY(3px)' }, { opacity: 1, transform: 'translateY(0)' }], { duration: 220 }));
    };
    const observer = new ResizeObserver(measure);
    const mutations = new MutationObserver(measure);
    observer.observe(inner);
    mutations.observe(inner, { childList: true, subtree: true, characterData: true });
    media.addEventListener('change', cancel);
    return () => { observer.disconnect(); mutations.disconnect(); media.removeEventListener('change', cancel); cancel(); };
  }, [morph, bare]);
  return <button {...props} ref={setRef} type={type} disabled={disabled || loading} aria-busy={loading || props['aria-busy']} aria-label={props['aria-label'] ?? ariaLabel} data-testid={props['data-testid'] ?? testId}
    className={bare ? className ?? legacyClass : `ax-button ax-button--${variant} ax-button--${size} ${children == null ? 'ax-button--icon' : ''} ${className ?? legacyClass ?? ''}`}
    onClick={event => { if (!disabled && !loading) onClick?.(event); }}>
    {bare ? children : <span ref={content} className="ax-button__content">
      {loading ? <LoaderCircle className="ax-spinner" size={16} aria-hidden="true" /> : icon && <span className="ax-button__icon" aria-hidden="true">{icon}</span>}
      {children != null && <span className="ax-button__label">{children}{loading && <span className="ax-button__shine" aria-hidden="true" />}</span>}
      {iconRight && <span className="ax-button__icon" aria-hidden="true">{iconRight}</span>}
    </span>}
  </button>;
}
