import { useEffect, useState } from 'preact/hooks';
import { ChangelogTimeline, type ChangelogEntry } from '../ui/ChangelogTimeline';
import { releaseEntries } from './notes';
import { Dialog } from '../Dialog';
import { Button } from '../ui/Button';
import { openReleaseLink } from './host';
import { BUILD_STATUS, RELEASE_HISTORY } from './ReleaseNotes';
import './update.css';

const releases = import.meta.glob<string>('../../../docs/release-notes/v*.md', { query: '?raw', import: 'default' });
export { CURRENT_RELEASE, useWhatsNew } from './useWhatsNew';
export function WhatsNew({ onClose }: { onClose: () => void }) {
  const [error, setError] = useState('');
  const [entries, setEntries] = useState<ChangelogEntry[] | null>(null);
  useEffect(() => {
    let alive = true;
    void Promise.all(Object.entries(releases).map(async ([path, load]) => {
      const version = path.match(/v([^/]+)\.md$/)![1];
      return releaseEntries(await load(), version, import.meta.env.AXDB_RELEASE_DATES?.[version]);
    })).then(notes => { if (alive) setEntries(notes.flat()); }).catch(() => { if (alive) setError('릴리스 노트를 불러오지 못했습니다. 창을 다시 열어 주세요.'); });
    return () => { alive = false; };
  }, []);
  return <Dialog className="whats-new-dialog" label="새로운 기능" closeLabel="새로운 기능 닫기" wide onClose={onClose} footer={<Button morph={false} onClick={onClose}>닫기</Button>}>
    {entries === null && !error && <p role="status">릴리스 노트를 불러오고 있습니다…</p>}
    {entries !== null && <ChangelogTimeline entries={entries} status={BUILD_STATUS} historyHref={RELEASE_HISTORY} onOpen={url => { void openReleaseLink(url).catch(() => setError('링크를 열지 못했습니다.')); }} />}
    {error && <p role="alert" class="update-error">{error}</p>}
  </Dialog>;
}
