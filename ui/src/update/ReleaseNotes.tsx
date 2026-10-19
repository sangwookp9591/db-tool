import type { ComponentChildren } from 'preact';
import { parseNotes, releaseEntries, safeExternalUrl, type Block } from './notes';
import { useMemo } from 'preact/hooks';
import { CURRENT_RELEASE } from './useWhatsNew';
import { ChangelogTimeline } from '../ui/ChangelogTimeline';

function inline(text: string, open: (url: string) => void): ComponentChildren {
  const parts: ComponentChildren[] = [];
  let last = 0;
  for (const match of text.matchAll(/\[([^\]\n]+)\]\(([^\s)]+)\)/g)) {
    parts.push(text.slice(last, match.index));
    const url = safeExternalUrl(match[2]);
    parts.push(url ? <a href={url} onClick={e => { e.preventDefault(); open(url); }}>{match[1]}</a> : match[0]);
    last = match.index! + match[0].length;
  }
  parts.push(text.slice(last));
  return parts;
}
function NoteBlocks({ blocks, onOpen }: { blocks: Block[]; onOpen: (url: string) => void }) {
  return <div class="update-notes">{blocks.map((block, i) => {
    if (block.kind === 'list') {
      const items = block.items.map((text, j) => <li key={j}>{inline(text, onOpen)}</li>);
      return block.ordered ? <ol key={i}>{items}</ol> : <ul key={i}>{items}</ul>;
    }
    const content = inline(block.text, onOpen);
    if (block.kind === 'heading') {
      const Heading = `h${Math.max(3, block.level)}` as 'h3' | 'h4' | 'h5' | 'h6';
      return <Heading key={i}>{content}</Heading>;
    }
    return <p key={i}>{content}</p>;
  })}</div>;
}

export const RELEASE_HISTORY = 'https://github.com/sangwookp9591/db-tool/releases';
export const BUILD_STATUS = { ref: `v${CURRENT_RELEASE}`, commit: import.meta.env.AXDB_BUILD_COMMIT };

export function ReleaseNotes({ notes, onOpen, version, date = '' }: { notes: string; onOpen: (url: string) => void; version: string; date?: string }) {
  const entries = useMemo(() => releaseEntries(notes, version, date || import.meta.env.AXDB_RELEASE_DATES?.[version] || ''), [notes, version, date]);
  return <section aria-label="릴리스 노트">
    <ChangelogTimeline entries={entries} status={BUILD_STATUS} historyHref={RELEASE_HISTORY} onOpen={onOpen} />
    {!entries.length && <NoteBlocks blocks={parseNotes(notes)} onOpen={onOpen} />}
  </section>;
}
