import { beforeAll, expect, it, vi } from 'vitest';
const invoke = vi.hoisted(() => vi.fn().mockResolvedValue(null));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
let api: typeof import('./api').api;
beforeAll(async () => { vi.stubGlobal('window', { __TAURI_INTERNALS__: {} }); api = (await import('./api')).api; });
it('uses the native bridge and camelCase command arguments with snake_case request bodies', async () => {
  await api('session_open', { profileId: 'p', secret: null });
  expect(invoke).toHaveBeenLastCalledWith('session_open', { profileId: 'p', secret: null });
  const req = { text: 'SELECT 1', params: { id: '9007199254740993' }, row_limit: 1000, graph_node_limit: 300, timeout_ms: 30000 };
  await api('execute', { sessionId: 's', runId: 'r', req });
  expect(invoke).toHaveBeenLastCalledWith('execute', { sessionId: 's', runId: 'r', req });
  await api('browse', { sessionId: 's', nodeId: 'opaque', offset: 100, limit: 100, runId: 'b' });
  expect(invoke).toHaveBeenLastCalledWith('browse', { sessionId: 's', nodeId: 'opaque', offset: 100, limit: 100, runId: 'b' });
  await api('expand', { sessionId: 's', elementId: 'element', limit: 300 });
  expect(invoke).toHaveBeenLastCalledWith('expand', { sessionId: 's', elementId: 'element', limit: 300 });
});

it('native timing opt-in preserves errors and excludes arguments', async () => {
  const { perf } = await import('./perf'); perf.clear(); perf.captureCommands = true;
  const failure = new Error('fixture failure'); invoke.mockRejectedValueOnce(failure);
  try {
    await expect(api('session_open', { profileId: 'p', secret: 'private-test-secret' })).rejects.toBe(failure);
    expect(perf.marks().map(m => m.name)).toEqual(['ipc:session_open']);
    expect(JSON.stringify(perf.marks())).not.toContain('private-test-secret');
  } finally { perf.captureCommands = false; perf.clear(); }
});

it('blocks new calls during recycling and counts pending calls through rejection', async () => {
  const native = await import('./api');
  let reject!: (error: Error) => void;
  invoke.mockImplementationOnce(() => new Promise((_resolve, fail) => { reject = fail; }));
  const pending = api('history_list', { limit: 1 });
  expect(native.activeRequests).toBe(1);
  native.freezeRequests(true);
  const before = invoke.mock.calls.length;
  await expect(api('profile_delete', { profileId: 'preserved' })).rejects.toMatchObject({ kind: 'Busy' });
  expect(invoke.mock.calls.length).toBe(before);
  native.freezeRequests(false);
  const error = new Error('read failed'); reject(error);
  await expect(pending).rejects.toBe(error);
  expect(native.activeRequests).toBe(0);
});
