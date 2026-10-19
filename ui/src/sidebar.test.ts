import { describe, expect, it } from 'vitest';
import { loadSidebarPreferences, sidebarPreferences, sidebarRatio } from './sidebar';
describe('sidebar preferences', () => {
  it('keeps both sections usable for invalid or extreme saved values', () => {
    expect(sidebarPreferences(null)).toEqual({ connectionsCollapsed: false, explorerCollapsed: false, compact: false, ratio: 35 });
    expect(sidebarPreferences({ compact: 'true', ratio: Infinity }).compact).toBe(false);
    expect(sidebarPreferences({ ratio: Infinity }).ratio).toBe(35);
    expect(sidebarRatio(-10)).toBe(20);
    expect(sidebarRatio(100)).toBe(65);
    expect(sidebarPreferences({ connectionsCollapsed: true, explorerCollapsed: true, compact: true, ratio: 42 })).toEqual({ connectionsCollapsed: true, explorerCollapsed: true, compact: true, ratio: 42 });
  });
  it('works when storage is unavailable', () => {
    expect(loadSidebarPreferences().ratio).toBe(35);
  });
});
