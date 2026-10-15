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
    const existing = old.get(n.id); if (existing) return { ...existing, fixed: true };
    const parent = old.get(parents.get(n.id) ?? '');
    const radius = parent ? 45 : Math.sqrt(i + 1) * 20;
    return { id: n.id, x: (parent?.x ?? 0) + Math.cos(i * 2.4) * radius, y: (parent?.y ?? 0) + Math.sin(i * 2.4) * radius };
  });
}
