import { afterEach, expect, it, vi } from 'vitest';
const host = vi.hoisted(() => ({ channels: [] as { onmessage: (bytes: ArrayBuffer) => void }[], invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({
  Channel: class { onmessage = (_bytes: ArrayBuffer) => {}; constructor() { host.channels.push(this); } },
  invoke: host.invoke
}));
vi.mock('../api', () => ({ isMock: false, api: vi.fn() }));
import { executeQuery } from './stream';
import { fixtureResult, fixtureTable } from './fixtures';
const args = { sessionId: 'test', runId: 'run', req: { text: 'SELECT 1', params: {}, row_limit: 1000, graph_node_limit: 300, timeout_ms: 1000 } };
const done = () => Uint8Array.from([0x81, 0xa4, 0x6b, 0x69, 0x6e, 0x64, 0xa4, 0x64, 0x6f, 0x6e, 0x65]).buffer;
afterEach(() => { host.channels.length = 0; host.invoke.mockReset(); vi.unstubAllGlobals(); });
it('releases the Channel callback on direct completion and ignores late data', async () => {
  vi.stubGlobal('requestAnimationFrame', (run: FrameRequestCallback) => setTimeout(() => run(performance.now()), 0));
  vi.stubGlobal('cancelAnimationFrame', clearTimeout);
  const result = fixtureResult(fixtureTable(1, 2)), update = vi.fn(); let receive: unknown;
  host.invoke.mockImplementation(async () => { receive = host.channels[0].onmessage; return [result]; });
  expect(await executeQuery(args, update)).toEqual([result]);
  expect(host.channels[0].onmessage).not.toBe(receive);
  const count = update.mock.calls.length; host.channels[0].onmessage(done()); expect(update).toHaveBeenCalledTimes(count);
});
it('releases the callback after a host failure or malformed binary frame', async () => {
  vi.stubGlobal('requestAnimationFrame', () => 0); vi.stubGlobal('cancelAnimationFrame', vi.fn());
  let receive: unknown;
  host.invoke.mockImplementation(async () => { receive = host.channels[0].onmessage; throw new Error('host failed'); });
  await expect(executeQuery(args, vi.fn())).rejects.toThrow('host failed');
  expect(host.channels[0].onmessage).not.toBe(receive);
  host.invoke.mockImplementation(async () => { receive = host.channels[1].onmessage; host.channels[1].onmessage(Uint8Array.from([0xc1]).buffer); return []; });
  await expect(executeQuery(args, vi.fn())).rejects.toThrow('Unsupported result MessagePack type');
  expect(host.channels[1].onmessage).not.toBe(receive);
});
