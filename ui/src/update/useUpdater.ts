import { scheduleChecks } from './schedule';
import { useEffect, useRef, useState } from 'preact/hooks';
import { checkUpdate, configure, restart, settings, type AppUpdate, type Preferences, type UpdateSettings } from './host';

export function useUpdater() {
  const [config, setConfig] = useState<UpdateSettings | null>(null);
  const [update, setUpdate] = useState<AppUpdate | null>(null);
  const [open, setOpen] = useState(false);
  const [phase, setPhase] = useState<'ready' | 'checking' | 'downloading' | 'installing' | 'installed'>('ready');
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const [progress, setProgress] = useState<{ downloaded: number; total?: number }>({ downloaded: 0 });
  const [saving, setSaving] = useState(false);
  const current = useRef<AppUpdate | null>(null);
  const locked = useRef(false);
  const mounted = useRef(true);
  const configRef = useRef(config); configRef.current = config;
  const installed = useRef(false);
  useEffect(() => {
    mounted.current = true;
    void settings().then(value => { if (mounted.current) setConfig(value); }).catch(() => {});
    return () => { mounted.current = false; void current.current?.close().catch(() => {}); };
  }, []);
  async function check(manual = false) {
    if (locked.current || installed.current) { if (manual) setOpen(true); return; }
    locked.current = true;
    if (manual) { setOpen(true); setPhase('checking'); setMessage(''); setError(''); }
    try {
      const next = await checkUpdate();
      if (!mounted.current) { await next?.close(); return; }
      if (!manual && next && next.version === configRef.current?.preferences.skipped_version) { await next.close(); return; }
      const old = current.current;
      current.current = next; setUpdate(next);
      await old?.close().catch(() => {});
      if (manual && !next) setMessage('최신 버전입니다');
    } catch {
      if (manual) setError('업데이트를 확인할 수 없습니다. 네트워크 연결을 확인하고 다시 시도하세요.');
    } finally { locked.current = false; if (manual) setPhase('ready'); }
  }
  useEffect(() => {
    if (config?.automatic && config.preferences.auto_check) return scheduleChecks(() => { void check(); });
  }, [config?.automatic, config?.preferences.auto_check]);
  async function savePreferences(preferences: Preferences) {
    if (!config || saving) return false;
    setSaving(true);
    try { await configure(preferences); setConfig({ ...config, preferences }); return true; }
    catch { setError('업데이트 설정을 저장하지 못했습니다.'); setOpen(true); return false; }
    finally { setSaving(false); }
  }
  async function skip() {
    if (!update || !config) return;
    if (await savePreferences({ ...config.preferences, skipped_version: update.version })) {
      const old = current.current; current.current = null; setUpdate(null); setOpen(false);
      await old?.close().catch(() => {});
    }
  }
  async function install() {
    if (!update || locked.current) return;
    locked.current = true; setError(''); setPhase('downloading'); setProgress({ downloaded: 0 });
    try {
      await update.download(event => {
        if (event.event === 'Started') setProgress({ downloaded: 0, total: event.data.contentLength });
        if (event.event === 'Progress') setProgress(previous => ({ ...previous, downloaded: previous.downloaded + event.data.chunkLength }));
      }, { timeout: 120_000 });
      setPhase('installing');
      await update.install();
      installed.current = true; setPhase('installed');
    } catch {
      setPhase('ready'); setError('업데이트를 설치하지 못했습니다. 다시 시도하세요.');
    } finally { locked.current = false; }
  }
  async function relaunch() {
    try { await restart(); } catch { setError('다시 시작하지 못했습니다. 앱을 종료한 뒤 다시 열어 주세요.'); }
  }
  return { config, update, open, phase, message, error, progress, saving, check, skip, install, relaunch, savePreferences, setOpen, setError };
}
