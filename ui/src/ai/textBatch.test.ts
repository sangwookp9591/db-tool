import { afterEach, expect, it, vi } from 'vitest';
import { textBatch } from './textBatch';
afterEach(() => vi.useRealTimers());
it('batches tokens for 50ms and releases timers on completion or unmount', () => {
  vi.useFakeTimers(); const update = vi.fn(), batch = textBatch(update);
  for (const token of ['한', '글', ' ', 'stream']) batch.push(token);
  expect(vi.getTimerCount()).toBe(1); vi.advanceTimersByTime(49); expect(update).not.toHaveBeenCalled();
  vi.advanceTimersByTime(1); expect(update).toHaveBeenCalledExactlyOnceWith('한글 stream'); expect(vi.getTimerCount()).toBe(0);
  batch.push(' tail'); batch.flush(); expect(update).toHaveBeenLastCalledWith('한글 stream tail');
  batch.push(' abandoned'); batch.dispose(); batch.push(' late'); vi.runAllTimers();
  expect(update).toHaveBeenCalledTimes(2); expect(vi.getTimerCount()).toBe(0);
});
