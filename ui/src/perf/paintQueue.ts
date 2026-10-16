// Let the first result paint before decoding a backlog of later stream chunks.
export function paintQueue<T>(consume: (item: T) => boolean, onError: (error: unknown) => void) {
  const pending: T[] = [];
  let disposed = false, held = false, painted = false, frame = 0, timer: ReturnType<typeof setTimeout> | undefined, holdTimer: ReturnType<typeof setTimeout> | undefined;
  function dispose() { disposed = true; pending.length = 0; cancelAnimationFrame(frame); clearTimeout(timer); clearTimeout(holdTimer); }
  function drain() {
    timer = undefined;
    if (disposed || held) return;
    const started = performance.now();
    while (pending.length && !disposed) {
      let first = false;
      try { first = consume(pending.shift()!); } catch (error) { dispose(); onError(error); return; }
      if (first && !painted) {
        painted = true; held = true;
        const release = () => { if (!held || disposed) return; held = false; clearTimeout(holdTimer); cancelAnimationFrame(frame); drain(); };
        frame = requestAnimationFrame(() => { frame = requestAnimationFrame(release); });
        // Occluded native windows may suspend rAF; query completion must still progress.
        holdTimer = setTimeout(release, 100);
        return;
      }
      if (pending.length && performance.now() - started >= 4) { timer = setTimeout(drain, 0); return; }
    }
  }
  return { push(item: T) { if (disposed) return; pending.push(item); if (!held && timer === undefined) drain(); }, dispose };
}
