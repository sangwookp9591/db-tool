export interface Positioned { id: string; x: number; y: number; fixed?: boolean }
interface Quad<T> { left: number; top: number; size: number; points: T[]; children?: Quad<T>[] }
export function hitIndex<T extends Positioned>(points: T[]) {
  const left = Math.min(0, ...points.map(p => p.x)), top = Math.min(0, ...points.map(p => p.y));
  const size = Math.max(1, ...points.map(p => Math.max(p.x - left, p.y - top))) + 1;
  function build(items: T[], x: number, y: number, size: number, depth: number): Quad<T> {
    if (items.length <= 8 || depth >= 24) return { left: x, top: y, size, points: items };
    const half = size / 2; const buckets: T[][] = [[], [], [], []];
    for (const p of items) buckets[(p.x >= x + half ? 1 : 0) + (p.y >= y + half ? 2 : 0)].push(p);
    return { left: x, top: y, size, points: [], children: buckets.map((b, i) => build(b, x + (i % 2) * half, y + (i >> 1) * half, half, depth + 1)) };
  }
  const root = build(points, left, top, size, 0);
  return (x: number, y: number, radius: number): T | undefined => {
    let best: T | undefined; let distance = radius * radius;
    function visit(q: Quad<T>) {
      const dx = Math.max(q.left - x, 0, x - q.left - q.size), dy = Math.max(q.top - y, 0, y - q.top - q.size);
      if (dx * dx + dy * dy > distance) return;
      for (const p of q.points) { const d = (p.x - x) ** 2 + (p.y - y) ** 2; if (d <= distance) { best = p; distance = d; } }
      q.children?.forEach(visit);
    }
    visit(root); return best;
  };
}
