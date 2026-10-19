import type { ExplorerNode } from '../contract';

export async function planTarget(target: string, children: (parent: string | null) => Promise<ExplorerNode[]>) {
  const queue: (string | null)[] = [null], seen = new Set<string | null>();
  const matches: ExplorerNode[] = [];
  while (queue.length) {
    const parent = queue.shift()!;
    if (seen.has(parent)) continue;
    if (seen.size >= 64) throw new Error('탐색 범위가 큽니다. 탐색기에서 대상 스키마를 직접 선택하세요.');
    seen.add(parent);
    for (const node of await children(parent)) {
      if ((node.id === target || node.label === target) && node.actions.some(action => action.kind === 'Browse')) matches.push(node);
      // Browse nodes are leaves here: never enumerate row data or large keyspaces.
      if (node.has_children && !node.actions.some(action => action.kind === 'Browse')) queue.push(node.id);
    }
  }
  if (matches.length !== 1) throw new Error(matches.length ? '같은 이름의 대상이 여러 개입니다. 탐색기에서 스키마를 선택하세요.' : '탐색기에서 해당 테이블 또는 인덱스를 직접 선택하세요.');
  return matches[0];
}
