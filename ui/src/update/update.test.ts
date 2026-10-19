import { afterEach, expect, it, vi } from 'vitest';
import { parseNotes, safeExternalUrl, summarizeNotes, releaseEntries } from './notes';
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

it('groups release sections, removes tone tags and preserves safe-render input', () => {
  const result = summarizeNotes('# Release\nOverview\n## 설치형 드라이버 {good}\n- **선택** 설치\n- More detail\n### Details\n<script>bad()</script>\n## 변경 주의\n- Removed option\n## 수정\nFallback paragraph');
  expect(result.sections.map(({ label, change, tone }) => ({ label, change, tone }))).toEqual([
    { label: '설치형 드라이버', change: '선택 설치', tone: 'good' },
    { label: '변경 주의', change: 'Removed option', tone: 'bad' },
    { label: '수정', change: 'Fallback paragraph', tone: 'neutral' },
  ]);
  expect(result.sections[0].blocks.at(-1)).toEqual({ kind: 'paragraph', text: '<script>bad()</script>' });
  expect(result.intro).toHaveLength(2);
  expect(summarizeNotes('Plain notes').sections).toEqual([]);
});

it('turns release bullets into dated timeline entries with legacy categories and safe links', () => {
  const notes = '<!-- date: 2026-10-19 -->\n# Release\n## 추가 {new}\n- 첫 문장입니다. 나머지 설명입니다.\n## 개선 {neutral}\n- v0.2.0 버전 지원. 설명\n## 수정 {fixed}\n- [안내](https://example.com/notes)\n## 이전 {good}\n- <script>bad()</script> [bad](javascript:alert)';
  const entries = releaseEntries(notes, '0.2.0', '2026-01-01');
  expect(entries.map(({ id, date, category, title, summary }) => ({ id, date, category, title, summary }))).toEqual([
    { id: '0.2.0:0', date: '2026-10-19', category: 'new', title: '첫 문장입니다.', summary: '나머지 설명입니다.' },
    { id: '0.2.0:1', date: '2026-10-19', category: 'improved', title: 'v0.2.0 버전 지원.', summary: '설명' },
    { id: '0.2.0:2', date: '2026-10-19', category: 'fixed', title: '안내', summary: '' },
    { id: '0.2.0:3', date: '2026-10-19', category: 'new', title: '<script>bad()</script> bad', summary: '' },
  ]);
  expect(entries[2].href).toBe('https://example.com/notes');
  expect(entries[3].href).toBeUndefined();
  expect(releaseEntries('## 개선 사항\n- 한 항목', '0.3.0', '2026-11-01T00:00:00Z')[0]).toMatchObject({ date: '2026-11-01', category: 'improved' });
  expect(parseNotes('<!-- date: 2026-10-19 -->\n# Notes')).toEqual([{ kind: 'heading', level: 1, text: 'Notes' }]);
});
