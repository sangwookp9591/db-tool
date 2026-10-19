import { useEffect, useState } from 'preact/hooks';
import { Button } from './Button';
// Interaction reference: affectUI Toaster by Flo, https://www.affectui.com/components/toaster; independent implementation, no source copied.
import type { MicroAccessibility } from './accessibility';
import './micro.css';

export interface ToastItem { id: string; message: string; tone?: 'success' | 'error' | 'info'; action?: { label: string; onClick: () => void } }
// Non-error toasts leave on their own after `autoDismissMs` (opt-in; paused while hovered or focused). Errors stay until closed.
function Toast({ item, onDismiss, autoDismissMs }: { item: ToastItem; onDismiss: (id: string) => void; autoDismissMs?: number }) {
  const [paused, setPaused] = useState(false);
  useEffect(() => {
    if (!autoDismissMs || item.tone === 'error' || paused) return;
    const timer = setTimeout(() => onDismiss(item.id), autoDismissMs);
    return () => clearTimeout(timer);
  }, [item.id, item.tone, autoDismissMs, paused]);
  return <div class="toast-stack__item" data-tone={item.tone ?? 'info'} onPointerEnter={() => setPaused(true)} onPointerLeave={() => setPaused(false)} onFocusIn={() => setPaused(true)} onFocusOut={() => setPaused(false)}>
    <span role={item.tone === 'error' ? 'alert' : 'status'} title={item.message}>{item.message}</span>
    {item.action && <Button bare type="button" onClick={item.action.onClick}>{item.action.label}</Button>}
    <Button bare type="button" aria-label={`Dismiss ${item.message}`} onClick={() => onDismiss(item.id)}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18" /></svg></Button>
  </div>;
}
export function ToastStack({ items, onDismiss, autoDismissMs, ...accessibility }: MicroAccessibility & { items: readonly ToastItem[]; onDismiss: (id: string) => void; autoDismissMs?: number }) {
  return <div class="toast-stack micro" role="region" aria-label={accessibility['aria-label'] ?? accessibility.ariaLabel ?? 'Notifications'} data-testid={accessibility['data-testid'] ?? accessibility.testId}>
    {items.map(item => <Toast key={item.id} item={item} onDismiss={onDismiss} autoDismissMs={autoDismissMs} />)}
  </div>;
}
