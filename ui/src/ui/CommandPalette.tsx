import { useCommandShortcut } from './useCommandShortcut';
import { Button } from './Button';
// Interaction reference: shadcn/ui Command (MIT), https://21st.dev/@shadcn/components/command; independent Preact implementation.
import { useId, useLayoutEffect, useRef, useState } from 'preact/hooks';
import { Dialog } from '../Dialog';
import { Clock3, CornerDownLeft, Database, FileCode2, Search, Sparkles, Table2 } from 'lucide-preact';
import type { MicroAccessibility } from './accessibility';
import './micro.css';

const groupLabels: Record<string, string> = { Operation: '작업', Connections: '연결' };
export interface CommandItem { id: string; label: string; group?: string; kind?: 'connection' | 'table' | 'query' | 'history'; keywords?: string; disabled?: boolean; onSelect: () => void }
export function filterCommands(items: readonly CommandItem[], query: string) {
  const words = query.normalize('NFC').toLocaleLowerCase().trim().split(/\s+/);
  const recentLabels = new Set<string>();
  return items.filter(item => {
    if (item.kind === 'query' || item.group?.startsWith('Recent ·')) {
      if (recentLabels.has(item.label)) return false;
      recentLabels.add(item.label);
    }
    return words.every(word => `${item.label} ${item.group ?? ''} ${item.keywords ?? ''}`.normalize('NFC').toLocaleLowerCase().includes(word));
  });
}
export interface CommandPaletteProps extends MicroAccessibility { open: boolean; onOpenChange: (open: boolean) => void; items: readonly CommandItem[]; onIntent?: (intent: string) => void }
export function CommandPalette({ open, onOpenChange, items, onIntent, ...accessibility }: CommandPaletteProps) {
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState('');
  const listId = useId();
  const list = useRef<HTMLDivElement>(null);
  const visible = open ? filterCommands(items, query) : [];
  if (open && onIntent && query.trim()) visible.push({id:'ax:ai-intent',label:`AI 계획 후보 · ${query.trim()}`,group:'자연어 → 검토용 계획',onSelect:()=>onIntent(query.trim())});
  const enabled = visible.filter(item => !item.disabled);
  const current = enabled.find(item => item.id === selected) ?? enabled[0];
  const select = (item: CommandItem) => { if (!item.disabled) { onOpenChange(false); item.onSelect(); } };
  useCommandShortcut(open, onOpenChange);
  useLayoutEffect(() => { if (!open) { setQuery(''); setSelected(''); } }, [open]);
  useLayoutEffect(() => { list.current?.querySelector('[aria-selected=true]')?.scrollIntoView({ block: 'nearest' }); }, [current?.id]);
  if (!open) return null;
  return <Dialog title="작업공간 검색" label={accessibility['aria-label'] ?? accessibility.ariaLabel ?? 'Command palette'} onClose={() => onOpenChange(false)}>
    <div className="command-palette micro" data-testid={accessibility['data-testid'] ?? accessibility.testId}>
      <div className="command-palette__search"><Search aria-hidden="true" /><input autoFocus autocorrect="off" autocapitalize="off" spellcheck={false} role="combobox" aria-label="Search commands" aria-expanded="true" aria-autocomplete="list" aria-controls={listId}
        aria-activedescendant={current ? `${listId}-${visible.indexOf(current)}` : undefined} value={query} placeholder="작업 검색 또는 자연어로 의도 입력…"
        onInput={e => { setQuery(e.currentTarget.value); setSelected(''); }} onKeyDown={e => {
          if (e.isComposing) return;
          if (e.key === 'Enter' && current) { e.preventDefault(); select(current); }
          if (!enabled.length || !['ArrowDown', 'ArrowUp'].includes(e.key)) return;
          e.preventDefault();
          const index = enabled.indexOf(current!);
          setSelected(enabled[(index + (e.key === 'ArrowDown' ? 1 : -1) + enabled.length) % enabled.length].id);
        }} /><Button bare type="button" aria-label="Close command palette" onClick={() => onOpenChange(false)}>Esc</Button></div>
      <div ref={list} id={listId} role="listbox" aria-label="Commands" className="command-palette__list">
        {visible.map((item, i) => <div key={item.id} id={`${listId}-${i}`} role="option" aria-selected={current?.id === item.id} aria-disabled={item.disabled || undefined}
          onPointerMove={() => { if (!item.disabled) setSelected(item.id); }} onMouseDown={e => e.preventDefault()} onClick={() => select(item)}>
          {(() => { const Icon = item.kind === 'connection' || item.group === 'Connections' ? Database : item.kind === 'table' || item.id.startsWith('node:') ? Table2 : item.kind === 'history' || item.id === 'history' ? Clock3 : item.kind === 'query' || item.group?.startsWith('Recent ·') ? FileCode2 : item.id.startsWith('ax:') ? Sparkles : CornerDownLeft; return <Icon aria-hidden="true" />; })()}<span>{item.label}</span>{item.group && <small>{groupLabels[item.group] ?? item.group}</small>}
        </div>)}
      </div>
      {!visible.length && <p role="status">일치하는 항목이 없습니다</p>}
      <p className="command-palette__hint"><span><kbd>↑</kbd><kbd>↓</kbd> 선택</span><span><kbd>Enter</kbd> 열기</span><span><kbd>Esc</kbd> 닫기</span></p>
    </div>
  </Dialog>;
}
