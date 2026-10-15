import { useEffect, useRef, useState } from 'preact/hooks';
import { forceSimulation, forceLink, forceManyBody, forceCenter, forceCollide, type SimulationNodeDatum, type SimulationLinkDatum } from 'd3-force';
import type { Graph, GNode, GRel } from './contract';
import { api } from './api';
import { mergeGraph, nodeCaption, nodeMatches } from './graph';
import { valueText } from './values';
interface LayoutNode extends SimulationNodeDatum { data: GNode; x: number; y: number }
interface LayoutLink extends SimulationLinkDatum<LayoutNode> { data: GRel; source: LayoutNode; target: LayoutNode }
interface Props { graph: Graph; sessionId: string; canExpand: boolean; limit: number; onError: (error: unknown) => void }
const colors = ['#4689ca', '#d19b3c', '#529579', '#b36c92', '#8c85cc', '#bd7555'];
export function GraphView({ graph: initial, sessionId, canExpand, limit, onError }: Props) {
  const [graph, setGraph] = useState(initial);
  const [search, setSearch] = useState('');
  const [selected, setSelected] = useState<GNode | GRel | null>(null);
  const [expanding, setExpanding] = useState(false);
  const canvas = useRef<HTMLCanvasElement>(null);
  const container = useRef<HTMLDivElement>(null);
  const scene = useRef<{ nodes: LayoutNode[]; links: LayoutLink[] }>({ nodes: [], links: [] });
  const camera = useRef({ x: 0, y: 0, scale: 1 });
  const draw = useRef<() => void>(() => {});
  const dimensions = useRef({ width: 600, height: 400 });
  const drag = useRef<{ node?: LayoutNode; x: number; y: number; moved: boolean } | null>(null);
  const latestSearch = useRef(search); latestSearch.current = search;
  const alive = useRef(true);
  const source = useRef(initial); source.current = initial;
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useEffect(() => { setGraph(initial); setSelected(null); }, [initial]);
  const safeLimit = Math.max(1, Math.floor(limit));
  const labels = [...new Set(graph.nodes.slice(0, safeLimit).flatMap(n => n.labels.length ? n.labels : ['Unlabeled']))];
  useEffect(() => {
    const element = canvas.current!;
    const context = element.getContext('2d')!;
    let frame = 0;
    let disposed = false;
    const previous = new Map(scene.current.nodes.map(n => [n.data.id, n]));
    const nodes: LayoutNode[] = graph.nodes.slice(0, safeLimit).map((data, i) => ({ data, x: previous.get(data.id)?.x ?? Math.cos(i * 2.4) * 110, y: previous.get(data.id)?.y ?? Math.sin(i * 2.4) * 110 }));
    const byId = new Map(nodes.map(n => [n.data.id, n]));
    const links: LayoutLink[] = graph.rels.filter(r => byId.has(r.start) && byId.has(r.end)).map(data => ({ data, source: byId.get(data.start)!, target: byId.get(data.end)! }));
    scene.current = { nodes, links };
    const palette = [...new Set(nodes.flatMap(n => n.data.labels.length ? n.data.labels : ['Unlabeled']))];
    const paint = () => {
      const { width, height } = dimensions.current;
      const { x, y, scale } = camera.current;
      const dark = matchMedia('(prefers-color-scheme: dark)').matches;
      context.setTransform(devicePixelRatio, 0, 0, devicePixelRatio, 0, 0);
      context.clearRect(0, 0, width, height);
      context.translate(width / 2 + x, height / 2 + y); context.scale(scale, scale);
      context.font = '11px system-ui'; context.textAlign = 'center';
      for (const link of links) {
        const a = link.source; const b = link.target;
        const angle = Math.atan2(b.y - a.y, b.x - a.x);
        const endX = b.x - Math.cos(angle) * 19; const endY = b.y - Math.sin(angle) * 19;
        context.strokeStyle = dark ? '#7c8795' : '#65717e'; context.fillStyle = context.strokeStyle; context.lineWidth = 1.2 / scale;
        context.beginPath(); context.moveTo(a.x, a.y); context.lineTo(endX, endY); context.stroke();
        context.beginPath(); context.moveTo(endX, endY); context.lineTo(endX - Math.cos(angle - 0.45) * 9, endY - Math.sin(angle - 0.45) * 9); context.lineTo(endX - Math.cos(angle + 0.45) * 9, endY - Math.sin(angle + 0.45) * 9); context.closePath(); context.fill();
        context.fillText(link.data.rel_type, (a.x + b.x) / 2, (a.y + b.y) / 2 - 5);
      }
      for (const node of nodes) {
        context.beginPath(); context.arc(node.x, node.y, 17, 0, Math.PI * 2);
        context.fillStyle = colors[Math.max(0, palette.indexOf(node.data.labels[0] ?? 'Unlabeled')) % colors.length]; context.fill();
        if (nodeMatches(node.data, latestSearch.current)) { context.strokeStyle = '#eec744'; context.lineWidth = 4 / scale; context.stroke(); }
        context.fillStyle = dark ? '#e2e7ed' : '#152331'; context.fillText(nodeCaption(node.data).slice(0, 35), node.x, node.y + 32);
      }
    };
    draw.current = paint;
    const observer = new ResizeObserver(() => {
      const rect = container.current!.getBoundingClientRect();
      dimensions.current = { width: rect.width, height: rect.height };
      element.width = Math.max(1, rect.width * devicePixelRatio); element.height = Math.max(1, rect.height * devicePixelRatio);
      paint();
    }); observer.observe(container.current!);
    const simulation = forceSimulation(nodes).stop().force('link', forceLink<LayoutNode, LayoutLink>(links).distance(140)).force('charge', forceManyBody().strength(-250)).force('center', forceCenter(0, 0)).force('collide', forceCollide(40));
    let ticks = 0;
    const step = () => {
      if (disposed) return;
      simulation.tick(Math.min(5, 300 - ticks)); ticks += 5; paint();
      if (ticks < 300) frame = requestAnimationFrame(step);
    }; frame = requestAnimationFrame(step);
    const wheel = (e: WheelEvent) => {
      e.preventDefault();
      const rect = element.getBoundingClientRect(); const c = camera.current;
      const pointerX = e.clientX - rect.left - dimensions.current.width / 2; const pointerY = e.clientY - rect.top - dimensions.current.height / 2;
      const next = Math.max(0.15, Math.min(6, c.scale * Math.exp(-e.deltaY * 0.001)));
      c.x = pointerX - (pointerX - c.x) * next / c.scale; c.y = pointerY - (pointerY - c.y) * next / c.scale; c.scale = next; paint();
    }; element.addEventListener('wheel', wheel, { passive: false });
    const media = matchMedia('(prefers-color-scheme: dark)'); media.addEventListener('change', paint);
    return () => { disposed = true; cancelAnimationFrame(frame); simulation.stop(); observer.disconnect(); element.removeEventListener('wheel', wheel); media.removeEventListener('change', paint); };
  }, [graph, safeLimit]);
  useEffect(() => {
    const node = scene.current.nodes.find(n => nodeMatches(n.data, search));
    if (node) { camera.current.x = -node.x * camera.current.scale; camera.current.y = -node.y * camera.current.scale; }
    draw.current();
  }, [search]);
  function point(event: MouseEvent | PointerEvent) {
    const rect = canvas.current!.getBoundingClientRect(); const c = camera.current;
    return { x: (event.clientX - rect.left - dimensions.current.width / 2 - c.x) / c.scale, y: (event.clientY - rect.top - dimensions.current.height / 2 - c.y) / c.scale };
  }
  function hitNode(event: MouseEvent | PointerEvent) { const p = point(event); return scene.current.nodes.find(n => Math.hypot(n.x - p.x, n.y - p.y) < 21); }
  async function expandNode(node: GNode) {
    if (!canExpand || expanding) return;
    setExpanding(true);
    const expected = source.current;
    try { const result = await api('expand', { sessionId, elementId: node.id, limit: safeLimit }); if (alive.current && source.current === expected && result.graph) setGraph(g => mergeGraph(g, result.graph!)); }
    catch (error) { if (alive.current && source.current === expected) onError(error); }
    finally { if (alive.current) setExpanding(false); }
  }
  return <div class="graph-area">
    <div class="graph-main"><div class="toolbar"><input aria-label="Search graph" placeholder="Search labels or property values" value={search} onInput={e => setSearch(e.currentTarget.value)} /><button onClick={() => { camera.current = { x: 0, y: 0, scale: 1 }; draw.current(); }}>Reset view</button><span>{expanding ? 'Expanding…' : 'Wheel: zoom · drag: pan · double-click: expand'}</span></div>
      {graph.nodes.length > safeLimit && <div class="warning" role="status">{graph.nodes.length}개 중 {safeLimit}개 표시</div>}
      <div class="canvas-wrap" ref={container}><canvas ref={canvas} aria-label={`Graph: ${Math.min(graph.nodes.length, safeLimit)} nodes, ${graph.rels.length} relationships`} role="img"
        onPointerDown={e => { e.currentTarget.setPointerCapture(e.pointerId); drag.current = { node: hitNode(e), x: e.clientX, y: e.clientY, moved: false }; }}
        onPointerMove={e => { const d = drag.current; if (!d) return; const dx = e.clientX - d.x; const dy = e.clientY - d.y; if (Math.abs(dx) + Math.abs(dy) > 2) d.moved = true; if (d.node) { const p = point(e); d.node.x = p.x; d.node.y = p.y; d.node.fx = p.x; d.node.fy = p.y; } else { camera.current.x += dx; camera.current.y += dy; } d.x = e.clientX; d.y = e.clientY; draw.current(); }}
        onPointerUp={e => { const d = drag.current; drag.current = null; if (d?.moved) return; const node = hitNode(e); if (node) { setSelected(node.data); return; } const p = point(e); const link = scene.current.links.find(l => { const ax = l.source.x, ay = l.source.y, bx = l.target.x, by = l.target.y; const t = Math.max(0, Math.min(1, ((p.x - ax) * (bx - ax) + (p.y - ay) * (by - ay)) / ((bx - ax) ** 2 + (by - ay) ** 2 || 1))); return Math.hypot(p.x - ax - t * (bx - ax), p.y - ay - t * (by - ay)) < 8 / camera.current.scale; }); setSelected(link?.data ?? null); }}
        onPointerCancel={() => { drag.current = null; }}
        onDblClick={e => { const node = hitNode(e); if (node) void expandNode(node.data); }} /></div>
      <div class="legend">{labels.map((label, i) => <span key={label}><i style={{ background: colors[i % colors.length] }} />{label} {graph.nodes.slice(0, safeLimit).filter(n => n.labels.includes(label) || (!n.labels.length && label === 'Unlabeled')).length}</span>)}</div>
      <details class="graph-accessible"><summary>Nodes & relationships (keyboard access)</summary>{graph.nodes.slice(0, safeLimit).map(n => <button onClick={() => setSelected(n)} key={n.id}>{nodeCaption(n)}</button>)}{graph.rels.filter(r => graph.nodes.slice(0, safeLimit).some(n => n.id === r.start) && graph.nodes.slice(0, safeLimit).some(n => n.id === r.end)).map(r => <button onClick={() => setSelected(r)} key={r.id}>{r.rel_type} · {r.id}</button>)}</details>
    </div>
    {selected && <aside class="properties" aria-label="Graph properties"><div class="section-title"><strong>Properties</strong><button onClick={() => setSelected(null)} aria-label="Close properties">×</button></div><dl><dt>id</dt><dd>{selected.id}</dd><dt>{'labels' in selected ? 'labels' : 'type'}</dt><dd>{'labels' in selected ? selected.labels.join(', ') : selected.rel_type}</dd>{'start' in selected && <><dt>start → end</dt><dd>{selected.start} → {selected.end}</dd></>}{selected.props.map(([key, value]) => <div key={key}><dt>{key}</dt><dd><pre>{valueText(value, true)}</pre></dd></div>)}</dl>{'labels' in selected && canExpand && <button disabled={expanding} onClick={() => void expandNode(selected)}>Expand neighbors</button>}</aside>}
  </div>;
}
