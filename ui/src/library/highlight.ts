/** The host marks matches as <mark>…</mark> inside plain text. Never inject it as HTML; split it into segments instead. */
export interface Segment { text: string; mark: boolean }
export const ROW_TEXT_LIMIT = 240;
export function oneLine(text: string, limit = ROW_TEXT_LIMIT) {
  const flat = text.replace(/\s+/g, ' ').trim();
  return flat.length > limit ? flat.slice(0, limit) : flat;
}
export function highlightSegments(highlight: string | undefined, fallback: string, limit = ROW_TEXT_LIMIT): Segment[] {
  if (!highlight || !highlight.includes('<mark>')) return [{ text: oneLine(highlight ?? fallback, limit), mark: false }];
  const out: Segment[] = []; let used = 0;
  for (const part of highlight.split(/(<mark>.*?<\/mark>)/s)) {
    if (!part) continue;
    const mark = part.startsWith('<mark>') && part.endsWith('</mark>');
    const text = (mark ? part.slice(6, -7) : part).replace(/\s+/g, ' ');
    if (!text) continue;
    if (used + text.length > limit) { out.push({ text: text.slice(0, Math.max(0, limit - used)), mark }); break; }
    out.push({ text, mark }); used += text.length;
  }
  return out;
}
export function parseTags(input: string) {
  return [...new Set(input.split(/[,\n#]/).map(t => t.trim()).filter(Boolean))].slice(0, 12).map(t => t.slice(0, 32));
}
