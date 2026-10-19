import { useLayoutEffect, useRef, useState } from 'preact/hooks';

// A visible scene plays once; leaving the viewport or hiding the window settles it immediately.
export function useIllustration(duration: number) {
  const root = useRef<HTMLDivElement>(null);
  const [playing, setPlaying] = useState(false);
  useLayoutEffect(() => {
    const node = root.current!, media = matchMedia('(prefers-reduced-motion: reduce)');
    let started = false, timer: ReturnType<typeof setTimeout> | undefined;
    const settle = () => { clearTimeout(timer); setPlaying(false); node.getAnimations({ subtree: true }).forEach(animation => animation.cancel()); };
    const visibility = () => { if (document.hidden || media.matches) settle(); };
    const observer = new IntersectionObserver(entries => {
      if (!node.isConnected) return;
      if (!entries[0].isIntersecting) { if (started) settle(); return; }
      if (started || document.hidden || media.matches) return;
      started = true; setPlaying(true); timer = setTimeout(settle, duration);
    });
    observer.observe(node);
    const dialog = node.closest('dialog');
    dialog?.addEventListener('close', settle);
    document.addEventListener('visibilitychange', visibility);
    media.addEventListener('change', visibility);
    return () => { clearTimeout(timer); observer.disconnect(); node.getAnimations({ subtree: true }).forEach(animation => animation.cancel()); dialog?.removeEventListener('close', settle); document.removeEventListener('visibilitychange', visibility); media.removeEventListener('change', visibility); };
  }, [duration]);
  return { root, playing };
}
