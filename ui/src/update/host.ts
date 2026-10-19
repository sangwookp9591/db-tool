import { safeExternalUrl } from './notes';
import { invoke } from '@tauri-apps/api/core';
import type { Update } from '@tauri-apps/plugin-updater';
import { isMock } from '../api';

export interface Preferences { auto_check: boolean; skipped_version: string | null }
export interface UpdateSettings { preferences: Preferences; automatic: boolean; current_version: string }
export type AppUpdate = Pick<Update, 'version' | 'currentVersion' | 'date' | 'body' | 'download' | 'install' | 'close'>;
let mockPreferences: Preferences = { auto_check: true, skipped_version: null };
export async function settings(): Promise<UpdateSettings> {
  return isMock ? { preferences: mockPreferences, automatic: new URLSearchParams(location.search).has('update'), current_version: '0.2.0' } : invoke('updater_settings');
}
export async function configure(preferences: Preferences) {
  if (isMock) mockPreferences = preferences;
  else await invoke('updater_configure', { preferences });
}
export async function checkUpdate(): Promise<AppUpdate | null> {
  return isMock ? (await import('../mock')).mockUpdater() : (await import('@tauri-apps/plugin-updater')).check({ timeout: 15_000 });
}
export async function restart() {
  if (isMock) { window.dispatchEvent(new Event('axdb-mock-restart')); return; }
  await (await import('@tauri-apps/plugin-process')).relaunch();
}
export async function openReleaseLink(value: string) {
  const url = safeExternalUrl(value);
  if (!url) return;
  if (isMock) window.open(url, '_blank', 'noopener,noreferrer');
  else await (await import('@tauri-apps/plugin-opener')).openUrl(url);
}
