import type { TextEdit } from './editorContract';
export function applyTextEdits(text: string, edits: TextEdit[]): string {
 const sorted=[...edits].sort((a,b)=>a.start-b.start || a.end-b.end);
 let end=0;
 for(const edit of sorted){if(!Number.isInteger(edit.start)||!Number.isInteger(edit.end)||edit.start<end||edit.end<edit.start||edit.end>text.length)throw new Error('유효하지 않거나 겹치는 편집 범위입니다.');end=edit.end;}
 return sorted.reverse().reduce((value,edit)=>value.slice(0,edit.start)+edit.text+value.slice(edit.end),text);
}
