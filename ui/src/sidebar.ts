export interface SidebarPreferences { connectionsCollapsed: boolean; explorerCollapsed: boolean; compact: boolean; ratio: number }
export const SIDEBAR_STORAGE_KEY = 'axdb.sidebar';
export const sidebarRatio = (ratio: number) => Math.max(20, Math.min(65, ratio));
export function sidebarPreferences(value: unknown): SidebarPreferences {
  const data = value && typeof value === 'object' ? value as Partial<SidebarPreferences> : {};
  return {
    connectionsCollapsed: data.connectionsCollapsed === true,
    explorerCollapsed: data.explorerCollapsed === true,
    compact: data.compact === true,
    ratio: typeof data.ratio === 'number' && Number.isFinite(data.ratio) ? sidebarRatio(data.ratio) : 35,
  };
}
export function loadSidebarPreferences(): SidebarPreferences {
  try { return sidebarPreferences(JSON.parse(localStorage.getItem(SIDEBAR_STORAGE_KEY) ?? 'null')); }
  catch { return sidebarPreferences(null); }
}
