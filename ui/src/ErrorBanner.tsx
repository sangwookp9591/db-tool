import { useEffect, useState } from 'preact/hooks';
import { ChevronRight, X } from 'lucide-preact';
import { Button } from './ui/Button';
import type { DbError } from './contract';
import { errorKindLabel } from './previewCopy';
import { readableConnectionError } from './connectionError';

// One banner per error. Long details (SQL echoes) fold into a height-capped <details>; Connection errors belong to the global banner only.
export function ErrorBanner({ error, onDismiss, dismissLabel = 'Dismiss error', scope = 'global' }: { error: DbError | null; onDismiss?: () => void; dismissLabel?: string; scope?: 'global' | 'tab' }) {
  const [open, setOpen] = useState(false);
  useEffect(() => setOpen(false), [error]);
  if (!error || (scope === 'tab' && error.kind === 'Connection')) return null;
  error = readableConnectionError(error);
  const long = !!error.detail && (error.detail.length > 140 || error.detail.includes('\n'));
  return <div class={`db-error error-${error.kind}`} role="alert" data-scope={scope}>
    <div class="db-error__head"><span class="error-badge">{errorKindLabel(error.kind)}</span><strong>{error.message}</strong>{error.position !== null && <small>위치: {error.position}</small>}</div>
    {error.detail && (long
      ? <details class="db-error__detail" open={open} onToggle={e => setOpen(e.currentTarget.open)}><summary><ChevronRight class="summary-chevron" aria-hidden="true" />상세 보기</summary><pre>{error.detail}</pre></details>
      : <pre>{error.detail}</pre>)}
    {onDismiss && <Button bare class="db-error__dismiss" aria-label={dismissLabel} onClick={onDismiss}><X /></Button>}
  </div>;
}
