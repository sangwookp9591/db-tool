import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { debounce } from './debounce';
import { highlightSegments, oneLine, parseTags } from './highlight';
import { Pager } from './pager';

describe('debounced search', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());
  it('runs once, 250ms after the last keystroke, with the latest value', () => {
    const run = vi.fn(); const search = debounce(run, 250);
    search('s'); vi.advanceTimersByTime(200); search('se'); vi.advanceTimersByTime(200); search('sel');
    expect(run).not.toHaveBeenCalled(); expect(search.pending()).toBe(true);
    vi.advanceTimersByTime(249); expect(run).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1); expect(run).toHaveBeenCalledTimes(1); expect(run).toHaveBeenCalledWith('sel');
    expect(vi.getTimerCount()).toBe(0);
  });
  it('cancel drops the pending call and flush runs it immediately', () => {
    const run = vi.fn(); const search = debounce(run, 250);
    search('a'); search.cancel(); vi.advanceTimersByTime(500); expect(run).not.toHaveBeenCalled();
    search('b'); search.flush(); expect(run).toHaveBeenCalledWith('b'); expect(vi.getTimerCount()).toBe(0);
  });
  it('only the newest query reaches the list when an older request resolves late', async () => {
    vi.useRealTimers();
    const resolvers = new Map<string, (rows: string[]) => void>();
    const build = (query: string) => () => new Promise<{ items: string[]; next_cursor: null }>(resolve => resolvers.set(query, items => resolve({ items, next_cursor: null })));
    const pager = new Pager<string>(build('a'));
    pager.reset(build('ab'));
    resolvers.get('ab')!(['ab-row']); await new Promise(r => setTimeout(r, 0));
    resolvers.get('a')!(['a-row']); await new Promise(r => setTimeout(r, 0));
    expect(pager.get(0)).toBe('ab-row'); expect(pager.total).toBe(1);
  });
});

describe('highlight', () => {
  it('splits host <mark> markers into text segments instead of HTML', () => {
    expect(highlightSegments('select <mark>id</mark> from <b>t</b>', 'x')).toEqual([{ text: 'select ', mark: false }, { text: 'id', mark: true }, { text: ' from <b>t</b>', mark: false }]);
  });
  it('falls back to the single-line, truncated text', () => {
    expect(oneLine('a\n  b\t c')).toBe('a b c');
    expect(highlightSegments(undefined, 'x'.repeat(500))[0].text).toHaveLength(240);
  });
  it('normalises tags', () => { expect(parseTags('#a, b ,a,,\nc')).toEqual(['a', 'b', 'c']); });
});
