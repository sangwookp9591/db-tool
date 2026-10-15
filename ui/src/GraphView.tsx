import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'preact/hooks';
import type { Graph, GNode, GRel } from './contract';
import { api } from './api';
import { mergeGraph, nodeCaption, nodeMatches, seedPositions, fitGraph, relationOffsets, edgeCurve, curvePoint, curveDistance, bitmapTransform, type GraphPosition } from './graph';
import { hitIndex } from './perf/spatial';
import { start, finish, painted, registerGraphProbe } from './perf';
import { valueText } from './values';
interface LayoutNode extends GraphPosition { data: GNode; color: string; caption: string; match: boolean; visible: boolean }
interface LayoutLink { data: GRel; source: LayoutNode; target: LayoutNode; offset: number }
interface Props { graph: Graph; sessionId: string; canExpand: boolean; limit: number; onError: (error: unknown) => void; active?: boolean }
const colors = ['#4689ca', '#d19b3c', '#529579', '#b36c92', '#8c85cc', '#bd7555'];
export function GraphView({ graph: initial, sessionId, canExpand, limit, onError, active = true }: Props) {
  const [graph, setGraph] = useState(initial);
  const [search, setSearch] = useState('');
  const [selected, setSelected] = useState<GNode | GRel | null>(null);
  const [expanding, setExpanding] = useState(false);
  const [accessible, setAccessible] = useState(false);
  const [ready, setReady] = useState(false);
  const canvas = useRef<HTMLCanvasElement>(null), container = useRef<HTMLDivElement>(null);
  const scene = useRef<{ nodes: LayoutNode[]; links: LayoutLink[] }>({ nodes: [], links: [] });
  const hit = useRef<ReturnType<typeof hitIndex<LayoutNode>>>(() => undefined);
  const camera = useRef({ x: 0, y: 0, scale: 1 });
  const draw = useRef<() => void>(() => {}), dimensions = useRef({ width: 600, height: 400 });
  const worker = useRef<Worker | null>(null), revision = useRef(0);
  const activeRef = useRef(active && !document.hidden); activeRef.current = active && !document.hidden;
  const readyRef = useRef(false), frames = useRef(0), fitMode = useRef(true);
  const interact = useRef<() => void>(() => {}), invalidate = useRef<() => void>(() => {});
  const stopIntro = useRef<() => void>(() => {});
  const request = useRef(0), expandStarted = useRef<number | null>(null);
  const drag = useRef<{ node?: LayoutNode; x: number; y: number; moved: boolean } | null>(null);
  const alive = useRef(true), source = useRef(initial); source.current = initial;
  const safeLimit = Math.max(1, Math.min(5000, Math.floor(limit)));
  const visible = useMemo(() => graph.nodes.slice(0, safeLimit), [graph, safeLimit]);
  const ids = useMemo(() => new Set(visible.map(n => n.id)), [visible]);
  const labels = useMemo(() => [...new Set(visible.flatMap(n => n.labels.length ? n.labels : ['Unlabeled']))], [visible]);
  const counts = useMemo(() => { const map = new Map<string, number>(); for (const n of visible) for (const l of n.labels.length ? n.labels : ['Unlabeled']) map.set(l, (map.get(l) ?? 0) + 1); return map; }, [visible]);
  function schedule() { if (!request.current && activeRef.current) request.current = requestAnimationFrame(() => { request.current = 0; draw.current(); }); }
  function fit() { invalidate.current(); camera.current = fitGraph(scene.current.nodes.filter(n => n.visible), dimensions.current.width, dimensions.current.height); fitMode.current = true; schedule(); }
  useEffect(() => registerGraphProbe(() => ({ frames: frames.current, ready: readyRef.current, positions: Float32Array.from(scene.current.nodes.filter(n => n.visible).flatMap(n => [n.x, n.y])) })), []);
  useEffect(() => { alive.current = true; return () => { alive.current = false; worker.current?.terminate(); cancelAnimationFrame(request.current); }; }, []);
  useLayoutEffect(() => { readyRef.current = false; setReady(false); setGraph(initial); setSelected(null); }, [initial]);
  useLayoutEffect(() => {
    const element = canvas.current!, display = element.getContext('2d')!;
    const bitmap = document.createElement('canvas'), bitmapContext = bitmap.getContext('2d')!;
    let bitmapCamera: typeof camera.current | null = null, interacting = false, interactionTimer = 0;
    invalidate.current = () => { bitmapCamera = null; interacting = false; clearTimeout(interactionTimer); };
    interact.current = () => {
      if (!interacting) { stopIntro.current(); stopIntro.current = () => {}; }
      interacting = true; clearTimeout(interactionTimer);
      interactionTimer = window.setTimeout(() => { interacting = false; schedule(); }, 120);
    };
    const presentBitmap = () => {
      const { width, height } = dimensions.current, t = bitmapTransform(bitmapCamera!, camera.current, width, height);
      display.setTransform(devicePixelRatio, 0, 0, devicePixelRatio, 0, 0); display.clearRect(0, 0, width, height);
      display.setTransform(devicePixelRatio * t.scale, 0, 0, devicePixelRatio * t.scale, devicePixelRatio * t.x, devicePixelRatio * t.y);
      display.drawImage(bitmap, 0, 0, width, height);
    };
    const since = start('graph-layout'), currentRevision = ++revision.current;
    const positions = seedPositions({ nodes: visible, rels: graph.rels }, scene.current.nodes), incremental = positions.some(p => p.fixed);
    if (!incremental) { readyRef.current = false; setReady(false); }
    const data = new Map(visible.map(n => [n.id, n]));
    const nodes: LayoutNode[] = positions.map(p => { const node = data.get(p.id)!; return { ...p, visible: !!p.fixed, data: node, color: colors[Math.max(0, labels.indexOf(node.labels[0] ?? 'Unlabeled')) % colors.length], caption: nodeCaption(node).slice(0, 35), match: nodeMatches(node, search) }; });
    const byId = new Map(nodes.map(n => [n.id, n])), offsets = relationOffsets(graph.rels);
    const links: LayoutLink[] = graph.rels.filter(r => byId.has(r.start) && byId.has(r.end)).map(data => ({ data, source: byId.get(data.start)!, target: byId.get(data.end)!, offset: offsets.get(data.id) ?? 0 }));
    scene.current = { nodes, links }; hit.current = hitIndex(nodes.filter(n => n.visible));
    let first = true, settled = false;
    const profiling = new URLSearchParams(location.search).has('graphProfile');
    const paint = () => {
      if (!activeRef.current || !readyRef.current) return;
      const renderStart = performance.now(); frames.current++;
      let phase = renderStart;
      const checkpoint = (name: string) => { if (profiling) { finish(`graph-phase-${name}`, phase); phase = performance.now(); } };
      if (interacting && bitmapCamera) { presentBitmap(); finish('graph-bitmap', renderStart); return; }
      const { width, height } = dimensions.current;
      if (interacting) {
        // shortcut: rasterizing the first gesture can stall one frame; prewarm if that spike needs eliminating.
        bitmap.width = element.width; bitmap.height = element.height;
        // Cache the whole scene so panning reveals content beyond the previous viewport.
        const bounds = links.filter(l => l.source.visible && l.target.visible).flatMap(l => { const g = edgeCurve(l.source, l.target, l.offset, l.data.start === l.data.end); return g.d ? [g.a, g.b, g.c, g.d] : [g.a, g.b, g.c]; });
        bitmapCamera = fitGraph([...nodes.filter(n => n.visible), ...bounds], width, height);
      }
      const context = interacting ? bitmapContext : display;
      const { x, y, scale } = interacting ? bitmapCamera! : camera.current, dark = matchMedia('(prefers-color-scheme: dark)').matches;
      context.setTransform(devicePixelRatio, 0, 0, devicePixelRatio, 0, 0); context.clearRect(0, 0, width, height);
      context.translate(width / 2 + x, height / 2 + y); context.scale(scale, scale); context.font = '11px system-ui'; context.textAlign = 'center';
      const left = (-width / 2 - x) / scale - 40, right = (width / 2 - x) / scale + 40, top = (-height / 2 - y) / scale - 40, bottom = (height / 2 - y) / scale + 40;
      const onscreen = (n: LayoutNode) => n.visible && n.x >= left && n.x <= right && n.y >= top && n.y <= bottom;
      checkpoint('setup');
      const shownLinks = links.filter(l => l.source.visible && l.target.visible).map(link => ({ link, curve: edgeCurve(link.source, link.target, link.offset, link.data.start === link.data.end) })).filter(({ curve: g }) => Math.max(g.a.x, g.b.x, g.c.x, g.d?.x ?? g.c.x) >= left && Math.min(g.a.x, g.b.x, g.c.x, g.d?.x ?? g.c.x) <= right && Math.max(g.a.y, g.b.y, g.c.y, g.d?.y ?? g.c.y) >= top && Math.min(g.a.y, g.b.y, g.c.y, g.d?.y ?? g.c.y) <= bottom);
      checkpoint('curves-cull');
      context.strokeStyle = dark ? '#7c8795' : '#65717e'; context.fillStyle = context.strokeStyle; context.lineWidth = 1.2 / scale;
      context.beginPath(); for (const { curve: g } of shownLinks) { context.moveTo(g.a.x, g.a.y); if (g.d) context.bezierCurveTo(g.c.x, g.c.y, g.d.x, g.d.y, g.b.x, g.b.y); else context.quadraticCurveTo(g.c.x, g.c.y, g.b.x, g.b.y); } context.stroke();
      checkpoint('edges');
      if (scale >= .6) {
        context.beginPath(); for (const { curve: g } of shownLinks) { const control = g.d ?? g.c, angle = Math.atan2(g.b.y - control.y, g.b.x - control.x); context.moveTo(g.b.x, g.b.y); context.lineTo(g.b.x - Math.cos(angle - .45) * 9, g.b.y - Math.sin(angle - .45) * 9); context.lineTo(g.b.x - Math.cos(angle + .45) * 9, g.b.y - Math.sin(angle + .45) * 9); context.closePath(); } context.fill();
        for (const { link, curve } of shownLinks) { const p = curvePoint(curve, .5); context.fillText(link.data.rel_type, p.x, p.y - 5); }
      }
      checkpoint('arrows-edge-text');
      const shown = nodes.filter(onscreen);
      for (const color of colors) { context.beginPath(); for (const n of shown) if (n.color === color) { context.moveTo(n.x + 17, n.y); context.arc(n.x, n.y, 17, 0, Math.PI * 2); } context.fillStyle = color; context.fill(); }
      context.strokeStyle = '#eec744'; context.lineWidth = 4 / scale; context.beginPath(); for (const n of shown) if (n.match) { context.moveTo(n.x + 17, n.y); context.arc(n.x, n.y, 17, 0, Math.PI * 2); } context.stroke();
      checkpoint('nodes');
      if (scale >= .6) { context.fillStyle = dark ? '#e2e7ed' : '#152331'; for (const n of shown) context.fillText(n.caption, n.x, n.y + 32); }
      checkpoint('node-text');
      if (interacting) presentBitmap();
      if (first) { first = false; finish('graph-first-frame', since); }
      if (expandStarted.current !== null) { void painted('expand-frame', expandStarted.current); expandStarted.current = null; }
      finish('graph-render', renderStart, { nodes: nodes.length });
    };
    draw.current = paint;
    const resize = () => { const wrapper = container.current; if (!wrapper) return; const rect = wrapper.getBoundingClientRect(); if (!rect.width || !rect.height) return; dimensions.current = { width: rect.width, height: rect.height }; const width = Math.max(1, rect.width * devicePixelRatio), height = Math.max(1, rect.height * devicePixelRatio); if (element.width !== width || element.height !== height) { invalidate.current(); element.width = width; element.height = height; } if (fitMode.current && readyRef.current) camera.current = fitGraph(nodes, rect.width, rect.height); schedule(); };
    resize(); const observer = new ResizeObserver(resize); observer.observe(container.current!);
    if (!worker.current) worker.current = new Worker(new URL('./workers/graph.worker.ts', import.meta.url), { type: 'module' });
    worker.current.onerror = event => { if (alive.current) onError(new Error(`Graph layout: ${event.message}`)); };
    const receivePositions = (payload: { revision: number; positions: Float32Array; ticks: number; workMs: number; cached?: boolean }) => {
      if (payload.revision !== revision.current) return;
      const target = payload.positions;
      const apply = () => { invalidate.current(); nodes.forEach((n, i) => { if (drag.current?.node !== n) { n.x = target[i * 2]; n.y = target[i * 2 + 1]; } n.visible = true; }); hit.current = hitIndex(nodes); };
      const firstReply = !settled; settled = true; readyRef.current = true; setReady(true);
      if (firstReply) finish('graph-stable', since, { nodes: nodes.length, ticks: payload.ticks, workMs: payload.workMs, cached: !!payload.cached });
      if (firstReply && incremental && positions.some(n => !n.fixed) && !matchMedia('(prefers-reduced-motion: reduce)').matches) {
        fitMode.current = false; for (const n of nodes) n.visible = true;
        const seeds = nodes.map(n => ({ x: n.x, y: n.y })); const begun = performance.now(); let animation = 0;
        const complete = () => { cancelAnimationFrame(animation); apply(); };
        stopIntro.current = complete;
        const tween = (now: number) => { invalidate.current(); if (!activeRef.current) { complete(); return; } const t = Math.min(1, (now - begun) / 200), ease = 1 - (1 - t) ** 3; nodes.forEach((n, i) => { if (!n.fixed) { n.x = seeds[i].x + (target[i * 2] - seeds[i].x) * ease; n.y = seeds[i].y + (target[i * 2 + 1] - seeds[i].y) * ease; } }); hit.current = hitIndex(nodes); paint(); if (t < 1) animation = requestAnimationFrame(tween); else { complete(); stopIntro.current = () => {}; } };
        animation = requestAnimationFrame(tween);
      } else { apply(); if (firstReply && !incremental) { camera.current = fitGraph(nodes, dimensions.current.width, dimensions.current.height); fitMode.current = true; } schedule(); }
    };
    worker.current.onmessage = event => receivePositions(event.data);
    if (incremental && positions.every(n => n.fixed)) receivePositions({ revision: currentRevision, positions: Float32Array.from(positions.flatMap(n => [n.x, n.y])), ticks: 0, workMs: 0, cached: true });
    else worker.current.postMessage({ revision: currentRevision, nodes: positions, links: links.map(l => ({ source: l.source.id, target: l.target.id })), incremental });
    if (!activeRef.current) worker.current.postMessage({ kind: 'pause' });
    const wheel = (e: WheelEvent) => { e.preventDefault(); interact.current(); fitMode.current = false; const rect = element.getBoundingClientRect(), c = camera.current, px = e.clientX - rect.left - dimensions.current.width / 2, py = e.clientY - rect.top - dimensions.current.height / 2; const next = Math.max(.001, Math.min(6, c.scale * Math.exp(-e.deltaY * .001))); c.x = px - (px - c.x) * next / c.scale; c.y = py - (py - c.y) * next / c.scale; c.scale = next; schedule(); };
    const theme = () => { invalidate.current(); schedule(); };
    element.addEventListener('wheel', wheel, { passive: false }); const media = matchMedia('(prefers-color-scheme: dark)'); media.addEventListener('change', theme);
    return () => { invalidate.current(); bitmap.width = bitmap.height = 0; stopIntro.current(); stopIntro.current = () => {}; worker.current?.postMessage({ kind: 'pause' }); observer.disconnect(); element.removeEventListener('wheel', wheel); media.removeEventListener('change', theme); };
  }, [visible, graph.rels, safeLimit]);
  useEffect(() => {
    const visibility = () => { activeRef.current = active && !document.hidden; worker.current?.postMessage({ kind: activeRef.current ? 'resume' : 'pause' }); if (activeRef.current) schedule(); else { invalidate.current(); stopIntro.current(); cancelAnimationFrame(request.current); request.current = 0; } };
    visibility(); document.addEventListener('visibilitychange', visibility); return () => document.removeEventListener('visibilitychange', visibility);
  }, [active]);
  useLayoutEffect(() => { invalidate.current(); for (const n of scene.current.nodes) n.match = nodeMatches(n.data, search); const node = scene.current.nodes.find(n => n.match); if (node) { fitMode.current = false; camera.current.x = -node.x * camera.current.scale; camera.current.y = -node.y * camera.current.scale; } schedule(); }, [search]);
  function point(event: MouseEvent | PointerEvent) { const rect = canvas.current!.getBoundingClientRect(), c = camera.current; return { x: (event.clientX - rect.left - dimensions.current.width / 2 - c.x) / c.scale, y: (event.clientY - rect.top - dimensions.current.height / 2 - c.y) / c.scale }; }
  function hitNode(event: MouseEvent | PointerEvent) { const p = point(event); return hit.current(p.x, p.y, 21 / Math.min(1, camera.current.scale)); }
  function select(value: GNode | GRel | null) { const since = start('node-click'); setSelected(value); void painted('properties', since); }
  async function expandNode(node: GNode) {
    if (!canExpand || expanding) return; setExpanding(true); const expected = source.current, since = start('expand-request');
    try { const result = await api('expand', { sessionId, elementId: node.id, limit: safeLimit }); if (alive.current && source.current === expected && result.graph) { expandStarted.current = since; setGraph(g => { const mergeStart = start('expand-merge'); const merged = mergeGraph(g, result.graph!); finish('expand-merge', mergeStart); return merged; }); } }
    catch (error) { if (alive.current && source.current === expected) onError(error); } finally { if (alive.current) setExpanding(false); }
  }
  const accessibleContent = useMemo(() => accessible ? <>{visible.map(n => <button onClick={() => select(n)} key={n.id}>{nodeCaption(n)}</button>)}{graph.rels.filter(r => ids.has(r.start) && ids.has(r.end)).map(r => <button onClick={() => select(r)} key={r.id}>{r.rel_type} · {r.id}</button>)}</> : null, [accessible, visible, graph.rels, ids]);
  return <div class="graph-area"><div class="graph-main"><div class="toolbar"><input disabled={!ready} aria-label="Search graph" placeholder="Search labels or property values" value={search} onInput={e => setSearch(e.currentTarget.value)} /><button onClick={fit}>Reset view</button><span class="graph-message" role="status">{graph.nodes.length > safeLimit ? `${graph.nodes.length}개 중 ${safeLimit}개 표시` : expanding ? 'Expanding…' : 'Wheel: zoom · drag: pan · double-click: expand'}</span></div>
    <div class="canvas-wrap" ref={container}><canvas ref={canvas} hidden={!ready} data-ready={ready} data-total-nodes={graph.nodes.length} aria-label={`Graph: ${visible.length} nodes, ${graph.rels.length} relationships`} role="img"
      onPointerDown={e => { e.currentTarget.setPointerCapture(e.pointerId); drag.current = { node: hitNode(e), x: e.clientX, y: e.clientY, moved: false }; }}
      onPointerMove={e => { const d = drag.current; if (!d) return; const dx = e.clientX - d.x, dy = e.clientY - d.y; if (Math.abs(dx) + Math.abs(dy) > 2) d.moved = true; if (d.node) { invalidate.current(); const p = point(e); d.node.x = p.x; d.node.y = p.y; worker.current?.postMessage({ kind: 'pin', id: d.node.id, ...p }); hit.current = hitIndex(scene.current.nodes); } else { interact.current(); fitMode.current = false; camera.current.x += dx; camera.current.y += dy; } d.x = e.clientX; d.y = e.clientY; schedule(); }}
      onPointerUp={e => { const d = drag.current; drag.current = null; if (d?.moved) return; const node = hitNode(e); if (node) { select(node.data); return; } const p = point(e); const link = scene.current.links.find(l => l.source.visible && l.target.visible && curveDistance(edgeCurve(l.source, l.target, l.offset, l.data.start === l.data.end), p) < 8 / camera.current.scale); select(link?.data ?? null); }}
      onPointerCancel={() => { drag.current = null; }} onDblClick={e => { const node = hitNode(e); if (node) void expandNode(node.data); }} />{!ready && <div class="graph-loading" role="status">Preparing stable graph…</div>}</div>
    <div class="legend">{labels.map((label, i) => <span key={label}><i style={{ background: colors[i % colors.length] }} />{label} {counts.get(label)}</span>)}</div>
    <details class="graph-accessible" onToggle={e => setAccessible(e.currentTarget.open)}><summary>Nodes & relationships (keyboard access)</summary><div class="graph-node-list">{accessibleContent}</div></details>
  </div>{selected && <aside class="properties" aria-label="Graph properties"><div class="section-title"><strong>Properties</strong><button onClick={() => setSelected(null)} aria-label="Close properties">×</button></div><dl><dt>id</dt><dd>{selected.id}</dd><dt>{'labels' in selected ? 'labels' : 'type'}</dt><dd>{'labels' in selected ? selected.labels.join(', ') : selected.rel_type}</dd>{'start' in selected && <><dt>start → end</dt><dd>{selected.start} → {selected.end}</dd></>}{selected.props.map(([key, value]) => <div key={key}><dt>{key}</dt><dd><pre>{valueText(value, true)}</pre></dd></div>)}</dl>{'labels' in selected && canExpand && <button disabled={expanding} onClick={() => void expandNode(selected)}>Expand neighbors</button>}</aside>}</div>;
}
