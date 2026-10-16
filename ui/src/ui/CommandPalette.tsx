import { Button } from './Button';
// Interaction reference: shadcn/ui Command (MIT), https://21st.dev/@shadcn/components/command; independent Preact implementation.
import { useId, useLayoutEffect, useRef, useState } from 'preact/hooks';
import { Dialog } from '../Dialog';
import type { MicroAccessibility } from './accessibility';
import './micro.css';

export interface CommandItem { id: string; label: string; group?: string; keywords?: string; disabled?: boolean; onSelect: () => void }
export function filterCommands(items: readonly CommandItem[], query: string) {
  const words = query.normalize('NFC').toLocaleLowerCase().trim().split(/\s+/);
  return items.filter(item => words.every(word => `${item.label} ${item.group ?? ''} ${item.keywords ?? ''}`.normalize('NFC').toLocaleLowerCase().includes(word)));
}
export interface CommandPaletteProps extends MicroAccessibility { open: boolean; onOpenChange: (open: boolean) => void; items: readonly CommandItem[] }
export function CommandPalette({ open, onOpenChange, items, ...accessibility }: CommandPaletteProps) {
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState('');
  const listId = useId();
  const list = useRef<HTMLDivElement>(null);
  const visible = open ? filterCommands(items, query) : [];
  const enabled = visible.filter(item => !item.disabled);
  const current = enabled.find(item => item.id === selected) ?? enabled[0];
  const select = (item: CommandItem) => { if (!item.disabled) { onOpenChange(false); item.onSelect(); } };
  useLayoutEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing || event.altKey || !(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== 'k') return;
      if (!open && document.querySelector('dialog[open]')) return;
      event.preventDefault(); onOpenChange(!open);
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, [open, onOpenChange]);
  useLayoutEffect(() => { if (!open) { setQuery(''); setSelected(''); } }, [open]);
  useLayoutEffect(() => { list.current?.querySelector('[aria-selected=true]')?.scrollIntoView({ block: 'nearest' }); }, [current?.id]);
  if (!open) return null;
  return <Dialog title="작업공간 검색" label={accessibility['aria-label'] ?? accessibility.ariaLabel ?? 'Command palette'} onClose={() => onOpenChange(false)}>
    <div className="command-palette micro" data-testid={accessibility['data-testid'] ?? accessibility.testId}>
      <div className="command-palette__search"><input autoFocus role="combobox" aria-label="Search commands" aria-expanded="true" aria-autocomplete="list" aria-controls={listId}
        aria-activedescendant={current ? `${listId}-${visible.indexOf(current)}` : undefined} value={query} placeholder="연결, 테이블, 최근 쿼리 검색…"
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
          <span>{item.label}</span>{item.group && <small>{item.group}</small>}
        </div>)}
      </div>
      {!visible.length && <p role="status">일치하는 항목이 없습니다</p>}
      <p className="command-palette__hint">↑ ↓ 선택 · Enter 열기 · Escape 닫기</p>
    </div>
  </Dialog>;
}
