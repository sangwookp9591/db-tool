import { useEffect, useState } from 'preact/hooks';
import { onboardingStatus, shouldShowOnboarding } from './host';

export function useOnboarding(ready: boolean, report: (error: unknown) => void) {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    if (!ready) return;
    let alive = true;
    void onboardingStatus().then(status => { if (alive && shouldShowOnboarding(status)) setOpen(true); }).catch(error => { if (alive) report(error); });
    return () => { alive = false; };
  }, [ready]);
  return { open, show: () => setOpen(true), close: () => setOpen(false) };
}

