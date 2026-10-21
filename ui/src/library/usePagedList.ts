import { useCallback, useEffect, useMemo, useRef, useState } from 'preact/hooks';
import { debounce } from './debounce';
import { Pager, type Cursor, type PageFetcher } from './pager';

export const SEARCH_DELAY_MS = 250;
/** One keyset-paged, debounced search list. The search box is uncontrolled so typing never re-renders the rows. */
export function usePagedList<T, C extends Cursor = Cursor>(build: (query: string | undefined) => PageFetcher<T, C>, deps: readonly unknown[], active: boolean, isCursorError?: (error: unknown) => boolean) {
  const query = useRef('');
  const buildRef = useRef(build); buildRef.current = build;
  const pager = useRef<Pager<T, C>>(null as never);
  const [selected, setSelected] = useState(0);
  if (!pager.current) pager.current = new Pager(build(undefined), 3, { isCursorError, onRestart: () => setSelected(0) });
  const [, rerender] = useState(0);
  useEffect(() => pager.current.subscribe(() => rerender(n => n + 1)), []);
  const reload = useCallback(() => { pager.current.reset(buildRef.current(query.current || undefined)); setSelected(0); }, []);
  const search = useMemo(() => debounce((text: string) => { query.current = text.trim(); reload(); }, SEARCH_DELAY_MS), [reload]);
  const seen = useRef({ deps: true, active: true });
  useEffect(() => { if (seen.current.deps) seen.current.deps = false; else { search.cancel(); reload(); } }, deps);
  useEffect(() => { if (seen.current.active) seen.current.active = false; else if (active) { search.cancel(); reload(); } }, [active]);
  useEffect(() => () => { search.cancel(); pager.current.dispose(); }, [search]);
  const onRange = useCallback((first: number, last: number) => pager.current.ensure(first, last), []);
  const move = useCallback((delta: number) => setSelected(current => Math.max(0, Math.min(pager.current.total - 1, current + delta))), []);
  return { pager: pager.current, selected, setSelected, search, reload, onRange, move, query };
}
