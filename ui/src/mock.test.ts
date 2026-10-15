import { expect, it } from 'vitest';
import { mockInvoke } from './mock';
import type { ExecRequest } from './contract';
const req = (text: string): ExecRequest => ({ text, params: {}, row_limit: 100000, graph_node_limit: 300, timeout_ms: 30000 });
it('executes large rows, caps results, and preserves raw values', async () => {
  const profiles = await mockInvoke('profiles_list', {});
  const sessionId = await mockInvoke('session_open', { profileId: profiles[0].id, secret: null });
  const [result] = await mockInvoke('execute', { sessionId, runId: 'large-test', req: req('SELECT large;') });
  expect(result.table?.rows).toHaveLength(100000);
  expect(result.table?.rows[0][0]).toEqual({ t: 'Int', v: '9007199254740993' });
  const [capped] = await mockInvoke('execute', { sessionId, runId: 'cap-test', req: { ...req('SELECT large;'), row_limit: 10 } });
  expect(capped.row_count).toBe(10); expect(capped.truncated).toBe(true);
  const [normal] = await mockInvoke('execute', { sessionId, runId: 'comment-test', req: req('SELECT * FROM people; -- error slow') });
  expect(normal.row_count).toBe(24);
  await mockInvoke('session_close', { sessionId });
});
it('cancels and times out asynchronous runs, and records history', async () => {
  const profiles = await mockInvoke('profiles_list', {});
  const sessionId = await mockInvoke('session_open', { profileId: profiles[0].id, secret: null });
  const running = mockInvoke('execute', { sessionId, runId: 'cancel-test', req: req('SELECT slow;') });
  const cancelled = expect(running).rejects.toMatchObject({ kind: 'Cancelled' });
  await mockInvoke('cancel', { runId: 'cancel-test' }); await cancelled;
  await expect(mockInvoke('execute', { sessionId, runId: 'timeout-test', req: { ...req('SELECT slow;'), timeout_ms: 5 } })).rejects.toMatchObject({ kind: 'Timeout' });
  await expect(mockInvoke('execute', { sessionId, runId: 'error-test', req: req('SELECT error;') })).rejects.toMatchObject({ kind: 'Query', position: 8 });
  const history = await mockInvoke('history_list', { limit: 100 });
  expect(history.some(h => h.text === 'SELECT slow;' && h.error?.includes('cancelled'))).toBe(true);
});
it('returns graph plus table, and expands with duplicate seed for merge checks', async () => {
  const profiles = await mockInvoke('profiles_list', {});
  const sessionId = await mockInvoke('session_open', { profileId: profiles[1].id, secret: null });
  const [result] = await mockInvoke('execute', { sessionId, runId: 'graph-test', req: req('MATCH (n) RETURN n') });
  expect(result.graph?.nodes).toHaveLength(3); expect(result.table).not.toBeNull();
  const expanded = await mockInvoke('expand', { sessionId, elementId: 'alice', limit: 300 });
  expect(expanded.graph?.nodes[0].id).toBe('alice');
  const page = await mockInvoke('browse', { sessionId, nodeId: 'people', offset: 99900, limit: 100, runId: 'browse-test' });
  expect(page.page?.has_more).toBe(false);
});
