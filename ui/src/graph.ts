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
