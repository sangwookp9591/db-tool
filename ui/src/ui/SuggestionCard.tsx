// 사용자 제공 레퍼런스 재구현
import { useId, useLayoutEffect, useRef, useState } from 'preact/hooks';
import { Check, ChevronDown, Sparkles, Undo2 } from 'lucide-preact';
import type { MicroAccessibility } from './accessibility';
import { Button } from './Button';
import './micro-extra.css';

export interface Suggestion {
  id: string;
  title: string;
  query: string;
  reason: string;
  signal: 0 | 1 | 2 | 3;
  evidence: string;
}
export interface SuggestionCardProps extends MicroAccessibility {
  suggestions: Suggestion[];
  acceptedId?: string;
  onAccept: (suggestion: Suggestion) => void;
  onUndo: (suggestion: Suggestion) => void;
  disabled?: boolean;
  onValidate?: (suggestion: Suggestion) => void;
  validating?: boolean;
}
function Signal({ suggestion, focusable = true }: { suggestion: Suggestion; focusable?: boolean }) {
  return <span className="suggestion-signal" data-level={suggestion.signal} role="img" tabIndex={focusable ? 0 : undefined} aria-label={`검사 ${suggestion.signal}/3: ${suggestion.evidence}`} title={suggestion.evidence}>
    {[1, 2, 3].map(level => <i key={level} data-lit={level <= suggestion.signal} />)}
  </span>;
}
export function SuggestionCard({ suggestions, acceptedId, onAccept, onUndo, disabled, onValidate, validating = false, ...accessibility }: SuggestionCardProps) {
  const [selectedId, setSelectedId] = useState(suggestions[0]?.id);
  const [open, setOpen] = useState(true);
  const [menu, setMenu] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const id = useId();
  const selected = suggestions.find(item => item.id === (acceptedId ?? selectedId)) ?? suggestions[0];
  const accepted = suggestions.find(item => item.id === acceptedId);
  useLayoutEffect(() => {
    if (!menu) return;
    root.current?.querySelector<HTMLButtonElement>('[role="menuitemradio"][aria-checked="true"]')?.focus();
    const outside = (event: PointerEvent) => { if (!root.current?.contains(event.target as Node)) setMenu(false); };
    document.addEventListener('pointerdown', outside);
    return () => document.removeEventListener('pointerdown', outside);
  }, [menu]);
  if (!selected) return null;
  return <article className="suggestion-card" aria-label={accessibility['aria-label'] ?? accessibility.ariaLabel ?? 'Query suggestion'} data-testid={accessibility['data-testid'] ?? accessibility.testId}>
    <div className="suggestion-card__surface">
      <Button bare type="button" className="suggestion-card__header" aria-expanded={open} aria-controls={`${id}-body`} onClick={() => setOpen(!open)}>
        <Sparkles size={16} aria-hidden="true" /><strong>{selected.title}</strong>{selected.id !== suggestions[0].id && <span className="ax-pill">대안</span>}<ChevronDown size={16} className="ax-chevron" data-open={open} aria-hidden="true" />
      </Button>
      <div id={`${id}-body`} hidden={!open}>
        <div key={`${selected.id}-${selected.reason}`} className="suggestion-card__body ax-reveal"><p>{selected.reason}</p><pre>{selected.query}</pre></div>
      </div>
      <footer className="suggestion-card__footer">
        <Signal suggestion={selected} />
        <div ref={root} className="suggestion-card__alternatives" onKeyDown={event => {
          if (event.key === 'Escape' && menu) { event.preventDefault(); setMenu(false); trigger.current?.focus(); }
          if (!menu || !['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
          event.preventDefault();
          const items = [...root.current!.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')];
          const index = items.indexOf(document.activeElement as HTMLButtonElement);
          items[event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (index + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length]?.focus();
        }} onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget as Node)) setMenu(false); }}>
          <Button bare ref={trigger} type="button" disabled={disabled || !!accepted} aria-haspopup="menu" aria-expanded={menu} aria-controls={`${id}-menu`} onClick={() => setMenu(!menu)}>대안 선택 <ChevronDown size={14} aria-hidden="true" /></Button>
          {menu && <div id={`${id}-menu`} role="menu" aria-label="Query alternatives" className="suggestion-card__menu">
            {suggestions.map((item, index) => <Button bare key={item.id} type="button" role="menuitemradio" aria-checked={selected.id === item.id} onClick={() => { setSelectedId(item.id); setMenu(false); trigger.current?.focus(); }}><span>{item.title}{index === 0 && <small>추천</small>}</span><Signal suggestion={item} focusable={false} /></Button>)}
          </div>}
        </div>
        {onValidate && <Button loading={validating} disabled={disabled} onClick={() => onValidate(selected)}>검증</Button>}
        {accepted ? <><span className="suggestion-card__accepted" role="status"><Check size={14} aria-hidden="true" />{accepted.title} 삽입됨</span><Button icon={<Undo2 size={14} />} disabled={disabled} onClick={() => onUndo(accepted)}>실행 취소</Button></> : <Button variant="primary" disabled={disabled} onClick={() => onAccept(selected)}>새 탭에 삽입</Button>}
      </footer>
    </div>
  </article>;
}
