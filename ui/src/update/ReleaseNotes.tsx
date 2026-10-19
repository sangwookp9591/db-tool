import type { ComponentChildren } from 'preact';
import { parseNotes, safeExternalUrl } from './notes';

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
export function ReleaseNotes({ notes, onOpen }: { notes: string; onOpen: (url: string) => void }) {
  return <section class="update-notes" aria-label="릴리스 노트">{parseNotes(notes).map((block, i) => {
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
  })}</section>;
}
