import { describe, expect, it } from 'vitest';
import { mockInvoke } from '../mock';
import type { Snippet } from '../contract';
import { Pager, type PageFetcher } from './pager';
import { isCursorError } from './snippetActions';

const flush = () => new Promise(resolve => setTimeout(resolve, 0));
const fetcher = (tag: string, limit = 3): PageFetcher<Snippet, string> => cursor => mockInvoke('snippet_list', { tag, limit, before_id: cursor ?? undefined });

describe('snippet list over the pager', () => {
  it('shows pinned rows first and flips the flag in place on an optimistic toggle', async () => {
    for (let i = 0; i < 5; i++) await mockInvoke('snippet_save', { title: `pl-${i}`, body: `SELECT ${i}`, tags: ['pl'], pinned: i === 1 });
    const pager = new Pager(fetcher('pl'), 3, { isCursorError });
    pager.ensure(0, 10); await flush(); await flush();
    expect(pager.total).toBe(5); expect([0, 1, 2, 3, 4].map(i => pager.get(i)!.pinned)).toEqual([true, false, false, false, false]);
    const target = pager.get(3)!;
    expect(pager.update(3, s => ({ ...s, pinned: true }))).toBe(true);
    expect(pager.total).toBe(5); expect(pager.get(3)!.id).toBe(target.id); expect(pager.get(3)!.pinned).toBe(true);
    // reconcile: the next first-page load shows the server order.
    pager.reset(fetcher('pl')); pager.ensure(0, 10); await flush(); await flush();
    expect([0, 1].map(i => pager.get(i)!.pinned)).toEqual([true, false]);
  });
  it('restarts from the first page when the host rejects the snapshot cursor', async () => {
    for (let i = 0; i < 5; i++) await mockInvoke('snippet_save', { title: `rs-${i}`, body: `SELECT ${i}`, tags: ['rs'] });
    let forged = true; const sent: (string | null)[] = [];
    const flaky: PageFetcher<Snippet, string> = (cursor, signal) => {
      sent.push(cursor);
      if (cursor && forged) { forged = false; return Promise.reject({ kind: 'Query', message: 'Legacy snippet cursor is unsupported; restart pagination', detail: null, position: null }); }
      return fetcher('rs', 2)(cursor, signal);
    };
    let restarts = 0;
    const pager = new Pager(flaky, 3, { isCursorError, onRestart: () => restarts++ });
    for (let i = 0; i < 8; i++) { pager.ensure(0, 10); await flush(); await flush(); }
    expect(restarts).toBe(1); expect(pager.error).toBeNull(); expect(pager.total).toBe(5);
    expect(new Set([0, 1, 2, 3, 4].map(i => pager.get(i)!.id)).size).toBe(5);
    expect(sent.filter(c => c === null)).toHaveLength(2);
  });
  it('recognises only cursor errors from the host', () => {
    expect(isCursorError({ kind: 'Query', message: 'Invalid snippet cursor' })).toBe(true);
    expect(isCursorError({ kind: 'Query', message: 'syntax error' })).toBe(false);
    expect(isCursorError(null)).toBe(false);
  });
});
