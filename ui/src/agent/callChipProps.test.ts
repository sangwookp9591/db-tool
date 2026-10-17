import { describe, expect, it } from 'vitest';
import { callChipProps } from './callChip';
import type { AgentCall, Profile } from '../contract';

const profiles = [{ id: 'p1', name: 'Analytics sandbox' }] as Profile[];
const call = (patch: Partial<AgentCall>): AgentCall => ({ id: 'c', name: 'run_query', client_id: 'local', connection_id: 'p1', status: 'success', ...patch });

describe('callChipProps', () => {
  it('maps tool names to icons and statuses', () => {
    expect(callChipProps(call({}), profiles)).toMatchObject({ icon: 'terminal', status: 'done' });
    expect(callChipProps(call({ name: 'get_schema_context', status: 'running' }), profiles)).toMatchObject({ icon: 'search', status: 'running' });
    expect(callChipProps(call({ name: 'explore', status: 'error' }), profiles)).toMatchObject({ icon: 'file', status: 'error' });
  });
  it('shows the connection name and a localized row count', () => {
    expect(callChipProps(call({ row_count: 1200 }), profiles).argument).toBe(`Analytics sandbox · ${(1200).toLocaleString()}행`);
    expect(callChipProps(call({ connection_id: 'gone' }), profiles).argument).toBe('gone');
  });
});
