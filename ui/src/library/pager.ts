/** Keyset pager that keeps only a few chunks of rows in memory; evicted chunks stay as known-length placeholders. */
export type Cursor = number | string;
export interface PageResult<T, C extends Cursor = Cursor> { items: T[]; next_cursor: C | null }
export type PageFetcher<T, C extends Cursor = Cursor> = (cursor: C | null, signal: AbortSignal) => Promise<PageResult<T, C>>;
interface Chunk<T, C extends Cursor> { cursor: C | null; len: number | null; items: T[] | null; next: C | null }
export interface PagerOptions {
  /** A failed non-first page whose error matches (e.g. a rejected/expired cursor) restarts from the first page once instead of surfacing. Cursors are never persisted. */
  isCursorError?: (error: unknown) => boolean;
  onRestart?: () => void;
}
export const PREFETCH_ROWS = 24;

export class Pager<T, C extends Cursor = Cursor> {
  private chunks: Chunk<T, C>[] = [];
  private loading = new Set<Chunk<T, C>>();
  private listeners = new Set<() => void>();
  private controller = new AbortController();
  private range = { first: 0, last: 0 };
  private fetcher: PageFetcher<T, C>;
  private options: PagerOptions;
  private restarts = 0;
  readonly maxChunks: number;
  generation = 0;
  error: unknown = null;
  constructor(fetcher: PageFetcher<T, C>, maxChunks = 3, options: PagerOptions = {}) { this.fetcher = fetcher; this.maxChunks = maxChunks; this.options = options; this.start(); }

  subscribe(listener: () => void) { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; }
  private emit() { for (const listener of this.listeners) listener(); }
  private start() { this.chunks = [{ cursor: null, len: null, items: null, next: null }]; this.range = { first: 0, last: 0 }; this.error = null; this.load(this.chunks[0]); }

  /** Replace the query. In-flight requests of the previous query are aborted and their results discarded. */
  reset(fetcher: PageFetcher<T, C> = this.fetcher) { this.restarts = 0; this.restart(fetcher); }
  private restart(fetcher: PageFetcher<T, C> = this.fetcher) {
    this.controller.abort(); this.controller = new AbortController();
    this.generation++; this.loading.clear(); this.fetcher = fetcher; this.start(); this.emit();
  }
  retry() { this.error = null; this.ensure(this.range.first, this.range.last); this.emit(); }
  dispose() { this.controller.abort(); this.generation++; this.listeners.clear(); }

  /** Rows known so far, including evicted ones. */
  get total() { let n = 0; for (const c of this.chunks) n += c.len ?? 0; return n; }
  get loaded() { return this.chunks.reduce((n, c) => n + (c.items?.length ?? 0), 0); }
  get hasMore() { const last = this.chunks.at(-1)!; return last.len === null || last.next !== null; }
  get busy() { return this.loading.size > 0; }
  /** total plus one placeholder row while more pages exist. */
  get count() { return this.total + (this.hasMore && !this.error ? 1 : 0); }
  get isEmpty() { return !this.hasMore && this.total === 0; }

  get(index: number): T | undefined {
    let start = 0;
    for (const c of this.chunks) { const len = c.len ?? 0; if (index < start + len) return c.items?.[index - start]; start += len; }
    return undefined;
  }

  /** Called by the view with the visible row range; loads what is missing and evicts far chunks. */
  ensure(first: number, last: number) {
    this.range = { first, last };
    if (this.error) return;
    let start = 0;
    for (const c of this.chunks) {
      const len = c.len ?? 0;
      if (c.len === null ? !this.loading.has(c) : !c.items && start <= last && start + len > first) this.load(c);
      start += len;
    }
    const tail = this.chunks.at(-1)!;
    if (tail.len !== null && tail.next !== null && last >= start - PREFETCH_ROWS && !this.loading.has(tail)) {
      const chunk: Chunk<T, C> = { cursor: tail.next, len: null, items: null, next: null };
      this.chunks.push(chunk); this.load(chunk);
    }
  }

  removeAt(index: number) {
    let start = 0;
    for (const c of this.chunks) {
      const len = c.len ?? 0;
      if (index < start + len) { if (c.items) { c.items.splice(index - start, 1); c.len = len - 1; this.emit(); } return; }
      start += len;
    }
  }

  /** Patch a loaded row in place (same position, same length), e.g. an optimistic flag flip. Returns false when the row is not in memory. */
  update(index: number, patch: (item: T) => T): boolean {
    let start = 0;
    for (const c of this.chunks) {
      const len = c.len ?? 0;
      if (index < start + len) { const at = index - start; if (!c.items) return false; c.items[at] = patch(c.items[at]); this.emit(); return true; }
      start += len;
    }
    return false;
  }

  private load(chunk: Chunk<T, C>) {
    if (this.loading.has(chunk)) return;
    this.loading.add(chunk);
    const generation = this.generation;
    this.fetcher(chunk.cursor, this.controller.signal).then(page => {
      if (generation !== this.generation) return;
      this.loading.delete(chunk);
      chunk.items = page.items; chunk.len = page.items.length; chunk.next = page.next_cursor;
      this.evict(); this.ensure(this.range.first, this.range.last); this.emit();
    }, error => {
      if (generation !== this.generation) return;
      this.loading.delete(chunk);
      if (chunk.cursor !== null && this.restarts < 2 && this.options.isCursorError?.(error)) { this.restarts++; this.options.onRestart?.(); this.restart(); return; }
      this.error = error; this.emit();
    });
  }

  private evict() {
    const loaded = this.chunks.filter(c => c.items);
    if (loaded.length <= this.maxChunks) return;
    const mid = (this.range.first + this.range.last) >> 1;
    const spans = new Map<Chunk<T, C>, [number, number]>(); let start = 0;
    for (const c of this.chunks) { spans.set(c, [start, start + (c.len ?? 0)]); start += c.len ?? 0; }
    const distance = (c: Chunk<T, C>) => { const [a, b] = spans.get(c)!; return mid < a ? a - mid : mid >= b ? mid - b + 1 : 0; };
    const keep = new Set(loaded.sort((a, b) => distance(a) - distance(b)).slice(0, this.maxChunks));
    for (const c of loaded) if (!keep.has(c)) c.items = null;
  }
}
