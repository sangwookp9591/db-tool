import { describe, expect, it, vi } from 'vitest';
import type { ExplorerNode } from './contract';
import { loadExplorerCatalog, sqlTableNames, type ExplorerTree } from './explorer';

const node = (id: string, kind: string, label = id): ExplorerNode => ({ id, label, kind, icon: 'Folder', has_children: true, detail: null, actions: [] });
const fixture: ExplorerTree = {
  $root: [node('public', 'schema'), node('app', 'schema')],
  public: [node('public.tables', 'folder')],
  'public.tables': [],
  app: [node('app.tables', 'folder'), node('app.views', 'folder')],
  'app.tables': [node('users', 'table'), node('orders', 'table')],
  'app.views': [node('active_users', 'view')],
};

describe('connection catalog', () => {
  it('loads tables and views outside an empty public schema without expanding table columns', async () => {
    const tree: ExplorerTree = {};
    const load = vi.fn(async (parent: string | null) => fixture[parent ?? '$root']);
    await loadExplorerCatalog(load, true, () => true, (parent, nodes) => { tree[parent] = nodes; }, error => { throw error; });
    expect(tree).toEqual(fixture);
    expect(load.mock.calls.map(([parent]) => parent)).not.toContain('users');
    expect(sqlTableNames(tree)).toEqual([
      { name: 'users', namespace: ['app'] }, { name: 'orders', namespace: ['app'] }, { name: 'active_users', namespace: ['app'] },
    ]);
  });

  it('supports MySQL databases and SQLite root folders without parsing driver node IDs', async () => {
    for (const root of [node('opaque-id', 'database', 'analytics'), node('opaque-id', 'folder', 'Tables')]) {
      const tree: ExplorerTree = {};
      await loadExplorerCatalog(async parent => parent ? [node('opaque-table', 'table', 'order.items')] : [root], true, () => true, (parent, nodes) => { tree[parent] = nodes; }, error => { throw error; });
      expect(sqlTableNames(tree)).toEqual([{ name: 'order.items', namespace: root.kind === 'database' ? ['analytics'] : [] }]);
    }
  });

  it('keeps other branches available when one schema cannot be loaded', async () => {
    const tree: ExplorerTree = {}, failed = vi.fn();
    await loadExplorerCatalog(async parent => {
      if (parent === 'public.tables') throw new Error('Permission denied');
      return fixture[parent ?? '$root'];
    }, true, () => true, (parent, nodes) => { tree[parent] = nodes; }, failed);
    expect(failed).toHaveBeenCalledOnce();
    expect(sqlTableNames(tree)).toHaveLength(3);
    expect(tree['public.tables']).toBeUndefined();
  });

  it('ignores responses after refresh/disconnect and does not start child requests', async () => {
    let current = true;
    const loaded = vi.fn(), load = vi.fn(async () => { current = false; return fixture.$root; });
    await loadExplorerCatalog(load, true, () => current, loaded, vi.fn());
    expect(load).toHaveBeenCalledOnce();
    expect(loaded).not.toHaveBeenCalled();
  });

  it('leaves non-relational explorers lazy and prevents container cycles', async () => {
    const load = vi.fn(async () => [node('loop', 'folder')]);
    await loadExplorerCatalog(load, false, () => true, vi.fn(), vi.fn());
    expect(load).toHaveBeenCalledOnce();
    load.mockClear();
    await loadExplorerCatalog(load, true, () => true, vi.fn(), vi.fn());
    expect(load).toHaveBeenCalledTimes(2);
  });
});
