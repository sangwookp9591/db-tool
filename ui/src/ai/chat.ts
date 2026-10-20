export interface ChatMessage { id: string; role: 'user' | 'assistant'; text: string; complete: boolean; contextLabels?:string[]; sensitive?: boolean; error?: string; phase?:number; startedAt?:number; elapsedMs?:number; context?:{schema:string;elapsedMs:number}; versions?:{text:string;error?:string}[]; version?:number; feedback?:import('../ui/MessageActions').Feedback }
export interface MarkdownBlock { kind: 'text' | 'code'; text: string; language?: string }
// Keep committed blocks stable; only the unfinished tail is scanned on each 50ms flush.
export function blockStream() {
  const blocks: MarkdownBlock[] = []; let consumed = 0;
  return (text: string, complete: boolean) => {
    if (text.length < consumed) { blocks.length = 0; consumed = 0; }
    let tail = text.slice(consumed);
    while (tail) {
      const fence = /^```([^\n]*)\n/.exec(tail);
      if (fence) {
        const end = tail.indexOf('\n```', fence[0].length);
        if (end < 0) break;
        const length = end + 4;
        blocks.push({ kind: 'code', text: tail.slice(fence[0].length, end), language: fence[1].trim() });
        consumed += length; tail = text.slice(consumed);
      } else {
        const end = tail.search(/\n\n|(?:^|\n)```/);
        if (end < 0) break;
        const length = end + (tail.slice(end).startsWith('\n\n') ? 2 : tail[end] === '\n' ? 1 : 0);
        if (!length) break;
        blocks.push({ kind: 'text', text: tail.slice(0, length) }); consumed += length; tail = text.slice(consumed);
      }
    }
    if (complete && tail) { blocks.push({kind:'text',text:tail}); consumed=text.length; tail=''; }
    return { blocks: [...blocks], tail };
  };
}
export function conversationContext(messages: ChatMessage[]) {
  let bytes = 0;
  return messages.filter(m => m.complete && !m.error && !m.sensitive).slice(-20).reverse().filter(m => {
    bytes += new TextEncoder().encode(m.text).length;
    return bytes <= 32000;
  }).reverse().map(m => ({role:m.role,content:m.text}));
}
