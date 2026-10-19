import { useEffect, useState } from 'preact/hooks';
import { api } from '../api';

export function useAiReady() {
  const [ready, setReady] = useState(false);
  useEffect(() => {
    let generation = 0;
    async function refresh() {
      const current = ++generation;
      setReady(false);
      try {
        const data = await api('ai_settings', {});
        const authenticated = data.settings.auth === 'api_key' ? data.has_key
          : data.settings.auth === 'claude_login' ? (await api('ai_login_status', {})).logged_in
          : !!data.accounts?.some(account => account.active && account.logged_in);
        if (current === generation) setReady(data.settings.enabled && authenticated);
      } catch { if (current === generation) setReady(false); }
    }
    void refresh();
    window.addEventListener('ax:ai-changed', refresh);
    return () => { generation++; window.removeEventListener('ax:ai-changed', refresh); };
  }, []);
  return ready;
}
