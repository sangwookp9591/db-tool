import type {} from './ax/administrationContract';
import type { ConnectionCommands } from './ax/connectionContract';
import type { EditorCommands } from './ax/editorContract';
import { perf, finish } from './perf';
import type { Scope, AiSettings, AiReply } from './ai/types';
import type { Channel } from '@tauri-apps/api/core';
import { invoke } from '@tauri-apps/api/core';
import type { DriverManifest, Profile, ExplorerNode, ResultSet, ExecRequest, HistoryEntry, ChangePreview, AgentClient, AgentAudit, SchemaGraph, Value, EditRequest, EditResult } from './contract';
export interface Commands extends EditorCommands, ConnectionCommands {
  context_get: { args: { sessionId: string; scope: Scope }; result: Record<string, unknown> };
  context_error: { args: { runId: string }; result: Record<string, unknown> };
  ai_settings: { args: Record<string, never>; result: { settings: AiSettings; has_key: boolean; has_ant_cli: boolean; usage?: Partial<Record<import('./ax/aiContract').Provider, import('./ax/aiContract').Usage>>; accounts?: {id: string; label: string; active: boolean; logged_in?: boolean; expired?:boolean}[] } };
  ai_login_start: { args: Record<string, never>; result: { waiting_for_code: boolean; url: string | null } };
  ai_login_submit: { args: { code: string }; result: { logged_in: boolean; account: string | null } };
  ai_login_poll: { args: Record<string, never>; result: { state: 'idle' | 'waiting' | 'done' | 'failed'; account?: string | null } };
  ai_login_cancel: { args: Record<string, never>; result: void };
  ai_login_status: { args: Record<string, never>; result: { installed: boolean; logged_in: boolean } };
  ai_logout: { args: Record<string, never>; result: void };
  ai_configure: { args: { settings: AiSettings; apiKey: string | null }; result: void };
  ai_request: { args: { sessionId: string; scope: Scope; question: string; channel: Channel<string>; runId?: string; history?: {role: 'user' | 'assistant'; content: string}[]; onEvent?: Channel<{phase:number;schema?:string;context_elapsed_ms?:number}> }; result: AiReply };

  agent_clients_list: { args: Record<string, never>; result: AgentClient[] };
  agent_client_save: { args: { input: Omit<AgentClient, 'id' | 'revoked'> & { id?: string } }; result: { client: AgentClient; token: string | null } };
  agent_client_rotate: { args: { clientId: string }; result: { token: string } };
  agent_client_revoke: { args: { clientId: string }; result: void };
  agent_audit_list: { args: Record<string, never>; result: AgentAudit[] };
  agent_approve_write: { args: { sessionId: string; runId: string; req: ExecRequest }; result: void };
  agent_bridge_ready: { args: Record<string, never>; result: void };
  agent_bridge_status: { args: Record<string, never>; result: boolean };
  agent_bridge_set: { args: { enabled: boolean }; result: void };
  agent_bridge_reply: { args: { requestId: string; result: unknown }; result: void };
  drivers_list: { args: Record<string, never>; result: DriverManifest[] };
  profiles_list: { args: Record<string, never>; result: Profile[] };
  profile_save: { args: { profile: Profile; secret: string | null }; result: Profile };
  profile_delete: { args: { profileId: string }; result: void };
  connection_test: { args: { profile: Profile; secret: string | null }; result: string };
  session_info: { args: { sessionId: string }; result: string };
  session_open_progress: { args: { profileId: string; secret: string | null; onEvent: Channel<import('./ax/connectionContract').ConnectionEvent> }; result: string };
  session_open: { args: { profileId: string; secret: string | null; onEvent?: Channel<import('./ax/connectionContract').ConnectionEvent> }; result: string };
  session_close: { args: { sessionId: string }; result: void };
  explorer_children: { args: { sessionId: string; parent: string | null }; result: ExplorerNode[] };
  schema_graph: { args: { sessionId: string; scope: string | null }; result: SchemaGraph };
  browse: { args: { filter?: [string, Value][]; sessionId: string; nodeId: string; offset: number; limit: number; runId: string }; result: ResultSet };
  execute: { args: { sessionId: string; runId: string; req: ExecRequest }; result: ResultSet[] };
  preview: { args: { sessionId: string; runId: string; req: ExecRequest; source?: 'ui' | 'agent' }; result: ChangePreview };
  edit_preview: { args: { sessionId: string; runId: string; req: EditRequest }; result: ChangePreview };
  edit_apply: { args: { sessionId: string; runId: string; req: EditRequest }; result: EditResult };
  execute_agent: { args: { sessionId: string; runId: string; req: ExecRequest }; result: ResultSet[] };
  cancel: { args: { runId: string }; result: void };
  expand: { args: { sessionId: string; elementId: string; limit: number }; result: ResultSet };
  history_delete: { args: { timestamp: string; profileName: string; text: string }; result: void };
  history_list: { args: { limit: number }; result: HistoryEntry[] };
}
export let activeRequests = 0;
let recycleFrozen = false;
export function freezeRequests(value: boolean) { recycleFrozen = value; }
export const isMock = !('__TAURI_INTERNALS__' in window);
export async function api<K extends keyof Commands>(command: K, args: Commands[K]['args']): Promise<Commands[K]['result']> {
  if (recycleFrozen) throw { kind: 'Busy', message: '작업공간 복원 중입니다. 잠시 후 다시 시도하세요.' };
  if (isMock) return (await import('./mock')).mockInvoke(command, args);
  activeRequests++;
  const since = performance.now();
  try { return await invoke<Commands[K]['result']>(command, args); }
  finally { activeRequests--; if (perf.captureCommands) finish(`ipc:${command}`, since); }
}
