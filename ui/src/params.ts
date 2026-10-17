import type { JsonValue } from './contract';
export function parseParams(source: string): Record<string, JsonValue> {
  const value: unknown = JSON.parse(source);
  if (value === null || Array.isArray(value) || typeof value !== 'object') throw new Error('파라미터는 JSON 객체여야 합니다.');
  function check(value: unknown): void {
    if (typeof value === 'number' && (!Number.isFinite(value) || (Number.isInteger(value) && !Number.isSafeInteger(value)))) throw new Error('안전하지 않은 숫자 파라미터입니다. 큰 정수·소수는 문자열로 전달하고 쿼리에서 형변환하세요.');
    if (Array.isArray(value)) value.forEach(check);
    else if (value && typeof value === 'object') Object.values(value).forEach(check);
  }
  check(value);
  return value as Record<string, JsonValue>;
}
