import { afterEach, expect, it, vi } from 'vitest';
import { parseNotes, safeExternalUrl } from './notes';
import { CHECK_INTERVAL, scheduleChecks } from './schedule';

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });
it('parses only headings and lists, preserving HTML as literal text', () => {
  expect(parseNotes('# Notes\r\n- first\r\n- second\n1. save\n2. restart\n<script>alert(1)</script>')).toEqual([
    { kind: 'heading', level: 1, text: 'Notes' },
    { kind: 'list', ordered: false, items: ['first', 'second'] },
    { kind: 'list', ordered: true, items: ['save', 'restart'] },
    { kind: 'paragraph', text: '<script>alert(1)</script>' },
  ]);
});
it('rejects script, data, file, relative, and credential-bearing links', () => {
  for (const url of ['javascript:alert(1)', 'data:text/html,bad', 'file:///tmp/private', '//example.org', '/release', 'https://user:secret@example.org']) expect(safeExternalUrl(url)).toBeNull();
  expect(safeExternalUrl('https://github.com/sangwookp9591/db-tool/releases')).toBe('https://github.com/sangwookp9591/db-tool/releases');
});
it('waits ten seconds AND idle before checking, repeats daily, and cancels on disable', () => {
  vi.useFakeTimers();
  const check = vi.fn();
  const idle = vi.fn().mockReturnValue(7), cancel = vi.fn();
  vi.stubGlobal('window', { requestIdleCallback: idle, cancelIdleCallback: cancel });
  const stop = scheduleChecks(check);
  vi.advanceTimersByTime(9999); expect(idle).not.toHaveBeenCalled();
  vi.advanceTimersByTime(1); expect(idle).toHaveBeenCalledTimes(1); expect(check).not.toHaveBeenCalled();
  idle.mock.calls[0][0](); expect(check).toHaveBeenCalledTimes(1);
  vi.advanceTimersByTime(CHECK_INTERVAL); expect(idle).toHaveBeenCalledTimes(2);
  stop(); expect(cancel).toHaveBeenCalledWith(7);
  vi.advanceTimersByTime(CHECK_INTERVAL); expect(idle).toHaveBeenCalledTimes(2);
});
it('falls back to a delayed check when requestIdleCallback is unavailable', () => {
  vi.useFakeTimers(); vi.stubGlobal('window', {});
  const check = vi.fn(), stop = scheduleChecks(check);
  vi.advanceTimersByTime(10_000); expect(check).toHaveBeenCalledOnce(); stop();
});
