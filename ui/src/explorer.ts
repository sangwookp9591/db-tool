import type { ExplorerNode } from './contract';

export type ExplorerTree = Record<string, ExplorerNode[]>;
export interface ExplorerCatalog { sessionId: string; tree: ExplorerTree }

/** Load containers down to the table/view level, leaving columns and data lazy. */
export async function loadExplorerCatalog(
  load: (parent: string | null) => Promise<ExplorerNode[]>,
  relational: boolean,
  current: () => boolean,
  loaded: (parent: string, nodes: ExplorerNode[]) => void,
  failed: (error: unknown) => void,
) {
  const queue: (string | null)[] = [null];
  const seen = new Set<string>();
  while (queue.length && current()) {
    // Bound metadata requests even when a server exposes many databases/schemas.
    await Promise.all(queue.splice(0, 4).map(async parent => {
      if (!current()) return;
      try {
        const nodes = await load(parent);
        if (!current()) return;
        loaded(parent ?? '$root', nodes);
        if (relational) for (const node of nodes) {
          if (node.has_children && ['schema', 'database', 'folder'].includes(node.kind) && !seen.has(node.id)) {
            seen.add(node.id);
            queue.push(node.id);
          }
        }
      } catch (error) { if (current()) failed(error); }
    }));
  }
}

export interface SqlTableName { name: string; namespace: string[] }
export function sqlTableNames(tree: ExplorerTree): SqlTableName[] {
  const tables: SqlTableName[] = [], seen = new Set<string>();
  function visit(parent: string, namespace: string[]) {
    if (seen.has(parent)) return;
    seen.add(parent);
    for (const node of tree[parent] ?? []) {
      if (node.kind === 'table' || node.kind === 'view') tables.push({ name: node.label, namespace });
      else if (['schema', 'database', 'folder'].includes(node.kind)) {
        visit(node.id, node.kind === 'folder' ? namespace : [...namespace, node.label]);
      }
    }
  }
  visit('$root', []);
  return tables;
}
