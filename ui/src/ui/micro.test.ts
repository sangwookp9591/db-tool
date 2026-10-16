// React Bits interaction ports (MIT + Commons Clause), original author David Haz.
import { afterEach, expect, it, vi } from 'vitest';
import { spring, springValue } from './spring';
import { startFuse } from './FuseButton';
import { clampValue, sliderKey } from './slider';
import { radioKey } from './JellyRadio';
import { filterCommands } from './CommandPalette';
import { beamPoint } from './IntegrationBeam';

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

it('command search matches every word across label, group and keywords with Unicode normalization', () => {
  const items = [{ id: 'orders', label: '주문', group: 'Tables', keywords: 'public.orders', onSelect: () => {} }, { id: 'history', label: 'Recent query', group: 'History', onSelect: () => {} }];
  expect(filterCommands(items, 'TABLES 주문'.normalize('NFD'))).toEqual([items[0]]);
  expect(filterCommands(items, 'public.orders missing')).toEqual([]);
  expect(filterCommands(items, '  ')).toEqual(items);
});

it('beam curve preserves endpoints and reverses consistently in horizontal and vertical layouts', () => {
  for (const end of [{ x: 100, y: 30 }, { x: 30, y: 100 }]) {
    const start = { x: 0, y: 0 };
    expect(beamPoint(start, end, -1)).toEqual(start);
    expect(beamPoint(start, end, 2)).toEqual(end);
    const forward = beamPoint(start, end, .3), reverse = beamPoint(end, start, .7);
    expect(forward.x).toBeCloseTo(reverse.x); expect(forward.y).toBeCloseTo(reverse.y);
  }
});

it('critical spring converges monotonically without overshoot, then stops scheduling', () => {
  const values = [0, 20, 60, 100, 220, 500].map(t => springValue(0, 1, t));
  expect(values[0]).toBe(0);
  expect(values).toEqual([...values].sort((a, b) => a - b));
  expect(values.every(v => v >= 0 && v <= 1)).toBe(true);
  expect(values.at(-1)).toBeCloseTo(1, 6);
  const queue: FrameRequestCallback[] = [];
  const now = performance.now();
  vi.stubGlobal('requestAnimationFrame', vi.fn((cb: FrameRequestCallback) => { queue.push(cb); return queue.length; }));
  const paint = vi.fn();
  spring(0, 1, paint);
  queue.shift()!(now + 100);
  queue.shift()!(now + 300);
  expect(paint).toHaveBeenLastCalledWith(1);
  expect(queue).toHaveLength(0);
});

it('reduced motion paints the target without scheduling a frame', () => {
  vi.stubGlobal('matchMedia', () => ({ matches: true }));
  const raf = vi.fn(); vi.stubGlobal('requestAnimationFrame', raf);
  const paint = vi.fn(); spring(0, 1, paint);
  expect(paint).toHaveBeenCalledExactlyOnceWith(1);
  expect(raf).not.toHaveBeenCalled();
});

it('fuse commits once at the deadline; undo cancels the pending commit', () => {
  vi.useFakeTimers();
  const commit = vi.fn(), undo = vi.fn();
  const fuse = startFuse(commit, undo);
  vi.advanceTimersByTime(4999); expect(commit).not.toHaveBeenCalled();
  vi.advanceTimersByTime(1); expect(commit).toHaveBeenCalledTimes(1);
  fuse.dispose(); expect(commit).toHaveBeenCalledTimes(1);
  const second = startFuse(commit, undo);
  vi.advanceTimersByTime(2000); second.undo();
  vi.advanceTimersByTime(5000); second.dispose();
  expect(undo).toHaveBeenCalledTimes(1); expect(commit).toHaveBeenCalledTimes(1);
});

it('unmount policy commits by default and cancels when explicitly selected', () => {
  vi.useFakeTimers();
  const commit = vi.fn(), undo = vi.fn();
  startFuse(commit, undo).dispose();
  startFuse(commit, undo, 5000, 'cancel').dispose();
  vi.runAllTimers(); expect(commit).toHaveBeenCalledTimes(1); expect(undo).not.toHaveBeenCalled();
});

it('slider clamps invalid, fractional and off-grid endpoints, and supports all keyboard steps', () => {
  expect(clampValue(-10, 1, 600)).toBe(1);
  expect(clampValue(Infinity, 1, 600)).toBe(1);
  expect(clampValue(999, 1, 600)).toBe(600);
  expect(clampValue(.3, 0, 1, .1)).toBe(.3);
  expect(clampValue(99, 0, 99, 10)).toBe(99);
  expect(sliderKey('Home', 30, 1, 600)).toBe(1);
  expect(sliderKey('End', 30, 1, 600)).toBe(600);
  expect(sliderKey('ArrowUp', 599, 1, 600, 10)).toBe(600);
  expect(sliderKey('ArrowLeft', 1, 1, 600)).toBe(1);
  expect(sliderKey('PageDown', 30, 1, 600)).toBe(20);
  expect(sliderKey('ArrowRight', 30, 1, 600, 1, true)).toBe(40);
  expect(sliderKey('Tab', 30, 1, 600)).toBeUndefined();
});

it('radio keyboard wraps, skips disabled options, and handles Home/End and empty groups', () => {
  const items = [{}, { disabled: true }, {}];
  expect(radioKey('ArrowRight', 0, items)).toBe(2);
  expect(radioKey('ArrowDown', 2, items)).toBe(0);
  expect(radioKey('ArrowLeft', 0, items)).toBe(2);
  expect(radioKey('Home', 2, items)).toBe(0);
  expect(radioKey('End', 0, items)).toBe(2);
  expect(radioKey('ArrowRight', 0, [{ disabled: true }])).toBeUndefined();
  expect(radioKey('ArrowRight', -1, [])).toBeUndefined();
  expect(radioKey('Tab', 0, items)).toBeUndefined();
});
