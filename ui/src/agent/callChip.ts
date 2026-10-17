import type { AgentCall, Profile } from '../contract';
export function callChipProps(call: AgentCall, profiles: Profile[]) {
  return { icon: call.name.includes('query') ? 'terminal' as const : call.name.includes('schema') ? 'search' as const : 'file' as const, name: call.name,
    argument: [profiles.find(profile => profile.id === call.connection_id)?.name ?? call.connection_id, call.row_count != null ? `${call.row_count.toLocaleString()}행` : ''].filter(Boolean).join(' · '),
    status: call.status === 'running' ? 'running' as const : call.status === 'error' ? 'error' as const : 'done' as const, durationMs: call.duration_ms, expectedMs: Math.max(900, call.duration_ms ?? 2400) };
}
