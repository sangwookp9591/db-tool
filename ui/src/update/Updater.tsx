import { Button } from '../ui/Button';
import type { useUpdater } from './useUpdater';
import './update.css';

type Controller = ReturnType<typeof useUpdater>;
export function UpdateSettings({ updater: u }: { updater: Controller }) {
  return <div class="update-settings"><label><span>업데이트 자동 확인</span><input type="checkbox" checked={u.config?.preferences.auto_check ?? true} disabled={!u.config || u.saving} onChange={e => { if (u.config) void u.savePreferences({ ...u.config.preferences, auto_check: e.currentTarget.checked }); }} /></label><Button morph={false} disabled={u.phase === 'checking'} onClick={() => { void u.check(true); }}>업데이트 확인…</Button></div>;
}
export function UpdateBadge({ updater: u }: { updater: Controller }) {
  return u.update ? <Button morph={false} class="update-badge" onClick={() => u.setOpen(true)}>업데이트 {u.update.version}</Button> : null;
}
