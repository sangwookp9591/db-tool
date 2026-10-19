import { useLayoutEffect } from 'preact/hooks';
export function useCommandShortcut(open: boolean, onOpenChange: (open: boolean) => void) {
  useLayoutEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing || event.altKey || !(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== 'k') return;
      if (!open && document.querySelector('dialog[open]')) return;
      event.preventDefault(); onOpenChange(!open);
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, [open, onOpenChange]);
}
