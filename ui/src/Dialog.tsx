import { Button } from './ui/Button';
import { useEffect, useRef } from 'preact/hooks';
import type { ComponentChildren } from 'preact';
import { X } from 'lucide-preact';
import { BorderGlow } from './ui/BorderGlow';

export function Dialog({ label, onClose, children, footer, closeLabel, wide = false, closeDisabled = false, title, open = true, className = '' }: { label: string; title?: string; open?: boolean; onClose: () => void; children: ComponentChildren; footer?: ComponentChildren; closeLabel?: string; wide?: boolean; closeDisabled?: boolean; className?: string }) {
  const element = useRef<HTMLDialogElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  function edges() {
    const node = body.current;
    if (!node) return;
    node.parentElement!.dataset.above = String(node.scrollTop > 1);
    node.parentElement!.dataset.below = String(node.scrollTop + node.clientHeight < node.scrollHeight - 1);
  }
  useEffect(() => {
    const dialog = element.current!, scroll = body.current!;
    edges();
    const observer = new ResizeObserver(edges); observer.observe(scroll); if (scroll.firstElementChild) observer.observe(scroll.firstElementChild);
    return () => { clearTimeout(timer.current); observer.disconnect(); dialog.close(); };
  }, []);
  useEffect(() => { const dialog=element.current!; if(open&&!dialog.open)dialog.showModal(); else if(!open&&dialog.open)dialog.close(); edges(); }, [open]);
  return <dialog ref={element} class={`modal-dialog ${wide ? 'modal-wide' : ''} ${className}`} aria-label={open ? label : undefined} onCancel={e => { e.preventDefault(); if (!closeDisabled) onClose(); }}>
    <BorderGlow><div class="dialog-shell">
      <header class="dialog-header"><span class="axdb-mark" aria-hidden="true" /><h2>{title ?? label}</h2><Button morph={false} type="button" disabled={closeDisabled} aria-label={closeLabel ?? `Close ${label}`} onClick={onClose}><X /></Button></header>
      <div class="dialog-body-wrap"><div ref={body} class="dialog-body" onScroll={() => { edges(); body.current?.classList.add('is-scrolling'); clearTimeout(timer.current); timer.current = setTimeout(() => body.current?.classList.remove('is-scrolling'), 650); }}>{children}</div></div>
      {footer && <footer class="dialog-footer">{footer}</footer>}
    </div></BorderGlow>
  </dialog>;
}
