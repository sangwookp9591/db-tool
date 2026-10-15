import type { Graph, GNode } from './contract';
import { valueText } from './values';
export function mergeGraph(a: Graph, b: Graph): Graph {
  return { nodes: [...new Map([...a.nodes, ...b.nodes].map(n => [n.id, n])).values()], rels: [...new Map([...a.rels, ...b.rels].map(r => [r.id, r])).values()] };
}
export function nodeCaption(node: GNode): string {
  const preferred = ['name', 'title'].map(key => node.props.find(([k, v]) => k === key && v.t === 'Text'));
  const prop = preferred.find(Boolean) ?? node.props.find(([, v]) => v.t === 'Text');
  return prop ? valueText(prop[1]) : node.labels[0] ?? node.id;
}
export function nodeMatches(node: GNode, search: string): boolean {
  const query = search.trim().toLocaleLowerCase();
  return !!query && [...node.labels, ...node.props.map(([, value]) => valueText(value))].some(s => s.toLocaleLowerCase().includes(query));
}
export interface GraphPosition { id: string; x: number; y: number; fixed?: boolean }
export function seedPositions(graph: Graph, previous: GraphPosition[]): GraphPosition[] {
  const old = new Map(previous.map(n => [n.id, n]));
  const parents = new Map<string, string>();
  for (const r of graph.rels) { if (old.has(r.start)) parents.set(r.end, r.start); if (old.has(r.end)) parents.set(r.start, r.end); }
  return graph.nodes.map((n, i) => {
    const existing = old.get(n.id); if (existing) return { id: existing.id, x: existing.x, y: existing.y, fixed: true };
    const parent = old.get(parents.get(n.id) ?? '');
    const radius = parent ? 45 : Math.sqrt(i + 1) * 20;
    return { id: n.id, x: (parent?.x ?? 0) + Math.cos(i * 2.4) * radius, y: (parent?.y ?? 0) + Math.sin(i * 2.4) * radius };
  });
}
export function fitGraph(nodes: GraphPosition[], width: number, height: number, padding = 48) {
  if (!nodes.length) return { x: 0, y: 0, scale: 1 };
  const left = Math.min(...nodes.map(n => n.x)) - 20, right = Math.max(...nodes.map(n => n.x)) + 20;
  const top = Math.min(...nodes.map(n => n.y)) - 20, bottom = Math.max(...nodes.map(n => n.y)) + 20;
  padding = Math.min(padding, Math.max(0, width / 6), Math.max(0, height / 6));
  const scale = Math.max(.001, Math.min(2, Math.max(1, width - padding * 2) / (right - left), Math.max(1, height - padding * 2) / (bottom - top)));
  return { x: -(left + right) / 2 * scale, y: -(top + bottom) / 2 * scale, scale };
}
export function relationOffsets(rels: Graph['rels']) {
  const groups = new Map<string, Graph['rels']>();
  for (const r of rels) { const key = JSON.stringify([r.start, r.end].sort()); const group = groups.get(key) ?? []; group.push(r); groups.set(key, group); }
  const offsets = new Map<string, number>();
  for (const group of groups.values()) group.forEach((r, i) => offsets.set(r.id, r.start === r.end ? (i + 1) * 24 : (i - (group.length - 1) / 2) * 32 * (r.start < r.end ? 1 : -1)));
  return offsets;
}
interface Point { x: number; y: number }
export interface EdgeCurve { a: Point; b: Point; c: Point; d?: Point }
export function edgeCurve(a: Point, b: Point, offset: number, loop = false): EdgeCurve {
  if (loop) return { a: { x: a.x + 12, y: a.y - 12 }, b: { x: a.x - 12, y: a.y - 12 }, c: { x: a.x + 40 + offset, y: a.y - 60 - offset }, d: { x: a.x - 40 - offset, y: a.y - 60 - offset } };
  const dx = b.x - a.x, dy = b.y - a.y, length = Math.hypot(dx, dy) || 1;
  const c = { x: (a.x + b.x) / 2 - dy / length * offset * 2, y: (a.y + b.y) / 2 + dx / length * offset * 2 };
  const start = Math.atan2(c.y - a.y, c.x - a.x), end = Math.atan2(b.y - c.y, b.x - c.x);
  return { a: { x: a.x + Math.cos(start) * 19, y: a.y + Math.sin(start) * 19 }, b: { x: b.x - Math.cos(end) * 19, y: b.y - Math.sin(end) * 19 }, c };
}
export function curvePoint(g: EdgeCurve, t: number): Point {
  const u = 1 - t;
  return g.d ? { x: u ** 3 * g.a.x + 3 * u * u * t * g.c.x + 3 * u * t * t * g.d.x + t ** 3 * g.b.x, y: u ** 3 * g.a.y + 3 * u * u * t * g.c.y + 3 * u * t * t * g.d.y + t ** 3 * g.b.y } : { x: u * u * g.a.x + 2 * u * t * g.c.x + t * t * g.b.x, y: u * u * g.a.y + 2 * u * t * g.c.y + t * t * g.b.y };
}
export function curveDistance(g: EdgeCurve, point: Point) {
  let distance = Infinity, a = g.a;
  for (let i = 1; i <= 32; i++) { const b = curvePoint(g, i / 32), dx = b.x - a.x, dy = b.y - a.y, t = Math.max(0, Math.min(1, ((point.x - a.x) * dx + (point.y - a.y) * dy) / (dx * dx + dy * dy || 1))); distance = Math.min(distance, Math.hypot(point.x - a.x - t * dx, point.y - a.y - t * dy)); a = b; }
  return distance;
}
