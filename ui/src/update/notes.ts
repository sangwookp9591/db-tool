export type Block = { kind: 'heading'; level: number; text: string } | { kind: 'paragraph'; text: string } | { kind: 'list'; ordered: boolean; items: string[] };
export function parseNotes(source: string): Block[] {
  const blocks: Block[] = [];
  for (const line of source.replace(/\r\n?/g, '\n').split('\n')) {
    if (!line.trim()) continue;
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
