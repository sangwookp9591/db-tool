import { Button } from './Button';
// User-provided Integration Beam reference: docs/design-shots/ref-integration-beam.md and .png; independent Preact implementation.
import type { ComponentChildren, Ref } from 'preact';
import { useImperativeHandle, useLayoutEffect, useRef, useState } from 'preact/hooks';
import type { MicroAccessibility } from './accessibility';
import { reducedMotion } from './spring';
import './micro.css';

export type BeamTone = 'primary' | 'success' | 'warning';
export interface BeamNode { id: string; label: string; side: 'source' | 'target'; state: 'connected' | 'disconnected' | 'pending' | 'denied'; icon?: ComponentChildren }
export interface IntegrationBeamHandle { pulse: (fromId: string, toId: string, tone?: BeamTone) => boolean }
export interface IntegrationBeamProps extends MicroAccessibility { nodes: readonly BeamNode[]; controllerRef: Ref<IntegrationBeamHandle>; onSelect: (id: string) => void; hubId?: string; hubLabel?: string; status?: string }
type Point = { x: number; y: number };
export function beamPoint(from: Point, to: Point, progress: number): Point {
  const t = Math.max(0, Math.min(1, progress)), u = 1 - t;
  const horizontal = Math.abs(to.x - from.x) >= Math.abs(to.y - from.y);
  const first = horizontal ? { x: (from.x + to.x) / 2, y: from.y } : { x: from.x, y: (from.y + to.y) / 2 };
  const second = horizontal ? { x: first.x, y: to.y } : { x: to.x, y: first.y };
  return { x: u ** 3 * from.x + 3 * u * u * t * first.x + 3 * u * t * t * second.x + t ** 3 * to.x, y: u ** 3 * from.y + 3 * u * u * t * first.y + 3 * u * t * t * second.y + t ** 3 * to.y };
}
function curve(from: Point, to: Point) {
  return Math.abs(to.x - from.x) >= Math.abs(to.y - from.y)
    ? `M${from.x},${from.y} C${(from.x + to.x) / 2},${from.y} ${(from.x + to.x) / 2},${to.y} ${to.x},${to.y}`
    : `M${from.x},${from.y} C${from.x},${(from.y + to.y) / 2} ${to.x},${(from.y + to.y) / 2} ${to.x},${to.y}`;
}

export function IntegrationBeam({ nodes, controllerRef, onSelect, hubId = 'axdb', hubLabel = 'AxDB', status, ...accessibility }: IntegrationBeamProps) {
  const root = useRef<HTMLDivElement>(null);
  const layer = useRef<HTMLDivElement>(null);
  const tiles = useRef(new Map<string, HTMLSpanElement>());
  const positions = useRef<Record<string, Point>>(Object.create(null));
  const [paths, setPaths] = useState<Record<string, string>>({});
  const [highlight, setHighlight] = useState<string | null>(null);
  const animations = useRef(new Map<Animation, HTMLElement>());
  const keys = nodes.map(node => node.id).join('\0');
  const cancel = () => { for (const [animation, element] of animations.current) { animation.cancel(); element.remove(); } animations.current.clear(); };
  useLayoutEffect(() => {
    const measure = () => {
      const rect = root.current!.getBoundingClientRect();
      const centers: Record<string, Point> = Object.create(null);
      for (const [id, tile] of tiles.current) { const box = tile.getBoundingClientRect(); centers[id] = { x: box.left + box.width / 2 - rect.left, y: box.top + box.height / 2 - rect.top }; }
      positions.current = centers;
      const next = Object.fromEntries(nodes.filter(node => centers[node.id] && centers[hubId]).map(node => [node.id, curve(centers[node.id], centers[hubId])]));
      setPaths(previous => JSON.stringify(previous) === JSON.stringify(next) ? previous : next);
    };
    const observer = new ResizeObserver(measure);
    observer.observe(root.current!);
    for (const tile of tiles.current.values()) observer.observe(tile);
    return () => { observer.disconnect(); cancel(); };
  }, [keys, hubId]);
  useLayoutEffect(() => {
    const preference = matchMedia('(prefers-reduced-motion: reduce)');
    const change = () => { if (preference.matches) cancel(); };
    preference.addEventListener('change', change);
    return () => { preference.removeEventListener('change', change); cancel(); };
  }, []);
  const animate = (element: HTMLElement, frames: Keyframe[], duration: number, finished?: () => void) => {
    const animation = element.animate(frames, { duration, easing: 'ease-in-out' });
    animations.current.set(animation, element);
    animation.onfinish = () => { animations.current.delete(animation); element.remove(); finished?.(); };
  };
  useImperativeHandle(controllerRef, () => ({ pulse(fromId, toId, tone = 'primary') {
    if (fromId === toId || (fromId !== hubId && toId !== hubId)) return false;
    const from = positions.current[fromId], to = positions.current[toId], target = tiles.current.get(toId);
    if (!from || !to || !target || !layer.current) return false;
    if (reducedMotion()) return true;
    // Bound concurrent visual work; never delay or replay the underlying DB/agent event.
    if (animations.current.size >= 12) { const [oldest, element] = animations.current.entries().next().value!; oldest.cancel(); element.remove(); animations.current.delete(oldest); }
    const beam = document.createElement('span'); beam.className = 'integration-beam__pulse'; beam.dataset.tone = tone;
    layer.current.append(beam);
    const frames = Array.from({ length: 41 }, (_, i) => {
      const t = i / 40, head = beamPoint(from, to, t), tail = beamPoint(from, to, Math.max(0, t - .2));
      const length = Math.min(84, Math.hypot(head.x - tail.x, head.y - tail.y));
      return { transform: `translate(${tail.x}px,${tail.y}px) rotate(${Math.atan2(head.y - tail.y, head.x - tail.x)}rad) scaleX(${length})`, opacity: i === 0 || i === 40 ? 0 : 1 };
    });
    animate(beam, frames, 650, () => {
      if (!target.isConnected || reducedMotion()) return;
      const ring = document.createElement('span'); ring.className = 'integration-beam__ring'; ring.dataset.tone = tone; ring.setAttribute('aria-hidden', 'true'); target.append(ring);
      animate(ring, [{ transform: 'scale(.96)', opacity: 0 }, { transform: 'scale(1)', opacity: .8, offset: .2 }, { transform: 'scale(1.14)', opacity: 0 }], 760);
    });
    return true;
  } }), [keys, hubId]);
  const tileRef = (id: string) => (element: HTMLSpanElement | null) => { if (element) tiles.current.set(id, element); else tiles.current.delete(id); };
  const column = (side: BeamNode['side']) => <div className="integration-beam__column">{nodes.filter(node => node.side === side).map(node => <Button bare type="button" key={node.id} className="integration-beam__node" data-state={node.state} aria-label={`${node.label}: ${node.state}. Open details`} onClick={() => onSelect(node.id)}
    onPointerEnter={() => setHighlight(node.id)} onPointerLeave={() => setHighlight(null)} onFocus={() => setHighlight(node.id)} onBlur={() => setHighlight(null)}>
    <span ref={tileRef(node.id)} className="integration-beam__tile" aria-hidden="true">{node.icon ?? node.label.slice(0, 1)}</span><span className="integration-beam__label">{node.label}</span>
  </Button>)}</div>;
  return <div className="integration-beam micro" role="group" aria-label={accessibility['aria-label'] ?? accessibility.ariaLabel ?? 'Connection activity'} data-testid={accessibility['data-testid'] ?? accessibility.testId}>
    <div ref={root} className="integration-beam__layout">
      <svg className="integration-beam__paths" aria-hidden="true">{nodes.map(node => <path key={node.id} d={paths[node.id]} data-state={node.state} data-highlight={highlight === node.id} style={{ opacity: highlight && highlight !== node.id ? .35 : 1 }} />)}</svg>
      <div ref={layer} className="integration-beam__pulses" aria-hidden="true" />
      {column('source')}
      <div className="integration-beam__hub"><span ref={tileRef(hubId)} className="integration-beam__tile">{hubLabel === "AxDB" ? <><span className="axdb-mark" aria-hidden="true" /><span className="axdb-wordmark">AxDB</span></> : hubLabel}</span><span className="integration-beam__status" role="status">{status ?? `${nodes.filter(node => node.state === 'connected').length} of ${nodes.length} connected`}</span></div>
      {column('target')}
    </div>
  </div>;
}
