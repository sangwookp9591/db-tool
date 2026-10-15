import type { JsonValue } from './contract';
export function parseParams(source: string): Record<string, JsonValue> {
  const value: unknown = JSON.parse(source);
  if (value === null || Array.isArray(value) || typeof value !== 'object') throw new Error('Parameters must be a JSON object.');
  function check(value: unknown): void {
    if (typeof value === 'number' && (!Number.isFinite(value) || (Number.isInteger(value) && !Number.isSafeInteger(value)))) throw new Error('Unsafe numeric parameter. Pass large integers/decimals as strings and cast in the query.');
    if (Array.isArray(value)) value.forEach(check);
    else if (value && typeof value === 'object') Object.values(value).forEach(check);
  }
  check(value);
  return value as Record<string, JsonValue>;
}
