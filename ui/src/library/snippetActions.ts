import type { Snippet } from '../contract';
import { parseTags } from './highlight';

export interface SnippetDraft { id?: number; title: string; body: string; tagsText: string; profile_id: string | null; pinned: boolean }
export interface SnippetSaveArgs { id?: number; title: string; body: string; tags: string[]; profile_id: string | null; language: 'sql'; pinned: boolean }
export const TITLE_LIMIT = 80;

export function snippetTitle(body: string) {
  const line = body.split('\n').map(l => l.trim()).find(Boolean) ?? '';
  return line.length > 60 ? `${line.slice(0, 60)}…` : line;
}
/** Validate the editor draft and build the host arguments. Whitespace-only SQL is rejected before any request. */
export function toSaveArgs(draft: SnippetDraft): SnippetSaveArgs {
  const body = draft.body.replace(/\s+$/, '');
  if (!body.trim()) throw new Error('메모에 저장할 SQL이 비어 있습니다.');
  return { ...(draft.id === undefined ? {} : { id: draft.id }), title: (draft.title.trim() || snippetTitle(body)).slice(0, TITLE_LIMIT), body, tags: parseTags(draft.tagsText), profile_id: draft.profile_id, language: 'sql', pinned: draft.pinned };
}
export function draftFrom(snippet: Snippet): SnippetDraft {
  return { id: snippet.id, title: snippet.title, body: snippet.body, tagsText: snippet.tags.join(', '), profile_id: snippet.profile_id, pinned: snippet.pinned };
}
export interface LibraryHost {
  profiles: readonly { id: string; name: string }[];
  insert(text: string, hint?: { profileId?: string | null; profileName?: string }): void;
  openTab(text: string, hint?: { profileId?: string | null; profileName?: string }): void;
  notify(message: string): void;
  report(error: unknown): void;
}
export interface SnippetApi {
  get(id: number): Promise<Snippet>;
}
/** List rows carry a truncated or match-centred body, so insert always reads the stored full text. */
export async function applySnippet(snippet: Snippet, mode: 'insert' | 'tab', host: Pick<LibraryHost, 'insert' | 'openTab'>, source: SnippetApi) {
  const full = await source.get(snippet.id);
  const hint = { profileId: full.profile_id };
  if (mode === 'insert') host.insert(full.body, hint); else host.openTab(full.body, hint);
  return full.body;
}

/** Host rejects stale/legacy snapshot cursors with a Query error that mentions the cursor; the list then restarts from the first page. */
export const isCursorError = (e: unknown) => !!e && typeof e === 'object' && (e as { kind?: unknown }).kind === 'Query' && /cursor/i.test(String((e as { message?: unknown }).message ?? ''));
