import { describe, expect, it, vi } from 'vitest';
import { mockInvoke } from '../mock';
import { applySnippet, draftFrom, snippetTitle, toSaveArgs, type SnippetDraft } from './snippetActions';

const draft = (patch: Partial<SnippetDraft> = {}): SnippetDraft => ({ title: '', body: 'SELECT 1;\n\n', tagsText: '#a, b', profile_id: null, pinned: false, ...patch });

describe('snippet save', () => {
  it('derives the title from the first non-empty line and trims trailing whitespace', () => {
    expect(snippetTitle('\n  SELECT * FROM t\nWHERE 1')).toBe('SELECT * FROM t');
    expect(toSaveArgs(draft())).toEqual({ title: 'SELECT 1;', body: 'SELECT 1;', tags: ['a', 'b'], profile_id: null, language: 'sql', pinned: false });
  });
  it('rejects empty SQL in Korean without calling the host', () => {
    expect(() => toSaveArgs(draft({ body: ' \n ' }))).toThrow('메모에 저장할 SQL이 비어 있습니다.');
  });
  it('saves, edits and deletes through the command contract', async () => {
    const saved = await mockInvoke('snippet_save', toSaveArgs(draft({ title: '주문 조회', body: 'SELECT * FROM orders' })));
    expect(saved.id).toBeGreaterThan(0);
    const edited = await mockInvoke('snippet_save', toSaveArgs({ ...draftFrom(saved), body: 'SELECT count(*) FROM orders', pinned: true }));
    expect(edited.id).toBe(saved.id); expect(edited.pinned).toBe(true);
    const found = await mockInvoke('snippet_list', { query: 'count', limit: 10 });
    expect(found.items.map(s => s.id)).toContain(saved.id); expect(found.items[0].highlight).toContain('<mark>count</mark>');
    await mockInvoke('snippet_delete', { id: saved.id });
    expect((await mockInvoke('snippet_list', { limit: 10 })).items.some(s => s.id === saved.id)).toBe(false);
  });
});

describe('snippet insert', () => {
  it('inserts the stored full body (not the truncated list row) into the editor, or opens a new tab', async () => {
    const full = await mockInvoke('snippet_save', toSaveArgs(draft({ body: 'SELECT ' + 'x'.repeat(2000), profile_id: 'demo-0' })));
    const listRow = { ...full, body: full.body.slice(0, 1024) };
    const host = { insert: vi.fn(), openTab: vi.fn() };
    const source = { get: (id: number) => mockInvoke('snippet_get', { id }) };
    expect(await applySnippet(listRow, 'insert', host, source)).toBe(full.body);
    expect(host.insert).toHaveBeenCalledWith(full.body, { profileId: 'demo-0' });
    await applySnippet(listRow, 'tab', host, source);
    expect(host.openTab).toHaveBeenCalledWith(full.body, { profileId: 'demo-0' });
  });
});

describe('history search contract', () => {
  it('pages with a keyset cursor and filters by status', async () => {
    const { seedHistory } = await import('../mock');
    seedHistory(250);
    const first = await mockInvoke('history_search', { limit: 100 });
    expect(first.items).toHaveLength(100); expect(first.next_cursor).toBe(first.items.at(-1)!.id);
    const second = await mockInvoke('history_search', { limit: 100, before_id: first.next_cursor! });
    expect(second.items[0].id).toBeLessThan(first.items.at(-1)!.id!);
    const errors = await mockInvoke('history_search', { status: 'error', limit: 1000 });
    expect(errors.items.length).toBeGreaterThan(0); expect(errors.items.every(e => e.error)).toBe(true);
    const id = first.items[0].id!;
    await mockInvoke('history_delete', { id });
    await expect(mockInvoke('history_get', { id })).rejects.toMatchObject({ message: 'History entry not found' });
  });
});

describe('history date range', () => {
  it('treats since as inclusive and until as exclusive and rejects an inverted range', async () => {
    const { seedHistory } = await import('../mock');
    seedHistory(30);
    const all = (await mockInvoke('history_search', { limit: 1000 })).items;
    const stamps = all.map(e => Number(e.timestamp)).sort((a, b) => a - b), mid = stamps[Math.floor(stamps.length / 2)];
    const since = (await mockInvoke('history_search', { limit: 1000, since: new Date(mid).toISOString() })).items;
    expect(since.length).toBeGreaterThan(0); expect(since.every(e => Number(e.timestamp) >= mid)).toBe(true); expect(since.some(e => Number(e.timestamp) === mid)).toBe(true);
    const until = (await mockInvoke('history_search', { limit: 1000, until: new Date(mid).toISOString() })).items;
    expect(until.every(e => Number(e.timestamp) < mid)).toBe(true);
    expect(since.length + until.length).toBe(all.length);
    const both = await mockInvoke('history_search', { limit: 1000, since: new Date(mid).toISOString(), until: new Date(mid + 1).toISOString() });
    expect(both.items.every(e => Number(e.timestamp) === mid)).toBe(true);
    await expect(mockInvoke('history_search', { since: new Date(mid + 5).toISOString(), until: new Date(mid).toISOString() })).rejects.toMatchObject({ kind: 'Query' });
  });
});

describe('snippet pinned-first paging', () => {
  const make = (title: string, pinned = false) => mockInvoke('snippet_save', { title, body: `SELECT '${title}'`, tags: ['pf'], pinned });
  it('lists pinned first, pages with an opaque string cursor and never repeats a row after a pin toggle', async () => {
    const rows = [];
    for (let i = 0; i < 7; i++) rows.push(await make(`pf-${i}`, i === 2));
    const first = await mockInvoke('snippet_list', { tag: 'pf', limit: 3 });
    expect(first.items[0].title).toBe('pf-2'); expect(first.items.map(s => s.pinned)).toEqual([true, false, false]);
    expect(typeof first.next_cursor).toBe('string');
    // pin a not-yet-listed row after the first page: the snapshot cursor keeps the remaining order and yields no duplicates.
    await mockInvoke('snippet_save', { id: rows[0].id, title: 'pf-0', body: rows[0].body, tags: ['pf'], pinned: true });
    const seen = first.items.map(s => s.id); let cursor = first.next_cursor;
    while (cursor) { const page = await mockInvoke('snippet_list', { tag: 'pf', limit: 3, before_id: cursor }); seen.push(...page.items.map(s => s.id)); cursor = page.next_cursor; }
    expect(new Set(seen).size).toBe(seen.length); expect(seen).toHaveLength(7);
    const fresh = await mockInvoke('snippet_list', { tag: 'pf', limit: 10 });
    expect(fresh.items.slice(0, 2).map(s => s.pinned)).toEqual([true, true]); expect(fresh.items.slice(2).some(s => s.pinned)).toBe(false);
    expect(fresh.next_cursor).toBeNull();
  });
  it('rejects a legacy numeric cursor and a forged one with a cursor Query error', async () => {
    await make('pf-cursor');
    await expect(mockInvoke('snippet_list', { limit: 1, before_id: 12 as unknown as string })).rejects.toMatchObject({ kind: 'Query', message: expect.stringMatching(/cursor/i) });
    await expect(mockInvoke('snippet_list', { limit: 1, before_id: 'zz' })).rejects.toMatchObject({ kind: 'Query', message: expect.stringMatching(/cursor/i) });
  });
});
