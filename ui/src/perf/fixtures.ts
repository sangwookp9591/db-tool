import type { Graph, ResultSet, Table } from '../contract';
export function fixtureTable(count: number, width: number, offset = 0): Table {
  return { columns: Array.from({ length: width }, (_, c) => ({ name: `column_${c}`, type_name: c % 2 ? 'text' : 'int8' })), rows: Array.from({ length: count }, (_, r) => Array.from({ length: width }, (_, c) => c % 2 ? { t: 'Text' as const, v: `row ${r + offset} column ${c}` } : { t: 'Int' as const, v: String(9007199254740993n + BigInt(r + offset + c)) })) };
}
export function fixtureGraph(count: number, offset = 0, parent?: string): Graph {
  return { nodes: Array.from({ length: count }, (_, i) => ({ id: `bench:n${i + offset}`, labels: [`Label${i % 4}`], props: [['name', { t: 'Text', v: `Node ${i + offset}` }]] })), rels: Array.from({ length: count * 2 }, (_, i) => ({ id: `bench:r${i + offset * 2}`, start: parent && i < 2 ? parent : `bench:n${offset + i % count}`, end: `bench:n${offset + (i * 7 + 1) % count}`, rel_type: 'LINK', props: [] })) };
}
export function fixtureResult(table: Table | null = null, graph: Graph | null = null): ResultSet { return { title: 'Benchmark', table, graph, documents: null, message: null, affected: null, row_count: table?.rows.length ?? graph?.nodes.length ?? 0, truncated: false, page: null, elapsed_ms: 0 }; }
