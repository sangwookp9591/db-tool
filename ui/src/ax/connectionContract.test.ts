import { describe, expect, it } from 'vitest';
import { connectionMock, networkStatus, type ConnectionEvent } from './connectionContract';
describe('connection browser contract', () => {
  it('returns pasted credentials only in the transient secret field', async () => {
    const result = await connectionMock('connection_parse', { input: 'mongodb://alice:hidden@localhost:27017/app?authSource=admin' });
    expect(result).toMatchObject({ driver_id: 'mongodb', has_secret: true, secret: 'hidden', fields: { user: 'alice', database: 'app' } });
    const parsed = result as { fields: Record<string,string>; sanitized_input: string };
    expect(parsed.sanitized_input).not.toContain('hidden');
    expect(Object.values(parsed.fields).join(' ')).not.toContain('hidden');
  });
  it('marks unperformed mock transport checks as skipped', async () => {
    const events: ConnectionEvent[] = [];
    await connectionMock('connection_test_steps', { onEvent: { onmessage: (event: ConnectionEvent) => events.push(event) } });
    expect(events).toHaveLength(6);
    expect(events.filter(event => event.event === 'stage' && event.status === 'skipped')).toHaveLength(3);
  });
});

it('shows changed SSH identity as blocked and supports older network events', () => {
  const event = {event:'network',profile_id:'demo',state:'disconnected',attempt:2} as const;
  expect(networkStatus({...event,code:'SSH_HOST_KEY_CHANGED'})).toContain('연결을 차단');
  expect(networkStatus(event)).toBe('네트워크 연결 끊김');
  expect(networkStatus({...event,state:'reconnecting'})).toContain('재연결 중');
});
