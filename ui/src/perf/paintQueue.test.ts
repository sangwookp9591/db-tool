import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { paintQueue } from './paintQueue';
beforeEach(() => { vi.useFakeTimers(); vi.stubGlobal('requestAnimationFrame', (run: () => void) => setTimeout(run, 16)); vi.stubGlobal('cancelAnimationFrame', clearTimeout); });
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });
describe('paint-priority stream delivery', () => {
  it('preserves FIFO and defers the backlog until two frames after the first rows', () => {
    const seen: number[] = [], error = vi.fn(); const queue = paintQueue<number>(n => { seen.push(n); return n === 1; }, error);
    [0, 1, 2, 3].forEach(queue.push); expect(seen).toEqual([0, 1]); vi.advanceTimersByTime(16); expect(seen).toEqual([0, 1]); vi.advanceTimersByTime(16); expect(seen).toEqual([0, 1, 2, 3]); expect(error).not.toHaveBeenCalled(); queue.dispose();
  });
  it('continues when an occluded window suspends animation frames', () => {
    vi.stubGlobal('requestAnimationFrame', () => 0); const seen: number[] = []; const queue = paintQueue<number>(n => { seen.push(n); return n === 1; }, vi.fn());
    [1, 2].forEach(queue.push); vi.advanceTimersByTime(100); expect(seen).toEqual([1, 2]); queue.dispose();
  });
  it('cancels queued work and rejects a malformed chunk once', () => {
    const seen: number[] = [], error = vi.fn(); const queue = paintQueue<number>(n => { if(n===2)throw new Error('invalid chunk'); seen.push(n); return n===1; }, error);
    [1, 2, 3].forEach(queue.push); vi.runAllTimers(); expect(seen).toEqual([1]); expect(error).toHaveBeenCalledTimes(1); queue.push(4); expect(seen).toEqual([1]);
    const abandoned = paintQueue<number>(n => {seen.push(n);return true;},error); abandoned.push(5); abandoned.push(6); abandoned.dispose(); vi.runAllTimers(); expect(seen).toEqual([1,5]);
  });
});
