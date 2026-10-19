import { invoke } from '@tauri-apps/api/core';
import { api, isMock } from '../api';

export const ONBOARDING_VERSION = 1;
export interface OnboardingStatus { version: number; automatic: boolean }
export function shouldShowOnboarding(status: OnboardingStatus) {
  return status.automatic && status.version < ONBOARDING_VERSION;
}
let mockVersion = 0;
let mockBridge = false;
export async function onboardingStatus(): Promise<OnboardingStatus> {
  if (isMock) return { version: mockVersion, automatic: new URLSearchParams(location.search).has('onboarding') };
  return invoke('onboarding_status');
}
export async function completeOnboarding() {
  if (isMock) { mockVersion = ONBOARDING_VERSION; return; }
  await invoke('onboarding_complete');
}
export async function bridgeStatus() { return isMock ? mockBridge : api('agent_bridge_status', {}); }
export async function setBridge(enabled: boolean) {
  if (isMock) mockBridge = enabled;
  else await api('agent_bridge_set', { enabled });
  window.dispatchEvent(new CustomEvent('axdb-bridge-change', { detail: enabled }));
}
export async function copyMcpConfig() {
  if (isMock) throw new Error('모의 화면에서는 실제 MCP 설정을 복사할 수 없습니다. 데스크톱 앱에서 켜 주세요.');
  const config = await invoke('onboarding_mcp_config');
  await navigator.clipboard.writeText(JSON.stringify(config, null, 2));
}
