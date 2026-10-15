import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import type { Graph, GNode, GRel } from './contract';
import { api } from './api';
import { mergeGraph, nodeCaption, nodeMatches, seedPositions, type GraphPosition } from './graph';
import { hitIndex } from './perf/spatial';
import { start, finish, painted } from './perf';
import { valueText } from './values';
interface LayoutNode extends GraphPosition { data: GNode; color: string; caption: string; match: boolean }
interface LayoutLink { data: GRel; source: LayoutNode; target: LayoutNode }
interface Props { graph: Graph; sessionId: string; canExpand: boolean; limit: number; onError: (error: unknown) => void; active?: boolean }
const colors = ['#4689ca', '#d19b3c', '#529579', '#b36c92', '#8c85cc', '#bd7555'];
export function GraphView({ graph: initial, sessionId, canExpand, limit, onError, active = true }: Props) {
  const [graph, setGraph] = useState(initial);
  const [search, setSearch] = useState('');
  const [selected, setSelected] = useState<GNode | GRel | null>(null);
  const [expanding, setExpanding] = useState(false);
  const [accessible, setAccessible] = useState(false);
  const canvas = useRef<HTMLCanvasElement>(null), container = useRef<HTMLDivElement>(null);
  const scene = useRef<{ nodes: LayoutNode[]; links: LayoutLink[] }>({ nodes: [], links: [] });
  const hit = useRef<ReturnType<typeof hitIndex<LayoutNode>>>(() => undefined);
  const camera = useRef({ x: 0, y: 0, scale: 1 });
  const draw = useRef<() => void>(() => {}), dimensions = useRef({ width: 600, height: 400 });
  const worker = useRef<Worker | null>(null), revision = useRef(0);
  const activeRef = useRef(active); activeRef.current = active;
  const request = useRef(0), expandStarted = useRef<number | null>(null);
  const drag = useRef<{ node?: LayoutNode; x: number; y: number; moved: boolean } | null>(null);
  const alive = useRef(true), source = useRef(initial); source.current = initial;
  const safeLimit = Math.max(1, Math.min(5000, Math.floor(limit)));
  const visible = useMemo(() => graph.nodes.slice(0, safeLimit), [graph, safeLimit]);
  const ids = useMemo(() => new Set(visible.map(n => n.id)), [visible]);
  const labels = useMemo(() => [...new Set(visible.flatMap(n => n.labels.length ? n.labels : ['Unlabeled']))], [visible]);
  const counts = useMemo(() => { const map = new Map<string, number>(); for (const n of visible) for (const l of n.labels.length ? n.labels : ['Unlabeled']) map.set(l, (map.get(l) ?? 0) + 1); return map; }, [visible]);
  function schedule() { if (!request.current && activeRef.current) request.current = requestAnimationFrame(() => { request.current = 0; draw.current(); }); }
  useEffect(() => { alive.current = true; return () => { alive.current = false; worker.current?.terminate(); cancelAnimationFrame(request.current); }; }, []);
  useEffect(() => { scene.current = { nodes: [], links: [] }; setGraph(initial); setSelected(null); }, [initial]);
  useEffect(() => {
    const element = canvas.current!, context = element.getContext('2d')!;
    const since = start('graph-layout'); const currentRevision = ++revision.current;
    const positions = seedPositions({ nodes: visible, rels: graph.rels }, scene.current.nodes);
    const data = new Map(visible.map(n => [n.id, n]));
    const nodes: LayoutNode[] = positions.map(p => { const node = data.get(p.id)!; return { ...p, data: node, color: colors[Math.max(0, labels.indexOf(node.labels[0] ?? 'Unlabeled')) % colors.length], caption: nodeCaption(node).slice(0, 35), match: nodeMatches(node, search) }; });
    const byId = new Map(nodes.map(n => [n.id, n]));
    const links: LayoutLink[] = graph.rels.filter(r => byId.has(r.start) && byId.has(r.end)).map(data => ({ data, source: byId.get(data.start)!, target: byId.get(data.end)! }));
    scene.current = { nodes, links }; hit.current = hitIndex(nodes);
    let first = true, settled = false;
    const paint = () => {
      if (!activeRef.current) return;
      const { width, height } = dimensions.current, { x, y, scale } = camera.current;
      const dark = matchMedia('(prefers-color-scheme: dark)').matches;
      context.setTransform(devicePixelRatio, 0, 0, devicePixelRatio, 0, 0); context.clearRect(0, 0, width, height);
      context.translate(width / 2 + x, height / 2 + y); context.scale(scale, scale);
      context.font = '11px system-ui'; context.textAlign = 'center';
      const left = (-width / 2 - x) / scale - 40, right = (width / 2 - x) / scale + 40, top = (-height / 2 - y) / scale - 40, bottom = (height / 2 - y) / scale + 40;
      const onscreen = (n: LayoutNode) => n.x >= left && n.x <= right && n.y >= top && n.y <= bottom;
      const shownLinks = links.filter(l => Math.max(l.source.x, l.target.x) >= left && Math.min(l.source.x, l.target.x) <= right && Math.max(l.source.y, l.target.y) >= top && Math.min(l.source.y, l.target.y) <= bottom);
      context.strokeStyle = dark ? '#7c8795' : '#65717e'; context.fillStyle = context.strokeStyle; context.lineWidth = 1.2 / scale;
      context.beginPath(); for (const l of shownLinks) { context.moveTo(l.source.x, l.source.y); context.lineTo(l.target.x, l.target.y); } context.stroke();
      if (scale >= .6) {
        context.beginPath();
        for (const link of shownLinks) { const a = link.source, b = link.target, angle = Math.atan2(b.y - a.y, b.x - a.x), ex = b.x - Math.cos(angle) * 19, ey = b.y - Math.sin(angle) * 19; context.moveTo(ex, ey); context.lineTo(ex - Math.cos(angle - .45) * 9, ey - Math.sin(angle - .45) * 9); context.lineTo(ex - Math.cos(angle + .45) * 9, ey - Math.sin(angle + .45) * 9); context.closePath(); }
        context.fill();
        for (const l of shownLinks) context.fillText(l.data.rel_type, (l.source.x + l.target.x) / 2, (l.source.y + l.target.y) / 2 - 5);
      }
      const shown = nodes.filter(onscreen);
      for (const color of colors) { context.beginPath(); for (const n of shown) if (n.color === color) { context.moveTo(n.x + 17, n.y); context.arc(n.x, n.y, 17, 0, Math.PI * 2); } context.fillStyle = color; context.fill(); }
      context.strokeStyle = '#eec744'; context.lineWidth = 4 / scale; context.beginPath(); for (const n of shown) if (n.match) { context.moveTo(n.x + 17, n.y); context.arc(n.x, n.y, 17, 0, Math.PI * 2); } context.stroke();
      if (scale >= .6) { context.fillStyle = dark ? '#e2e7ed' : '#152331'; for (const n of shown) context.fillText(n.caption, n.x, n.y + 32); }
      if (first) { first = false; finish('graph-first-frame', since); }
      if (expandStarted.current !== null) { void painted('expand-frame', expandStarted.current); expandStarted.current = null; }
    };
    draw.current = paint;
    const observer = new ResizeObserver(() => { const wrapper = container.current; if (!wrapper) return; const rect = wrapper.getBoundingClientRect(); if (!rect.width || !rect.height) return; dimensions.current = { width: rect.width, height: rect.height }; element.width = Math.max(1, rect.width * devicePixelRatio); element.height = Math.max(1, rect.height * devicePixelRatio); schedule(); }); observer.observe(container.current!);
    if (!worker.current) worker.current = new Worker(new URL('./workers/graph.worker.ts', import.meta.url), { type: 'module' });
    worker.current.onerror = event => { if (alive.current) onError(new Error(`Graph layout: ${event.message}`)); };
    worker.current.onmessage = event => {
      if (event.data.revision !== revision.current) return;
      const { positions, stable } = event.data as { positions: Float32Array; stable: boolean };
      nodes.forEach((n, i) => { if (drag.current?.node !== n) { n.x = positions[i * 2]; n.y = positions[i * 2 + 1]; } });
      hit.current = hitIndex(nodes); schedule(); if (stable && !settled) { settled = true; finish('graph-stable', since, { nodes: nodes.length }); }
    };
    worker.current.postMessage({ revision: currentRevision, nodes: positions, links: links.map(l => ({ source: l.source.id, target: l.target.id })), incremental: positions.some(n => n.fixed) });
    if (!activeRef.current) worker.current.postMessage({ kind: 'pause' });
    schedule();
    const wheel = (e: WheelEvent) => { e.preventDefault(); const rect = element.getBoundingClientRect(), c = camera.current, px = e.clientX - rect.left - dimensions.current.width / 2, py = e.clientY - rect.top - dimensions.current.height / 2; const next = Math.max(.15, Math.min(6, c.scale * Math.exp(-e.deltaY * .001))); c.x = px - (px - c.x) * next / c.scale; c.y = py - (py - c.y) * next / c.scale; c.scale = next; schedule(); };
    element.addEventListener('wheel', wheel, { passive: false }); const media = matchMedia('(prefers-color-scheme: dark)'); media.addEventListener('change', schedule);
    return () => { worker.current?.postMessage({ kind: 'pause' }); observer.disconnect(); element.removeEventListener('wheel', wheel); media.removeEventListener('change', schedule); };
  }, [visible, graph.rels, safeLimit]);
  useEffect(() => { worker.current?.postMessage({ kind: active ? 'resume' : 'pause' }); if (active) schedule(); else { cancelAnimationFrame(request.current); request.current = 0; } }, [active]);
  useEffect(() => { for (const n of scene.current.nodes) n.match = nodeMatches(n.data, search); const node = scene.current.nodes.find(n => n.match); if (node) { camera.current.x = -node.x * camera.current.scale; camera.current.y = -node.y * camera.current.scale; } schedule(); }, [search]);
  function point(event: MouseEvent | PointerEvent) { const rect = canvas.current!.getBoundingClientRect(), c = camera.current; return { x: (event.clientX - rect.left - dimensions.current.width / 2 - c.x) / c.scale, y: (event.clientY - rect.top - dimensions.current.height / 2 - c.y) / c.scale }; }
  function hitNode(event: MouseEvent | PointerEvent) { const p = point(event); return hit.current(p.x, p.y, 21 / Math.max(1, camera.current.scale)); }
  function select(value: GNode | GRel | null) { const since = start('node-click'); setSelected(value); void painted('properties', since); }
  async function expandNode(node: GNode) {
    if (!canExpand || expanding) return; setExpanding(true); const expected = source.current, since = start('expand-request');
    try { const result = await api('expand', { sessionId, elementId: node.id, limit: safeLimit }); if (alive.current && source.current === expected && result.graph) { expandStarted.current = since; setGraph(g => { const mergeStart = start('expand-merge'); const merged = mergeGraph(g, result.graph!); finish('expand-merge', mergeStart); return merged; }); } }
    catch (error) { if (alive.current && source.current === expected) onError(error); } finally { if (alive.current) setExpanding(false); }
  }
  const accessibleContent = useMemo(() => accessible ? <>{visible.map(n => <button onClick={() => select(n)} key={n.id}>{nodeCaption(n)}</button>)}{graph.rels.filter(r => ids.has(r.start) && ids.has(r.end)).map(r => <button onClick={() => select(r)} key={r.id}>{r.rel_type} · {r.id}</button>)}</> : null, [accessible, visible, graph.rels, ids]);
  return <div class="graph-area"><div class="graph-main"><div class="toolbar"><input aria-label="Search graph" placeholder="Search labels or property values" value={search} onInput={e => setSearch(e.currentTarget.value)} /><button onClick={() => { camera.current = { x: 0, y: 0, scale: 1 }; schedule(); }}>Reset view</button><span>{expanding ? 'Expanding…' : 'Wheel: zoom · drag: pan · double-click: expand'}</span></div>
    {graph.nodes.length > safeLimit && <div class="warning" role="status">{graph.nodes.length}개 중 {safeLimit}개 표시</div>}
    <div class="canvas-wrap" ref={container}><canvas ref={canvas} aria-label={`Graph: ${visible.length} nodes, ${graph.rels.length} relationships`} role="img"
      onPointerDown={e => { e.currentTarget.setPointerCapture(e.pointerId); drag.current = { node: hitNode(e), x: e.clientX, y: e.clientY, moved: false }; }}
      onPointerMove={e => { const d = drag.current; if (!d) return; const dx = e.clientX - d.x, dy = e.clientY - d.y; if (Math.abs(dx) + Math.abs(dy) > 2) d.moved = true; if (d.node) { const p = point(e); d.node.x = p.x; d.node.y = p.y; worker.current?.postMessage({ kind: 'pin', id: d.node.id, ...p }); hit.current = hitIndex(scene.current.nodes); } else { camera.current.x += dx; camera.current.y += dy; } d.x = e.clientX; d.y = e.clientY; schedule(); }}
      onPointerUp={e => { const d = drag.current; drag.current = null; if (d?.moved) return; const node = hitNode(e); if (node) { select(node.data); return; } const p = point(e); const link = scene.current.links.find(l => { const a = l.source, b = l.target, t = Math.max(0, Math.min(1, ((p.x - a.x) * (b.x - a.x) + (p.y - a.y) * (b.y - a.y)) / ((b.x - a.x) ** 2 + (b.y - a.y) ** 2 || 1))); return Math.hypot(p.x - a.x - t * (b.x - a.x), p.y - a.y - t * (b.y - a.y)) < 8 / camera.current.scale; }); select(link?.data ?? null); }}
      onPointerCancel={() => { drag.current = null; }} onDblClick={e => { const node = hitNode(e); if (node) void expandNode(node.data); }} /></div>
    <div class="legend">{labels.map((label, i) => <span key={label}><i style={{ background: colors[i % colors.length] }} />{label} {counts.get(label)}</span>)}</div>
    <details class="graph-accessible" onToggle={e => setAccessible(e.currentTarget.open)}><summary>Nodes & relationships (keyboard access)</summary>{accessibleContent}</details>
  </div>{selected && <aside class="properties" aria-label="Graph properties"><div class="section-title"><strong>Properties</strong><button onClick={() => setSelected(null)} aria-label="Close properties">×</button></div><dl><dt>id</dt><dd>{selected.id}</dd><dt>{'labels' in selected ? 'labels' : 'type'}</dt><dd>{'labels' in selected ? selected.labels.join(', ') : selected.rel_type}</dd>{'start' in selected && <><dt>start → end</dt><dd>{selected.start} → {selected.end}</dd></>}{selected.props.map(([key, value]) => <div key={key}><dt>{key}</dt><dd><pre>{valueText(value, true)}</pre></dd></div>)}</dl>{'labels' in selected && canExpand && <button disabled={expanding} onClick={() => void expandNode(selected)}>Expand neighbors</button>}</aside>}</div>;
}
