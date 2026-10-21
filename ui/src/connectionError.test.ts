import { expect, it } from 'vitest';
import { readableConnectionError } from './connectionError';
it('turns connection codes and untranslated help keys into actionable copy', () => {
  const error = readableConnectionError({kind:'Connection',message:'DB_CONNECT_FAILED',detail:'connection.help.db_connect_failed',position:null});
  expect(error.message).toBe('데이터베이스에 연결하지 못했습니다.');
  expect(error.detail).toContain('호스트·포트');
});
it('preserves meaningful diagnostics and unrelated query errors', () => {
  const error = {kind:'Connection' as const,message:'DB_AUTH_REJECTED',detail:'Role qa cannot authenticate',position:null};
  expect(readableConnectionError(error).detail).toBe(error.detail);
  const query = {...error,kind:'Query' as const,message:'Syntax error'};
  expect(readableConnectionError(query)).toBe(query);
});
