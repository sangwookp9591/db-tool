import { useEffect, useRef } from 'preact/hooks';
import type { ComponentChildren } from 'preact';
export function Dialog({ label, onClose, children }: { label: string; onClose: () => void; children: ComponentChildren }) {
  const element = useRef<HTMLDialogElement>(null);
  useEffect(() => { const dialog = element.current!; dialog.showModal(); return () => dialog.close(); }, []);
  return <dialog ref={element} class="modal-dialog" aria-label={label} onCancel={e => { e.preventDefault(); onClose(); }}>{children}</dialog>;
}
