import { useCallback, useEffect, useRef, useState } from 'preact/hooks';
import { api } from '../api';
import type { Snippet } from '../contract';
import { HistoryView } from './HistoryView';
import { draftFrom, type LibraryHost, type SnippetDraft } from './snippetActions';
import { SnippetEditor } from './SnippetEditor';
import { SnippetsView } from './SnippetsView';
import './library.css';

export type LibraryTab = 'history' | 'snippets';
export interface LibraryDraft { nonce: number; body: string; profileId: string | null }
export interface LibraryPanelProps { host: LibraryHost; open: boolean; focus: { nonce: number; tab?: LibraryTab }; draft: LibraryDraft | null; onClose: () => void }

/** Execution history + SQL notepad. Loaded on demand; owns no timers, listeners or host subscriptions while idle. */
export default function LibraryPanel({ host, open, focus, draft, onClose }: LibraryPanelProps) {
  const [tab, setTab] = useState<LibraryTab>('history');
  const [editor, setEditor] = useState<SnippetDraft | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const seenDraft = useRef(0);
  useEffect(() => { if (focus.tab) setTab(focus.tab); }, [focus.nonce]);
  const root = useRef<HTMLDivElement>(null);
  const openEditor = useCallback((value: SnippetDraft) => { setTab('snippets'); setEditor(value); }, []);
  useEffect(() => {
    if (!draft || draft.nonce === seenDraft.current) return;
    seenDraft.current = draft.nonce; openEditor({ title: '', body: draft.body, tagsText: '', profile_id: draft.profileId, pinned: false });
  }, [draft?.nonce]);
  const edit = useCallback((snippet: Snippet) => { void api('snippet_get', { id: snippet.id }).then(full => openEditor(draftFrom(full)), host.report); }, [host]);
  const closeEditor = () => { setEditor(null); };
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key !== 'Escape' || e.isComposing) return;
    e.preventDefault(); e.stopPropagation();
    if (editor) closeEditor(); else onClose();
  };
  const saveFromHistory = useCallback((body: string, profileId: string | null) => openEditor({ title: '', body, tagsText: '', profile_id: profileId, pinned: false }), [openEditor]);
  return <div class="lib-panel" ref={root} onKeyDown={onKeyDown}>
    <div class="lib-head">
      <div role="tablist" aria-label="이력과 메모">
        <button type="button" role="tab" id="lib-tab-history" aria-selected={tab === 'history'} aria-controls="lib-pane" onClick={() => setTab('history')}>이력</button>
        <button type="button" role="tab" id="lib-tab-snippets" aria-selected={tab === 'snippets'} aria-controls="lib-pane" onClick={() => setTab('snippets')}>메모</button>
      </div>
      <kbd title="이력 검색">⌘⇧H</kbd>
      <button type="button" class="lib-close" aria-label="이력 패널 닫기" onClick={onClose}>닫기 (Esc)</button>
    </div>
    <div id="lib-pane" role="tabpanel" aria-labelledby={`lib-tab-${tab}`} class="lib-pane">
      {tab === 'history' && <HistoryView host={host} active={open} focusNonce={focus.nonce} onSave={saveFromHistory} />}
      {tab === 'snippets' && <div class="lib-pane-inner" hidden={!!editor}><SnippetsView host={host} active={open && tab === 'snippets' && !editor} focusNonce={focus.nonce} refreshKey={refreshKey} onNew={() => openEditor({ title: '', body: '', tagsText: '', profile_id: null, pinned: false })} onEdit={edit} /></div>}
      {tab === 'snippets' && editor && <SnippetEditor draft={editor} profiles={host.profiles} onCancel={closeEditor} onSaved={saved => { setEditor(null); setRefreshKey(k => k + 1); host.notify(`메모를 저장했습니다 · ${saved.title}`); }} />}
    </div>
  </div>;
}
