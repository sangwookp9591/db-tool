import { useEffect, useRef, useState } from 'preact/hooks';
import { isMock } from '../api';
import { onboardingStatus, shouldShowOnboarding } from '../onboarding/host';

export const CURRENT_RELEASE = '0.2.0';
const SEEN_RELEASE = 'axdb.changelog.seen';
export function useWhatsNew(ready: boolean, automatic: boolean) {
  const [open, setOpen] = useState(false);
  const considered = useRef(false);
  useEffect(() => {
    if (!ready || !automatic || considered.current || (isMock && !new URLSearchParams(location.search).has('changelog'))) return;
    let alive = true;
    let observer: MutationObserver | undefined;
    void onboardingStatus().then(status => {
      if (!alive || shouldShowOnboarding(status)) return;
      try { if (localStorage.getItem(SEEN_RELEASE) === CURRENT_RELEASE) { considered.current = true; return; } } catch { return; }
      const showWhenFree = () => {
        if (considered.current) { observer?.disconnect(); return; }
        if (!alive || document.querySelector('dialog[open]')) return;
        considered.current = true;
        observer?.disconnect();
        setOpen(true);
        try { localStorage.setItem(SEEN_RELEASE, CURRENT_RELEASE); } catch { /* Storage can be unavailable; this session still shows it only once. */ }
      };
      observer = new MutationObserver(showWhenFree);
      observer.observe(document.body, { subtree: true, childList: true, attributes: true, attributeFilter: ['open'] });
      showWhenFree();
    }).catch(() => {});
    return () => { alive = false; observer?.disconnect(); };
  }, [ready, automatic]);
  return { open, show: () => { considered.current = true; setOpen(true); try { localStorage.setItem(SEEN_RELEASE, CURRENT_RELEASE); } catch { /* Manual access also works without storage. */ } }, close: () => setOpen(false) };
}
