/** History date presets. Converted to since/until only when a request is built, never at render time and never by a timer. */
export type Period = 'all' | 'hour' | 'today' | 'week' | 'month';
export const PERIOD_OPTIONS: readonly { value: Period; label: string }[] = [
  { value: 'all', label: '전체' }, { value: 'hour', label: '최근 1시간' }, { value: 'today', label: '오늘' }, { value: 'week', label: '최근 7일' }, { value: 'month', label: '최근 30일' },
];
export type Clock = () => number;
const HOUR = 3_600_000, DAY = 24 * HOUR;

/** since is inclusive and until exclusive (RFC3339, UTC). 'today' starts at the viewer's local midnight. Open-ended presets leave until unset. */
export function periodRange(period: Period, clock: Clock = Date.now): { since?: string; until?: string } {
  const now = clock();
  switch (period) {
    case 'hour': return { since: new Date(now - HOUR).toISOString() };
    case 'today': { const d = new Date(now); d.setHours(0, 0, 0, 0); return { since: d.toISOString() }; }
    case 'week': return { since: new Date(now - 7 * DAY).toISOString() };
    case 'month': return { since: new Date(now - 30 * DAY).toISOString() };
    default: return {};
  }
}
