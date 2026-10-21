import { memo } from 'preact/compat';
import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'preact/hooks';
import { api } from '../api';
import type { HistoryEntry } from '../contract';
import { debounce } from './debounce';
import { Marked, describeError, fullWhen, when } from './parts';
import type { PageFetcher } from './pager';
import { PERIOD_OPTIONS, periodRange, type Period } from './period';
import { VirtualList } from './VirtualList';
import { usePagedList } from './usePagedList';
import type { LibraryHost } from './snippetActions';
import { Select, type SelectOption } from '../ui/Select';

type Item = HistoryEntry & { id: number; highlight?: string };
type Status = 'all' | 'ok' | 'error';
type Kind = 'all' | 'execute' | 'preview';
export interface HistoryViewProps { host: LibraryHost; active: boolean; focusNonce: number; onSave: (text: string, profileId: string | null) => void }

interface RowProps { uid: string; index: number; total: number; item: Item | undefined; active: boolean; sentinel: boolean; onPick: (index: number) => void; onUse: (index: number) => void }
const Row = memo(function Row({ uid, index, total, item, active, sentinel, onPick, onUse }: RowProps) {
  if (!item) return <div class="lib-row lib-placeholder" role="option" id={`${uid}-${index}`} aria-selected="false" aria-busy="true" aria-posinset={index + 1} aria-setsize={total}>{sentinel ? '불러오는 중…' : ''}</div>;
  return <div class="lib-row lib-history-row" role="option" id={`${uid}-${index}`} aria-selected={active} data-active={active || undefined} data-error={item.error ? '' : undefined} aria-posinset={index + 1} aria-setsize={total}
    onMouseDown={e => e.preventDefault()} onClick={() => onPick(index)} onDblClick={() => onUse(index)}>
    <span class="lib-dot" aria-label={item.error ? '오류' : '성공'} />
    <time>{when(item.timestamp)}</time>
    <code><Marked highlight={item.highlight} text={item.text} /></code>
    <small class="lib-badge">{item.error ? '오류' : item.kind === 'preview' ? '미리보기' : `${item.row_count.toLocaleString()}행 · ${item.elapsed_ms.toLocaleString()}ms`}</small>
  </div>;
});

export function HistoryView({ host, active, focusNonce, onSave }: HistoryViewProps) {
  const uid = useId();
  const input = useRef<HTMLInputElement>(null);
  const [profile, setProfile] = useState(''), [status, setStatus] = useState<Status>('all'), [kind, setKind] = useState<Kind>('all'), [period, setPeriod] = useState<Period>('all');
  const profileOptions = useMemo<SelectOption[]>(() => [{ value: '', label: '모든 연결' }, ...host.profiles.map(p => ({ value: p.id, label: p.name }))], [host.profiles]);
  const statusOptions = useMemo<SelectOption[]>(() => [{ value: 'all', label: '성공·오류' }, { value: 'ok', label: '성공만' }, { value: 'error', label: '오류만' }], []);
  const kindOptions = useMemo<SelectOption[]>(() => [{ value: 'all', label: '실행·미리보기' }, { value: 'execute', label: '실행' }, { value: 'preview', label: '미리보기' }], []);
  const periodOptions = useMemo<SelectOption[]>(() => PERIOD_OPTIONS.map(o => ({ ...o })), []);
  const [confirm, setConfirm] = useState(false);
  /** The period is resolved to since/until here, when a query starts, and then stays fixed for every page of that query. */
  const build = useCallback((query: string | undefined): PageFetcher<Item> => {
    const range = periodRange(period);
    return cursor => api('history_search', { query, profile_id: profile || undefined, status, kind: kind === 'all' ? undefined : kind, ...range, before_id: typeof cursor === 'number' ? cursor : undefined, limit: 100 }) as Promise<{ items: Item[]; next_cursor: number | null }>;
  }, [profile, status, kind, period]);
  const list = usePagedList<Item>(build, [profile, status, kind, period], active);
  const { pager, selected, setSelected } = list;
  const current = pager.get(selected);
  const [full, setFull] = useState<{ id: number; text: string } | null>(null);
  const loadFull = useMemo(() => debounce((id: number) => { void api('history_get', { id }).then(entry => setFull(f => f?.id === id ? f : { id, text: entry.text }), () => {}); }, 120), []);
  useEffect(() => () => loadFull.cancel(), [loadFull]);
  useEffect(() => { setConfirm(false); if (current) loadFull(current.id); }, [current?.id]);
  useEffect(() => { if (active) input.current?.focus(); }, [active, focusNonce]);
  const text = current && full?.id === current.id ? full.text : current?.text ?? '';

  const fresh = async (item: Item) => (await api('history_get', { id: item.id })).text;
  const hint = (item: Item) => ({ profileId: item.profile_id, profileName: item.profile_name });
  const live = useRef(host); live.current = host;
  const use = useCallback((index: number, mode: 'insert' | 'tab' = 'insert') => {
    const item = pager.get(index); if (!item) return;
    void fresh(item).then(sql => mode === 'insert' ? live.current.insert(sql, hint(item)) : live.current.openTab(sql, hint(item))).catch(e => live.current.report(e));
  }, [pager]);
  const pick = useCallback((index: number) => setSelected(index), []);
  const useRow = useCallback((index: number) => use(index), [use]);
  const copy = (item: Item) => void fresh(item).then(sql => navigator.clipboard.writeText(sql)).then(() => host.notify('쿼리를 복사했습니다')).catch(host.report);
  const remove = (item: Item) => void api('history_delete', { id: item.id }).then(() => { pager.removeAt(selected); setConfirm(false); host.notify('이력을 삭제했습니다'); }).catch(host.report);

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.isComposing || e.target !== input.current) return;
    const step = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : e.key === 'PageDown' ? 10 : e.key === 'PageUp' ? -10 : 0;
    if (step) { e.preventDefault(); list.move(step); }
    else if (e.key === 'Enter') { e.preventDefault(); list.search.flush(); use(selected, e.shiftKey ? 'tab' : 'insert'); }
  };
  const count = pager.count;
  return <div class="lib-view" onKeyDown={onKeyDown}>
    <div class="lib-filters">
      <input ref={input} class="lib-search" type="text" role="combobox" aria-expanded="true" aria-controls={`${uid}-list`} aria-activedescendant={current ? `${uid}-${selected}` : undefined} aria-label="이력 검색" placeholder="SQL 검색 (Enter: 편집기에 넣기 · Shift+Enter: 새 탭)" autocomplete="off" spellcheck={false}
        onInput={e => list.search(e.currentTarget.value)} />
      <Select class="lib-filter-select lib-filter-profile" size="sm" ariaLabel="연결 필터" value={profile} options={profileOptions} onChange={value => setProfile(value)} />
      <Select class="lib-filter-select lib-filter-status" size="sm" ariaLabel="결과 필터" value={status} options={statusOptions} onChange={value => setStatus(value as Status)} />
      <Select class="lib-filter-select lib-filter-kind" size="sm" ariaLabel="종류 필터" value={kind} options={kindOptions} onChange={value => setKind(value as Kind)} />
      <Select class="lib-filter-select lib-filter-period" size="sm" ariaLabel="기간 필터" value={period} options={periodOptions} onChange={value => setPeriod(value as Period)} />
    </div>
    <div class="lib-body">
      <div class="lib-list">
        {pager.error != null && <p class="lib-error-line" role="alert">{describeError(pager.error)} <button type="button" onClick={() => pager.retry()}>다시 시도</button></p>}
        {pager.isEmpty && !pager.busy && <p class="lib-empty">{list.query.current || profile || status !== 'all' || kind !== 'all' || period !== 'all' ? '조건에 맞는 이력이 없습니다.' : '실행 이력이 없습니다.'}</p>}
        <VirtualList id={`${uid}-list`} label="실행 이력" count={count} selected={selected} onRange={list.onRange} renderRow={i => <Row key={i} uid={uid} index={i} total={count} item={pager.get(i)} active={i === selected} sentinel={i === pager.total} onPick={pick} onUse={useRow} />} />
      </div>
      <aside class="lib-detail" aria-label="선택한 이력">
        {current ? <>
          <div class="lib-meta"><span title={fullWhen(current.timestamp)}>{when(current.timestamp)}</span><span>{current.profile_name}</span><span>{current.source === 'agent' ? '에이전트' : 'UI'}</span>{current.kind === 'preview' && <span>미리보기</span>}<span class={current.error ? 'lib-bad' : 'lib-good'}>{current.error ? '오류' : `${current.row_count.toLocaleString()}행 · ${current.elapsed_ms.toLocaleString()}ms`}</span></div>
          <pre class="lib-sql" tabIndex={0} aria-label="전체 SQL">{text}</pre>
          <pre class="lib-error-text" hidden={!current.error}>{current.error ?? ''}</pre>
          <div class="lib-actions" role="group" aria-label="이력 작업">{confirm
            ? <><span>이 이력을 삭제할까요?</span><button type="button" class="danger" onClick={() => remove(current)}>삭제</button><button type="button" onClick={() => setConfirm(false)}>취소</button></>
            : <><button type="button" onClick={() => use(selected)}>편집기에 넣기</button><button type="button" onClick={() => use(selected, 'tab')}>새 탭에서 열기</button><button type="button" onClick={() => copy(current)}>복사</button><button type="button" onClick={() => void fresh(current).then(sql => onSave(sql, current.profile_id ?? null)).catch(host.report)}>메모로 저장</button><button type="button" class="danger" onClick={() => setConfirm(true)}>삭제</button></>}</div>
        </> : <p class="lib-empty">이력을 선택하면 전체 SQL이 여기에 표시됩니다.</p>}
      </aside>
    </div>
  </div>;
}
