import { invoke } from '@tauri-apps/api/core';
import type { DriverManifest, Profile, ExplorerNode, ResultSet, ExecRequest, HistoryEntry } from './contract';
export interface Commands {
  agent_bridge_status: { args: Record<string, never>; result: boolean };
  agent_bridge_set: { args: { enabled: boolean }; result: void };
  agent_bridge_reply: { args: { requestId: string; result: unknown }; result: void };
  drivers_list: { args: Record<string, never>; result: DriverManifest[] };
  profiles_list: { args: Record<string, never>; result: Profile[] };
  profile_save: { args: { profile: Profile; secret: string | null }; result: Profile };
  profile_delete: { args: { profileId: string }; result: void };
  connection_test: { args: { profile: Profile; secret: string | null }; result: string };
  session_open: { args: { profileId: string; secret: string | null }; result: string };
  session_close: { args: { sessionId: string }; result: void };
  explorer_children: { args: { sessionId: string; parent: string | null }; result: ExplorerNode[] };
  browse: { args: { sessionId: string; nodeId: string; offset: number; limit: number; runId: string }; result: ResultSet };
  execute: { args: { sessionId: string; runId: string; req: ExecRequest }; result: ResultSet[] };
  execute_agent: { args: { sessionId: string; runId: string; req: ExecRequest }; result: ResultSet[] };
  cancel: { args: { runId: string }; result: void };
  expand: { args: { sessionId: string; elementId: string; limit: number }; result: ResultSet };
  history_list: { args: { limit: number }; result: HistoryEntry[] };
}
export const isMock = !('__TAURI_INTERNALS__' in window);
export async function api<K extends keyof Commands>(command: K, args: Commands[K]['args']): Promise<Commands[K]['result']> {
  if (isMock) return (await import('./mock')).mockInvoke(command, args);
  return invoke<Commands[K]['result']>(command, args);
}
