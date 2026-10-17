// Korean display text for enum-like values that cross the wire in English. The wire values never change; unknown values pass through.
const kinds: Record<string, string> = { Insert: '추가', Update: '수정', Delete: '삭제', Ddl: 'DDL', GraphWrite: '그래프 쓰기', Other: '기타' };
export const statementKindLabel = (kind: string) => kinds[kind] ?? kind;
const errorKinds: Record<string, string> = { Connection: '연결', Auth: '인증', Query: '쿼리', Timeout: '시간 초과', Cancelled: '취소됨', Unsupported: '미지원', Internal: '내부', Conflict: '충돌' };
const decisions: Record<string, string> = { allowed: '허용', denied: '거부', approved: '승인' };
export const decisionLabel = (decision: string) => decisions[decision] ?? decision;
export const errorKindLabel = (kind: string) => errorKinds[kind] ?? kind;
