import type { Channel } from '@tauri-apps/api/core';
import type { Profile } from '../contract';
export interface SshHop { host: string; port: number; user: string; auth: 'password' | 'key' | 'agent'; key_file?: string | null }
export interface ConnectionNetwork {
  ssh?: (SshHop & { jump_hosts?: SshHop[] }) | null;
  proxy?: { kind: 'socks5' | 'http_connect'; host: string; port: number; user?: string | null } | null;
}
export type ConnectionAuth = { kind: 'password' } | { kind: 'iam_aws'; profile: string; region: string } | { kind: 'iam_gcp'; adc_file?: string | null };
export type ConnectionStage = 'dns' | 'ssh' | 'tunnel' | 'tls' | 'db_auth' | 'permissions';
export type ConnectionEvent =
  | { event: 'stage'; stage: ConnectionStage; status: 'running' | 'passed' | 'failed' | 'skipped'; elapsed_ms: number; code: string | null; help_key: string | null }
  | { event: 'host_key'; request_id: string; host: string; port: number; fingerprint: string }
  | { event: 'network'; profile_id: string; state: 'connected' | 'reconnecting' | 'disconnected'; attempt: number; code?: string | null };
export function networkStatus(event:Extract<ConnectionEvent,{event:'network'}>) {
  if(event.code==='SSH_HOST_KEY_CHANGED')return 'SSH 서버 지문이 변경되어 연결을 차단했습니다. 서버 관리자에게 새 지문을 확인하세요.';
  return event.state==='reconnecting'?`네트워크 재연결 중 · ${event.attempt}회`:event.state==='disconnected'?'네트워크 연결 끊김':'';
}
export interface ParsedConnection { driver_id: string; fields: Record<string, string>; network: ConnectionNetwork | null; has_secret: boolean; sanitized_input: string; secret: string | null }
export interface ConnectionSecrets { ssh_password?: string; key_passphrase?: string; proxy_password?: string; jumps?: { ssh_password?: string; key_passphrase?: string }[] }
export interface ConnectionCommands {
  connection_parse: { args: { input: string }; result: ParsedConnection };
  connection_test_steps: { args: { profile: Profile; secret: string | null; networkSecrets: ConnectionSecrets; onEvent: Channel<ConnectionEvent> }; result: string };
  connection_host_key_confirm: { args: { requestId: string; accept: boolean }; result: void };
  connection_credentials_save: { args: { profileId: string; secrets: ConnectionSecrets }; result: void };
  iam_profiles: { args: Record<string, never>; result: { aws: string[]; gcp: string[] } };
}
const mockHostKeys=new Map<string,(accept:boolean)=>void>();
export async function connectionMock(command: string, args: unknown): Promise<unknown> {
  globalThis.window?.dispatchEvent(new CustomEvent('ax:mock-command', { detail: { command } }));
  if (command === 'iam_profiles') return { aws: ['demo'], gcp: [] };
  if (command === 'connection_credentials_save') return;
  if (command === 'connection_host_key_confirm') { const {requestId,accept}=args as ConnectionCommands['connection_host_key_confirm']['args'];mockHostKeys.get(requestId)?.(accept);mockHostKeys.delete(requestId);return; }
  if (command === 'connection_test_steps') {
    const { onEvent, profile, networkSecrets } = args as ConnectionCommands['connection_test_steps']['args'];
    globalThis.window?.dispatchEvent(new CustomEvent('ax:mock-test-secrets', { detail: { keys: Object.keys(networkSecrets) } }));
    if(profile?.network?.ssh){const request_id=crypto.randomUUID();const accepted=new Promise<boolean>(resolve=>mockHostKeys.set(request_id,resolve));window.dispatchEvent(new CustomEvent('ax:mock-network',{detail:{event:'host_key',request_id,host:profile.network.ssh.host,port:profile.network.ssh.port,fingerprint:'SHA256:MOCK-FINGERPRINT-NOT-A-REAL-SERVER'}}));if(!await accepted)throw new Error('모의 SSH 지문 확인을 거부했습니다.');}
    for (const stage of ['dns', 'ssh', 'tunnel', 'tls', 'db_auth', 'permissions'] as const) {
      onEvent.onmessage({ event: 'stage', stage, status: stage === 'ssh' || stage === 'tunnel' || stage === 'tls' ? 'skipped' : 'passed', elapsed_ms: 12, code: null, help_key: null });
    }
    return 'Mock database · browser demo';
  }
  if (command === 'connection_parse') {
    const { input } = args as ConnectionCommands['connection_parse']['args'];
    // Browser demo only; native parsing and keychain storage are authoritative.
    const u = new URL(input.trim().replace(/^jdbc:/, ''));
    const driver = /postgres/.test(u.protocol) ? 'postgres' : u.protocol.startsWith('mongodb') ? 'mongodb' : u.protocol.startsWith('redis') ? 'redis' : 'neo4j';
    const fields: Record<string, string> = { host: u.hostname, port: u.port || ({ postgres: '5432', mongodb: '27017', redis: '6379', neo4j: '7687' }[driver]), user: decodeURIComponent(u.username) };
    const querySecret = u.searchParams.has('password') ? u.searchParams.get('password') : u.searchParams.get('sslpassword');
    const secret = u.password !== '' ? decodeURIComponent(u.password) : querySecret;
    const has_secret = u.password !== '' || u.searchParams.has('password') || u.searchParams.has('sslpassword');
    u.searchParams.delete('password'); u.searchParams.delete('sslpassword');
    u.password = ''; u.username = '';
    if (driver === 'mongodb' || driver === 'neo4j') fields.uri = u.toString();
    if (u.pathname.length > 1) fields[driver === 'redis' ? 'db' : 'database'] = decodeURIComponent(u.pathname.slice(1));
    return { driver_id: driver, fields, network: null, has_secret, sanitized_input: `${fields.host}:${fields.port}`, secret } satisfies ParsedConnection;
  }
  throw new Error(`Unknown connection mock command: ${command}`);
}
