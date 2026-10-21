import { describe, expect, it } from 'vitest';
import { Pager, type PageFetcher } from './pager';

const flush = () => new Promise(resolve => setTimeout(resolve, 0));
/** Descending ids like the host's keyset (`before_id`) pages. */
function source(total: number, size = 100, log: (number | null)[] = []): PageFetcher<number, number> {
  return async cursor => {
    log.push(cursor);
    const from = cursor === null ? total : cursor - 1, items: number[] = [];
    for (let id = from; id >= 1 && items.length < size; id--) items.push(id);
    return { items, next_cursor: items.length === size && items.at(-1)! > 1 ? items.at(-1)! : null };
  };
}

describe('Pager', () => {
  it('loads the first page and exposes one placeholder row while more pages exist', async () => {
    const pager = new Pager(source(250));
    pager.ensure(0, 20); await flush();
    expect(pager.total).toBe(100); expect(pager.count).toBe(101); expect(pager.get(0)).toBe(250); expect(pager.hasMore).toBe(true);
  });
  it('prefetches the next keyset page near the end and keeps a bounded number of rows in memory', async () => {
    const log: (number | null)[] = [];
    const pager = new Pager(source(10_000, 100, log), 3);
    let top = 0;
    for (let step = 0; step < 120; step++) { pager.ensure(top, top + 20); await flush(); top += 90; if (top + 20 > pager.count) top = pager.count - 21; }
    expect(log.length).toBeGreaterThan(50);
    expect(log.slice(0, 3)).toEqual([null, 9_901, 9_801]);
    expect(pager.loaded).toBeLessThanOrEqual(300);
    expect(pager.total).toBeGreaterThan(5_000);
  });
  it('reloads an evicted page by its stored cursor when scrolling back', async () => {
    const log: (number | null)[] = [];
    const pager = new Pager(source(1_000, 100, log), 2);
    for (let top = 0; top < 600; top += 50) { pager.ensure(top, top + 20); await flush(); }
    expect(pager.get(0)).toBeUndefined();
    pager.ensure(0, 20); await flush();
    expect(pager.get(0)).toBe(1_000); expect(pager.loaded).toBeLessThanOrEqual(2 * 100 + 100);
  });
  it('ignores a stale response after reset and aborts its signal', async () => {
    const signals: AbortSignal[] = [];
    let release!: (value: { items: string[]; next_cursor: null }) => void;
    const slow: PageFetcher<string> = (_cursor, signal) => { signals.push(signal); return new Promise(resolve => { release = resolve; }); };
    const pager = new Pager<string>(slow);
    const stale = release;
    pager.reset(async () => ({ items: ['new'], next_cursor: null })); await flush();
    stale({ items: ['old'], next_cursor: null }); await flush();
    expect(signals[0].aborted).toBe(true); expect(pager.get(0)).toBe('new'); expect(pager.total).toBe(1);
  });
  it('keeps scanning when a page is empty but a cursor remains, and reports an empty result at the end', async () => {
    const pages = [{ items: [] as number[], next_cursor: 5 as number | null }, { items: [] as number[], next_cursor: null as number | null }];
    const pager = new Pager<number, number>(async () => pages.shift()!);
    pager.ensure(0, 10); await flush(); await flush();
    expect(pager.isEmpty).toBe(true); expect(pager.count).toBe(0);
  });
  it('removes a loaded row without refetching and surfaces fetch errors with retry', async () => {
    const pager = new Pager(source(5));
    pager.ensure(0, 4); await flush();
    pager.removeAt(1); expect(pager.total).toBe(4); expect(pager.get(1)).toBe(3);
    let fail = true;
    const flaky = new Pager<number>(async () => { if (fail) throw { message: 'boom' }; return { items: [1], next_cursor: null }; });
    await flush(); expect(flaky.error).toEqual({ message: 'boom' });
    fail = false; flaky.retry(); await flush();
    expect(flaky.error).toBeNull(); expect(flaky.get(0)).toBe(1);
  });
  it('indexes 10k rows quickly', async () => {
    const pager = new Pager(source(10_000, 200), 3);
    const started = performance.now();
    let top = 0;
    for (let step = 0; step < 60; step++) { pager.ensure(top, top + 25); await flush(); top = Math.min(top + 190, pager.count - 26); for (let i = top; i < top + 25; i++) pager.get(i); }
    const elapsed = performance.now() - started;
    console.info(`pager: 60 viewport steps over ${pager.total} known rows (${pager.loaded} in memory) in ${elapsed.toFixed(1)}ms`);
    expect(pager.loaded).toBeLessThanOrEqual(600);
  });
  it('pages with opaque string cursors, passing them back unchanged', async () => {
    const log: (string | null)[] = [];
    const fetcher: PageFetcher<number, string> = async cursor => {
      log.push(cursor);
      const page = cursor === null ? 0 : Number(cursor.split(':')[1]);
      return { items: [page * 2, page * 2 + 1], next_cursor: page < 2 ? `snap7:${page + 1}` : null };
    };
    const pager = new Pager(fetcher);
    for (let i = 0; i < 4; i++) { pager.ensure(0, 10); await flush(); }
    expect(log).toEqual([null, 'snap7:1', 'snap7:2']);
    expect([0, 1, 2, 3, 4, 5].map(i => pager.get(i))).toEqual([0, 1, 2, 3, 4, 5]);
    expect(pager.hasMore).toBe(false);
  });
  it('patches a loaded row in place without changing length or position', async () => {
    const pager = new Pager(source(5));
    pager.ensure(0, 4); await flush();
    let emitted = 0; pager.subscribe(() => emitted++);
    expect(pager.update(2, n => n * 10)).toBe(true);
    expect(pager.total).toBe(5); expect(pager.get(2)).toBe(30); expect(emitted).toBe(1);
    expect(pager.update(99, n => n)).toBe(false);
  });
  it('restarts from the first page when a later page is rejected for its cursor, then keeps working', async () => {
    const log: (string | null)[] = [];
    let epoch = 1, restarted = 0;
    const fetcher: PageFetcher<string, string> = async cursor => {
      log.push(cursor);
      if (cursor === null) { epoch++; return { items: [`a${epoch}`], next_cursor: `c${epoch}` }; }
      if (cursor === 'c2') throw { kind: 'Query', message: 'Invalid snippet cursor' };
      return { items: [`b${epoch}`], next_cursor: null };
    };
    const pager = new Pager(fetcher, 3, { isCursorError: e => (e as { message?: string }).message?.includes('cursor') === true, onRestart: () => restarted++ });
    for (let i = 0; i < 6; i++) { pager.ensure(0, 5); await flush(); }
    expect(restarted).toBe(1); expect(pager.error).toBeNull();
    expect(log).toEqual([null, 'c2', null, 'c3']);
    expect([pager.get(0), pager.get(1)]).toEqual(['a3', 'b3']);
  });
  it('does not restart on the first page or on unrelated errors, and gives up after repeated cursor errors', async () => {
    const cursorError = { kind: 'Query', message: 'Invalid snippet cursor' };
    const isCursorError = (e: unknown) => e === cursorError;
    const first = new Pager<number, string>(async () => { throw cursorError; }, 3, { isCursorError });
    await flush(); expect(first.error).toBe(cursorError);
    const other = new Pager<number, string>(async c => { if (c) throw { message: 'boom' }; return { items: [1], next_cursor: 'x' }; }, 3, { isCursorError });
    other.ensure(0, 5); await flush(); await flush(); expect(other.error).toEqual({ message: 'boom' });
    let calls = 0;
    const stuck = new Pager<number, string>(async c => { calls++; if (c) throw cursorError; return { items: [1], next_cursor: 'x' }; }, 3, { isCursorError });
    for (let i = 0; i < 12; i++) { stuck.ensure(0, 5); await flush(); }
    expect(stuck.error).toBe(cursorError); expect(calls).toBeLessThan(10);
  });
});
