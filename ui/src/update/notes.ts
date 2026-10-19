import type { ChangelogCategory, ChangelogEntry } from '../ui/ChangelogTimeline';

export type Block = { kind: 'heading'; level: number; text: string } | { kind: 'paragraph'; text: string } | { kind: 'list'; ordered: boolean; items: string[] };
export function parseNotes(source: string): Block[] {
  const blocks: Block[] = [];
  for (const line of source.replace(/\r\n?/g, '\n').split('\n')) {
    if (!line.trim() || /^<!--\s*date:\s*\d{4}-\d{2}-\d{2}\s*-->$/.test(line.trim())) continue;
    const heading = /^(#{1,6})\s+(.+)$/.exec(line);
    const item = /^\s*(?:([-*+])|\d+[.)])\s+(.+)$/.exec(line);
    if (heading) blocks.push({ kind: 'heading', level: heading[1].length, text: heading[2] });
    else if (item) {
      const ordered = !item[1], last = blocks.at(-1);
      if (last?.kind === 'list' && last.ordered === ordered) last.items.push(item[2]);
      else blocks.push({ kind: 'list', ordered, items: [item[2]] });
    } else blocks.push({ kind: 'paragraph', text: line });
  }
  return blocks;
}

export function safeExternalUrl(value: string): string | null {
  try { const url = new URL(value); return ['https:', 'http:'].includes(url.protocol) && !url.username && !url.password ? url.href : null; } catch { return null; }
}

export interface NoteSection { label: string; change: string; tone: 'good' | 'bad' | 'neutral'; category: ChangelogCategory; blocks: Block[] }
export function summarizeNotes(source: string): { intro: Block[]; sections: NoteSection[] } {
  const intro: Block[] = [], sections: NoteSection[] = [];
  for (const block of parseNotes(source)) {
    if (block.kind === 'heading' && block.level === 2) {
      const tag = /\s*\{(new|improved|fixed|good|bad|neutral)\}\s*$/.exec(block.text);
      const label = block.text.replace(/\s*\{(new|improved|fixed|good|bad|neutral)\}\s*$/, '').trim();
      const tone = tag ? ({ new: 'good', improved: 'neutral', fixed: 'neutral', good: 'good', bad: 'bad', neutral: 'neutral' } as const)[tag[1] as 'new' | 'improved' | 'fixed' | 'good' | 'bad' | 'neutral'] : /제거|변경 주의/.test(label) ? 'bad' : /추가|개선/.test(label) ? 'good' : 'neutral';
      const category: ChangelogCategory = tag ? ({ new: 'new', improved: 'improved', fixed: 'fixed', good: 'new', neutral: 'improved', bad: 'fixed' } as const)[tag[1] as 'new' | 'improved' | 'fixed' | 'good' | 'neutral' | 'bad'] : /수정|고침|오류/.test(label) ? 'fixed' : /추가|새로운/.test(label) ? 'new' : 'improved';
      sections.push({ label, tone, category, change: '', blocks: [] });
    } else if (sections.length) sections.at(-1)!.blocks.push(block);
    else intro.push(block);
  }
  for (const section of sections) {
    const first = section.blocks.find(block => block.kind === 'list');
    const paragraph = section.blocks.find(block => block.kind === 'paragraph');
    section.change = (first?.kind === 'list' ? first.items[0] : paragraph?.kind === 'paragraph' ? paragraph.text : '자세히 보기').replace(/\[([^\]]+)\]\([^)]+\)/g, '$1').replace(/\*\*|`/g, '');
  }
  return { intro, sections };
}

export function releaseEntries(source: string, version: string, fallbackDate = ''): ChangelogEntry[] {
  const declared = /^<!--\s*date:\s*(\d{4}-\d{2}-\d{2})\s*-->$/m.exec(source)?.[1];
  const date = declared ?? fallbackDate.slice(0, 10);
  const { intro, sections } = summarizeNotes(source);
  return [{ category: 'improved' as const, blocks: intro }, ...sections].flatMap(section =>
    section.blocks.flatMap(block => block.kind === 'list' && !block.ordered ? block.items : []).map(item => {
      const text = item.replace(/\[([^\]]+)\]\([^)]+\)/g, '$1').replace(/\*\*|`/g, '');
      const split = /[.!?。](?:\s+|$)/.exec(text);
      const end = split ? split.index + 1 : text.length;
      const href = /\[[^\]]+\]\(([^\s)]+)\)/.exec(item)?.[1];
      return { date, category: section.category, title: text.slice(0, end), summary: text.slice(end).trim(), href: href ? safeExternalUrl(href) ?? undefined : undefined };
    })
  ).map((entry, index) => ({ ...entry, id: `${version}:${index}` }));
}
