// Interaction reference: 21st.dev tooltip catalog; independent Preact implementation, no third-party source copied.
import { cloneElement, type ComponentChildren, type VNode } from 'preact';
import { useId, useLayoutEffect, useRef } from 'preact/hooks';
import type { MicroAccessibility } from './accessibility';
import './micro.css';

export function Tooltip({ open, onOpenChange, content, children, ...accessibility }: MicroAccessibility & { open: boolean; onOpenChange: (open: boolean) => void; content: ComponentChildren; children: VNode<{ 'aria-describedby'?: string }> }) {
  const id = useId();
  const root = useRef<HTMLSpanElement>(null);
  const tip = useRef<HTMLSpanElement>(null);
  useLayoutEffect(() => {
    if (!open || !tip.current || !root.current) return;
    const element = tip.current;
    element.showPopover();
    const rect = root.current.getBoundingClientRect();
    element.style.left = `${Math.max(8, Math.min(rect.left, innerWidth - element.offsetWidth - 8))}px`;
    element.style.top = `${rect.bottom + element.offsetHeight + 8 < innerHeight ? rect.bottom : Math.max(8, rect.top - element.offsetHeight)}px`;
    const dismiss = () => onOpenChange(false);
    const key = (event: KeyboardEvent) => { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); dismiss(); } };
    window.addEventListener('keydown', key, true);
    window.addEventListener('resize', dismiss);
    window.addEventListener('scroll', dismiss, true);
    return () => { element.hidePopover(); window.removeEventListener('keydown', key, true); window.removeEventListener('resize', dismiss); window.removeEventListener('scroll', dismiss, true); };
  }, [open, onOpenChange]);
  return <span ref={root} className="micro-tooltip micro" onPointerEnter={() => onOpenChange(true)} onPointerLeave={e => { if (!e.currentTarget.contains(document.activeElement)) onOpenChange(false); }}
    onFocusIn={() => onOpenChange(true)} onFocusOut={e => { if (!e.currentTarget.contains(e.relatedTarget as Node | null)) onOpenChange(false); }}
    onKeyDown={e => { if (e.key === 'Escape' && open) { e.preventDefault(); e.stopPropagation(); onOpenChange(false); } }}>
    {cloneElement(children, { 'aria-describedby': [children.props['aria-describedby'], open ? id : ''].filter(Boolean).join(' ') || undefined })}
    {open && <span ref={tip} popover="manual" id={id} role="tooltip" className="micro-tooltip__content" aria-label={accessibility['aria-label'] ?? accessibility.ariaLabel} data-testid={accessibility['data-testid'] ?? accessibility.testId}>{content}</span>}
  </span>;
}
