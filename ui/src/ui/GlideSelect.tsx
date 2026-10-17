// Based on React Bits (MIT + Commons Clause); independent Preact implementation.
import type { ComponentChildren } from 'preact';
import { createPortal } from 'preact/compat';
import { useEffect, useId, useLayoutEffect, useRef, useState } from 'preact/hooks';
import { Check, ChevronDown } from 'lucide-preact';
import './glide-select.css';

export interface GlideSelectOption { value: string; label: ComponentChildren; tag?: string }
export interface GlideSelectProps {
  options: readonly (string | GlideSelectOption)[]; value?: string; defaultValue?: string;
  onChange?: (value: string, option: GlideSelectOption) => void; placeholder?: string; showTags?: boolean;
  size?: 'sm' | 'md' | 'lg'; disabled?: boolean; ariaLabel: string; name?: string; required?: boolean;
  class?: string; className?: string;
  /** 메뉴 폭을 트리거 폭에 맞춘다(기본은 최소 176px). 좁은 팝오버 안에서 사용. */
  fitTrigger?: boolean;
}
const normalize = (items: GlideSelectProps['options']) => items.map(item => typeof item === 'string' ? { value: item, label: item } : item);
const labelOf = (item: GlideSelectOption) => typeof item.label === 'string' ? item.label : item.value;
export function glideNavigation(key: string, active: number, selected: number, length: number): number | undefined {
  if (!length) return undefined;
  if (key === 'Home') return 0;
  if (key === 'End') return length - 1;
  if (key === 'ArrowDown' || key === 'ArrowUp') return Math.max(0, Math.min(length - 1, (active < 0 ? Math.max(0, selected) : active) + (key === 'ArrowDown' ? 1 : -1)));
  return undefined;
}
export function glideTypeahead(items: readonly GlideSelectOption[], from: number, character: string) {
  if (!items.length) return -1;
  for (let offset = 1; offset <= items.length; offset++) { const index = (from + offset + items.length) % items.length; if (labelOf(items[index]).toLocaleLowerCase().startsWith(character.toLocaleLowerCase())) return index; }
  return from;
}
export const glideValue = (controlled: string | undefined, internal: string) => controlled ?? internal;
export const glideFormValue = (current: string) => current;

export function GlideSelect({ options, value, defaultValue = '', onChange, placeholder = '선택…', showTags = true, size = 'md', disabled, ariaLabel, name, required, class: className, className: legacyClass, fitTrigger }: GlideSelectProps) {
  const items = normalize(options), id = useId();
  const [inner, setInner] = useState(defaultValue);
  const current = glideValue(value, inner), selected = items.findIndex(item => item.value === current);
  const [open, setOpen] = useState(false), [active, setActive] = useState(-1), [side, setSide] = useState<'top'|'bottom'>('bottom');
  const trigger = useRef<HTMLButtonElement>(null), menu = useRef<HTMLDivElement>(null);
  const [rect, setRect] = useState<DOMRect | null>(null);
  const refresh = () => {
    const el = trigger.current; if (!el) return;
    const r = el.getBoundingClientRect(); setRect(r);
    const height = Math.min(items.length * rowHeight(size) + 8, innerHeight - 16);
    const dialog = el.closest('dialog[open]');
    const dialogHeader = dialog?.querySelector<HTMLElement>('.dialog-header')?.getBoundingClientRect().bottom ?? 0;
    const dialogFooter = dialog?.querySelector<HTMLElement>('.dialog-footer')?.getBoundingClientRect().top ?? innerHeight;
    const topEdge = Math.max(8, dialogHeader), bottomEdge = Math.min(innerHeight - 8, dialogFooter);
    const below = bottomEdge - r.bottom, above = r.top - topEdge;
    setSide(below < height + 6 && above > below ? 'top' : 'bottom');
  };
  useLayoutEffect(() => { if (open) refresh(); }, [open, size, items.length]);
  useEffect(() => {
    if (!open) return;
    const reposition = () => refresh();
    const dismiss = (event: PointerEvent) => { if (trigger.current?.contains(event.target as Node) || menu.current?.contains(event.target as Node)) return; setOpen(false); };
    window.addEventListener('resize', reposition); window.addEventListener('scroll', reposition, true); document.addEventListener('pointerdown', dismiss, true);
    return () => { window.removeEventListener('resize', reposition); window.removeEventListener('scroll', reposition, true); document.removeEventListener('pointerdown', dismiss, true); };
  }, [open]);
  const choose = (index: number) => { const item = items[index]; if (!item) return; if (value === undefined) setInner(item.value); onChange?.(item.value, item); setOpen(false); trigger.current?.focus({ preventScroll: true }); };
  const openMenu = (keyboard = false) => { if (disabled) return; refresh(); setActive(selected >= 0 ? selected : keyboard ? 0 : -1); setOpen(true); };
  const keyDown = (event: KeyboardEvent) => {
    if (!open) { if (['Enter', ' ', 'ArrowDown', 'ArrowUp'].includes(event.key)) { event.preventDefault(); openMenu(true); } return; }
    if (event.key === 'Escape' || event.key === 'Tab') { if (event.key === 'Escape') event.preventDefault(); setOpen(false); return; }
    if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); choose(active >= 0 ? active : selected); return; }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp' || event.key === 'Home' || event.key === 'End') {
      event.preventDefault(); const next = glideNavigation(event.key, active, selected, items.length); if (next !== undefined) setActive(next); return;
    }
    if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
      setActive(glideTypeahead(items, Math.max(active, selected, -1), event.key));
    }
  };
  const menuWidth = rect ? (fitTrigger ? rect.width : Math.max(rect.width, 176)) : 0;
  const menuStyle = rect ? { position: 'fixed' as const, left: Math.max(8, Math.min(rect.left, innerWidth - menuWidth - 8)), width: menuWidth, '--gs-row': `${rowHeight(size)}px`, ...(side === 'bottom' ? { top: rect.bottom + 6 } : { bottom: innerHeight - rect.top + 6 }) } : {};
  const portalHost = trigger.current?.closest('dialog[open]') ?? (typeof document !== 'undefined' ? document.body : null);
  const menuNode = open && rect && portalHost ? createPortal(<div ref={menu} class="glide-select__menu" style={menuStyle} data-side={side}>
    <div class="glide-select__list" role="listbox" aria-label={ariaLabel} id={`${id}-list`} onPointerDown={e => { if (e.button === 0) { (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); setActive(Math.floor((e.clientY - r.top - 4) / (rowHeight(size) + 1))); } }} onPointerMove={e => { if ((e.currentTarget as HTMLElement).hasPointerCapture(e.pointerId)) { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); const i = Math.floor((e.clientY - r.top - 4) / (rowHeight(size) + 1)); setActive(i >= 0 && i < items.length ? i : -1); } }} onPointerUp={e => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); const i = Math.floor((e.clientY - r.top - 4) / (rowHeight(size) + 1)); if (i >= 0 && i < items.length) choose(i); }} onPointerOver={e => { const row = (e.target as HTMLElement).closest<HTMLElement>('[data-index]'); if (row) setActive(Number(row.dataset.index)); }}>
      <span class="glide-select__pill" style={{ transform: `translateY(${Math.max(0, active) * (rowHeight(size) + 1)}px)`, opacity: active >= 0 ? 1 : 0 }} />
      {items.map((item, index) => <div id={`${id}-${index}`} key={item.value} role="option" aria-selected={index === selected} data-index={index} class="glide-select__option" onClick={() => choose(index)}><span class="glide-select__name">{item.label}</span>{showTags && item.tag && <span class="glide-select__tag">{item.tag}</span>}<Check class="glide-select__check" data-on={index === selected ? '' : undefined} aria-hidden="true" /></div>)}
    </div>
  </div>, portalHost) : null;
  return <span class={`glide-select ${className ?? legacyClass ?? ''}`} data-size={size} data-disabled={disabled || undefined}>
    <button ref={trigger} type="button" role="combobox" aria-label={ariaLabel} aria-haspopup="listbox" aria-expanded={open} aria-controls={`${id}-list`} aria-activedescendant={active >= 0 ? `${id}-${active}` : undefined} disabled={disabled} class="glide-select__trigger" onClick={() => open ? setOpen(false) : openMenu()} onKeyDown={keyDown}><span class="glide-select__label">{selected >= 0 ? items[selected].label : placeholder}</span><ChevronDown class="glide-select__chevron" aria-hidden="true" /></button>
    {name && <input class="glide-select__form-value" type="text" tabIndex={-1} aria-hidden="true" name={name} required={required} value={glideFormValue(current)} />}
    {menuNode}
  </span>;
}
function rowHeight(size: GlideSelectProps['size']) { return size === 'sm' ? 26 : size === 'lg' ? 40 : 30; }
