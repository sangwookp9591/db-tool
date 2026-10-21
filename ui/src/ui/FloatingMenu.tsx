import type { ComponentChildren } from 'preact';
import { createPortal } from 'preact/compat';
import { useLayoutEffect, useRef } from 'preact/hooks';
import './floating-menu.css';

export function menuPosition(anchor: { left: number; top: number; bottom: number }, size: { width: number; height: number }, viewport: { width: number; height: number }) {
  const left = Math.max(8, Math.min(anchor.left, viewport.width - size.width - 8));
  const below = anchor.bottom + 4;
  const top = below + size.height <= viewport.height - 8 ? below : Math.max(8, Math.min(anchor.top - size.height - 4, viewport.height - size.height - 8));
  return { left, top: Math.max(8, top) };
}

/** Outside scrolling/masked cards; modal menus remain in their dialog's top layer. */
export function FloatingMenu({ anchor, point, label, className = '', onClose, children }: {
  anchor: HTMLElement; point?: { x: number; y: number }; label: string; className?: string;
  onClose: () => void; children: ComponentChildren;
}) {
  const menu = useRef<HTMLDivElement>(null), close = useRef(onClose); close.current = onClose;
  useLayoutEffect(() => {
    const node = menu.current!;
    const items = () => [...node.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')];
    node.querySelectorAll('button').forEach(button=>button.setAttribute('role','menuitem'));
    const initial=anchor.getBoundingClientRect();
    const position = () => {
      const rect = anchor.getBoundingClientRect();
      const origin = point ? { left: rect.left+point.x-initial.left, top: rect.top+point.y-initial.top, bottom: rect.top+point.y-initial.top } : rect;
      const at = menuPosition(origin, node.getBoundingClientRect(), { width: innerWidth, height: innerHeight });
      Object.assign(node.style, { left: `${at.left}px`, top: `${at.top}px` });
    };
    position(); items()[0]?.focus({ preventScroll: true });
    const observer = new ResizeObserver(position); observer.observe(node);
    const dismiss = (e: Event) => { if (!node.contains(e.target as Node) && !anchor.contains(e.target as Node)) close.current(); };
    const scroll = (e: Event) => { if (!node.contains(e.target as Node)) position(); };
    const keyboard = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close.current(); anchor.focus({ preventScroll: true }); }
      if (e.key === 'Tab') close.current();
      if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) return;
      e.preventDefault(); e.stopPropagation();
      const buttons = items(), current = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const next = e.key === 'Home' ? 0 : e.key === 'End' ? buttons.length - 1 : (current + (e.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length;
      buttons[next]?.focus();
    };
    document.addEventListener('pointerdown', dismiss, true); document.addEventListener('keydown', keyboard, true);
    window.addEventListener('resize', position); window.addEventListener('scroll', scroll, true);
    return () => {
      if (node.contains(document.activeElement)) anchor.focus({ preventScroll: true });
      observer.disconnect(); document.removeEventListener('pointerdown', dismiss, true); document.removeEventListener('keydown', keyboard, true);
      window.removeEventListener('resize', position); window.removeEventListener('scroll', scroll, true);
    };
  }, [anchor, point?.x, point?.y]);
  return createPortal(<div ref={menu} class={`floating-menu ${className}`} role="menu" aria-label={label}>{children}</div>, anchor.closest('dialog[open]') ?? document.body);
}
