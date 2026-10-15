export interface Metric { name: string; startTime: number; duration: number; detail?: unknown }
const metrics: Metric[] = [];
export function start(name: string) { performance.clearMarks(`dbtool:${name}`); performance.mark(`dbtool:${name}`); return performance.now(); }
export function finish(name: string, since: number, detail?: unknown) {
  const duration = performance.now() - since;
  performance.clearMeasures(`dbtool:${name}`);
  performance.measure(`dbtool:${name}`, { start: since, duration, detail });
  metrics.push({ name, startTime: since, duration, detail });
  if (metrics.length > 2000) metrics.shift();
  return duration;
}
export const frame = () => new Promise<number>(resolve => requestAnimationFrame(resolve));
export async function painted(name: string, since = start(name)) { await frame(); await frame(); return finish(name, since); }
export function stats(samples: number[]) {
  const sorted = [...samples].sort((a, b) => a - b);
  const at = (p: number) => sorted[Math.max(0, Math.ceil(sorted.length * p) - 1)] ?? 0;
  return { p50: at(.5), p95: at(.95), max: at(1), samples: samples.length };
}
export async function sampleFrames(action: (index: number) => void, count = 60) {
  const samples: number[] = []; let previous = await frame();
  for (let i = 0; i < count; i++) { action(i); const now = await frame(); samples.push(now - previous); previous = now; }
  return stats(samples);
}
const scenarios = new Map<string, () => Promise<unknown>>();
export function registerScenario(name: string, run: () => Promise<unknown>) { scenarios.set(name, run); }
export const perf = {
  marks: () => [...metrics],
  clear: () => { metrics.length = 0; for (const e of performance.getEntriesByType('mark')) if (e.name.startsWith('dbtool:')) performance.clearMarks(e.name); for (const e of performance.getEntriesByType('measure')) if (e.name.startsWith('dbtool:')) performance.clearMeasures(e.name); },
  run: async (scenario: string) => { const run = scenarios.get(scenario); if (!run) throw new Error(`Unknown scenario: ${scenario}`); return run(); }
};
declare global { interface DBTool { perf: typeof perf } interface Window { dbtool: DBTool } }
if (typeof window !== 'undefined') window.dbtool = { ...window.dbtool, perf };
const graphProbes = new Map<symbol, () => { frames: number; positions: Float32Array; ready: boolean }>();
export function registerGraphProbe(read: () => { frames: number; positions: Float32Array; ready: boolean }) { const key = Symbol(); graphProbes.set(key, read); return () => { graphProbes.delete(key); }; }
export function readGraphProbes() { return [...graphProbes.values()].map(read => read()); }
