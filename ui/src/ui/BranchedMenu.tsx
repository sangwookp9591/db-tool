// Based on React Bits BranchedMenu (MIT + Commons Clause, user-provided reference); independent Preact + CSS implementation, no third-party source copied.
// Lines are plain CSS boxes (one trunk + one elbow per row, no SVG paths) so a group of hundreds of rows adds no animated paths.
import type { ComponentChildren } from 'preact';
import { useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';
import './branched-menu.css';

export interface BranchNode { value: string; label: string; icon?: ComponentChildren; children?: BranchNode[] }

const reduced = () => typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;

/** Flip one key in an open-set without mutating the previous one. */
export function toggleOpen(open: ReadonlySet<string>, key: string): Set<string> {
  const next = new Set(open);
  if (!next.delete(key)) next.add(key);
  return next;
}

/** Values from the top-level section down to `value`; empty when `value` is not in the tree. */
export function activePath(items: BranchNode[], value: string): string[] {
  for (const item of items) {
    if (item.value === value) return [item.value];
    const below = item.children ? activePath(item.children, value) : [];
    if (below.length) return [item.value, ...below];
  }
  return [];
}

/** Index of the first sibling on the active path (the row the accent line reaches), or -1. */
export function reachIndex(siblings: string[], path: ReadonlySet<string>): number {
  return siblings.findIndex(key => path.has(key));
}

/** 'pass' rows sit above the reached row, so the trunk is drawn through them; 'to' is the reached row itself. */
export function reachState(index: number, reached: number): 'pass' | 'to' | undefined {
  return reached < 0 || index > reached ? undefined : index === reached ? 'to' : 'pass';
}

export type TreeKeyResult = { type: 'focus'; index: number } | { type: 'open' } | { type: 'close' } | null;
/** Keyboard model for a visible-row list: arrows move, Right opens/enters a group, Left closes/leaves it. */
export function treeKey(key: string, row: { index: number; count: number; hasChildren: boolean; open: boolean; parentIndex: number }): TreeKeyResult {
  const { index, count, hasChildren, open, parentIndex } = row;
  if (!count) return null;
  if (key === 'ArrowDown') return { type: 'focus', index: Math.min(count - 1, index + 1) };
  if (key === 'ArrowUp') return { type: 'focus', index: Math.max(0, index - 1) };
  if (key === 'Home') return { type: 'focus', index: 0 };
  if (key === 'End') return { type: 'focus', index: count - 1 };
  if (key === 'ArrowRight') return hasChildren ? (open ? { type: 'focus', index: Math.min(count - 1, index + 1) } : { type: 'open' }) : null;
  if (key === 'ArrowLeft') return hasChildren && open ? { type: 'close' } : parentIndex >= 0 ? { type: 'focus', index: parentIndex } : null;
  return null;
}

/** Fold wrapper: children stay mounted while the grid-rows animation closes, then unmount. No per-frame JS. */
export function BranchGroup({ open, children, class: className = '', ...rest }: { open: boolean; children: ComponentChildren; class?: string } & Record<string, unknown>) {
  const [mounted, setMounted] = useState(open);
  const [shown, setShown] = useState(open);
  const first = useRef(true);
  useLayoutEffect(() => {
    if (open) { setMounted(true); if (reduced() || first.current) setShown(true); else requestAnimationFrame(() => requestAnimationFrame(() => setShown(true))); return; }
    setShown(false);
    if (reduced()) { setMounted(false); return; }
    const timer = setTimeout(() => setMounted(false), 360);
    return () => clearTimeout(timer);
  }, [open]);
  useEffect(() => { first.current = false; }, []);
  if (!mounted && !open) return null;
  return <div class={`bm-fold ${className}`} data-open={shown && open ? '' : undefined} inert={!open} {...rest}><div class="bm-fold__inner">{children}</div></div>;
}

/** One row plus its optional nested group. `last` shortens the trunk; `reach` paints the accent line. */
export function BranchItem({ last, reach, section, row, nested, class: className = '' }: { last: boolean; reach?: 'pass' | 'to'; section?: boolean; row: ComponentChildren; nested?: ComponentChildren; class?: string }) {
  return <div class={`bm-item ${className}`} data-last={last ? '' : undefined} data-reach={reach} data-section-active={section ? '' : undefined}>
    <span class="bm-trunk" aria-hidden="true" /><span class="bm-elbow" aria-hidden="true" />
    <div class="bm-row">{row}</div>
    {nested}
  </div>;
}

export interface BranchedMenuProps {
  items: BranchNode[]; defaultOpen?: string[]; defaultActive?: string;
  onSelect?: (value: string, item: BranchNode) => void; onToggle?: (value: string, open: boolean) => void;
  /** Accessible name for the navigation landmark. */
  label?: string; class?: string;
}

/** Sections fold, children hang off a trunk with curved branches, the accent line reaches the active row and a marker sits on the rail. */
export function BranchedMenu({ items, defaultOpen = [], defaultActive = '', onSelect, onToggle, label = 'Branched menu', class: className = '' }: BranchedMenuProps) {
  const [open, setOpen] = useState<Set<string>>(() => new Set(defaultOpen));
  const [active, setActive] = useState(defaultActive);
  const path = new Set(activePath(items, active));
  const toggle = (item: BranchNode) => { const next = toggleOpen(open, item.value); setOpen(next); onToggle?.(item.value, next.has(item.value)); };
  const choose = (item: BranchNode) => { if (item.children) { toggle(item); return; } setActive(item.value); onSelect?.(item.value, item); };
  const rows = (nodes: BranchNode[], depth: number): ComponentChildren => {
    const reached = reachIndex(nodes.map(node => node.value), path);
    return nodes.map((node, index) => <BranchItem key={node.value} last={index === nodes.length - 1} reach={depth ? reachState(index, reached) : undefined} section={!depth && path.has(node.value) && open.has(node.value)}
      row={<button type="button" class="bm-button" data-depth={depth} data-active={node.value === active ? '' : undefined} aria-expanded={node.children ? open.has(node.value) : undefined} aria-current={node.value === active ? 'true' : undefined} onClick={() => choose(node)}>
        {node.icon ? <span class="bm-icon" aria-hidden="true">{node.icon}</span> : null}<span class="bm-label">{node.label}</span>
      </button>}
      nested={node.children ? <BranchGroup open={open.has(node.value)}>{rows(node.children, depth + 1)}</BranchGroup> : undefined} />);
  };
  return <nav class={`branched-menu ${className}`} aria-label={label}><div class="bm-root">{rows(items, 0)}</div></nav>;
}
