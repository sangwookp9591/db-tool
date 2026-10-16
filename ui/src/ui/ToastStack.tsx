import { Button } from './Button';
// Interaction reference: affectUI Toaster by Flo, https://www.affectui.com/components/toaster; independent implementation, no source copied.
import type { MicroAccessibility } from './accessibility';
import './micro.css';

export interface ToastItem { id: string; message: string; tone?: 'success' | 'error' | 'info' }
export function ToastStack({ items, onDismiss, ...accessibility }: MicroAccessibility & { items: readonly ToastItem[]; onDismiss: (id: string) => void }) {
  return <div className="toast-stack micro" role="region" aria-label={accessibility['aria-label'] ?? accessibility.ariaLabel ?? 'Notifications'} data-testid={accessibility['data-testid'] ?? accessibility.testId}>
    {items.map(item => <div key={item.id} className="toast-stack__item" data-tone={item.tone ?? 'info'}>
      <span role={item.tone === 'error' ? 'alert' : 'status'}>{item.message}</span>
      <Button bare type="button" aria-label={`Dismiss ${item.message}`} onClick={() => onDismiss(item.id)}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18" /></svg></Button>
    </div>)}
  </div>;
}
