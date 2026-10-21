import { useEffect, useRef, useState } from 'preact/hooks';
import { api } from '../api';
import type { Snippet } from '../contract';
import { describeError } from './parts';
import { toSaveArgs, type SnippetDraft } from './snippetActions';
import { useMemo } from 'preact/hooks';
import { Select, type SelectOption } from '../ui/Select';

export interface SnippetEditorProps { draft: SnippetDraft; profiles: readonly { id: string; name: string }[]; onSaved: (snippet: Snippet) => void; onCancel: () => void }
export function SnippetEditor({ draft, profiles, onSaved, onCancel }: SnippetEditorProps) {
  const [value, setValue] = useState(draft);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const title = useRef<HTMLInputElement>(null);
  const profileOptions = useMemo<SelectOption[]>(() => [{ value: '', label: '모든 연결' }, ...profiles.map(p => ({ value: p.id, label: p.name }))], [profiles]);
  useEffect(() => { title.current?.focus(); }, []);
  const set = <K extends keyof SnippetDraft>(key: K, next: SnippetDraft[K]) => setValue(v => ({ ...v, [key]: next }));
  const save = async () => {
    if (saving) return;
    let args; try { args = toSaveArgs(value); } catch (e) { setError(describeError(e)); return; }
    setSaving(true); setError(null);
    try { onSaved(await api('snippet_save', args)); } catch (e) { setError(describeError(e)); setSaving(false); }
  };
  return <form class="lib-editor" aria-label={value.id === undefined ? '새 메모' : '메모 편집'} onSubmit={e => { e.preventDefault(); void save(); }}
    onKeyDown={e => { if (!e.isComposing && e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); void save(); } }}>
    <div class="lib-editor-fields">
      <input ref={title} aria-label="제목" placeholder="제목 (비우면 첫 줄)" maxLength={80} value={value.title} onInput={e => set('title', e.currentTarget.value)} />
      <input aria-label="태그" placeholder="태그 (쉼표로 구분)" value={value.tagsText} onInput={e => set('tagsText', e.currentTarget.value)} />
      <Select class="lib-editor-profile" size="sm" ariaLabel="연결 범위" value={value.profile_id ?? ''} options={profileOptions} onChange={next => set('profile_id', next || null)} />
      <label class="lib-check"><input type="checkbox" checked={value.pinned} onChange={e => set('pinned', e.currentTarget.checked)} />고정</label>
    </div>
    <textarea class="lib-body-input" aria-label="SQL" spellcheck={false} placeholder="SQL" value={value.body} onInput={e => set('body', e.currentTarget.value)} />
    <div class="lib-actions"><button type="submit" class="primary" disabled={saving}>저장 (⌘/Ctrl+Enter)</button><button type="button" onClick={onCancel}>취소 (Esc)</button><span class="lib-error-line" role="alert" hidden={!error}>{error}</span></div>
  </form>;
}
