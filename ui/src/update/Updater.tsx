import { Dialog } from '../Dialog';
import { Button } from '../ui/Button';
import { openReleaseLink } from './host';
import { ReleaseNotes } from './ReleaseNotes';
import type { useUpdater } from './useUpdater';
import './update.css';

type Controller = ReturnType<typeof useUpdater>;
export function UpdateSettings({ updater: u }: { updater: Controller }) {
  return <div class="update-settings"><label><span>업데이트 자동 확인</span><input type="checkbox" checked={u.config?.preferences.auto_check ?? true} disabled={!u.config || u.saving} onChange={e => { if (u.config) void u.savePreferences({ ...u.config.preferences, auto_check: e.currentTarget.checked }); }} /></label><Button morph={false} disabled={u.phase === 'checking'} onClick={() => { void u.check(true); }}>업데이트 확인…</Button></div>;
}
export function UpdateBadge({ updater: u }: { updater: Controller }) {
  return u.update ? <Button morph={false} class="update-badge" onClick={() => u.setOpen(true)}>업데이트 {u.update.version}</Button> : null;
}
export function UpdateDialog({ updater: u }: { updater: Controller }) {
  if (!u.open) return null;
  const busy = ['checking', 'downloading', 'installing'].includes(u.phase);
  const date = u.update?.date ? new Date(u.update.date) : null;
  const percent = u.progress.total ? Math.min(100, Math.round(u.progress.downloaded / u.progress.total * 100)) : undefined;
  return <Dialog label="AxDB 업데이트" closeLabel="업데이트 닫기" onClose={() => u.setOpen(false)} closeDisabled={busy} className="update-dialog" footer={<>
    {u.phase === 'installed' ? <Button morph={false} variant="primary" onClick={() => { void u.relaunch(); }}>다시 시작</Button> : <>
      {u.update && <Button morph={false} disabled={busy || u.saving || !u.config} onClick={() => { void u.skip(); }}>이 버전 건너뛰기</Button>}
      <Button morph={false} disabled={busy} onClick={() => u.setOpen(false)}>나중에</Button>
      {u.update ? <Button morph={false} variant="primary" disabled={busy} onClick={() => { void u.install(); }}>{u.phase === 'downloading' ? '다운로드 중…' : u.phase === 'installing' ? '설치 중…' : '지금 업데이트'}</Button> : <Button morph={false} disabled={busy} onClick={() => { void u.check(true); }}>다시 확인</Button>}
    </>}
  </>}>
    {u.update && <><p class="update-versions">{u.update.currentVersion} <span aria-label="에서">→</span> <strong>{u.update.version}</strong></p>{date && Number.isFinite(date.getTime()) && <p class="update-date">릴리스 날짜 · <time dateTime={u.update.date}>{date.toLocaleDateString('ko-KR')}</time></p>}<h3>릴리스 노트</h3><ReleaseNotes notes={u.update.body || '등록된 릴리스 노트가 없습니다.'} onOpen={url => { void openReleaseLink(url).catch(() => u.setError('링크를 열지 못했습니다.')); }} /><p class="update-hint">업데이트 전에 쿼리와 편집 내용을 저장해 주세요. Windows에서는 설치 시 앱이 종료되고 다시 열립니다.</p></>}
    {!u.update && u.config && <p>현재 버전 {u.config.current_version}</p>}
    <div role="status" aria-live="polite">{u.phase === 'checking' ? '업데이트를 확인하고 있습니다…' : u.phase === 'installed' ? '설치가 완료되었습니다. 다시 시작하면 새 버전이 적용됩니다.' : u.phase === 'installing' ? '업데이트를 설치하고 있습니다…' : u.message}</div>
    {u.phase === 'downloading' && <div class="update-progress"><label htmlFor="update-download">다운로드 {percent === undefined ? `${Math.round(u.progress.downloaded / 1024).toLocaleString()} KB` : `${percent}%`}</label><progress id="update-download" max={100} value={percent ?? 0} aria-valuetext={percent === undefined ? `다운로드 ${u.progress.downloaded} 바이트, 전체 크기 확인 중` : `${percent}%`} /></div>}
    {u.error && <p role="alert" class="update-error">{u.error}</p>}
  </Dialog>;
}
