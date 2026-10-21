import { describe, expect, it } from 'vitest';
import { PERIOD_OPTIONS, periodRange } from './period';

const NOW = new Date(2026, 9, 21, 15, 30, 0).getTime();
const clock = () => NOW;

describe('periodRange', () => {
  it('offers the Korean presets in order', () => {
    expect(PERIOD_OPTIONS.map(o => o.label)).toEqual(['전체', '최근 1시간', '오늘', '최근 7일', '최근 30일']);
  });
  it('maps 전체 to no bounds', () => { expect(periodRange('all', clock)).toEqual({}); });
  it('maps relative presets to an inclusive since measured from the injected clock', () => {
    expect(periodRange('hour', clock)).toEqual({ since: new Date(NOW - 3_600_000).toISOString() });
    expect(periodRange('week', clock)).toEqual({ since: new Date(NOW - 7 * 86_400_000).toISOString() });
    expect(periodRange('month', clock)).toEqual({ since: new Date(NOW - 30 * 86_400_000).toISOString() });
  });
  it('maps 오늘 to the local midnight', () => { expect(periodRange('today', clock)).toEqual({ since: new Date(2026, 9, 21, 0, 0, 0, 0).toISOString() }); });
  it('is evaluated per call, so a later request sees a later window', () => {
    let t = NOW; const moving = () => t;
    const first = periodRange('hour', moving).since; t += 60_000;
    expect(periodRange('hour', moving).since).not.toBe(first);
  });
  it('emits RFC3339 timestamps', () => { expect(periodRange('hour', clock).since).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/); });
});
