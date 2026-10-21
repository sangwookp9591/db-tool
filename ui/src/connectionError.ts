import type { DbError } from './contract';

const messages: Record<string, [string, string]> = {
  DB_CONNECT_FAILED: ['데이터베이스에 연결하지 못했습니다.', '호스트·포트, 서버 실행 상태와 네트워크 접근 권한을 확인한 뒤 다시 테스트하세요.'],
  DB_CONNECT_TIMEOUT: ['데이터베이스 연결 시간이 초과되었습니다.', '호스트·포트와 방화벽, VPN 또는 SSH 터널 상태를 확인하세요.'],
  DB_AUTH_REJECTED: ['데이터베이스 인증에 실패했습니다.', '연결을 편집하여 사용자 이름과 비밀번호를 확인하세요.'],
  DB_ACCESS_POLICY_REJECTED: ['데이터베이스 접근 정책에 의해 연결이 거부되었습니다.', '서버의 접속 허용 목록, TLS 설정과 사용자 권한을 확인하세요.'],
  DNS_FAILED: ['서버 주소를 찾을 수 없습니다.', '호스트 이름과 DNS, VPN 연결을 확인하세요.'],
  TLS_FAILED: ['보안 연결을 설정하지 못했습니다.', '서버의 TLS 설정, CA 인증서와 인증서의 호스트 이름을 확인하세요.'],
  TLS_TIMEOUT: ['보안 연결 시간이 초과되었습니다.', '서버의 TLS 포트와 네트워크 경로를 확인하세요.'],
  SSH_AUTH_REJECTED: ['SSH 인증에 실패했습니다.', 'SSH 사용자 이름, 비밀번호 또는 개인 키를 확인하세요.'],
  SSH_HOST_KEY_CHANGED: ['SSH 서버 지문이 변경되었습니다.', '서버 관리자에게 새 지문을 확인한 뒤 연결 설정을 검토하세요.'],
  SSH_KEY_PERMISSIONS: ['SSH 개인 키 파일에 접근할 수 없습니다.', '키 파일 경로와 파일 소유자 및 접근 권한을 확인하세요.'],
  SSH_TRANSPORT_FAILED: ['SSH 서버에 연결하지 못했습니다.', 'SSH 호스트·포트와 네트워크 연결을 확인하세요.'],
  TUNNEL_TIMEOUT: ['터널 연결 시간이 초과되었습니다.', 'SSH·프록시 설정과 대상 데이터베이스의 접근 가능 여부를 확인하세요.'],
};

export function readableConnectionError(error: DbError): DbError {
  const copy = messages[error.message];
  const internalHelp = error.detail?.startsWith('connection.help.');
  if (!copy && !internalHelp) return error;
  return { ...error, message: copy?.[0] ?? '연결 설정을 확인해야 합니다.', detail: internalHelp || !error.detail ? copy?.[1] ?? '연결을 편집하여 네트워크·인증 설정을 확인한 뒤 다시 테스트하세요.' : error.detail };
}
