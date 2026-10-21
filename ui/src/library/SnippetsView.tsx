import { memo } from 'preact/compat';
import { useCallback, useEffect, useId, useRef, useState } from 'preact/hooks';
import { api } from '../api';
import type { Snippet } from '../contract';
import { Marked, describeError, when } from './parts';
import type { PageFetcher } from './pager';
import { applySnippet, isCursorError, type LibraryHost } from './snippetActions';
import { usePagedList } from './usePagedList';
import { VirtualList } from './VirtualList';
import { useMemo } from 'preact/hooks';
import { Select, type SelectOption } from '../ui/Select';

const PinIcon = () => <svg viewBox="0 0 16 16" width="11" height="11" aria-hidden="true" focusable="false"><path fill="currentColor" d="M10.5 1.5 14.5 5.5l-1.4.4-2.3 2.3.2 3.2-1 1L6.6 9.6 3 13.2 2.8 13l3.6-3.6L2.9 6.8l1-1 3.2.2 2.3-2.3.4-1.4z" /></svg>;
export interface SnippetsViewProps { host: LibraryHost; active: boolean; focusNonce: number; refreshKey: number; onNew: () => void; onEdit: (snippet: Snippet) => void }
interface RowProps { uid: string; index: number; total: number; item: Snippet | undefined; active: boolean; sentinel: boolean; divider: boolean; onPick: (index: number) => void; onUse: (index: number) => void }
const Row = memo(function Row({ uid, index, total, item, active, sentinel, divider, onPick, onUse }: RowProps) {
  if (!item) return <div class="lib-row lib-placeholder" role="option" id={`${uid}-${index}`} aria-selected="false" aria-busy="true" aria-posinset={index + 1} aria-setsize={total}>{sentinel ? '불러오는 중…' : ''}</div>;
  return <div class="lib-row lib-snippet-row" role="option" id={`${uid}-${index}`} aria-selected={active} data-active={active || undefined} data-divider={divider || undefined} data-pinned={item.pinned || undefined} aria-posinset={index + 1} aria-setsize={total}
    onMouseDown={e => e.preventDefault()} onClick={() => onPick(index)} onDblClick={() => onUse(index)}>
    <span class="lib-pin" role={item.pinned ? 'img' : undefined} aria-label={item.pinned ? '고정됨' : undefined}>{item.pinned && <PinIcon />}</span>
    <strong>{item.title}</strong>
    <code><Marked highlight={item.highlight} text={item.body} /></code>
    <small class="lib-badge">{item.tags.slice(0, 2).map(t => `#${t}`).join(' ')}</small>
  </div>;
});

export function SnippetsView({ host, active, focusNonce, refreshKey, onNew, onEdit }: SnippetsViewProps) {
  const uid = useId();
  const input = useRef<HTMLInputElement>(null);
  const [profile, setProfile] = useState(''), [tag, setTag] = useState('');
  const profileOptions = useMemo<SelectOption[]>(() => [{ value: '', label: '모든 연결' }, ...host.profiles.map(p => ({ value: p.id, label: p.name }))], [host.profiles]);
  const [confirm, setConfirm] = useState(false);
  const build = useCallback((query: string | undefined): PageFetcher<Snippet> => cursor =>
    api('snippet_list', { query, tag: tag || undefined, profile_id: profile || undefined, before_id: typeof cursor === 'string' ? cursor : undefined, limit: 100 }), [profile, tag]);
  const list = usePagedList<Snippet>(build, [profile, tag, refreshKey], active, isCursorError);
  const { pager, selected, setSelected } = list;
  const current = pager.get(selected);
  useEffect(() => { setConfirm(false); }, [current?.id]);
  useEffect(() => { if (active) input.current?.focus(); }, [active, focusNonce]);
  const live = useRef(host); live.current = host;
  const use = useCallback((index: number, mode: 'insert' | 'tab' = 'insert') => {
    const item = pager.get(index); if (!item) return;
    void applySnippet(item, mode, live.current, { get: id => api('snippet_get', { id }) }).catch(e => live.current.report(e));
  }, [pager]);
  const pick = useCallback((index: number) => setSelected(index), []);
  const useRow = useCallback((index: number) => use(index), [use]);
  const full = (item: Snippet) => api('snippet_get', { id: item.id });
  /** Flips the flag in place first (no row moves, so nothing shifts or flickers); the server's pinned-first order is picked up when the list next loads from its first page. */
  const togglePin = (item: Snippet) => {
    const pinned = !item.pinned, index = selected;
    const set = (value: boolean, stamp?: string) => { if (pager.get(index)?.id === item.id) pager.update(index, s => ({ ...s, pinned: value, updated_at: stamp ?? s.updated_at })); };
    set(pinned);
    void full(item).then(s => api('snippet_save', { id: s.id, title: s.title, body: s.body, tags: s.tags, profile_id: s.profile_id, language: s.language, pinned })).then(saved => set(saved.pinned, saved.updated_at), e => { set(item.pinned); host.report(e); });
  };
  const remove = (item: Snippet) => void api('snippet_delete', { id: item.id }).then(() => { pager.removeAt(selected); setConfirm(false); host.notify('메모를 삭제했습니다'); }).catch(host.report);

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.isComposing || e.target !== input.current) return;
    const step = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : e.key === 'PageDown' ? 10 : e.key === 'PageUp' ? -10 : 0;
    if (step) { e.preventDefault(); list.move(step); }
    else if (e.key === 'Enter') { e.preventDefault(); list.search.flush(); use(selected, e.shiftKey ? 'tab' : 'insert'); }
  };
  const count = pager.count;
  return <div class="lib-view" onKeyDown={onKeyDown}>
    <div class="lib-filters">
      <input ref={input} class="lib-search" type="text" role="combobox" aria-expanded="true" aria-controls={`${uid}-list`} aria-activedescendant={current ? `${uid}-${selected}` : undefined} aria-label="메모 검색" placeholder="제목·SQL·태그 검색 (Enter: 편집기에 넣기)" autocomplete="off" spellcheck={false}
        onInput={e => list.search(e.currentTarget.value)} />
      <Select class="lib-filter-select lib-filter-profile" size="sm" ariaLabel="연결 필터" value={profile} options={profileOptions} onChange={value => setProfile(value)} />
      <input class="lib-tag" aria-label="태그 필터" placeholder="태그" value={tag} onInput={e => setTag(e.currentTarget.value.replace(/^#/, '').trim())} />
      <button type="button" class="lib-new" onClick={onNew}>새 메모</button>
    </div>
    <div class="lib-body">
      <div class="lib-list">
        {pager.error != null && <p class="lib-error-line" role="alert">{describeError(pager.error)} <button type="button" onClick={() => pager.retry()}>다시 시도</button></p>}
        {pager.isEmpty && !pager.busy && <p class="lib-empty">{list.query.current || profile || tag ? '조건에 맞는 메모가 없습니다.' : '저장한 메모가 없습니다. 이력에서 “메모로 저장”하거나 새 메모를 만드세요.'}</p>}
        <VirtualList id={`${uid}-list`} label="SQL 메모" count={count} selected={selected} onRange={list.onRange} renderRow={i => <Row key={i} uid={uid} index={i} total={count} item={pager.get(i)} active={i === selected} sentinel={i === pager.total} divider={i > 0 && pager.get(i - 1)?.pinned === true && pager.get(i)?.pinned === false} onPick={pick} onUse={useRow} />} />
      </div>
      <aside class="lib-detail" aria-label="선택한 메모">
        {current ? <>
          <div class="lib-meta"><strong>{current.title}</strong><span>{when(current.updated_at)}</span>{current.tags.map(t => <span key={t}>#{t}</span>)}</div>
          <pre class="lib-sql" tabIndex={0} aria-label="메모 SQL">{current.body}</pre>
          <div class="lib-actions" role="group" aria-label="메모 작업">{confirm
            ? <><span>이 메모를 삭제할까요?</span><button type="button" class="danger" onClick={() => remove(current)}>삭제</button><button type="button" onClick={() => setConfirm(false)}>취소</button></>
            : <><button type="button" onClick={() => use(selected)}>편집기에 넣기</button><button type="button" onClick={() => use(selected, 'tab')}>새 탭에서 열기</button><button type="button" onClick={() => onEdit(current)}>편집</button><button type="button" aria-pressed={current.pinned} onClick={() => togglePin(current)}>{current.pinned ? '고정 해제' : '고정'}</button><button type="button" class="danger" onClick={() => setConfirm(true)}>삭제</button></>}</div>
        </> : <p class="lib-empty">메모를 선택하면 SQL이 여기에 표시됩니다.</p>}
      </aside>
    </div>
  </div>;
}
